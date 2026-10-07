use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use crate::model::Confidence;

const MAX_MANIFEST: u64 = 1024 * 1024;
const TTL: Duration = Duration::from_secs(5);

const MANIFESTS: &[&str] = &[
    "package.json",
    "deno.json",
    "deno.jsonc",
    "Cargo.toml",
    "go.mod",
    "pyproject.toml",
    "requirements.txt",
    "Pipfile",
    "manage.py",
    "Gemfile",
    "composer.json",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "mix.exs",
    "Package.swift",
];

const ROOT_MARKERS: &[&str] = &[
    "pnpm-workspace.yaml",
    "turbo.json",
    "nx.json",
    "lerna.json",
    "go.work",
    "docker-compose.yml",
    "docker-compose.yaml",
    "compose.yml",
    "compose.yaml",
];

const COMPOSE_FILES: &[&str] = &[
    "compose.yaml",
    "compose.yml",
    "docker-compose.yaml",
    "docker-compose.yml",
];

const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
    "vendor",
    "venv",
    "__pycache__",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub root: PathBuf,
    pub package: PathBuf,
    pub confidence: Confidence,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Runner {
    Package {
        manager: &'static str,
        script: String,
    },
    DenoTask(String),
    CargoRun,
    ComposeUp,
    Django,
}

impl Runner {
    pub fn program_and_args(&self) -> (&'static str, Vec<String>) {
        match self {
            Self::Package { manager, script } => (manager, vec!["run".into(), script.clone()]),
            Self::DenoTask(task) => ("deno", vec!["task".into(), task.clone()]),
            Self::CargoRun => ("cargo", vec!["run".into()]),
            Self::ComposeUp => ("docker", vec!["compose".into(), "up".into()]),
            Self::Django => (
                if cfg!(windows) { "python" } else { "python3" },
                vec!["manage.py".into(), "runserver".into()],
            ),
        }
    }

    pub fn display(&self) -> String {
        let (program, args) = self.program_and_args();
        std::iter::once(program.to_string())
            .chain(args)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptDef {
    pub name: String,
    pub runner: Runner,
    pub long_running: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Package {
    pub name: Option<String>,
    pub ecosystems: Vec<&'static str>,
    pub dependencies: Vec<String>,
    pub scripts: Vec<ScriptDef>,
    pub package_manager: Option<&'static str>,
}

struct Cached<T> {
    checked: Instant,
    stamp: Vec<Option<SystemTime>>,
    value: T,
}

#[derive(Default)]
pub struct ProjectIndex {
    locations: HashMap<PathBuf, (Instant, Option<Location>)>,
    packages: HashMap<PathBuf, Cached<Arc<Package>>>,
}

pub fn home() -> Option<PathBuf> {
    std::env::home_dir()
}

pub fn display_path(path: &Path) -> String {
    if let Some(home) = home()
        && let Ok(rest) = path.strip_prefix(&home)
    {
        let sep = std::path::MAIN_SEPARATOR;
        return if rest.as_os_str().is_empty() {
            "~".into()
        } else {
            format!("~{sep}{}", rest.display())
        };
    }
    path.display().to_string()
}

pub fn project_id(root: &Path) -> String {
    let text = root.to_string_lossy();
    let text = if cfg!(any(windows, target_os = "macos")) {
        text.to_lowercase()
    } else {
        text.into_owned()
    };
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{:012x}", hash & 0xffff_ffff_ffff)
}

fn is_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_file())
}

fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn read_small(path: &Path) -> Option<String> {
    let meta = fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_MANIFEST {
        return None;
    }
    fs::read_to_string(path).ok()
}

fn has_manifest(dir: &Path) -> bool {
    MANIFESTS.iter().any(|m| is_file(&dir.join(m)))
}

fn is_cargo_workspace(dir: &Path) -> bool {
    read_small(&dir.join("Cargo.toml"))
        .and_then(|t| t.parse::<toml::Table>().ok())
        .is_some_and(|t| t.contains_key("workspace"))
}

pub fn is_ignored(dir: &Path) -> bool {
    if dir.parent().is_none() {
        return true;
    }
    if let Some(home) = home() {
        if dir == home {
            return true;
        }
        if let Ok(rest) = dir.strip_prefix(&home) {
            let first = rest
                .components()
                .next()
                .and_then(|c| c.as_os_str().to_str())
                .unwrap_or("");
            return first.starts_with('.') || first == "Library" || first == "AppData";
        }
    }
    let text = dir.to_string_lossy().to_lowercase().replace('\\', "/");
    let system: &[&str] = if cfg!(windows) {
        &[
            "c:/windows",
            "c:/program files",
            "c:/programdata",
            "c:/$recycle.bin",
        ]
    } else {
        &[
            "/usr",
            "/bin",
            "/sbin",
            "/lib",
            "/etc",
            "/var",
            "/proc",
            "/sys",
            "/run",
            "/snap",
            "/system",
            "/library",
            "/applications",
            "/private",
            "/opt/homebrew",
            "/nix",
        ]
    };
    system.iter().any(|s| {
        text == *s || text.starts_with(&format!("{s}/")) || text.starts_with(&format!("{s} "))
    })
}

fn outside_node_modules(dir: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in dir.components() {
        if c.as_os_str() == "node_modules" {
            break;
        }
        out.push(c);
    }
    out
}

pub fn locate_uncached(start: &Path) -> Option<Location> {
    let start = outside_node_modules(start);
    if is_ignored(&start) {
        return None;
    }
    let mut package = None;
    let mut top = None;
    let mut root = None;
    for dir in start.ancestors().take(16) {
        if is_ignored(dir) {
            break;
        }
        let manifest = has_manifest(dir);
        if manifest && package.is_none() {
            package = Some(dir.to_path_buf());
        }
        if exists(&dir.join(".git")) {
            root = Some((dir.to_path_buf(), Confidence::High));
            break;
        }
        if ROOT_MARKERS.iter().any(|m| is_file(&dir.join(m))) || is_cargo_workspace(dir) {
            top = Some((dir.to_path_buf(), Confidence::High));
        } else if manifest && top.as_ref().is_none_or(|(_, c)| *c == Confidence::Medium) {
            top = Some((dir.to_path_buf(), Confidence::Medium));
        }
    }
    let (root, confidence) = root.or(top)?;
    let package = package
        .filter(|p| p.starts_with(&root))
        .unwrap_or_else(|| root.clone());
    Some(Location {
        root,
        package,
        confidence,
    })
}

impl ProjectIndex {
    pub fn locate(&mut self, start: &Path) -> Option<Location> {
        if let Some((at, loc)) = self.locations.get(start)
            && at.elapsed() < TTL * 6
        {
            return loc.clone();
        }
        let loc = locate_uncached(start);
        self.locations
            .insert(start.to_path_buf(), (Instant::now(), loc.clone()));
        loc
    }

    pub fn package(&mut self, dir: &Path, root: &Path) -> Arc<Package> {
        if let Some(c) = self.packages.get_mut(dir) {
            if c.checked.elapsed() < TTL {
                return c.value.clone();
            }
            let stamp = stamp(dir, root);
            if stamp == c.stamp {
                c.checked = Instant::now();
                return c.value.clone();
            }
        }
        let value = Arc::new(read_package(dir, root));
        self.packages.insert(
            dir.to_path_buf(),
            Cached {
                checked: Instant::now(),
                stamp: stamp(dir, root),
                value: value.clone(),
            },
        );
        value
    }

    pub fn packages_in(&mut self, root: &Path) -> Vec<(PathBuf, Arc<Package>)> {
        let mut dirs = vec![root.to_path_buf()];
        for child in subdirs(root) {
            if has_manifest(&child) {
                dirs.push(child.clone());
            }
            let name = child.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if matches!(name, "apps" | "packages" | "services" | "examples") {
                dirs.extend(subdirs(&child).into_iter().filter(|d| has_manifest(d)));
            }
        }
        dirs.truncate(48);
        dirs.into_iter()
            .map(|d| {
                let pkg = self.package(&d, root);
                (d, pkg)
            })
            .filter(|(_, p)| !p.ecosystems.is_empty())
            .collect()
    }
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .take(400)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            !name.starts_with('.') && !SKIP_DIRS.contains(&name.as_ref())
        })
        .map(|e| e.path())
        .collect();
    out.sort();
    out
}

fn stamp(dir: &Path, root: &Path) -> Vec<Option<SystemTime>> {
    MANIFESTS
        .iter()
        .chain(COMPOSE_FILES)
        .map(|m| dir.join(m))
        .chain(LOCKFILES.iter().map(|(f, _)| root.join(f)))
        .map(|p| fs::symlink_metadata(p).and_then(|m| m.modified()).ok())
        .collect()
}

const LOCKFILES: &[(&str, &str)] = &[
    ("pnpm-lock.yaml", "pnpm"),
    ("bun.lock", "bun"),
    ("bun.lockb", "bun"),
    ("yarn.lock", "yarn"),
    ("package-lock.json", "npm"),
];

fn package_manager(dir: &Path, root: &Path, declared: Option<&str>) -> &'static str {
    if let Some(declared) = declared {
        for name in ["pnpm", "yarn", "bun", "npm"] {
            if declared.starts_with(name) {
                return name;
            }
        }
    }
    for d in dir.ancestors() {
        for (file, manager) in LOCKFILES {
            if is_file(&d.join(file)) {
                return manager;
            }
        }
        if d == root {
            break;
        }
    }
    "npm"
}

fn long_running(name: &str) -> bool {
    [
        "dev",
        "start",
        "serve",
        "watch",
        "preview",
        "storybook",
        "server",
        "up",
    ]
    .iter()
    .any(|k| name.split([':', '-', '_']).any(|part| part == *k))
}

fn read_package(dir: &Path, root: &Path) -> Package {
    let mut pkg = Package::default();
    if let Some(json) = read_small(&dir.join("package.json"))
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
    {
        pkg.ecosystems.push("node");
        pkg.name = json["name"].as_str().map(str::to_string);
        for key in ["dependencies", "devDependencies", "peerDependencies"] {
            if let Some(deps) = json[key].as_object() {
                pkg.dependencies.extend(deps.keys().cloned());
            }
        }
        let manager = package_manager(dir, root, json["packageManager"].as_str());
        pkg.package_manager = Some(manager);
        if let Some(scripts) = json["scripts"].as_object() {
            for name in scripts.keys().take(64) {
                pkg.scripts.push(ScriptDef {
                    name: name.clone(),
                    runner: Runner::Package {
                        manager,
                        script: name.clone(),
                    },
                    long_running: long_running(name),
                });
            }
        }
    }
    for file in ["deno.json", "deno.jsonc"] {
        if let Some(json) = read_small(&dir.join(file))
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        {
            pkg.ecosystems.push("deno");
            if let Some(tasks) = json["tasks"].as_object() {
                for name in tasks.keys().take(64) {
                    pkg.scripts.push(ScriptDef {
                        name: name.clone(),
                        runner: Runner::DenoTask(name.clone()),
                        long_running: long_running(name),
                    });
                }
            }
            break;
        }
    }
    if let Some(cargo) =
        read_small(&dir.join("Cargo.toml")).and_then(|t| t.parse::<toml::Table>().ok())
    {
        pkg.ecosystems.push("rust");
        if pkg.name.is_none() {
            pkg.name = cargo
                .get("package")
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
                .map(str::to_string);
        }
        for table in [
            cargo.get("dependencies"),
            cargo.get("workspace").and_then(|w| w.get("dependencies")),
        ]
        .into_iter()
        .flatten()
        {
            if let Some(t) = table.as_table() {
                pkg.dependencies.extend(t.keys().cloned());
            }
        }
        if cargo.contains_key("package")
            && (is_file(&dir.join("src/main.rs")) || cargo.contains_key("bin"))
        {
            pkg.scripts.push(ScriptDef {
                name: "run".into(),
                runner: Runner::CargoRun,
                long_running: true,
            });
        }
    }
    let mut python = false;
    if let Some(py) =
        read_small(&dir.join("pyproject.toml")).and_then(|t| t.parse::<toml::Table>().ok())
    {
        python = true;
        let project = py.get("project");
        if pkg.name.is_none() {
            pkg.name = project
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
                .map(str::to_string);
        }
        if let Some(deps) = project
            .and_then(|p| p.get("dependencies"))
            .and_then(|d| d.as_array())
        {
            pkg.dependencies
                .extend(deps.iter().filter_map(|d| d.as_str()).map(requirement_name));
        }
        if let Some(deps) = py
            .get("tool")
            .and_then(|t| t.get("poetry"))
            .and_then(|p| p.get("dependencies"))
            .and_then(|d| d.as_table())
        {
            pkg.dependencies
                .extend(deps.keys().map(|k| k.to_lowercase()));
        }
    }
    if let Some(reqs) = read_small(&dir.join("requirements.txt")) {
        python = true;
        pkg.dependencies.extend(
            reqs.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with(['#', '-']))
                .map(requirement_name),
        );
    }
    if python || is_file(&dir.join("Pipfile")) || is_file(&dir.join("manage.py")) {
        pkg.ecosystems.push("python");
    }
    if is_file(&dir.join("manage.py")) {
        pkg.scripts.push(ScriptDef {
            name: "runserver".into(),
            runner: Runner::Django,
            long_running: true,
        });
    }
    if let Some(gomod) = read_small(&dir.join("go.mod")) {
        pkg.ecosystems.push("go");
        for line in gomod.lines() {
            let line = line.trim();
            if let Some(module) = line.strip_prefix("module ") {
                pkg.name.get_or_insert_with(|| module.trim().to_string());
                continue;
            }
            let line = line.strip_prefix("require ").unwrap_or(line);
            if let Some((path, version)) = line.split_once(' ')
                && version.trim_start().starts_with('v')
                && path.contains('/')
            {
                pkg.dependencies.push(path.to_string());
            }
        }
    }
    if let Some(gemfile) = read_small(&dir.join("Gemfile")) {
        pkg.ecosystems.push("ruby");
        pkg.dependencies.extend(gemfile.lines().filter_map(|l| {
            let rest = l.trim().strip_prefix("gem ")?;
            let name = rest.split(',').next()?.trim().trim_matches(['"', '\'']);
            Some(name.to_string())
        }));
    }
    if let Some(json) = read_small(&dir.join("composer.json"))
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
    {
        pkg.ecosystems.push("php");
        if let Some(req) = json["require"].as_object() {
            pkg.dependencies.extend(req.keys().cloned());
        }
    }
    if ["pom.xml", "build.gradle", "build.gradle.kts"]
        .iter()
        .any(|f| is_file(&dir.join(f)))
    {
        pkg.ecosystems.push("java");
    }
    if is_file(&dir.join("mix.exs")) {
        pkg.ecosystems.push("elixir");
    }
    if is_file(&dir.join("Package.swift")) {
        pkg.ecosystems.push("swift");
    }
    if COMPOSE_FILES.iter().any(|f| is_file(&dir.join(f))) {
        pkg.ecosystems.push("docker");
        pkg.scripts.push(ScriptDef {
            name: "compose up".into(),
            runner: Runner::ComposeUp,
            long_running: true,
        });
    } else if is_file(&dir.join("Dockerfile")) {
        pkg.ecosystems.push("docker");
    }
    pkg.dependencies.sort();
    pkg.dependencies.dedup();
    pkg.scripts
        .sort_by_key(|s| (!s.long_running, script_rank(&s.name), s.name.clone()));
    pkg
}

fn script_rank(name: &str) -> usize {
    [
        "dev",
        "start",
        "serve",
        "preview",
        "run",
        "runserver",
        "compose up",
    ]
    .iter()
    .position(|n| *n == name)
    .unwrap_or(99)
}

fn requirement_name(spec: &str) -> String {
    spec.split(['<', '>', '=', '!', '~', '[', ';', ' '])
        .next()
        .unwrap_or(spec)
        .trim()
        .to_lowercase()
}

pub fn relative(root: &Path, dir: &Path) -> String {
    dir.strip_prefix(root)
        .map(|p| {
            p.components()
                .filter_map(|c| match c {
                    Component::Normal(s) => s.to_str(),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("/")
        })
        .unwrap_or_default()
}

pub fn candidate_dirs(cwd: Option<&Path>, cmd: &[String], exe: Option<&Path>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(cwd) = cwd {
        out.push(cwd.to_path_buf());
    }
    for arg in cmd.iter().skip(1).take(8) {
        let path = Path::new(arg);
        if path.is_absolute()
            && !arg.contains("node_modules")
            && path.extension().is_some()
            && let Some(parent) = path.parent()
        {
            out.push(parent.to_path_buf());
        }
    }
    if let Some(exe) = exe {
        let mut dir = PathBuf::new();
        for c in exe.components() {
            if c.as_os_str() == "target" {
                out.push(dir.clone());
                break;
            }
            dir.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap()
            .join("target/test-projects")
            .join(format!("{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn finds_monorepo_root_and_package() {
        let root = temp("mono");
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::create_dir_all(root.join("frontend/src")).unwrap();
        fs::write(
            root.join("frontend/package.json"),
            r#"{"name":"web","scripts":{"dev":"next dev","lint":"eslint"},"dependencies":{"next":"15"}}"#,
        )
        .unwrap();
        fs::write(root.join("frontend/pnpm-lock.yaml"), "").unwrap();
        let loc = locate_uncached(&root.join("frontend/src")).unwrap();
        assert_eq!(loc.root, root);
        assert_eq!(loc.package, root.join("frontend"));
        assert_eq!(loc.confidence, Confidence::High);
        assert_eq!(relative(&loc.root, &loc.package), "frontend");

        let mut index = ProjectIndex::default();
        let pkg = index.package(&loc.package, &loc.root);
        assert_eq!(pkg.package_manager, Some("pnpm"));
        assert_eq!(pkg.scripts[0].name, "dev");
        assert!(pkg.scripts[0].long_running);
        assert!(!pkg.scripts[1].long_running);
        assert_eq!(pkg.scripts[0].runner.display(), "pnpm run dev");
        assert!(pkg.dependencies.contains(&"next".to_string()));

        let all = index.packages_in(&root);
        assert_eq!(all.len(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn reads_python_dependencies() {
        let root = temp("py");
        fs::write(
            root.join("pyproject.toml"),
            "[project]\nname = \"api\"\ndependencies = [\"fastapi>=0.110\", \"uvicorn[standard]\"]\n",
        )
        .unwrap();
        let pkg = read_package(&root, &root);
        assert_eq!(pkg.name.as_deref(), Some("api"));
        assert!(pkg.dependencies.contains(&"fastapi".to_string()));
        assert!(pkg.dependencies.contains(&"uvicorn".to_string()));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ids_are_stable() {
        assert_eq!(project_id(Path::new("/a/b")), project_id(Path::new("/a/b")));
        assert_ne!(project_id(Path::new("/a/b")), project_id(Path::new("/a/c")));
    }
}
