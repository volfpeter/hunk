//! Running git: the one place that spawns it.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Overrides that keep porcelain output in the shape the parsers expect, whatever the user's
/// config says.
const CONFIG: [&str; 14] = [
    "-c",
    "color.ui=never",
    "-c",
    "color.diff=never",
    "-c",
    "core.quotePath=false",
    "-c",
    "diff.mnemonicPrefix=false",
    "-c",
    "diff.noprefix=false",
    "-c",
    "diff.relative=false",
    "-c",
    "log.showSignature=false",
];

/// A failed git command, with what it printed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitError(String);

impl GitError {
    pub fn new(message: impl Into<String>) -> GitError {
        GitError(message.into())
    }
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for GitError {}

pub type Result<T> = std::result::Result<T, GitError>;

/// A repository, addressed by the root of its worktree.
#[derive(Clone, Debug)]
pub struct Repo {
    root: PathBuf,
    git_dir: PathBuf,
}

impl Repo {
    /// The repository containing `dir`.
    pub fn discover(dir: &Path) -> Result<Repo> {
        let paths = run_in(dir, &["rev-parse", "--show-toplevel", "--absolute-git-dir"], &[0])?;
        let (root, git_dir) = paths.trim_end().split_once('\n').ok_or_else(|| GitError(format!("unexpected rev-parse output: {paths}")))?;
        Ok(Repo { root: PathBuf::from(root), git_dir: PathBuf::from(git_dir) })
    }

    /// The root of the worktree.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The repository's git folder.
    pub fn git_dir(&self) -> &Path {
        &self.git_dir
    }

    /// The name of the worktree's root folder.
    pub fn name(&self) -> String {
        self.root.file_name().map_or_else(|| self.root.display().to_string(), |n| n.to_string_lossy().into_owned())
    }

    /// The folder for the app's own state.
    pub fn state_dir(&self) -> PathBuf {
        self.git_dir.join(crate::NAME)
    }

    /// The worktree's file at `path`, unless it can't be read.
    pub fn worktree_file(&self, path: &str) -> Option<String> {
        let bytes = fs::read(self.root.join(path)).ok()?;
        Some(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// Runs git at the root and returns its output.
    pub fn run(&self, args: &[&str]) -> Result<String> {
        self.run_accepting(args, &[0])
    }

    /// [`Repo::run`], treating each of `ok_codes` as success.
    pub fn run_accepting(&self, args: &[&str], ok_codes: &[i32]) -> Result<String> {
        run_in(&self.root, args, ok_codes)
    }

    /// [`Repo::run`] on `paths`, taken literally, or nothing without paths: git would take that
    /// to mean every file. The flag rather than `GIT_LITERAL_PATHSPECS`, which would reach the
    /// `:/` that `stash --include-untracked` passes to the `clean` it runs.
    pub fn run_on(&self, args: &[&str], paths: &[String]) -> Result<String> {
        if paths.is_empty() {
            return Ok(String::new());
        }
        let paths = paths.iter().map(String::as_str);
        self.run(&["--literal-pathspecs"].iter().chain(args).chain(&["--"]).copied().chain(paths).collect::<Vec<_>>())
    }

    /// Whether git ignores all of `paths`.
    pub fn all_ignored(&self, paths: &[String]) -> Result<bool> {
        let args: Vec<&str> = ["check-ignore", "--"].into_iter().chain(paths.iter().map(String::as_str)).collect();
        let ignored = self.run_accepting(&args, &[0, 1])?;
        Ok(ignored.lines().count() == paths.len())
    }

    /// The commit `HEAD` points at, unless the current branch has none yet.
    pub fn head_commit(&self) -> Result<Option<String>> {
        self.commit("HEAD")
    }

    /// The commit `revision` points at, if it points at one.
    pub fn commit(&self, revision: &str) -> Result<Option<String>> {
        let out = self.run_accepting(&["rev-parse", "--verify", "-q", revision], &[0, 1])?;
        Ok(Some(out.trim().to_owned()).filter(|commit| !commit.is_empty()))
    }
}

fn run_in(dir: &Path, args: &[&str], ok_codes: &[i32]) -> Result<String> {
    let output = Command::new("git")
        .args(CONFIG)
        .args(args)
        .current_dir(dir)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| GitError(format!("git {}: {e}", args.join(" "))))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.code().is_some_and(|code| ok_codes.contains(&code)) {
        return Ok(stdout.into_owned());
    }
    // Commands that change things report some failures, like conflicts, on stdout.
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(GitError([stdout.trim(), stderr.trim()].into_iter().filter(|part| !part.is_empty()).collect::<Vec<_>>().join("\n")))
}

#[cfg(test)]
pub mod testing {
    use std::fs;

    use tempfile::TempDir;

    use super::Repo;

    /// A real repository in a temporary folder.
    pub struct Fixture {
        dir: TempDir,
        pub repo: Repo,
    }

    impl Fixture {
        pub fn new() -> Fixture {
            let dir = TempDir::new().unwrap();
            super::run_in(dir.path(), &["init", "-q", "-b", "main"], &[0]).unwrap();
            let repo = Repo::discover(dir.path()).unwrap();
            let fixture = Fixture { dir, repo };
            fixture.git(&["config", "user.name", "Test"]);
            fixture.git(&["config", "user.email", "test@example.com"]);
            fixture
        }

        pub fn git(&self, args: &[&str]) -> String {
            self.repo.run(args).unwrap()
        }

        pub fn write(&self, path: &str, text: &str) {
            fs::write(self.dir.path().join(path), text).unwrap();
        }

        /// The worktree's file at `path`, unless it is gone.
        pub fn read(&self, path: &str) -> Option<String> {
            self.repo.worktree_file(path)
        }

        /// A short hash of only digits for a copy of the newest stash, which git's stash commands
        /// would read as a place in the stash list.
        pub fn stash_with_digits_only_hash(&self) -> String {
            let (tree, parents) = (self.git(&["rev-parse", "stash@{0}^{tree}"]), self.git(&["rev-parse", "stash@{0}^@"]));
            let mut args = vec!["commit-tree", tree.trim()];
            args.extend(parents.lines().flat_map(|parent| ["-p", parent]));
            (0..10_000)
                .map(|n| {
                    let message = n.to_string();
                    let commit = self.git(&[&args[..], &["-m", &message]].concat());
                    commit[..7].to_owned()
                })
                .find(|hash| hash.bytes().all(|byte| byte.is_ascii_digit()))
                .unwrap()
        }

        /// Writes `path` and commits it.
        pub fn commit(&self, path: &str, text: &str, message: &str) {
            self.write(path, text);
            self.git(&["add", path]);
            self.git(&["commit", "-q", "-m", message]);
        }
    }
}
