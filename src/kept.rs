//! Files the app keeps for itself, outside any repository: in its state folder, or its data
//! folder where there is no such thing.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::thread;

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Where the app keeps the file `name`, if the platform has a place for it.
pub fn path(name: &str) -> Option<PathBuf> {
    dirs::state_dir().or_else(dirs::data_dir).map(|dir| dir.join(crate::NAME).join(name))
}

/// What `file` holds, if it reads as a `T`.
pub fn read<T: DeserializeOwned>(file: &Path) -> Option<T> {
    fs::read_to_string(file).ok().and_then(|text| serde_json::from_str(&text).ok())
}

/// Writes `value` to `file` in the background. Losing it costs little, so a failure only says so.
pub fn save<T: Serialize + Send + 'static>(file: PathBuf, value: T) {
    thread::spawn(move || {
        if let Err(error) = write(&file, &value) {
            eprintln!("{}: {}: {error}", crate::NAME, file.display());
        }
    });
}

/// Replaces `file` in one step, so a crash leaves the old contents or the new, never half.
pub fn write<T: Serialize>(file: &Path, value: &T) -> io::Result<()> {
    let text = serde_json::to_string_pretty(value)?;
    let partial = file.with_extension("json.tmp");
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&partial, text)?;
    fs::rename(&partial, file)
}
