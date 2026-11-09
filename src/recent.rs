//! The repositories opened lately, kept in the app's own state folder.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::kept;

const FILE_NAME: &str = "recent.json";
const VERSION: u32 = 1;
/// How many are remembered.
const KEPT: usize = 20;

#[derive(Serialize, Deserialize)]
struct StoreFile {
    version: u32,
    repositories: Vec<PathBuf>,
}

/// Repository roots, newest first. Writes happen in the background. The default remembers
/// nothing and keeps nothing.
#[derive(Clone, Debug, Default)]
pub struct Recents {
    /// Where they are kept, if the platform has a place for app state.
    file: Option<PathBuf>,
    roots: Vec<PathBuf>,
}

impl Recents {
    /// Reads the ones kept in the app's state folder, or its data folder where there is no such
    /// thing. A file that can't be read counts as none.
    pub fn load() -> Recents {
        let file = kept::path(FILE_NAME);
        Recents { roots: file.as_deref().map(read_file).unwrap_or_default(), file }
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// Puts `root` first.
    pub fn remember(&mut self, root: &Path) {
        self.roots.retain(|kept| kept != root);
        self.roots.insert(0, root.to_owned());
        self.roots.truncate(KEPT);
        self.save();
    }

    /// Lets go of `gone`.
    pub fn forget(&mut self, gone: &[PathBuf]) {
        let before = self.roots.len();
        self.roots.retain(|root| !gone.contains(root));
        if self.roots.len() != before {
            self.save();
        }
    }

    fn save(&self) {
        if let Some(file) = self.file.clone() {
            kept::save(file, StoreFile { version: VERSION, repositories: self.roots.clone() });
        }
    }
}

fn read_file(file: &Path) -> Vec<PathBuf> {
    let stored = kept::read::<StoreFile>(file);
    stored.filter(|stored| stored.version == VERSION).map(|stored| stored.repositories).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn remembers_the_newest_first_once_each_and_forgets_on_request() {
        let mut recents = Recents::default();
        for root in ["/a", "/b", "/a"] {
            recents.remember(Path::new(root));
        }
        assert_eq!(recents.roots(), [PathBuf::from("/a"), PathBuf::from("/b")]);

        recents.forget(&[PathBuf::from("/a")]);
        assert_eq!(recents.roots(), [PathBuf::from("/b")]);
    }

    #[test]
    fn keeps_them_across_restarts() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join(crate::NAME).join(FILE_NAME);
        assert!(read_file(&file).is_empty());

        kept::write(&file, &StoreFile { version: VERSION, repositories: vec![PathBuf::from("/a")] }).unwrap();
        assert_eq!(read_file(&file), [PathBuf::from("/a")]);
    }
}
