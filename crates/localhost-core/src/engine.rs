use std::collections::{BTreeMap, HashMap, HashSet};
use std::ffi::OsString;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::{Notify, broadcast, mpsc, watch};
use tokio::task::JoinSet;

use crate::detect::project::{self, Location, ProjectIndex, Runner};
use crate::detect::{self, classify, classify_image};
use crate::docker::{Container, Docker};
use crate::health::{self, Probe};
use crate::logs::{LogBuffer, LogLine, Stream, now_ms};
use crate::model::*;
use crate::redact;
use crate::runner::{Managed, RunnerEvent, Spec};
use crate::store::Store;
use crate::system::control::{self, ControlError};
use crate::system::sockets::{Listener, SocketScanner};
use crate::system::{Proc, ProcessTable};

const STARTING_GRACE: u64 = 90;
const STOP_TIMEOUT: Duration = Duration::from_secs(6);

#[derive(Clone, Debug, Default)]
pub struct Config {
    pub data_dir: Option<PathBuf>,
    pub helper: Option<PathBuf>,
    pub host: Option<u32>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum Event {
    Crashed {
        id: String,
        title: String,
        code: Option<i32>,
    },
    PortInUse {
        id: String,
        title: String,
        port: u16,
        owner: Option<String>,
    },
    StopTimedOut {
        id: String,
        title: String,
    },
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", tag = "code")]
pub enum ActionError {
    NotFound,
    Protected,
    Denied,
    Unsupported,
    Failed { message: String },
    NeedsTrust { command: String, path: String },
    NotFoundProgram { program: String },
    InvalidFolder,
}

impl From<ControlError> for ActionError {
    fn from(e: ControlError) -> Self {
        match e {
            ControlError::Gone => Self::NotFound,
            ControlError::Denied => Self::Denied,
            ControlError::Unsupported => Self::Unsupported,
            ControlError::Failed => Self::Failed {
                message: "control".into(),
            },
        }
    }
}

pub type ActionResult<T = ()> = Result<T, ActionError>;

#[derive(Clone, Debug)]
enum ServiceRef {
    Process {
        root: u32,
        start: u64,
        members: Vec<u32>,
        title: String,
        controllable: bool,
    },
    Managed(String),
    Container {
        id: String,
    },
}

#[derive(Default)]
struct DockerCache {
    client: Option<Docker>,
    containers: Vec<Container>,
    state: DockerState,
    refreshed: Option<Instant>,
    detected: Option<Instant>,
}

#[derive(Clone, Copy)]
struct HealthEntry {
    probe: Option<Probe>,
    requested: Instant,
    seen: bool,
}

struct Scanner {
    table: ProcessTable,
    sockets: SocketScanner,
    projects: ProjectIndex,
    listeners: Vec<Listener>,
}

struct Inner {
    config: Config,
    scanner: Mutex<Scanner>,
    store: Mutex<Store>,
    managed: Mutex<BTreeMap<String, Arc<Managed>>>,
    refs: Mutex<HashMap<String, ServiceRef>>,
    roots: Mutex<HashMap<String, PathBuf>>,
    health: Mutex<HashMap<(String, u16), HealthEntry>>,
    docker: tokio::sync::Mutex<DockerCache>,
    snapshot: watch::Sender<Arc<Snapshot>>,
    events: broadcast::Sender<Event>,
    runner_tx: mpsc::UnboundedSender<RunnerEvent>,
    wake: Notify,
    visible: AtomicBool,
    user_path: OnceLock<Option<OsString>>,
    adopted: AtomicU64,
    runtime: tokio::runtime::Handle,
}

#[derive(Clone)]
pub struct Engine {
    inner: Arc<Inner>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub struct LogSubscription {
    pub backlog: Vec<Arc<LogLine>>,
    pub receiver: broadcast::Receiver<Arc<LogLine>>,
    pub task: Option<tokio::task::AbortHandle>,
}

impl Engine {
    pub fn new(config: Config) -> Self {
        #[cfg(windows)]
        control::enable_ctrl_c();
        let store = match &config.data_dir {
            Some(dir) => Store::load(dir),
            None => Store::memory(),
        };
        let (runner_tx, runner_rx) = mpsc::unbounded_channel();
        let engine = Self {
            inner: Arc::new(Inner {
                config,
                scanner: Mutex::new(Scanner {
                    table: ProcessTable::new(),
                    sockets: SocketScanner::default(),
                    projects: ProjectIndex::default(),
                    listeners: Vec::new(),
                }),
                store: Mutex::new(store),
                managed: Mutex::new(BTreeMap::new()),
                refs: Mutex::new(HashMap::new()),
                roots: Mutex::new(HashMap::new()),
                health: Mutex::new(HashMap::new()),
                docker: tokio::sync::Mutex::new(DockerCache::default()),
                snapshot: watch::channel(Arc::new(Snapshot::default())).0,
                events: broadcast::channel(64).0,
                runner_tx,
                wake: Notify::new(),
                visible: AtomicBool::new(true),
                user_path: OnceLock::new(),
                adopted: AtomicU64::new(0),
                runtime: tokio::runtime::Handle::current(),
            }),
        };
        engine.spawn_runner_events(runner_rx);
        engine
    }

    pub fn start(&self) {
        let engine = self.clone();
        tokio::spawn(async move {
            let path = user_path().await;
            let _ = engine.inner.user_path.set(path);
        });
        let engine = self.clone();
        tokio::spawn(async move {
            loop {
                engine.tick().await;
                let wait = if engine.inner.visible.load(Ordering::Relaxed) {
                    Duration::from_secs(2)
                } else {
                    Duration::from_secs(10)
                };
                tokio::select! {
                    _ = tokio::time::sleep(wait) => {}
                    _ = engine.inner.wake.notified() => {}
                }
            }
        });
    }

    pub fn subscribe(&self) -> watch::Receiver<Arc<Snapshot>> {
        self.inner.snapshot.subscribe()
    }

    pub fn events(&self) -> broadcast::Receiver<Event> {
        self.inner.events.subscribe()
    }

    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.inner.snapshot.borrow().clone()
    }

    pub fn refresh(&self) {
        self.inner.wake.notify_one();
    }

    pub fn set_visible(&self, visible: bool) {
        let was = self.inner.visible.swap(visible, Ordering::Relaxed);
        if visible && !was {
            self.refresh();
        }
    }

    pub fn settings(&self) -> Settings {
        lock(&self.inner.store).data.settings.clone()
    }

    pub fn set_settings(&self, settings: Settings) {
        let mut store = lock(&self.inner.store);
        store.data.settings = settings;
        let _ = store.save();
        drop(store);
        self.refresh();
    }

    fn spawn_runner_events(&self, mut rx: mpsc::UnboundedReceiver<RunnerEvent>) {
        let engine = self.clone();
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                let event = match event {
                    RunnerEvent::Exited {
                        id,
                        title,
                        code,
                        crashed,
                    } => crashed.then_some(Event::Crashed { id, title, code }),
                    RunnerEvent::PortInUse { id, title, port } => {
                        let owner = engine
                            .snapshot()
                            .services
                            .iter()
                            .find(|s| s.id != id && s.ports.iter().any(|p| p.port == port))
                            .map(|s| s.id.clone());
                        Some(Event::PortInUse {
                            id,
                            title,
                            port,
                            owner,
                        })
                    }
                };
                if let Some(event) = event {
                    let _ = engine.inner.events.send(event);
                }
                engine.refresh();
            }
        });
    }

    async fn refresh_docker(&self) -> (Vec<Container>, DockerState) {
        let visible = self.inner.visible.load(Ordering::Relaxed);
        let mut cache = self.inner.docker.lock().await;
        if cache.client.is_none()
            && cache
                .detected
                .is_none_or(|t| t.elapsed() > Duration::from_secs(30))
        {
            cache.client = Docker::detect();
            cache.detected = Some(Instant::now());
        }
        let due = cache
            .refreshed
            .is_none_or(|t| t.elapsed() > Duration::from_secs(if visible { 3 } else { 15 }));
        if let Some(client) = cache.client.clone()
            && due
        {
            cache.refreshed = Some(Instant::now());
            match client.containers().await {
                Ok(list) => {
                    cache.containers = list;
                    cache.state = DockerState::Running;
                }
                Err(_) => {
                    cache.containers.clear();
                    cache.state = DockerState::Unavailable;
                    cache.client = None;
                }
            }
        }
        (cache.containers.clone(), cache.state)
    }

    async fn tick(&self) {
        let (containers, docker) = self.refresh_docker().await;
        let managed: Vec<Arc<Managed>> = lock(&self.inner.managed).values().cloned().collect();
        let engine = self.clone();
        let built =
            tokio::task::spawn_blocking(move || engine.build(&managed, &containers, docker)).await;
        let Ok((snapshot, probes)) = built else {
            return;
        };
        self.inner.snapshot.send_replace(Arc::new(snapshot));
        if !probes.is_empty() {
            self.run_probes(probes);
        }
    }

    fn run_probes(&self, probes: Vec<(String, u16, IpAddr, Protocol)>) {
        let _runtime = self.inner.runtime.enter();
        let engine = self.clone();
        tokio::spawn(async move {
            let mut set = JoinSet::new();
            for (id, port, addr, protocol) in probes {
                if let Some(target) = health::target(addr, port) {
                    set.spawn(async move { (id, port, health::probe(target, protocol).await) });
                }
            }
            let mut any = false;
            while let Some(Ok((id, port, probe))) = set.join_next().await {
                if let Some(entry) = lock(&engine.inner.health).get_mut(&(id, port)) {
                    entry.probe = Some(probe);
                    any = true;
                }
            }
            if any {
                engine.refresh();
            }
        });
    }

    fn build(
        &self,
        managed: &[Arc<Managed>],
        containers: &[Container],
        docker: DockerState,
    ) -> (Snapshot, Vec<(String, u16, IpAddr, Protocol)>) {
        let mut scanner = lock(&self.inner.scanner);
        let Scanner {
            table,
            sockets,
            projects,
            listeners,
        } = &mut *scanner;
        table.refresh();
        *listeners = sockets.listening();
        let settings = self.settings();

        let launched: HashSet<u32> = managed
            .iter()
            .filter_map(|m| m.state().pid)
            .flat_map(|pid| table.descendants(pid).into_iter().map(|(p, _)| p))
            .collect();
        let me = table.self_pid();
        let host = self.inner.config.host;
        let own = |pid: u32| {
            if pid == me || Some(pid) == host {
                return true;
            }
            if launched.contains(&pid) {
                return false;
            }
            let ancestors = table.ancestors(pid);
            ancestors.contains(&me) || host.is_some_and(|h| ancestors.contains(&h))
        };
        let mut by_pid: BTreeMap<u32, Vec<Listener>> = BTreeMap::new();
        for l in listeners.iter().filter(|l| !own(l.pid)) {
            by_pid.entry(l.pid).or_default().push(*l);
        }

        let mut services: Vec<Service> = Vec::new();
        let mut refs: HashMap<String, ServiceRef> = HashMap::new();
        let mut locations: HashMap<String, Location> = HashMap::new();
        let mut claimed: HashSet<u32> = HashSet::new();

        for m in managed {
            let state = m.state();
            let members: Vec<u32> = match state.pid {
                Some(pid) if table.get(pid).is_some() => {
                    table.descendants(pid).into_iter().map(|(p, _)| p).collect()
                }
                _ => Vec::new(),
            };
            claimed.extend(&members);
            let ports = collect_ports(&members, &by_pid);
            let procs: Vec<&Proc> = members.iter().filter_map(|p| table.get(*p)).collect();
            let loc = m.spec.project_root.as_ref().map(|root| Location {
                root: root.clone(),
                package: m.spec.package.clone().unwrap_or_else(|| root.clone()),
                confidence: Confidence::High,
            });
            let deps = loc
                .as_ref()
                .map(|l| projects.package(&l.package, &l.root).dependencies.clone())
                .unwrap_or_default();
            let mut identity = classify(&procs, &port_numbers(&ports), &deps);
            let protocol = detect::protocol(&identity);
            identity.evidence.insert(
                0,
                Evidence {
                    kind: EvidenceKind::Managed,
                    detail: m.spec.display.clone(),
                },
            );
            let age = state.started_at.map(|s| now_ms() / 1000 - s).unwrap_or(0);
            let status = match state.status {
                Status::Running
                    if ports.is_empty() && identity.kind != Kind::Task && age < STARTING_GRACE =>
                {
                    Status::Starting
                }
                s => s,
            };
            let id = m.spec.id.clone();
            if let Some(l) = &loc {
                locations.insert(id.clone(), l.clone());
            }
            services.push(Service {
                id: id.clone(),
                source: Source::Managed,
                project_id: loc.as_ref().map(|l| project::project_id(&l.root)),
                package: loc.as_ref().map(|l| project::relative(&l.root, &l.package)),
                title: m.spec.title.clone(),
                identity,
                status,
                ports: ports
                    .iter()
                    .map(|(port, addr)| port_entry(*port, *addr, protocol))
                    .collect(),
                pid: state.pid,
                process_count: members.len(),
                cpu: procs.iter().map(|p| p.cpu).sum(),
                memory: procs.iter().map(|p| p.memory).sum(),
                started_at: state.started_at,
                command: Some(redact::url_credentials(&m.spec.display)),
                cwd: Some(project::display_path(&m.spec.cwd)),
                image: None,
                system: false,
                controllable: true,
                has_logs: true,
                exit_code: state.exit_code,
                blocked_port: state.blocked_port,
            });
            refs.insert(id.clone(), ServiceRef::Managed(id));
        }

        let published: HashSet<u16> = containers
            .iter()
            .flat_map(|c| c.published().into_iter().map(|(_, p)| p))
            .collect();
        by_pid.retain(|pid, ls| {
            if claimed.contains(pid) {
                return false;
            }
            let forwarder = table
                .get(*pid)
                .is_some_and(|p| matches!(detect::runtime(p), Some("docker" | "wsl")));
            if forwarder {
                ls.retain(|l| !published.contains(&l.port));
            }
            !ls.is_empty()
        });

        let mut groups: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        for &pid in by_pid.keys() {
            let root = find_root(table, projects, pid);
            groups.entry(root).or_default().push(pid);
        }
        let grouped: HashSet<u32> = groups
            .keys()
            .flat_map(|r| table.descendants(*r).into_iter().map(|(p, _)| p))
            .collect();
        for p in table.iter() {
            if !p.owned || claimed.contains(&p.pid) || grouped.contains(&p.pid) || p.cmd.is_empty()
            {
                continue;
            }
            let id = classify(&[p], &[], &[]);
            if id.kind == Kind::Task && id.confidence >= 0.6 {
                let root = find_root(table, projects, p.pid);
                if !grouped.contains(&root) {
                    groups.entry(root).or_default();
                }
            }
        }
        let mut owner: HashMap<u32, u32> = HashMap::new();
        for p in table.iter() {
            let found = std::iter::once(p.pid)
                .chain(table.ancestors(p.pid))
                .find(|a| groups.contains_key(a));
            if let Some(root) = found {
                owner.insert(p.pid, root);
            }
        }

        for (root, listener_pids) in &groups {
            let Some(proc) = table.get(*root) else {
                continue;
            };
            let members: Vec<u32> = table
                .descendants(*root)
                .into_iter()
                .map(|(p, _)| p)
                .filter(|p| owner.get(p) == Some(root))
                .collect();
            let procs: Vec<&Proc> = members.iter().filter_map(|p| table.get(*p)).collect();
            let mut ports: Vec<(u16, IpAddr)> = Vec::new();
            for pid in listener_pids.iter().chain(members.iter()) {
                if let Some(ls) = by_pid.get(pid) {
                    ports.extend(ls.iter().map(|l| (l.port, l.addr)));
                }
            }
            let ports = merge_ports(ports);
            let loc = listener_pids
                .iter()
                .chain(std::iter::once(root))
                .filter_map(|p| table.get(*p))
                .find_map(|p| locate(projects, p));
            let deps = loc
                .as_ref()
                .map(|l| projects.package(&l.package, &l.root).dependencies.clone())
                .unwrap_or_default();
            let identity = classify(&procs, &port_numbers(&ports), &deps);
            let id = format!("p{}-{}", root, proc.start_time);
            let title = service_title(&identity, loc.as_ref(), projects, proc);
            let controllable = proc.owned && !members.iter().any(|p| table.is_protected(*p));
            let protocol = detect::protocol(&identity);
            if let Some(l) = &loc {
                locations.insert(id.clone(), l.clone());
            }
            refs.insert(
                id.clone(),
                ServiceRef::Process {
                    root: *root,
                    start: proc.start_time,
                    members: members.clone(),
                    title: title.clone(),
                    controllable,
                },
            );
            services.push(Service {
                id,
                source: Source::Process,
                project_id: loc.as_ref().map(|l| project::project_id(&l.root)),
                package: loc.as_ref().map(|l| project::relative(&l.root, &l.package)),
                title,
                identity,
                status: Status::Running,
                ports: ports
                    .iter()
                    .map(|(port, addr)| port_entry(*port, *addr, protocol))
                    .collect(),
                pid: Some(*root),
                process_count: members.len(),
                cpu: procs.iter().map(|p| p.cpu).sum(),
                memory: procs.iter().map(|p| p.memory).sum(),
                started_at: Some(proc.start_time),
                command: (!proc.cmd.is_empty()).then(|| redact::command(user_command(proc))),
                cwd: proc.cwd.as_deref().map(project::display_path),
                image: None,
                system: !proc.owned,
                controllable,
                has_logs: false,
                exit_code: None,
                blocked_port: None,
            });
        }

        for c in containers {
            let published = c.published();
            let ports: Vec<(u16, IpAddr)> = published
                .iter()
                .map(|(ip, port)| (*port, ip.parse().unwrap_or(IpAddr::from([0, 0, 0, 0]))))
                .collect();
            let identity = classify_image(&c.image, &port_numbers(&ports));
            let protocol = detect::protocol(&identity);
            let loc = c.compose_dir().filter(|d| d.is_dir()).and_then(|d| {
                projects.locate(&d).or(Some(Location {
                    root: d.clone(),
                    package: d,
                    confidence: Confidence::Medium,
                }))
            });
            let id = format!("c{}", c.id.chars().take(12).collect::<String>());
            if let Some(l) = &loc {
                locations.insert(id.clone(), l.clone());
            }
            refs.insert(id.clone(), ServiceRef::Container { id: c.id.clone() });
            services.push(Service {
                id,
                source: Source::Container,
                project_id: loc.as_ref().map(|l| project::project_id(&l.root)),
                package: None,
                title: c
                    .compose_service()
                    .map(str::to_string)
                    .unwrap_or_else(|| c.name()),
                identity,
                status: if c.state == "running" {
                    Status::Running
                } else {
                    Status::Starting
                },
                ports: ports
                    .iter()
                    .map(|(port, addr)| port_entry(*port, *addr, protocol))
                    .collect(),
                pid: None,
                process_count: 0,
                cpu: 0.0,
                memory: 0,
                started_at: Some(c.created),
                command: None,
                cwd: None,
                image: Some(c.image.clone()),
                system: false,
                controllable: true,
                has_logs: true,
                exit_code: None,
                blocked_port: None,
            });
        }

        let mut counts: HashMap<u16, usize> = HashMap::new();
        for s in &services {
            for p in &s.ports {
                *counts.entry(p.port).or_default() += 1;
            }
        }
        let mut probes = Vec::new();
        {
            let mut health = lock(&self.inner.health);
            for entry in health.values_mut() {
                entry.seen = false;
            }
            for s in &mut services {
                for p in &mut s.ports {
                    p.shared = counts.get(&p.port).copied().unwrap_or(0) > 1;
                    let key = (s.id.clone(), p.port);
                    let fresh = !health.contains_key(&key);
                    let entry = health.entry(key).or_insert_with(|| HealthEntry {
                        probe: None,
                        requested: Instant::now(),
                        seen: true,
                    });
                    entry.seen = true;
                    let probe_allowed = !s.system || s.identity.tool.is_some();
                    if fresh
                        && probe_allowed
                        && settings.health_checks
                        && s.status == Status::Running
                    {
                        probes.push((s.id.clone(), p.port, parse_addr(&p.address), p.protocol));
                    }
                    if let Some(probe) = entry.probe {
                        p.health = probe.health;
                        if probe.protocol != Protocol::Tcp || p.protocol.is_web() {
                            p.protocol = probe.protocol;
                        }
                    }
                    p.url = url_for(p);
                }
            }
            health.retain(|_, e| e.seen);
        }

        let store = lock(&self.inner.store);
        let mut roots: BTreeMap<String, (PathBuf, Confidence)> = BTreeMap::new();
        for loc in locations.values() {
            let id = project::project_id(&loc.root);
            let e = roots
                .entry(id)
                .or_insert((loc.root.clone(), loc.confidence));
            e.1 = e.1.max(loc.confidence);
        }
        for root in &store.data.projects {
            roots
                .entry(project::project_id(root))
                .or_insert((root.clone(), Confidence::High));
        }
        let mut projects_out = Vec::new();
        let mut root_map = HashMap::new();
        for (id, (root, confidence)) in roots {
            let packages = projects.packages_in(&root);
            let mut ecosystems: Vec<&'static str> = Vec::new();
            let mut scripts = Vec::new();
            let mut package_manager = None;
            for (dir, pkg) in &packages {
                for e in &pkg.ecosystems {
                    if !ecosystems.contains(e) {
                        ecosystems.push(e);
                    }
                }
                package_manager = package_manager.or(pkg.package_manager);
                let rel = project::relative(&root, dir);
                for s in &pkg.scripts {
                    scripts.push(Script {
                        id: script_id(&rel, &s.name, &s.runner),
                        name: s.name.clone(),
                        package: rel.clone(),
                        command: s.runner.display(),
                        long_running: s.long_running,
                    });
                }
            }
            let name = root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| root.display().to_string());
            projects_out.push(Project {
                id: id.clone(),
                name,
                path: project::display_path(&root),
                ecosystems,
                package_manager,
                scripts,
                saved: store.is_saved(&root),
                trusted: store.is_trusted(&root),
                confidence,
            });
            root_map.insert(id, root);
        }
        drop(store);
        projects_out.sort_by_key(|p| p.name.to_lowercase());
        services.sort_by(|a, b| {
            (
                a.system,
                a.project_id.is_none(),
                a.title.to_lowercase(),
                a.ports.first().map(|p| p.port),
            )
                .cmp(&(
                    b.system,
                    b.project_id.is_none(),
                    b.title.to_lowercase(),
                    b.ports.first().map(|p| p.port),
                ))
        });
        *lock(&self.inner.refs) = refs;
        *lock(&self.inner.roots) = root_map;
        (
            Snapshot {
                projects: projects_out,
                services,
                docker,
                platform: std::env::consts::OS,
            },
            probes,
        )
    }

    fn service_ref(&self, id: &str) -> ActionResult<ServiceRef> {
        lock(&self.inner.refs)
            .get(id)
            .cloned()
            .ok_or(ActionError::NotFound)
    }

    fn managed(&self, id: &str) -> ActionResult<Arc<Managed>> {
        lock(&self.inner.managed)
            .get(id)
            .cloned()
            .ok_or(ActionError::NotFound)
    }

    async fn docker(&self) -> ActionResult<Docker> {
        self.inner
            .docker
            .lock()
            .await
            .client
            .clone()
            .ok_or(ActionError::Unsupported)
    }

    fn verify_process(&self, root: u32, start: u64) -> ActionResult<()> {
        let mut scanner = lock(&self.inner.scanner);
        scanner.table.refresh();
        match scanner.table.get(root) {
            Some(p) if p.start_time == start => Ok(()),
            _ => Err(ActionError::NotFound),
        }
    }

    pub async fn stop(&self, id: &str, force: bool) -> ActionResult<()> {
        let result = match self.service_ref(id)? {
            ServiceRef::Managed(mid) => {
                let managed = self.managed(&mid)?;
                let pid = managed.state().pid;
                managed.stop(force, self.inner.config.helper.as_deref())?;
                if !force {
                    let engine = self.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(STOP_TIMEOUT).await;
                        if pid.is_some() && managed.state().pid == pid {
                            let _ = managed.stop(true, None);
                        }
                        engine.refresh();
                    });
                }
                Ok(())
            }
            ServiceRef::Container { id: cid } => {
                let docker = self.docker().await?;
                docker
                    .action(&cid, if force { "kill" } else { "stop" })
                    .await
                    .map_err(|e| ActionError::Failed {
                        message: e.to_string(),
                    })
            }
            ServiceRef::Process {
                root,
                start,
                members,
                title,
                controllable,
            } => {
                if !controllable {
                    return Err(ActionError::Protected);
                }
                self.verify_process(root, start)?;
                if force {
                    let mut failed = None;
                    for pid in members.iter().rev() {
                        if let Err(e) = control::kill(*pid)
                            && e != ControlError::Gone
                        {
                            failed = Some(e);
                        }
                    }
                    failed.map_or(Ok(()), |e| Err(e.into()))
                } else {
                    control::interrupt(root, self.inner.config.helper.as_deref())?;
                    let engine = self.clone();
                    let id = id.to_string();
                    tokio::spawn(async move {
                        tokio::time::sleep(STOP_TIMEOUT).await;
                        if engine.verify_process(root, start).is_ok() {
                            let _ = engine.inner.events.send(Event::StopTimedOut { id, title });
                        }
                        engine.refresh();
                    });
                    Ok(())
                }
            }
        };
        self.refresh();
        result
    }

    async fn wait_exit(&self, root: u32, start: u64) -> bool {
        for _ in 0..40 {
            tokio::time::sleep(Duration::from_millis(200)).await;
            if self.verify_process(root, start).is_err() {
                return true;
            }
        }
        false
    }

    pub async fn restart(&self, id: &str) -> ActionResult<String> {
        match self.service_ref(id)? {
            ServiceRef::Managed(mid) => {
                let managed = self.managed(&mid)?;
                if managed.is_running() {
                    managed.stop(false, self.inner.config.helper.as_deref())?;
                    for i in 0..40 {
                        tokio::time::sleep(Duration::from_millis(200)).await;
                        if !managed.is_running() {
                            break;
                        }
                        if i == 30 {
                            let _ = managed.stop(true, None);
                        }
                    }
                }
                self.launch(&managed)?;
                Ok(mid)
            }
            ServiceRef::Container { id: cid } => {
                let docker = self.docker().await?;
                docker
                    .action(&cid, "restart")
                    .await
                    .map_err(|e| ActionError::Failed {
                        message: e.to_string(),
                    })?;
                self.refresh();
                Ok(id.to_string())
            }
            ServiceRef::Process {
                root,
                start,
                members,
                title,
                controllable,
            } => {
                if !controllable {
                    return Err(ActionError::Protected);
                }
                let spec = {
                    let mut scanner = lock(&self.inner.scanner);
                    let proc = scanner
                        .table
                        .get(root)
                        .filter(|p| p.start_time == start)
                        .cloned()
                        .ok_or(ActionError::NotFound)?;
                    let program = proc.exe.clone().ok_or(ActionError::Unsupported)?;
                    let cwd = proc.cwd.clone().ok_or(ActionError::Unsupported)?;
                    let env = scanner.table.environment(root);
                    let loc = locate(&mut scanner.projects, &proc);
                    let n = self.inner.adopted.fetch_add(1, Ordering::Relaxed);
                    Spec {
                        id: format!("a{n}-{root}"),
                        title,
                        project_root: loc.as_ref().map(|l| l.root.clone()),
                        package: loc.map(|l| l.package),
                        program,
                        args: proc.cmd.iter().skip(1).cloned().collect(),
                        cwd,
                        env,
                        display: redact::command(&proc.cmd),
                    }
                };
                self.verify_process(root, start)?;
                control::interrupt(root, self.inner.config.helper.as_deref())
                    .or_else(|_| control::kill(root))?;
                if !self.wait_exit(root, start).await {
                    for pid in members.iter().rev() {
                        let _ = control::kill(*pid);
                    }
                    self.wait_exit(root, start).await;
                }
                let managed = Managed::new(spec);
                let new_id = managed.spec.id.clone();
                lock(&self.inner.managed).insert(new_id.clone(), managed.clone());
                self.launch(&managed)?;
                Ok(new_id)
            }
        }
    }

    fn launch(&self, managed: &Arc<Managed>) -> ActionResult<()> {
        let _runtime = self.inner.runtime.enter();
        let path = self.inner.user_path.get().cloned().flatten();
        managed
            .start(path.as_ref(), self.inner.runner_tx.clone())
            .map_err(|e| ActionError::Failed {
                message: e.to_string(),
            })?;
        self.refresh();
        Ok(())
    }

    pub fn start_script(
        &self,
        project_id: &str,
        script: &str,
        trust: bool,
    ) -> ActionResult<String> {
        let root = lock(&self.inner.roots)
            .get(project_id)
            .cloned()
            .ok_or(ActionError::NotFound)?;
        let (dir, def) = {
            let mut scanner = lock(&self.inner.scanner);
            scanner
                .projects
                .packages_in(&root)
                .into_iter()
                .find_map(|(dir, pkg)| {
                    let rel = project::relative(&root, &dir);
                    pkg.scripts
                        .iter()
                        .find(|s| script_id(&rel, &s.name, &s.runner) == script)
                        .cloned()
                        .map(|s| (dir.clone(), s))
                })
                .ok_or(ActionError::NotFound)?
        };
        let rel = project::relative(&root, &dir);
        let id = format!("s{project_id}-{}", short_hash(script));
        if let Ok(existing) = self.managed(&id)
            && existing.is_running()
        {
            return Ok(id);
        }
        {
            let mut store = lock(&self.inner.store);
            if !store.is_trusted(&root) {
                if !trust {
                    return Err(ActionError::NeedsTrust {
                        command: def.runner.display(),
                        path: project::display_path(&dir),
                    });
                }
                store.trust(&root);
                let _ = store.save();
            }
        }
        let (program, args) = def.runner.program_and_args();
        let path = self.inner.user_path.get().cloned().flatten();
        let resolved = match &path {
            Some(p) => which::which_in(program, Some(p), &dir),
            None => which::which(program),
        }
        .map_err(|_| ActionError::NotFoundProgram {
            program: program.to_string(),
        })?;
        let title = if rel.is_empty() {
            def.name.clone()
        } else {
            format!("{rel} · {}", def.name)
        };
        let spec = Spec {
            id: id.clone(),
            title,
            project_root: Some(root.clone()),
            package: Some(dir.clone()),
            program: resolved,
            args,
            cwd: dir,
            env: None,
            display: def.runner.display(),
        };
        let managed = {
            let mut map = lock(&self.inner.managed);
            let managed = Managed::new(spec);
            if let Some(old) = map.get(&id) {
                for line in old.logs.snapshot() {
                    managed.logs.push_at(Some(line.ts), line.stream, &line.text);
                }
            }
            map.insert(id.clone(), managed.clone());
            managed
        };
        self.launch(&managed)?;
        Ok(id)
    }

    pub fn dismiss(&self, id: &str) -> ActionResult<()> {
        let mut map = lock(&self.inner.managed);
        match map.get(id) {
            Some(m) if !m.is_running() => {
                map.remove(id);
                drop(map);
                self.refresh();
                Ok(())
            }
            Some(_) => Err(ActionError::Protected),
            None => Err(ActionError::NotFound),
        }
    }

    pub fn clear_logs(&self, id: &str) -> ActionResult<()> {
        self.managed(id)?.logs.clear();
        Ok(())
    }

    pub async fn subscribe_logs(&self, id: &str) -> ActionResult<LogSubscription> {
        match self.service_ref(id)? {
            ServiceRef::Managed(mid) => {
                let managed = self.managed(&mid)?;
                let receiver = managed.logs.subscribe();
                Ok(LogSubscription {
                    backlog: managed.logs.snapshot(),
                    receiver,
                    task: None,
                })
            }
            ServiceRef::Container { id: cid } => {
                let docker = self.docker().await?;
                let buffer = Arc::new(LogBuffer::default());
                let receiver = buffer.subscribe();
                let task = tokio::spawn(async move {
                    if let Err(e) = docker.follow_logs(&cid, buffer.clone()).await {
                        buffer.push(Stream::System, &format!("Log stream ended: {e}"));
                    }
                });
                Ok(LogSubscription {
                    backlog: Vec::new(),
                    receiver,
                    task: Some(task.abort_handle()),
                })
            }
            ServiceRef::Process { .. } => Err(ActionError::Unsupported),
        }
    }

    pub async fn inspect(&self, id: &str) -> ActionResult<ServiceDetail> {
        let service = self.service_ref(id)?;
        self.reprobe(id);
        match service {
            ServiceRef::Container { id: cid } => {
                let docker = self.docker().await?;
                let env = docker.environment(&cid).await.ok().map(redact_env);
                Ok(ServiceDetail {
                    id: id.to_string(),
                    tree: Vec::new(),
                    ancestry: Vec::new(),
                    environment: env,
                    executable: None,
                })
            }
            ServiceRef::Managed(mid) => {
                let pid = self.managed(&mid)?.state().pid;
                Ok(self.process_detail(id, pid))
            }
            ServiceRef::Process { root, .. } => Ok(self.process_detail(id, Some(root))),
        }
    }

    fn process_detail(&self, id: &str, root: Option<u32>) -> ServiceDetail {
        let mut scanner = lock(&self.inner.scanner);
        let Some(root) = root.filter(|r| scanner.table.get(*r).is_some()) else {
            return ServiceDetail {
                id: id.to_string(),
                tree: Vec::new(),
                ancestry: Vec::new(),
                environment: None,
                executable: None,
            };
        };
        let node = |table: &ProcessTable, listeners: &[Listener], pid: u32, depth: usize| {
            table.get(pid).map(|p| ProcessNode {
                pid,
                depth,
                name: p.name.clone(),
                command: if p.cmd.is_empty() {
                    p.name.clone()
                } else {
                    redact::command(&p.cmd)
                },
                cpu: p.cpu,
                memory: p.memory,
                ports: {
                    let mut ports: Vec<u16> = listeners
                        .iter()
                        .filter(|l| l.pid == pid)
                        .map(|l| l.port)
                        .collect();
                    ports.sort_unstable();
                    ports.dedup();
                    ports
                },
            })
        };
        let tree = scanner
            .table
            .descendants(root)
            .into_iter()
            .filter_map(|(pid, depth)| node(&scanner.table, &scanner.listeners, pid, depth))
            .collect();
        let ancestry = scanner
            .table
            .ancestors(root)
            .into_iter()
            .take(6)
            .filter_map(|pid| node(&scanner.table, &scanner.listeners, pid, 0))
            .collect();
        let executable = scanner
            .table
            .descendants(root)
            .into_iter()
            .filter_map(|(pid, _)| scanner.table.get(pid))
            .find(|p| !SHELLS.contains(&p.stem().as_str()) && p.stem() != "conhost")
            .and_then(|p| p.exe.as_deref().map(project::display_path));
        let owned = scanner.table.get(root).is_some_and(|p| p.owned);
        let environment = owned
            .then(|| scanner.table.environment(root))
            .flatten()
            .map(redact_env);
        ServiceDetail {
            id: id.to_string(),
            tree,
            ancestry,
            environment,
            executable,
        }
    }

    pub async fn reveal(&self, id: &str, name: &str) -> ActionResult<String> {
        let env = match self.service_ref(id)? {
            ServiceRef::Container { id: cid } => self
                .docker()
                .await?
                .environment(&cid)
                .await
                .map_err(|_| ActionError::NotFound)?,
            ServiceRef::Managed(mid) => {
                let pid = self
                    .managed(&mid)?
                    .state()
                    .pid
                    .ok_or(ActionError::NotFound)?;
                lock(&self.inner.scanner)
                    .table
                    .environment(pid)
                    .ok_or(ActionError::Denied)?
            }
            ServiceRef::Process { root, .. } => {
                let mut scanner = lock(&self.inner.scanner);
                if !scanner.table.get(root).is_some_and(|p| p.owned) {
                    return Err(ActionError::Denied);
                }
                scanner.table.environment(root).ok_or(ActionError::Denied)?
            }
        };
        env.into_iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v)
            .ok_or(ActionError::NotFound)
    }

    pub fn reprobe(&self, id: &str) {
        let mut health = lock(&self.inner.health);
        let snapshot = self.snapshot();
        let Some(service) = snapshot.services.iter().find(|s| s.id == id) else {
            return;
        };
        if !self.settings().health_checks {
            return;
        }
        let mut probes = Vec::new();
        for p in &service.ports {
            if let Some(entry) = health.get_mut(&(id.to_string(), p.port))
                && entry.requested.elapsed() > Duration::from_secs(5)
            {
                entry.requested = Instant::now();
                probes.push((id.to_string(), p.port, parse_addr(&p.address), p.protocol));
            }
        }
        drop(health);
        if !probes.is_empty() {
            self.run_probes(probes);
        }
    }

    pub fn url(&self, id: &str, port: u16) -> ActionResult<String> {
        self.snapshot()
            .services
            .iter()
            .find(|s| s.id == id)
            .and_then(|s| s.ports.iter().find(|p| p.port == port))
            .and_then(|p| p.url.clone())
            .ok_or(ActionError::NotFound)
    }

    pub fn project_root(&self, project_id: &str) -> ActionResult<PathBuf> {
        lock(&self.inner.roots)
            .get(project_id)
            .cloned()
            .ok_or(ActionError::NotFound)
    }

    pub fn service_dir(&self, id: &str) -> ActionResult<PathBuf> {
        if let ServiceRef::Managed(mid) = self.service_ref(id)? {
            return Ok(self.managed(&mid)?.spec.cwd.clone());
        }
        if let ServiceRef::Process { root, .. } = self.service_ref(id)? {
            let scanner = lock(&self.inner.scanner);
            if let Some(cwd) = scanner.table.get(root).and_then(|p| p.cwd.clone())
                && !project::is_ignored(&cwd)
            {
                return Ok(cwd);
            }
        }
        let snapshot = self.snapshot();
        let project = snapshot
            .services
            .iter()
            .find(|s| s.id == id)
            .and_then(|s| s.project_id.clone())
            .ok_or(ActionError::NotFound)?;
        self.project_root(&project)
    }

    pub fn add_project(&self, dir: &Path) -> ActionResult<String> {
        if !dir.is_dir() || dir.parent().is_none() || project::home().is_some_and(|h| h == dir) {
            return Err(ActionError::InvalidFolder);
        }
        let root = project::locate_uncached(dir)
            .map(|l| l.root)
            .filter(|r| dir.starts_with(r))
            .unwrap_or_else(|| dir.to_path_buf());
        let mut store = lock(&self.inner.store);
        store.add(&root);
        let _ = store.save();
        drop(store);
        self.refresh();
        Ok(project::project_id(&root))
    }

    pub fn remove_project(&self, project_id: &str) -> ActionResult<()> {
        let root = self.project_root(project_id)?;
        let mut store = lock(&self.inner.store);
        store.remove(&root);
        let _ = store.save();
        drop(store);
        self.refresh();
        Ok(())
    }

    pub async fn shutdown(&self) {
        let managed: Vec<Arc<Managed>> = lock(&self.inner.managed).values().cloned().collect();
        for m in &managed {
            let _ = m.stop(false, self.inner.config.helper.as_deref());
        }
        for _ in 0..25 {
            if managed.iter().all(|m| !m.is_running()) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        for m in &managed {
            let _ = m.stop(true, None);
        }
    }

    pub fn running_managed(&self) -> usize {
        lock(&self.inner.managed)
            .values()
            .filter(|m| m.is_running())
            .count()
    }
}

fn redact_env(vars: Vec<(String, String)>) -> Vec<EnvVar> {
    let mut out: Vec<EnvVar> = vars
        .into_iter()
        .map(|(name, value)| {
            let (value, secret) = redact::env_value(&name, &value);
            EnvVar {
                name,
                value,
                secret,
            }
        })
        .collect();
    out.sort_by_key(|v| v.name.to_lowercase());
    out
}

fn short_hash(s: &str) -> String {
    project::project_id(Path::new(s))[..8].to_string()
}

fn script_id(rel: &str, name: &str, runner: &Runner) -> String {
    let kind = match runner {
        Runner::Package { .. } => "pkg",
        Runner::DenoTask(_) => "deno",
        Runner::CargoRun => "cargo",
        Runner::ComposeUp => "compose",
        Runner::Django => "django",
    };
    format!("{kind}:{rel}:{name}")
}

fn parse_addr(address: &str) -> IpAddr {
    address
        .trim_matches(['[', ']'])
        .parse()
        .unwrap_or(IpAddr::from([127, 0, 0, 1]))
}

fn collect_ports(members: &[u32], by_pid: &BTreeMap<u32, Vec<Listener>>) -> Vec<(u16, IpAddr)> {
    let mut ports = Vec::new();
    for pid in members {
        if let Some(ls) = by_pid.get(pid) {
            ports.extend(ls.iter().map(|l| (l.port, l.addr)));
        }
    }
    merge_ports(ports)
}

fn merge_ports(mut ports: Vec<(u16, IpAddr)>) -> Vec<(u16, IpAddr)> {
    let rank = |a: &IpAddr| match a {
        a if a.is_unspecified() => 0,
        IpAddr::V4(v) if v.is_loopback() => 2,
        IpAddr::V6(v) if v.is_loopback() => 3,
        _ => 1,
    };
    ports.sort_by_key(|(port, addr)| (*port, rank(addr)));
    ports.dedup_by_key(|(port, _)| *port);
    ports
}

fn port_numbers(ports: &[(u16, IpAddr)]) -> Vec<u16> {
    ports.iter().map(|(p, _)| *p).collect()
}

fn port_entry(port: u16, addr: IpAddr, protocol: Protocol) -> Port {
    let address = match addr {
        IpAddr::V6(v6) => format!("[{v6}]"),
        IpAddr::V4(v4) => v4.to_string(),
    };
    Port {
        port,
        address,
        loopback: addr.is_loopback(),
        protocol,
        health: Health::default(),
        url: None,
        shared: false,
    }
}

fn url_for(p: &Port) -> Option<String> {
    if !p.protocol.is_web() {
        return None;
    }
    let scheme = if p.protocol == Protocol::Https {
        "https"
    } else {
        "http"
    };
    let addr = parse_addr(&p.address);
    let host = if addr.is_unspecified() || addr.is_loopback() {
        "localhost".to_string()
    } else {
        p.address.clone()
    };
    Some(format!("{scheme}://{host}:{}", p.port))
}

fn locate(projects: &mut ProjectIndex, proc: &Proc) -> Option<Location> {
    project::candidate_dirs(proc.cwd.as_deref(), &proc.cmd, proc.exe.as_deref())
        .into_iter()
        .find_map(|d| projects.locate(&d))
}

const STOP_AT: &[&str] = &[
    "explorer",
    "windowsterminal",
    "openconsole",
    "conhost",
    "code",
    "code helper",
    "cursor",
    "zed",
    "idea64",
    "idea",
    "webstorm64",
    "webstorm",
    "pycharm64",
    "pycharm",
    "rustrover64",
    "rustrover",
    "goland64",
    "goland",
    "sublime_text",
    "launchd",
    "systemd",
    "init",
    "sshd",
    "login",
    "sudo",
    "su",
    "doas",
    "tmux",
    "tmux: server",
    "screen",
    "zellij",
    "terminal",
    "iterm2",
    "alacritty",
    "wezterm-gui",
    "kitty",
    "ghostty",
    "warp",
    "hyper",
    "gnome-terminal-server",
    "konsole",
    "services",
    "svchost",
    "wininit",
    "tini",
];

const SHELLS: &[&str] = &[
    "bash",
    "zsh",
    "fish",
    "sh",
    "dash",
    "ksh",
    "tcsh",
    "csh",
    "nu",
    "cmd",
    "powershell",
    "pwsh",
    "elvish",
    "xonsh",
];

fn is_script_shell(p: &Proc) -> bool {
    p.cmd.iter().skip(1).any(|a| {
        let a = a.to_lowercase();
        a == "/c"
            || a == "-command"
            || (a.starts_with('-') && !a.starts_with("--") && a.len() <= 4 && a.contains('c'))
    })
}

fn find_root(table: &ProcessTable, projects: &mut ProjectIndex, pid: u32) -> u32 {
    let Some(start) = table.get(pid) else {
        return pid;
    };
    if !start.owned {
        return pid;
    }
    let package = |projects: &mut ProjectIndex, p: &Proc| locate(projects, p).map(|l| l.package);
    let own = package(projects, start);
    let mut cur = pid;
    for _ in 0..8 {
        let Some(parent) = table
            .get(cur)
            .and_then(|p| p.parent)
            .and_then(|pp| table.get(pp))
        else {
            break;
        };
        let name = parent.stem();
        if !parent.owned
            || table.is_protected(parent.pid)
            || STOP_AT.contains(&name.as_str())
            || (SHELLS.contains(&name.as_str()) && !is_script_shell(parent))
            || package(projects, parent) != own
        {
            break;
        }
        cur = parent.pid;
    }
    cur
}

fn service_title(
    identity: &Identity,
    loc: Option<&Location>,
    projects: &mut ProjectIndex,
    proc: &Proc,
) -> String {
    if let Some(loc) = loc {
        let rel = project::relative(&loc.root, &loc.package);
        if !rel.is_empty() {
            return rel;
        }
        if let Some(name) = &identity.name {
            return name.clone();
        }
        if let Some(name) = projects.package(&loc.package, &loc.root).name.clone() {
            return name;
        }
    }
    if let Some(name) = &identity.name {
        return name.clone();
    }
    let name = proc.name.strip_suffix(".exe").unwrap_or(&proc.name);
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => proc.name.clone(),
    }
}

fn user_command(proc: &Proc) -> &[String] {
    if SHELLS.contains(&proc.stem().as_str())
        && let Some(i) = proc.cmd.iter().position(|a| {
            let a = a.to_lowercase();
            a == "/c" || a == "-c" || a == "-command"
        })
        && i + 1 < proc.cmd.len()
    {
        return &proc.cmd[i + 1..];
    }
    &proc.cmd
}

async fn user_path() -> Option<OsString> {
    #[cfg(unix)]
    {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        let output = tokio::time::timeout(
            Duration::from_secs(4),
            tokio::process::Command::new(shell)
                .args(["-ilc", "printf '%s' \"$PATH\""])
                .stdin(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(true)
                .output(),
        )
        .await
        .ok()?
        .ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        let path = text.lines().last()?.trim();
        (!path.is_empty() && output.status.success()).then(|| OsString::from(path))
    }
    #[cfg(not(unix))]
    {
        None
    }
}
