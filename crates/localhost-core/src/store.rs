use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::model::Settings;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Data {
    pub projects: Vec<PathBuf>,
    pub trusted: Vec<PathBuf>,
    pub settings: Settings,
}

pub struct Store {
    path: Option<PathBuf>,
    pub data: Data,
}

impl Store {
    pub fn memory() -> Self {
        Self {
            path: None,
            data: Data::default(),
        }
    }

    pub fn load(dir: &Path) -> Self {
        let path = dir.join("localhost.json");
        let data = fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Self {
            path: Some(path),
            data,
        }
    }

    pub fn save(&self) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(
            &tmp,
            serde_json::to_vec_pretty(&self.data).map_err(io::Error::other)?,
        )?;
        fs::rename(tmp, path)
    }

    pub fn is_trusted(&self, root: &Path) -> bool {
        self.data.trusted.iter().any(|t| t == root)
    }

    pub fn is_saved(&self, root: &Path) -> bool {
        self.data.projects.iter().any(|p| p == root)
    }

    pub fn trust(&mut self, root: &Path) {
        if !self.is_trusted(root) {
            self.data.trusted.push(root.to_path_buf());
        }
    }

    pub fn add(&mut self, root: &Path) {
        if !self.is_saved(root) {
            self.data.projects.push(root.to_path_buf());
        }
    }

    pub fn remove(&mut self, root: &Path) {
        self.data.projects.retain(|p| p != root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let dir = std::env::temp_dir().join(format!("localhost-store-{}", std::process::id()));
        let mut store = Store::load(&dir);
        store.add(Path::new("/code/shop"));
        store.trust(Path::new("/code/shop"));
        store.data.settings.show_system = true;
        store.save().unwrap();
        let loaded = Store::load(&dir);
        assert_eq!(loaded.data, store.data);
        assert!(loaded.is_trusted(Path::new("/code/shop")));
        let _ = fs::remove_dir_all(dir);
    }
}
