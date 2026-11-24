//! What the app is set to, for every repository, kept in the app's own folder.

use std::path::PathBuf;

use rune_kit::diff::DiffMode;
use serde::{Deserialize, Serialize};

use crate::app::Kind;
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
    #[serde(default)]
    kind: Kind,
    #[serde(default = "shown")]
    file_tree: bool,
}

fn shown() -> bool {
    true
}

/// The settings, written in the background as they change. The default is split on staging,
/// and keeps nothing.
#[derive(Clone, Debug)]
pub struct Settings {
    /// Where they are kept, if the platform has a place for app state.
    file: Option<PathBuf>,
    /// How every diff lays out its two sides.
    layout: DiffMode,
    /// The kind of review on screen.
    kind: Kind,
    /// Whether the file overlay shows the worktree's files beside the file.
    file_tree: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { file: None, layout: DiffMode::Split, kind: Kind::default(), file_tree: true }
    }
}

impl Settings {
    /// Reads the kept settings. A file that can't be read counts as none.
    pub fn load() -> Settings {
        let file = kept::path(FILE_NAME);
        let stored = file.as_deref().and_then(kept::read::<StoreFile>).filter(|stored| stored.version == VERSION);
        let Some(stored) = stored else { return Settings { file, ..Settings::default() } };
        Settings { file, layout: stored.layout, kind: stored.kind, file_tree: stored.file_tree }
    }

    pub fn layout(&self) -> DiffMode {
        self.layout
    }

    pub fn set_layout(&mut self, layout: DiffMode) {
        self.layout = layout;
        self.save();
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    pub fn set_kind(&mut self, kind: Kind) {
        if kind != self.kind {
            self.kind = kind;
            self.save();
        }
    }

    pub fn file_tree(&self) -> bool {
        self.file_tree
    }

    pub fn set_file_tree(&mut self, shown: bool) {
        if shown != self.file_tree {
            self.file_tree = shown;
            self.save();
        }
    }

    fn save(&self) {
        if let Some(file) = self.file.clone() {
            kept::save(file, StoreFile { version: VERSION, layout: self.layout, kind: self.kind, file_tree: self.file_tree });
        }
    }
}
