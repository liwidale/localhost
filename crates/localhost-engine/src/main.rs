use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use localhost_core::{ActionError, Config, Engine, Settings, desktop};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{broadcast, mpsc};
use tokio::task::AbortHandle;

#[derive(Deserialize)]
struct Request {
    id: u64,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Deserialize)]
struct Id {
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Stop {
    id: String,
    force: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Project {
    project_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Run {
    project_id: String,
    script: String,
    trust: bool,
}

#[derive(Deserialize)]
struct Reveal {
    id: String,
    name: String,
}

#[derive(Deserialize)]
struct Url {
    id: String,
    port: u16,
}

#[derive(Deserialize)]
struct Folder {
    path: PathBuf,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum Kind {
    Project,
    Service,
}

#[derive(Deserialize)]
struct Target {
    target: Kind,
    id: String,
}

#[derive(Deserialize)]
struct Visible {
    visible: bool,
}

#[derive(Deserialize)]
struct Subscription {
    subscription: u64,
}

type Out = mpsc::UnboundedSender<String>;

struct State {
    engine: Engine,
    out: Out,
    subscriptions: Mutex<HashMap<u64, Vec<AbortHandle>>>,
    next: Mutex<u64>,
}

fn send(out: &Out, value: Value) {
    let _ = out.send(value.to_string());
}

fn params<T: DeserializeOwned>(value: Value) -> Result<T, ActionError> {
    serde_json::from_value(value).map_err(|e| ActionError::Failed {
        message: e.to_string(),
    })
}

fn failed(e: impl std::fmt::Display) -> ActionError {
    ActionError::Failed {
        message: e.to_string(),
    }
}

fn ok<T: serde::Serialize>(value: T) -> Result<Value, ActionError> {
    serde_json::to_value(value).map_err(failed)
}

impl State {
    fn dir(&self, target: Target) -> Result<PathBuf, ActionError> {
        match target.target {
            Kind::Project => self.engine.project_root(&target.id),
            Kind::Service => self.engine.service_dir(&target.id),
        }
    }

    async fn subscribe(self: &Arc<Self>, id: String) -> Result<Value, ActionError> {
        let sub = self.engine.subscribe_logs(&id).await?;
        let key = {
            let mut next = self.next.lock().unwrap_or_else(|e| e.into_inner());
            *next += 1;
            *next
        };
        let mut receiver = sub.receiver;
        let out = self.out.clone();
        let mut handles: Vec<AbortHandle> = sub.task.into_iter().collect();
        let task = tokio::spawn(async move {
            loop {
                let mut batch = match receiver.recv().await {
                    Ok(line) => vec![line],
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                };
                let deadline = tokio::time::sleep(Duration::from_millis(60));
                tokio::pin!(deadline);
                loop {
                    tokio::select! {
                        _ = &mut deadline => break,
                        line = receiver.recv() => match line {
                            Ok(line) => batch.push(line),
                            Err(broadcast::error::RecvError::Lagged(_)) => {}
                            Err(_) => break,
                        }
                    }
                }
                send(
                    &out,
                    json!({ "event": "logs", "data": { "sub": key, "lines": batch } }),
                );
            }
        });
        handles.push(task.abort_handle());
        self.subscriptions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key, handles);
        ok(json!({ "sub": key, "backlog": sub.backlog }))
    }

    fn unsubscribe(&self, key: u64) {
        if let Some(handles) = self
            .subscriptions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&key)
        {
            for h in handles {
                h.abort();
            }
        }
    }

    async fn call(self: &Arc<Self>, method: &str, p: Value) -> Result<Value, ActionError> {
        let engine = &self.engine;
        match method {
            "snapshot" => ok(&*engine.snapshot()),
            "refresh" => {
                engine.refresh();
                ok(())
            }
            "setVisible" => {
                engine.set_visible(params::<Visible>(p)?.visible);
                ok(())
            }
            "settings" => ok(engine.settings()),
            "setSettings" => {
                let settings: Settings = params(p)?;
                engine.set_settings(settings);
                ok(engine.settings())
            }
            "inspect" => ok(engine.inspect(&params::<Id>(p)?.id).await?),
            "reveal" => {
                let r: Reveal = params(p)?;
                ok(engine.reveal(&r.id, &r.name).await?)
            }
            "stop" => {
                let s: Stop = params(p)?;
                ok(engine.stop(&s.id, s.force).await?)
            }
            "restart" => ok(engine.restart(&params::<Id>(p)?.id).await?),
            "stopProject" => {
                let project = params::<Project>(p)?.project_id;
                let ids: Vec<String> = engine
                    .snapshot()
                    .services
                    .iter()
                    .filter(|s| s.project_id.as_deref() == Some(project.as_str()))
                    .filter(|s| s.controllable && (s.pid.is_some() || s.image.is_some()))
                    .map(|s| s.id.clone())
                    .collect();
                for id in ids {
                    let _ = engine.stop(&id, false).await;
                }
                ok(())
            }
            "startScript" => {
                let r: Run = params(p)?;
                ok(engine.start_script(&r.project_id, &r.script, r.trust)?)
            }
            "dismiss" => ok(engine.dismiss(&params::<Id>(p)?.id)?),
            "clearLogs" => ok(engine.clear_logs(&params::<Id>(p)?.id)?),
            "subscribeLogs" => self.subscribe(params::<Id>(p)?.id).await,
            "unsubscribeLogs" => {
                self.unsubscribe(params::<Subscription>(p)?.subscription);
                ok(())
            }
            "url" => {
                let u: Url = params(p)?;
                ok(engine.url(&u.id, u.port)?)
            }
            "folder" => ok(self.dir(params(p)?)?),
            "openTerminal" => ok(desktop::open_terminal(&self.dir(params(p)?)?).map_err(failed)?),
            "openEditor" => ok(desktop::open_editor(&self.dir(params(p)?)?).map_err(failed)?),
            "editor" => ok(desktop::editor().map(|(_, name)| name)),
            "addProject" => ok(engine.add_project(&params::<Folder>(p)?.path)?),
            "keepProject" => {
                let root = engine.project_root(&params::<Project>(p)?.project_id)?;
                ok(engine.add_project(&root)?)
            }
            "removeProject" => ok(engine.remove_project(&params::<Project>(p)?.project_id)?),
            "runningManaged" => ok(engine.running_managed()),
            _ => Err(ActionError::Unsupported),
        }
    }
}

fn arg(name: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == name {
            return args.next();
        }
    }
    None
}

fn main() {
    #[cfg(windows)]
    if let Some(code) = localhost_core::ctrl_c_helper(&std::env::args().collect::<Vec<_>>()) {
        std::process::exit(code);
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("tokio runtime");
    runtime.block_on(run());
}

async fn run() {
    let engine = Engine::new(Config {
        data_dir: arg("--data").map(PathBuf::from),
        helper: std::env::current_exe().ok(),
        host: arg("--host-pid").and_then(|p| p.parse().ok()),
    });
    engine.start();

    let (out, mut lines) = mpsc::unbounded_channel::<String>();
    let writer = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Some(line) = lines.recv().await {
            if stdout.write_all(line.as_bytes()).await.is_err()
                || stdout.write_all(b"\n").await.is_err()
                || stdout.flush().await.is_err()
            {
                break;
            }
        }
    });

    let mut snapshots = engine.subscribe();
    let snapshot_out = out.clone();
    tokio::spawn(async move {
        while snapshots.changed().await.is_ok() {
            let snapshot = snapshots.borrow_and_update().clone();
            send(
                &snapshot_out,
                json!({ "event": "snapshot", "data": &*snapshot }),
            );
        }
    });

    let mut events = engine.events();
    let event_out = out.clone();
    tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(event) => send(&event_out, json!({ "event": "engine", "data": event })),
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    });

    let state = Arc::new(State {
        engine: engine.clone(),
        out: out.clone(),
        subscriptions: Mutex::new(HashMap::new()),
        next: Mutex::new(0),
    });

    let mut stdin = BufReader::new(tokio::io::stdin()).lines();
    while let Ok(Some(line)) = stdin.next_line().await {
        let Ok(request) = serde_json::from_str::<Request>(&line) else {
            continue;
        };
        let state = state.clone();
        tokio::spawn(async move {
            let reply = match state.call(&request.method, request.params).await {
                Ok(value) => json!({ "id": request.id, "ok": true, "value": value }),
                Err(error) => json!({ "id": request.id, "ok": false, "error": error }),
            };
            send(&state.out, reply);
        });
    }

    engine.shutdown().await;
    drop(state);
    drop(out);
    let _ = tokio::time::timeout(Duration::from_secs(1), writer).await;
}
