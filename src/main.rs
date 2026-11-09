//! A review-first Git desktop app.

mod action;
mod app;
mod comments;
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

use std::path::Path;
use std::process::ExitCode;

use rune::{App, AppSpec};

use app::{Shell, State};
use open::Opened;
use recent::Recents;
use settings::Settings;

const NAME: &str = "hunk";
const WINDOW_SIZE: (f32, f32) = (1500.0, 950.0);

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
    let spec = AppSpec {
        setup: Some(State::setup),
        poll: Some(State::poll),
        on_key: Some(State::on_key),
        families: ("Inter", "ui-monospace, Inter"),
        ..AppSpec::new(NAME, WINDOW_SIZE, || unreachable!("App::init builds the state"), view::view)
    };
    let shell = Shell::new(recents, Settings::load());
    App::new(spec).init(move || State::new(opened, shell)).modifier_keys().run();
    ExitCode::SUCCESS
}
