//! The app's windows, one repository each, and what they share.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use rune::{App, AppHandle, AppSpec, WindowSpec};

use crate::app::{Shell, State};
use crate::open::Opened;
use crate::recent::Recents;
use crate::settings::Settings;
use crate::view;

const WINDOW_SIZE: (f32, f32) = (1500.0, 950.0);

/// What every window shares: the recent repositories, the settings, and the open windows.
#[derive(Default)]
pub struct Shared {
    pub recents: Recents,
    pub settings: Settings,
    windows: Vec<Window>,
    /// The id the next window takes.
    next: u64,
}

/// An open window and the repository it shows.
struct Window {
    id: u64,
    handle: AppHandle<State>,
    root: Option<PathBuf>,
}

impl Shared {
    pub fn new(recents: Recents, settings: Settings) -> Shared {
        Shared { recents, settings, ..Shared::default() }
    }

    /// Notes that window `id` shows the repository at `root`, or none.
    pub fn set_root(&mut self, id: u64, root: Option<&Path>) {
        if let Some(window) = self.windows.iter_mut().find(|window| window.id == id) {
            window.root = root.map(Path::to_owned);
        }
    }

    /// Focuses the window showing the repository at `root`; returns whether there is one.
    pub fn focus(&self, root: &Path) -> bool {
        let window = self.windows.iter().find(|window| window.root.as_deref() == Some(root));
        window.inspect(|window| window.handle.focus()).is_some()
    }

    /// Lets go of window `id`, which closed.
    pub fn forget(&mut self, id: u64) {
        self.windows.retain(|window| window.id != id);
    }
}

/// The first window, on what `opened` found, if anything.
pub fn first(opened: Option<Opened>, shared: &Rc<RefCell<Shared>>) -> App<State> {
    build(opened, shared).1
}

/// Opens another window on what `opened` found.
pub fn open(opened: Opened, shared: &Rc<RefCell<Shared>>) {
    let (id, app) = build(Some(opened), shared);
    rune::open_window(WindowSpec::from_app(format!("window{id}"), app));
}

fn build(opened: Option<Opened>, shared: &Rc<RefCell<Shared>>) -> (u64, App<State>) {
    let spec = AppSpec {
        setup: Some(State::setup),
        poll: Some(State::poll),
        on_key: Some(State::on_key),
        families: ("Inter", "ui-monospace, Inter"),
        ..AppSpec::new(crate::NAME, WINDOW_SIZE, || unreachable!("App::init builds the state"), view::view)
    };
    let root = opened.as_ref().and_then(Opened::root).map(Path::to_owned);
    let id = shared.borrow().next;
    let shell = Shell::new(shared.clone(), id);
    let app = App::new(spec).init(move || State::new(opened, shell)).on_focus(State::on_focus).on_close(State::on_close).modifier_keys();
    let mut shared = shared.borrow_mut();
    shared.next += 1;
    shared.windows.push(Window { id, handle: app.handle(), root });
    (id, app)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_window_still_showing_a_repository_is_focused() {
        let shared = Rc::new(RefCell::new(Shared::default()));
        let (one, _) = build(None, &shared);
        let (two, _) = build(None, &shared);
        let mut shared = shared.borrow_mut();
        shared.set_root(one, Some(Path::new("/a")));
        shared.set_root(two, Some(Path::new("/b")));
        assert!(shared.focus(Path::new("/a")));

        shared.set_root(one, None);
        assert!(!shared.focus(Path::new("/a")));

        shared.forget(two);
        assert!(!shared.focus(Path::new("/b")));
    }
}
