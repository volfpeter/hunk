//! Browsing the working tree: the files on offer, the ones opened lately, and any one of them
//! whole, with its changes.

use std::cell::Cell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use ignore::WalkBuilder;
use rune::{Latest, TextEdit, Tx, Waker};
use rune_kit::diff::{DiffMode, DiffState, LineRange, parse};
use rune_kit::{FileNode, FileTree, TreeModel};
use serde::{Deserialize, Serialize};

use crate::comments::Anchor;
use crate::fuzzy;
use crate::git::{GitError, Repo, Result};
use crate::kept;
use crate::review::file_changes;

const FILE_NAME: &str = "recent-files.json";
const VERSION: u32 = 1;
/// How many are remembered.
const KEPT: usize = 20;
/// The most files the picker lists.
const OFFERED: usize = 100;
/// How many files visited back and forward goes through.
const VISITS: usize = 50;

/// The key of the overlay's diff.
pub const BROWSED: &str = "browsed";
/// The key of the overlay's file tree.
pub const FILE_TREE: &str = "browse-tree";
/// The overlay's scroll viewport, which [`rune_kit::diff::DiffView`] keys below its own.
pub const BROWSED_SCROLL: &str = "browsed/scroll";

/// A file shown in the overlay, and where it was scrolled to when the reader left it.
struct Visit {
    path: String,
    scrolled: [f32; 2],
}

/// The file overlay: one worktree file whole, the files visited before and after it, and the
/// picker choosing it.
pub struct Browser {
    /// Whether the overlay is on screen.
    pub open: bool,
    /// Whether the overlay shows the file tree.
    pub tree_open: bool,
    /// The files on offer as folders, each marked when it has changes.
    pub tree: TreeModel<FileNode<bool>>,
    /// The files shown, oldest first.
    visits: Vec<Visit>,
    /// The visit on screen.
    at: usize,
    /// Where to scroll the file coming back once it lands.
    restore: Option<[f32; 2]>,
    pub diff: DiffState,
    /// Why the file shown can't be read.
    pub error: Option<String>,
    /// The patch on screen, so a reload that changes nothing leaves the view alone.
    patch: String,
    /// The lines to bring into view and select once the file lands.
    reveal: Option<LineRange>,
    /// Where the diff was scrolled to, put back when the overlay opens again.
    pub scrolled: Rc<Cell<[f32; 2]>>,
    pub picker_text: TextEdit,
    /// The highlighted row of the picker, while it is open.
    pub picker: Option<usize>,
    /// What the picker offers for what is typed.
    pub offer: Vec<String>,
    /// Every file on offer, sorted, once the worktree has been walked.
    files: Option<Vec<String>>,
    recent: RecentFiles,
    files_load: Latest<Vec<String>>,
    file_load: Latest<(String, Result<String>)>,
}

impl Browser {
    pub fn new(recent: RecentFiles, tree_open: bool) -> Browser {
        let mut diff = DiffState::default();
        diff.set_mode(DiffMode::Unified);
        Browser {
            open: false,
            tree_open,
            tree: TreeModel::new().key(FILE_TREE),
            visits: Vec::new(),
            at: 0,
            restore: None,
            diff,
            error: None,
            patch: String::new(),
            reveal: None,
            scrolled: Rc::default(),
            picker_text: TextEdit::default(),
            picker: None,
            offer: Vec::new(),
            files: None,
            recent,
            files_load: Latest::default(),
            file_load: Latest::default(),
        }
    }

    pub fn set_waker(&mut self, waker: Waker) {
        self.files_load.set_waker(waker.clone());
        self.file_load.set_waker(waker);
    }

    /// Walks the worktree at `root` again for the picker and the file tree.
    pub fn walk(&mut self, root: PathBuf) {
        self.files_load.request(root, |root| files(&root));
    }

    /// Whether the picker waits for the worktree's files.
    pub fn walking(&self) -> bool {
        self.files.is_none() && self.files_load.is_pending()
    }

    /// The file shown, once one is picked.
    pub fn path(&self) -> Option<&str> {
        self.visits.get(self.at).map(|visit| visit.path.as_str())
    }

    /// Shows the file `path`, at `reveal` once it lands, remembering it as a recent file and as a
    /// visit after the one on screen, which drops the visits after that.
    pub fn show(&mut self, repo: &Repo, path: String, reveal: Option<LineRange>) {
        self.reveal = reveal.map(|lines| LineRange { file: 0, ..lines });
        if self.path() != Some(path.as_str()) {
            self.recent.remember(&path);
            self.keep_place();
            self.visits.truncate(self.at + 1);
            self.visits.push(Visit { path, scrolled: [0.0; 2] });
            if self.visits.len() > VISITS {
                self.visits.remove(0);
            }
            self.at = self.visits.len() - 1;
            self.restore = None;
        }
        self.reload(repo);
    }

    /// Whether there is a visit to go to, after the one on screen or before it.
    pub fn can_go(&self, forward: bool) -> bool {
        if forward { self.at + 1 < self.visits.len() } else { self.at > 0 }
    }

    /// Goes to the visit after the one on screen, or before it, where the reader left it.
    pub fn go(&mut self, repo: &Repo, forward: bool) {
        if !self.can_go(forward) {
            return;
        }
        self.keep_place();
        self.at = if forward { self.at + 1 } else { self.at - 1 };
        self.reveal = None;
        self.restore = Some(self.visits[self.at].scrolled);
        self.reload(repo);
    }

    /// Notes where the visit on screen is scrolled to, once its file is the one drawn.
    fn keep_place(&mut self) {
        let drawn = self.diff.files().first().map(|file| file.path().to_owned());
        if let Some(visit) = self.visits.get_mut(self.at).filter(|visit| Some(&visit.path) == drawn.as_ref()) {
            visit.scrolled = self.scrolled.get();
        }
    }

    /// Reads the file shown again.
    pub fn reload(&mut self, repo: &Repo) {
        if let Some(path) = self.path().map(str::to_owned) {
            self.file_load.request((repo.clone(), path), |(repo, path)| {
                let patch = whole_file(&repo, &path);
                (path, patch)
            });
        }
    }

    /// Lands finished work; returns whether any landed, and whether the files on offer did.
    pub fn poll(&mut self, tx: &mut Tx) -> (bool, bool) {
        let files = self.files_load.take();
        let walked = files.is_some();
        if files.is_some() {
            self.files = files;
        }
        let file = self.file_load.take();
        let landed = walked || file.is_some();
        if let Some((path, patch)) = file.filter(|(path, _)| self.path() == Some(path.as_str())) {
            self.land(&path, patch, tx);
        }
        (landed, walked)
    }

    /// Fills the file tree from the files on offer, marking those `changed` says have changes,
    /// keeping its open folders and selection. A tree filled for the first time shows the file
    /// on screen.
    pub fn fill_tree(&mut self, changed: impl Fn(&str) -> bool, tx: &mut Tx) {
        let Some(files) = &self.files else { return };
        let mut tree = FileTree::new().key(FILE_TREE).build(files.iter().map(|path| (path, changed(path))));
        let old = &self.tree;
        for folder in (0..old.visible_len()).filter_map(|row| old.visible_id(row)).filter(|id| old.is_expanded(id)) {
            tree.set_expanded(folder, true);
        }
        if let Some(id) = old.selected().next() {
            tree.select(id);
        }
        let first = old.is_empty();
        self.tree = tree;
        if first {
            self.show_in_tree(tx);
        }
    }

    /// Opens the file tree's folders down to the file on screen, and selects and scrolls to it.
    pub fn show_in_tree(&mut self, tx: &mut Tx) {
        if let Some(visit) = self.visits.get(self.at) {
            self.tree.reveal(&visit.path, tx);
        }
    }

    /// Puts the file `path` on screen. A newer state of the file on screen keeps the reader's
    /// place and selection; another file starts at its top, or where a visit back to it left it.
    fn land(&mut self, path: &str, patch: Result<String>, tx: &mut Tx) {
        let patch = match patch {
            Ok(patch) => patch,
            Err(error) => {
                self.error = Some(error.to_string());
                self.patch.clear();
                self.diff.set_files(Vec::new());
                return;
            }
        };
        self.error = None;
        if patch != self.patch {
            let same_file = self.diff.files().first().is_some_and(|file| file.path() == path);
            let selection = self.diff.selection().filter(|_| same_file);
            self.diff.set_files(parse(&patch));
            self.patch = patch;
            if let Some(lines) = selection.filter(|lines| self.diff.files().first().is_some_and(|file| Anchor::on_lines(file, *lines).is_some())) {
                self.diff.set_selection(Some(lines));
            }
            if !same_file {
                self.show_in_tree(tx);
            }
            match self.restore.take().filter(|_| !same_file) {
                Some([x, y]) => tx.scroll_to_instant(BROWSED_SCROLL, x, y),
                None if !same_file => self.diff.scroll_to_file(0, tx),
                None => {}
            }
        }
        self.restore = None;
        if let Some(lines) = self.reveal.take().filter(|lines| self.diff.files().first().is_some_and(|file| Anchor::on_lines(file, *lines).is_some())) {
            self.diff.set_selection(Some(lines));
            self.diff.scroll_to_line(0, lines.start, tx);
        }
    }

    /// Fills the picker for what is typed: the files opened lately, then the `changed` ones,
    /// then the rest, as [`fuzzy::rank`] ranks them.
    pub fn rank(&mut self, changed: &[&str]) {
        let known = self.files.as_deref();
        let on_offer = |path: &&str| known.is_none_or(|files| files.binary_search_by(|file| file.as_str().cmp(path)).is_ok());
        let mut seen = HashSet::new();
        let recent = self.recent.paths().iter().map(String::as_str);
        let rest = known.unwrap_or_default().iter().map(String::as_str);
        let candidates: Vec<&str> = recent.chain(changed.iter().copied()).chain(rest).filter(on_offer).filter(|path| seen.insert(*path)).collect();
        let ranked = fuzzy::rank(&self.picker_text.text, candidates, |path| path);
        self.offer = ranked.into_iter().take(OFFERED).map(str::to_owned).collect();
        self.picker = self.picker.map(|_| 0);
    }

    /// Moves the picker's highlight by `rows`, stopping at the ends.
    pub fn move_picker(&mut self, rows: isize) {
        let last = self.offer.len().saturating_sub(1);
        self.picker = self.picker.map(|active| active.saturating_add_signed(rows).min(last));
    }
}

/// The worktree's files, sorted: what `.gitignore`, `.git/info/exclude` and the global excludes
/// file leave in, and what `.ignore` brings back. Hidden files count, `.git` doesn't.
pub fn files(root: &Path) -> Vec<String> {
    let walk = WalkBuilder::new(root).hidden(false).filter_entry(|entry| entry.file_name() != ".git").build();
    let mut files: Vec<String> = walk
        .filter_map(|entry| {
            let entry = entry.ok().filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))?;
            let path = entry.path().strip_prefix(root).ok()?;
            Some(path.to_string_lossy().replace('\\', "/"))
        })
        .collect();
    files.sort();
    files
}

/// The worktree's file `path` whole, as a patch against `HEAD` with every line in its one hunk:
/// its changes when it has any, staged or not, else every line unchanged.
pub fn whole_file(repo: &Repo, path: &str) -> Result<String> {
    let changes = file_changes(repo, path)?;
    if !changes.is_empty() {
        return Ok(changes);
    }
    let text = repo.worktree_file(path).ok_or_else(|| GitError::new(format!("{path} can't be read.")))?;
    Ok(unchanged(path, &text))
}

/// A patch showing `text`, the file at `path`, with nothing changed.
fn unchanged(path: &str, text: &str) -> String {
    let mut patch = format!("diff --git a/{path} b/{path}\n");
    if text.contains('\0') {
        patch.push_str(&format!("Binary files a/{path} and b/{path} differ\n"));
        return patch;
    }
    patch.push_str(&format!("--- a/{path}\n+++ b/{path}\n"));
    let lines = text.split_inclusive('\n').count();
    if lines > 0 {
        patch.push_str(&format!("@@ -1,{lines} +1,{lines} @@\n"));
    }
    for line in text.split_inclusive('\n') {
        patch.push(' ');
        patch.push_str(line);
    }
    if !text.is_empty() && !text.ends_with('\n') {
        patch.push_str("\n\\ No newline at end of file\n");
    }
    patch
}

#[derive(Serialize, Deserialize)]
struct StoreFile {
    version: u32,
    files: Vec<String>,
}

/// The files opened lately in a repository, newest first, kept in its git folder. Writes
/// happen in the background. The default remembers nothing and keeps nothing.
#[derive(Clone, Debug, Default)]
pub struct RecentFiles {
    file: Option<PathBuf>,
    paths: Vec<String>,
}

impl RecentFiles {
    /// Reads the ones kept for `repo`. A file that can't be read counts as none.
    pub fn load(repo: &Repo) -> RecentFiles {
        let file = repo.state_dir().join(FILE_NAME);
        let paths = kept::read::<StoreFile>(&file).filter(|kept| kept.version == VERSION).map(|kept| kept.files).unwrap_or_default();
        RecentFiles { file: Some(file), paths }
    }

    pub fn paths(&self) -> &[String] {
        &self.paths
    }

    /// Puts `path` first.
    pub fn remember(&mut self, path: &str) {
        if self.paths.first().is_some_and(|first| first == path) {
            return;
        }
        self.paths.retain(|kept| kept != path);
        self.paths.insert(0, path.to_owned());
        self.paths.truncate(KEPT);
        if let Some(file) = self.file.clone() {
            kept::save(file, StoreFile { version: VERSION, files: self.paths.clone() });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use rune_kit::diff::{LineKind, parse};

    use super::*;
    use crate::git::testing::Fixture;

    fn kinds(patch: &str) -> Vec<(LineKind, String)> {
        let files = parse(patch);
        assert_eq!(files.len(), 1, "{patch}");
        assert!(files[0].hunks.len() <= 1, "{patch}");
        files[0].hunks.iter().flat_map(|hunk| &hunk.lines).map(|line| (line.kind, line.text.clone())).collect()
    }

    #[test]
    fn offers_what_git_leaves_in_and_what_dot_ignore_brings_back() {
        let fixture = Fixture::new();
        fixture.write(".gitignore", "*.log\nbuild/\n.env\n");
        fixture.write(".ignore", "!.env\n");
        fixture.write(".env", "SECRET=1\n");
        fixture.write("a.log", "noise\n");
        fs::create_dir_all(fixture.repo.root().join("build")).unwrap();
        fixture.write("build/out.txt", "built\n");
        fs::create_dir_all(fixture.repo.root().join("src")).unwrap();
        fixture.write("src/main.rs", "fn main() {}\n");
        fs::write(fixture.repo.git_dir().join("info/exclude"), "local.txt\n").unwrap();
        fixture.write("local.txt", "mine\n");

        assert_eq!(files(fixture.repo.root()), [".env", ".gitignore", ".ignore", "src/main.rs"]);
    }

    #[test]
    fn shows_a_file_whole_with_its_staged_and_unstaged_changes() {
        let fixture = Fixture::new();
        fixture.commit("a.txt", "one\ntwo\nthree\nfour\n", "first");
        fixture.write("a.txt", "ONE\ntwo\nthree\nfour\n");
        fixture.git(&["add", "a.txt"]);
        fixture.write("a.txt", "ONE\ntwo\nthree\nFOUR\n");

        let lines = kinds(&whole_file(&fixture.repo, "a.txt").unwrap());

        let changed: Vec<&str> = lines.iter().filter(|(kind, _)| *kind != LineKind::Context).map(|(_, text)| text.as_str()).collect();
        assert_eq!(changed, ["one", "ONE", "four", "FOUR"]);
        assert_eq!(lines.len(), 6);
    }

    #[test]
    fn shows_an_unchanged_file_whole_and_a_new_one_as_added() {
        let fixture = Fixture::new();
        fixture.commit("a.txt", "one\ntwo", "first");
        fixture.write("new.txt", "fresh\n");

        let unchanged = kinds(&whole_file(&fixture.repo, "a.txt").unwrap());
        assert_eq!(unchanged, [(LineKind::Context, "one".to_owned()), (LineKind::Context, "two".to_owned())]);
        assert_eq!(kinds(&whole_file(&fixture.repo, "new.txt").unwrap()), [(LineKind::Added, "fresh".to_owned())]);
    }

    #[test]
    fn going_back_and_forward_walks_the_visits_and_a_new_one_drops_those_ahead() {
        let fixture = Fixture::new();
        let mut browser = Browser::new(RecentFiles::default(), true);
        for path in ["a", "b", "c"] {
            browser.show(&fixture.repo, path.to_owned(), None);
        }
        browser.go(&fixture.repo, false);
        browser.go(&fixture.repo, false);
        assert_eq!((browser.path(), browser.can_go(false)), (Some("a"), false));
        browser.go(&fixture.repo, true);
        assert_eq!(browser.path(), Some("b"));

        browser.show(&fixture.repo, "d".to_owned(), None);
        assert!(!browser.can_go(true));
        browser.go(&fixture.repo, false);
        assert_eq!(browser.path(), Some("b"));
    }

    #[test]
    fn the_file_tree_opens_on_the_file_shown_and_keeps_what_was_opened_since() {
        let fixture = Fixture::new();
        let mut browser = Browser::new(RecentFiles::default(), true);
        browser.show(&fixture.repo, "src/app.rs".to_owned(), None);
        browser.files = Some(["docs/a.md", "src/app.rs", "src/view.rs"].map(str::to_owned).to_vec());
        let tx = &mut Tx::default();

        browser.fill_tree(|path| path == "src/view.rs", tx);
        assert!(browser.tree.is_selected("src/app.rs") && browser.tree.is_expanded("src") && !browser.tree.is_expanded("docs"));
        assert_eq!(browser.tree.get("src/view.rs").and_then(|node| node.data), Some(true));

        browser.tree.set_expanded("docs", true);
        browser.tree.select("src/view.rs");
        browser.fill_tree(|_| false, tx);
        assert!(browser.tree.is_expanded("docs") && browser.tree.is_selected("src/view.rs"));
    }

    #[test]
    fn remembers_the_latest_files_first_and_keeps_them() {
        let fixture = Fixture::new();
        let mut recent = RecentFiles::load(&fixture.repo);
        for n in 0..KEPT + 2 {
            recent.remember(&format!("{n}.txt"));
        }
        recent.remember("5.txt");
        assert_eq!(recent.paths().len(), KEPT);
        assert_eq!(recent.paths()[..2], ["5.txt", "21.txt"]);
    }
}
