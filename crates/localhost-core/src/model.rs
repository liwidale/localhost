use serde::{Deserialize, Serialize};

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub projects: Vec<Project>,
    pub services: Vec<Service>,
    pub docker: DockerState,
    pub platform: &'static str,
}

#[derive(Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DockerState {
    #[default]
    Unavailable,
    Running,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
    pub ecosystems: Vec<&'static str>,
    pub package_manager: Option<&'static str>,
    pub scripts: Vec<Script>,
    pub saved: bool,
    pub trusted: bool,
    pub confidence: Confidence,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Script {
    pub id: String,
    pub name: String,
    pub package: String,
    pub command: String,
    pub long_running: bool,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    Process,
    Managed,
    Container,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    pub id: String,
    pub source: Source,
    pub project_id: Option<String>,
    pub package: Option<String>,
    pub title: String,
    pub identity: Identity,
    pub status: Status,
    pub ports: Vec<Port>,
    pub pid: Option<u32>,
    pub process_count: usize,
    pub cpu: f32,
    pub memory: u64,
    pub started_at: Option<u64>,
    pub command: Option<String>,
    pub cwd: Option<String>,
    pub image: Option<String>,
    pub system: bool,
    pub controllable: bool,
    pub has_logs: bool,
    pub exit_code: Option<i32>,
    pub blocked_port: Option<u16>,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Running,
    Starting,
    Stopping,
    Stopped,
    Exited,
    Crashed,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub tool: Option<&'static str>,
    pub name: Option<String>,
    pub kind: Kind,
    pub runtime: Option<&'static str>,
    pub confidence: f32,
    pub evidence: Vec<Evidence>,
}

#[derive(Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    DevServer,
    WebServer,
    Database,
    Cache,
    Queue,
    Search,
    Mail,
    Storage,
    Debugger,
    Task,
    Proxy,
    Notebook,
    Emulator,
    PortForward,
    #[default]
    Process,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Evidence {
    pub kind: EvidenceKind,
    pub detail: String,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EvidenceKind {
    Command,
    Executable,
    Port,
    Dependency,
    Container,
    Managed,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Port {
    pub port: u16,
    pub address: String,
    pub loopback: bool,
    pub protocol: Protocol,
    pub health: Health,
    pub url: Option<String>,
    pub shared: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum Protocol {
    Http,
    Https,
    Postgres,
    MySql,
    Redis,
    Mongo,
    Amqp,
    Smtp,
    #[default]
    Tcp,
}

impl Protocol {
    pub fn is_web(self) -> bool {
        matches!(self, Self::Http | Self::Https)
    }
}

#[derive(Serialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    pub state: HealthState,
    pub http_status: Option<u16>,
    pub latency_ms: Option<u32>,
    pub checked_at: Option<u64>,
}

#[derive(Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum HealthState {
    #[default]
    Listening,
    Responding,
    Error,
    Unreachable,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceDetail {
    pub id: String,
    pub tree: Vec<ProcessNode>,
    pub ancestry: Vec<ProcessNode>,
    pub environment: Option<Vec<EnvVar>>,
    pub executable: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProcessNode {
    pub pid: u32,
    pub depth: usize,
    pub name: String,
    pub command: String,
    pub cpu: f32,
    pub memory: u64,
    pub ports: Vec<u16>,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EnvVar {
    pub name: String,
    pub value: String,
    pub secret: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub show_system: bool,
    pub health_checks: bool,
    pub notifications: bool,
    pub theme: Theme,
    pub motion: Motion,
    pub language: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Motion {
    #[default]
    Full,
    Reduced,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            show_system: false,
            health_checks: true,
            notifications: true,
            theme: Theme::System,
            motion: Motion::Full,
            language: "auto".into(),
        }
    }
}
