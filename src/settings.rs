//! What the app is set to, for every repository, kept in the app's own folder.

use std::path::PathBuf;

use rune_kit::diff::DiffMode;
use serde::{Deserialize, Serialize};

use crate::kept;

const FILE_NAME: &str = "settings.json";
const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
#[serde(remote = "DiffMode", rename_all = "lowercase")]
enum LayoutName {
    Unified,
    Split,
}

#[derive(Serialize, Deserialize)]
struct StoreFile {
    version: u32,
    #[serde(with = "LayoutName")]
    layout: DiffMode,
}

/// The settings, written in the background as they change. The default is split, and keeps
/// nothing.
#[derive(Clone, Debug)]
pub struct Settings {
    /// Where they are kept, if the platform has a place for app state.
    file: Option<PathBuf>,
    /// How every diff lays out its two sides.
    layout: DiffMode,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { file: None, layout: DiffMode::Split }
    }
}

impl Settings {
    /// Reads the kept settings. A file that can't be read counts as none.
    pub fn load() -> Settings {
        let file = kept::path(FILE_NAME);
        let stored = file.as_deref().and_then(kept::read::<StoreFile>).filter(|stored| stored.version == VERSION);
        Settings { layout: stored.map_or(Settings::default().layout, |stored| stored.layout), file }
    }

    pub fn layout(&self) -> DiffMode {
        self.layout
    }

    pub fn set_layout(&mut self, layout: DiffMode) {
        self.layout = layout;
        if let Some(file) = self.file.clone() {
            kept::save(file, StoreFile { version: VERSION, layout });
        }
    }
}
