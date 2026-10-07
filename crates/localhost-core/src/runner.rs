use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex, MutexGuard};

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc::UnboundedSender;

use crate::logs::{LogBuffer, Stream, now_ms, port_in_use};
use crate::model::Status;
use crate::system::control::{self, ControlError, ControlResult};

#[derive(Clone, Debug)]
pub struct Spec {
    pub id: String,
    pub title: String,
    pub project_root: Option<PathBuf>,
    pub package: Option<PathBuf>,
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Option<Vec<(String, String)>>,
    pub display: String,
}

#[derive(Clone, Debug)]
pub enum RunnerEvent {
    Exited {
        id: String,
        title: String,
        code: Option<i32>,
        crashed: bool,
    },
    PortInUse {
        id: String,
        title: String,
        port: u16,
    },
}

pub struct RunState {
    pub pid: Option<u32>,
    pub status: Status,
    pub started_at: Option<u64>,
    pub exit_code: Option<i32>,
    pub stop_requested: bool,
    pub blocked_port: Option<u16>,
    #[cfg(windows)]
    job: Option<control::Job>,
}

pub struct Managed {
    pub spec: Spec,
    pub logs: Arc<LogBuffer>,
    state: Mutex<RunState>,
}

impl Managed {
    pub fn new(spec: Spec) -> Arc<Self> {
        Arc::new(Self {
            spec,
            logs: Arc::new(LogBuffer::default()),
            state: Mutex::new(RunState {
                pid: None,
                status: Status::Stopped,
                started_at: None,
                exit_code: None,
                stop_requested: false,
                blocked_port: None,
                #[cfg(windows)]
                job: None,
            }),
        })
    }

    pub fn state(&self) -> MutexGuard<'_, RunState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn is_running(&self) -> bool {
        self.state().pid.is_some()
    }

    pub fn start(
        self: &Arc<Self>,
        user_path: Option<&OsString>,
        events: UnboundedSender<RunnerEvent>,
    ) -> io::Result<u32> {
        let mut state = self.state();
        if let Some(pid) = state.pid {
            return Ok(pid);
        }
        let spec = &self.spec;
        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args)
            .current_dir(&spec.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(false);
        match &spec.env {
            Some(env) => {
                cmd.env_clear().envs(env.iter().map(|(k, v)| (k, v)));
            }
            None => {
                if let Some(path) = user_path {
                    cmd.env("PATH", path);
                }
                if std::env::var_os("FORCE_COLOR").is_none()
                    && std::env::var_os("NO_COLOR").is_none()
                {
                    cmd.env("FORCE_COLOR", "1");
                }
            }
        }
        #[cfg(unix)]
        cmd.process_group(0);
        #[cfg(windows)]
        cmd.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);

        self.logs
            .push(Stream::System, &format!("$ {}", spec.display));
        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                self.logs
                    .push(Stream::System, &format!("Could not start: {e}"));
                state.status = Status::Crashed;
                return Err(e);
            }
        };
        let pid = child.id().unwrap_or_default();
        #[cfg(windows)]
        {
            state.job = control::Job::new().filter(|job| {
                child.raw_handle().is_some_and(|h| unsafe {
                    job.assign(h as windows_sys::Win32::Foundation::HANDLE)
                })
            });
        }
        state.pid = Some(pid);
        state.status = Status::Running;
        state.started_at = Some(now_ms() / 1000);
        state.exit_code = None;
        state.stop_requested = false;
        state.blocked_port = None;
        drop(state);

        if let Some(out) = child.stdout.take() {
            tokio::spawn(pump(self.clone(), out, Stream::Out, events.clone()));
        }
        if let Some(err) = child.stderr.take() {
            tokio::spawn(pump(self.clone(), err, Stream::Err, events.clone()));
        }
        let this = self.clone();
        tokio::spawn(async move {
            let status = child.wait().await;
            let code = status.as_ref().ok().and_then(|s| s.code());
            let mut state = this.state();
            let crashed = !state.stop_requested && !status.as_ref().is_ok_and(|s| s.success());
            state.pid = None;
            state.exit_code = code;
            state.status = if state.stop_requested {
                Status::Stopped
            } else if crashed {
                Status::Crashed
            } else {
                Status::Exited
            };
            #[cfg(windows)]
            {
                state.job = None;
            }
            drop(state);
            let message = match code {
                Some(c) => format!("Exited with code {c}"),
                None => "Exited".to_string(),
            };
            this.logs.push(Stream::System, &message);
            let _ = events.send(RunnerEvent::Exited {
                id: this.spec.id.clone(),
                title: this.spec.title.clone(),
                code,
                crashed,
            });
        });
        Ok(pid)
    }

    pub fn stop(&self, force: bool, helper: Option<&std::path::Path>) -> ControlResult {
        let mut state = self.state();
        let pid = state.pid.ok_or(ControlError::Gone)?;
        state.stop_requested = true;
        state.status = Status::Stopping;
        #[cfg(windows)]
        {
            if !force && control::interrupt(pid, helper).is_ok() {
                return Ok(());
            }
            if let Some(job) = &state.job
                && job.terminate()
            {
                return Ok(());
            }
            control::kill(pid)
        }
        #[cfg(unix)]
        {
            let _ = helper;
            if force {
                control::kill_group(pid).or_else(|_| control::kill(pid))
            } else {
                control::interrupt_group(pid)
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (pid, force, helper);
            Err(ControlError::Unsupported)
        }
    }
}

async fn pump(
    managed: Arc<Managed>,
    reader: impl AsyncRead + Unpin,
    stream: Stream,
    events: UnboundedSender<RunnerEvent>,
) {
    let mut reader = BufReader::new(reader);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let text = String::from_utf8_lossy(&buf);
                let line = managed.logs.push(stream, &text);
                if let Some(port) = port_in_use(&line.text) {
                    let mut state = managed.state();
                    if state.blocked_port.is_none() {
                        state.blocked_port = Some(port);
                        drop(state);
                        let _ = events.send(RunnerEvent::PortInUse {
                            id: managed.spec.id.clone(),
                            title: managed.spec.title.clone(),
                            port,
                        });
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn echo_spec() -> Spec {
        let (program, args): (&str, Vec<String>) = if cfg!(windows) {
            (
                "cmd",
                vec!["/C".into(), "echo hello && echo oops 1>&2".into()],
            )
        } else {
            ("sh", vec!["-c".into(), "echo hello; echo oops >&2".into()])
        };
        Spec {
            id: "t".into(),
            title: "test".into(),
            project_root: None,
            package: None,
            program: which::which(program).unwrap(),
            args,
            cwd: std::env::temp_dir(),
            env: None,
            display: "echo".into(),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn captures_output_and_exit() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let managed = Managed::new(echo_spec());
        managed.start(None, tx).unwrap();
        let event = tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(event, RunnerEvent::Exited { crashed: false, .. }));
        tokio::time::sleep(Duration::from_millis(100)).await;
        let lines: Vec<String> = managed
            .logs
            .snapshot()
            .iter()
            .map(|l| l.text.clone())
            .collect();
        assert!(lines.iter().any(|l| l.trim() == "hello"), "{lines:?}");
        assert!(lines.iter().any(|l| l.trim() == "oops"), "{lines:?}");
        assert_eq!(managed.state().status, Status::Exited);
    }
}
