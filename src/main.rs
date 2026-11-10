//! A review-first Git desktop app.

mod action;
mod app;
mod comments;
mod forge;
mod gh;
mod git;
mod kept;
mod open;
mod overview;
mod recent;
mod review;
mod settings;
mod snippet;
mod switcher;
mod view;
mod watch;
mod windows;

use std::cell::RefCell;
use std::path::Path;
use std::process::ExitCode;
use std::rc::Rc;

use open::Opened;
use recent::Recents;
use settings::Settings;
use windows::Shared;

const NAME: &str = "hunk";
const APP_ID: &str = "io.github.volfpeter.hunk";

/// Starts on the folder passed, else the last repository if it is still one, else nothing. The
/// folder the app runs in doesn't count, so a launcher and a terminal start alike.
fn main() -> ExitCode {
    let recents = Recents::load();
    let opened = match rune::extra_args() {
        [] => recents.roots().first().and_then(|root| Opened::find(root).ok()).filter(|opened| matches!(opened, Opened::Repository(..))),
        [folder] => match Opened::find(Path::new(folder)) {
            Ok(opened) => Some(opened),
            Err(error) => {
                eprintln!("{NAME}: {error}");
                return ExitCode::from(2);
            }
        },
        _ => {
            eprintln!("usage: {NAME} [folder]");
            return ExitCode::from(2);
        }
    };
    rune::set_app_id(APP_ID);
    let shared = Rc::new(RefCell::new(Shared::new(recents, Settings::load())));
    windows::first(opened, &shared).run();
    ExitCode::SUCCESS
}
