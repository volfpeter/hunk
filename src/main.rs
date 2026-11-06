//! A review-first Git desktop app.

mod app;
mod comments;
mod git;
mod overview;
mod review;
mod view;

use std::process::ExitCode;

use rune::{App, AppSpec};

use app::State;
use comments::Comments;
use git::Repo;

const NAME: &str = "diffactive";
const WINDOW_SIZE: (f32, f32) = (1500.0, 950.0);

fn main() -> ExitCode {
    let repo = match std::env::current_dir().map_err(|e| e.to_string()).and_then(|dir| Repo::discover(&dir).map_err(|e| e.to_string())) {
        Ok(repo) => repo,
        Err(e) => {
            eprintln!("{NAME}: {e}");
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
    let comments = Comments::load(repo.clone());
    App::new(spec).init(move || State::new(repo, comments)).modifier_keys().run();
    ExitCode::SUCCESS
}
