pub mod control;
pub mod sockets;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, Uid, UpdateKind};

#[derive(Clone, Debug)]
pub struct Proc {
    pub pid: u32,
    pub parent: Option<u32>,
    pub name: String,
    pub exe: Option<PathBuf>,
    pub cmd: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub cpu: f32,
    pub memory: u64,
    pub start_time: u64,
    pub owned: bool,
}

impl Proc {
    pub fn stem(&self) -> String {
        stem(self.exe.as_deref().map_or(Path::new(&self.name), |p| p))
    }

    pub fn command_line(&self) -> String {
        if self.cmd.is_empty() {
            self.name.clone()
        } else {
            self.cmd.join(" ")
        }
    }
}

pub fn stem(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match name.strip_suffix(".exe") {
        Some(s) => s.to_string(),
        None => name,
    }
}

pub struct ProcessTable {
    system: System,
    procs: HashMap<u32, Proc>,
    children: HashMap<u32, Vec<u32>>,
    self_pid: u32,
    user: Option<Uid>,
}

impl Default for ProcessTable {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessTable {
    pub fn new() -> Self {
        Self {
            system: System::new(),
            procs: HashMap::new(),
            children: HashMap::new(),
            self_pid: std::process::id(),
            user: None,
        }
    }

    pub fn refresh(&mut self) {
        let kind = ProcessRefreshKind::nothing()
            .with_cpu()
            .with_memory()
            .with_exe(UpdateKind::OnlyIfNotSet)
            .with_cmd(UpdateKind::OnlyIfNotSet)
            .with_cwd(UpdateKind::OnlyIfNotSet)
            .with_user(UpdateKind::OnlyIfNotSet)
            .without_tasks();
        self.system
            .refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
        if self.user.is_none() {
            self.user = self
                .system
                .process(Pid::from_u32(self.self_pid))
                .and_then(|p| p.user_id().cloned());
        }
        self.procs.clear();
        self.children.clear();
        for (pid, p) in self.system.processes() {
            let pid = pid.as_u32();
            let parent = p.parent().map(Pid::as_u32).filter(|&pp| pp != pid);
            if let Some(parent) = parent {
                self.children.entry(parent).or_default().push(pid);
            }
            let owned = matches!((&self.user, p.user_id()), (Some(me), Some(u)) if me == u);
            self.procs.insert(
                pid,
                Proc {
                    pid,
                    parent,
                    name: p.name().to_string_lossy().into_owned(),
                    exe: p.exe().map(Path::to_path_buf),
                    cmd: p
                        .cmd()
                        .iter()
                        .map(|a| a.to_string_lossy().into_owned())
                        .collect(),
                    cwd: p.cwd().map(Path::to_path_buf),
                    cpu: p.cpu_usage(),
                    memory: p.memory(),
                    start_time: p.start_time(),
                    owned,
                },
            );
        }
        for kids in self.children.values_mut() {
            kids.sort_unstable();
        }
    }

    pub fn get(&self, pid: u32) -> Option<&Proc> {
        self.procs.get(&pid)
    }

    pub fn self_pid(&self) -> u32 {
        self.self_pid
    }

    pub fn iter(&self) -> impl Iterator<Item = &Proc> {
        self.procs.values()
    }

    pub fn children(&self, pid: u32) -> &[u32] {
        self.children.get(&pid).map_or(&[], Vec::as_slice)
    }

    pub fn descendants(&self, pid: u32) -> Vec<(u32, usize)> {
        let mut out = vec![(pid, 0)];
        let mut i = 0;
        while i < out.len() {
            let (p, depth) = out[i];
            if depth < 32 {
                for &c in self.children(p).iter().rev() {
                    if !out.iter().any(|&(x, _)| x == c) {
                        out.insert(i + 1, (c, depth + 1));
                    }
                }
            }
            i += 1;
        }
        out
    }

    pub fn ancestors(&self, pid: u32) -> Vec<u32> {
        let mut out = Vec::new();
        let mut cur = self.get(pid).and_then(|p| p.parent);
        while let Some(p) = cur {
            if out.contains(&p) || out.len() > 64 {
                break;
            }
            out.push(p);
            cur = self.get(p).and_then(|x| x.parent);
        }
        out
    }

    pub fn is_protected(&self, pid: u32) -> bool {
        pid <= 4 || pid == self.self_pid || self.ancestors(self.self_pid).contains(&pid)
    }

    pub fn environment(&mut self, pid: u32) -> Option<Vec<(String, String)>> {
        let spid = Pid::from_u32(pid);
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[spid]),
            false,
            ProcessRefreshKind::nothing().with_environ(UpdateKind::Always),
        );
        let env = self.system.process(spid)?.environ();
        if env.is_empty() {
            return None;
        }
        Some(
            env.iter()
                .filter_map(|e| {
                    let e = e.to_string_lossy();
                    let (k, v) = e.split_once('=')?;
                    (!k.is_empty()).then(|| (k.to_string(), v.to_string()))
                })
                .collect(),
        )
    }
}
