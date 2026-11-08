//! The repositories opened lately, kept in the app's own state folder.

use std::fs;
use std::path::{Path, PathBuf};
use std::thread;

use serde::{Deserialize, Serialize};

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
        let file = dirs::state_dir().or_else(dirs::data_dir).map(|dir| dir.join(crate::NAME).join(FILE_NAME));
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
        let Some(file) = self.file.clone() else { return };
        let repositories = self.roots.clone();
        thread::spawn(move || {
            // Losing the list costs little, so a failure only says so.
            if let Err(error) = write_file(&file, repositories) {
                eprintln!("{}: {}: {error}", crate::NAME, file.display());
            }
        });
    }
}

fn read_file(file: &Path) -> Vec<PathBuf> {
    let stored = fs::read_to_string(file).ok().and_then(|text| serde_json::from_str::<StoreFile>(&text).ok());
    stored.filter(|stored| stored.version == VERSION).map(|stored| stored.repositories).unwrap_or_default()
}

/// Replaces `file` in one step, so a crash leaves the old list or the new, never half.
fn write_file(file: &Path, repositories: Vec<PathBuf>) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(&StoreFile { version: VERSION, repositories })?;
    let partial = file.with_extension("json.tmp");
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&partial, text)?;
    fs::rename(&partial, file)
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

        write_file(&file, vec![PathBuf::from("/a")]).unwrap();
        assert_eq!(read_file(&file), [PathBuf::from("/a")]);
    }
}
