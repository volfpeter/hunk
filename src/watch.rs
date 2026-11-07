//! Noticing that a repository changed.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use rune::Waker;

use crate::git::Repo;

/// How long files must stay untouched before a burst of changes counts as finished.
const QUIET_FOR: Duration = Duration::from_millis(300);
/// How long a burst that won't end goes unreported.
const MOST_WAIT: Duration = Duration::from_secs(2);
/// Paths per `check-ignore`, to stay within what a command line can hold.
const CHECKED_AT_ONCE: usize = 500;
/// What in the git folder changes without changing what git reports.
const GIT_DIR_NOISE: [&str; 3] = ["objects", "logs", crate::NAME];

/// Watches a repository from its own thread, since watching means walking the worktree.
pub struct RepoWatch {
    reports: Receiver<Result<(), String>>,
}

impl RepoWatch {
    pub fn start(repo: Repo, waker: Waker) -> RepoWatch {
        let (report, reports) = mpsc::channel();
        thread::spawn(move || watch(&repo, &report, &waker));
        RepoWatch { reports }
    }

    /// What was noticed since the last call: whether the repository changed, and why watching
    /// it stopped working, if it did.
    pub fn take(&self) -> (bool, Option<String>) {
        let (mut changed, mut failure) = (false, None);
        for report in self.reports.try_iter() {
            match report {
                Ok(()) => changed = true,
                Err(error) => failure = Some(error),
            }
        }
        (changed, failure)
    }
}

fn watch(repo: &Repo, report: &Sender<Result<(), String>>, waker: &Waker) {
    let send = |result| {
        // With the receiver gone there is nobody left to tell.
        let _ = report.send(result);
        waker();
    };
    let (events, paths) = mpsc::channel();
    let started = start_watcher(repo, events);
    let _watcher = match started {
        Ok(watcher) => watcher,
        Err(error) => return send(Err(error.to_string())),
    };
    while let Some(burst) = next_burst(&paths) {
        if burst.iter().any(|path| counts_in_git_dir(path, repo.git_dir()) == Some(true)) || touches_tracked_or_new(repo, &burst) {
            send(Ok(()));
        }
    }
}

/// Watches the worktree, and the git folder too because it lies elsewhere in a linked worktree.
fn start_watcher(repo: &Repo, paths: Sender<PathBuf>) -> notify::Result<RecommendedWatcher> {
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
        let Ok(event) = event else { return };
        // Reading a file is an event too, but changes nothing.
        if !event.kind.is_access() {
            for path in event.paths {
                // With the receiver gone there is nobody left to tell.
                let _ = paths.send(path);
            }
        }
    })?;
    watcher.watch(repo.root(), RecursiveMode::Recursive)?;
    watcher.watch(repo.git_dir(), RecursiveMode::Recursive)?;
    Ok(watcher)
}

/// The paths of the next burst of changes, or `None` once nothing can arrive.
fn next_burst(paths: &Receiver<PathBuf>) -> Option<HashSet<PathBuf>> {
    let mut burst = HashSet::from([paths.recv().ok()?]);
    let latest = Instant::now() + MOST_WAIT;
    loop {
        let wait = QUIET_FOR.min(latest.saturating_duration_since(Instant::now()));
        match paths.recv_timeout(wait) {
            Ok(path) => {
                burst.insert(path);
            }
            Err(RecvTimeoutError::Timeout) => return Some(burst),
            Err(RecvTimeoutError::Disconnected) => return None,
        }
    }
}

/// For a path in the git folder, whether it can change what git reports.
fn counts_in_git_dir(path: &Path, git_dir: &Path) -> Option<bool> {
    let inside = path.strip_prefix(git_dir).ok()?;
    let noise = inside.components().next().is_some_and(|top| GIT_DIR_NOISE.iter().any(|name| top.as_os_str() == *name));
    Some(!noise && inside.extension().is_none_or(|extension| extension != "lock"))
}

/// Whether a path of `burst` lies in the worktree and git doesn't ignore it. A failure counts as
/// a yes: refreshing for nothing costs less than missing a change.
fn touches_tracked_or_new(repo: &Repo, burst: &HashSet<PathBuf>) -> bool {
    let paths: Vec<String> = burst.iter().filter(|path| counts_in_git_dir(path, repo.git_dir()).is_none()).map(|path| path.to_string_lossy().into_owned()).collect();
    paths.chunks(CHECKED_AT_ONCE).any(|chunk| !repo.all_ignored(chunk).unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::git::testing::Fixture;

    #[test]
    fn only_what_git_reports_counts_in_the_git_folder() {
        let git_dir = Path::new("/repo/.git");
        let counts = |path: &str| counts_in_git_dir(Path::new(path), git_dir);

        assert_eq!(counts("/repo/.git/index"), Some(true));
        assert_eq!(counts("/repo/.git/HEAD"), Some(true));
        assert_eq!(counts("/repo/.git/refs/heads/main"), Some(true));
        assert_eq!(counts("/repo/.git/index.lock"), Some(false));
        assert_eq!(counts("/repo/.git/objects/ab/cdef"), Some(false));
        assert_eq!(counts("/repo/.git/diffactive/comments.json"), Some(false));
        assert_eq!(counts("/repo/src/main.rs"), None);
    }

    #[test]
    fn ignored_files_do_not_count_as_a_change() {
        let fixture = Fixture::new();
        fixture.write(".gitignore", "target/\n");
        let burst = |paths: &[&str]| paths.iter().map(|path| fixture.repo.root().join(path)).collect::<HashSet<_>>();

        assert!(!touches_tracked_or_new(&fixture.repo, &burst(&["target/debug/app", "target/debug/app.d"])));
        assert!(touches_tracked_or_new(&fixture.repo, &burst(&["target/debug/app", "src/main.rs"])));
        assert!(touches_tracked_or_new(&fixture.repo, &burst(&["odd[1]name.rs"])));
    }

    /// Waits until `reports` passes `count`, or gives up.
    fn wait_for(reports: &AtomicUsize, count: usize) -> bool {
        let latest = Instant::now() + Duration::from_secs(10);
        while Instant::now() < latest {
            if reports.load(Ordering::SeqCst) >= count {
                return true;
            }
            thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn reports_a_burst_of_changes_once_and_stays_quiet_about_ignored_files() {
        let fixture = Fixture::new();
        fixture.write(".gitignore", "ignored.txt\n");
        let wakes = Arc::new(AtomicUsize::new(0));
        let counter = wakes.clone();
        let watch = RepoWatch::start(fixture.repo.clone(), Arc::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        }));
        // Watching starts on its own thread, and nothing says when it has.
        thread::sleep(Duration::from_millis(300));

        for text in ["one\n", "two\n", "three\n"] {
            fixture.write("a.txt", text);
        }
        assert!(wait_for(&wakes, 1), "the change went unnoticed");
        thread::sleep(QUIET_FOR * 2);
        assert_eq!(watch.take(), (true, None));
        assert_eq!(wakes.load(Ordering::SeqCst), 1);

        fixture.write("ignored.txt", "noise\n");
        thread::sleep(QUIET_FOR * 3);
        assert_eq!(watch.take(), (false, None));
    }
}
