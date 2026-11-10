//! Opening a repository: what a folder holds, and the folders to pick from.

use std::fs;
use std::path::{Path, PathBuf};

use crate::comments::Comments;
use crate::git::{self, Repo};

/// How many folders the open dialog offers at once.
const SHOWN: usize = 10;

/// What a folder turned out to hold.
pub enum Opened {
    /// The repository the folder is in, with its comments.
    Repository(Repo, Comments),
    /// A folder in no repository.
    Folder(PathBuf),
}

impl Opened {
    /// What `folder` holds.
    pub fn find(folder: &Path) -> Result<Opened, String> {
        let folder = fs::canonicalize(folder).map_err(|error| format!("{}: {error}", folder.display()))?;
        if !folder.is_dir() {
            return Err(format!("{} is not a folder.", folder.display()));
        }
        Ok(match Repo::discover(&folder) {
            Ok(repo) => {
                let comments = Comments::load(&repo);
                Opened::Repository(repo, comments)
            }
            Err(_) => Opened::Folder(folder),
        })
    }

    /// The root of the repository found, if any.
    pub fn root(&self) -> Option<&Path> {
        match self {
            Opened::Repository(repo, _) => Some(repo.root()),
            Opened::Folder(_) => None,
        }
    }

    /// Makes `folder` a repository, and opens it.
    pub fn init(folder: &Path) -> Result<Opened, String> {
        git::init(folder).map_err(|error| error.to_string())?;
        Opened::find(folder)
    }
}

/// A folder on offer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub path: PathBuf,
    pub name: String,
    pub detail: String,
    /// What the typed path becomes to complete it with this folder.
    pub completion: String,
}

/// What the open dialog offers for what has been typed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Offer {
    pub choices: Vec<Choice>,
    /// How many more match than are offered.
    pub more: usize,
    /// Recent repositories that are gone.
    pub gone: Vec<PathBuf>,
}

impl Offer {
    /// With nothing `typed`, the `recent` repositories still there. Otherwise the folders
    /// completing the path typed, which starts at the home folder unless it is absolute.
    pub fn new(typed: &str, recent: &[PathBuf]) -> Offer {
        if typed.trim().is_empty() {
            let (there, gone): (Vec<PathBuf>, _) = recent.iter().cloned().partition(|root| root.join(".git").exists());
            return Offer { choices: there.into_iter().map(recent_choice).collect(), more: 0, gone };
        }
        let typed = if typed == "~" { "~/" } else { typed };
        let (folder, start) = typed.split_at(typed.rfind('/').map_or(0, |slash| slash + 1));
        let mut choices = subfolders(&resolve(folder), start, folder);
        choices.sort_by_key(|choice| choice.name.to_lowercase());
        let more = choices.len().saturating_sub(SHOWN);
        choices.truncate(SHOWN);
        Offer { choices, more, gone: Vec::new() }
    }
}

/// The folder `typed` names: under the home folder unless absolute.
pub fn resolve(typed: &str) -> PathBuf {
    let home = dirs::home_dir().unwrap_or_default();
    match typed.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => home.join(rest.trim_start_matches('/')),
        _ if typed.starts_with('/') => PathBuf::from(typed),
        _ => home.join(typed),
    }
}

/// `path`, with `~` for the home folder.
pub fn tilde(path: &Path) -> String {
    match dirs::home_dir().and_then(|home| path.strip_prefix(home).ok().map(Path::to_owned)) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

fn recent_choice(root: PathBuf) -> Choice {
    let name = root.file_name().map_or_else(|| root.display().to_string(), |name| name.to_string_lossy().into_owned());
    let detail = tilde(&root);
    Choice { completion: format!("{detail}/"), path: root, name, detail }
}

/// The folders in `folder` whose names start with `start`, ignoring case, typed as under
/// `typed_folder`. Hidden ones only show when `start` asks for them.
fn subfolders(folder: &Path, start: &str, typed_folder: &str) -> Vec<Choice> {
    let Ok(entries) = fs::read_dir(folder) else { return Vec::new() };
    let start_lower = start.to_lowercase();
    entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().into_string().ok()?;
            let wanted = name.to_lowercase().starts_with(&start_lower) && (start.starts_with('.') || !name.starts_with('.'));
            let path = entry.path();
            if !wanted || !path.is_dir() {
                return None;
            }
            let detail = if path.join(".git").exists() { "repository" } else { "" };
            Some(Choice { completion: format!("{typed_folder}{name}/"), detail: detail.to_owned(), path, name })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::git::testing::Fixture;

    #[test]
    fn a_folder_inside_a_repository_opens_the_repository() {
        let fixture = Fixture::new();
        let inner = fixture.repo.root().join("src");
        fs::create_dir(&inner).unwrap();

        let Ok(Opened::Repository(repo, _)) = Opened::find(&inner) else { panic!("no repository") };
        assert_eq!(repo.root(), fs::canonicalize(fixture.repo.root()).unwrap());

        let outside = TempDir::new().unwrap();
        assert!(matches!(Opened::find(outside.path()), Ok(Opened::Folder(_))));
        assert!(Opened::find(&outside.path().join("missing")).is_err());
    }

    #[test]
    fn completes_folders_by_their_start_marking_repositories() {
        let dir = TempDir::new().unwrap();
        for folder in ["Alpha", "alps/.git", "beta", ".hidden"] {
            fs::create_dir_all(dir.path().join(folder)).unwrap();
        }
        fs::write(dir.path().join("almanac.txt"), "").unwrap();
        let typed = format!("{}/al", dir.path().display());

        let offer = Offer::new(&typed, &[]);

        let offered: Vec<(&str, &str)> = offer.choices.iter().map(|choice| (choice.name.as_str(), choice.detail.as_str())).collect();
        assert_eq!(offered, [("Alpha", ""), ("alps", "repository")]);
        assert_eq!(offer.choices[1].completion, format!("{}/alps/", dir.path().display()));
        assert_eq!(Offer::new(&format!("{}/.", dir.path().display()), &[]).choices[0].name, ".hidden");
    }

    #[test]
    fn with_nothing_typed_offers_the_recent_repositories_still_there() {
        let (fixture, gone) = (Fixture::new(), PathBuf::from("/nowhere/gone"));

        let offer = Offer::new(" ", &[gone.clone(), fixture.repo.root().to_owned()]);

        assert_eq!(offer.choices.iter().map(|choice| choice.path.as_path()).collect::<Vec<_>>(), [fixture.repo.root()]);
        assert_eq!(offer.gone, [gone]);
    }

    #[test]
    fn a_path_starts_at_home_unless_absolute() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(resolve("~"), home);
        assert_eq!(resolve("~/dev"), home.join("dev"));
        assert_eq!(resolve("dev"), home.join("dev"));
        assert_eq!(resolve("/tmp"), PathBuf::from("/tmp"));
        assert_eq!(resolve("/"), PathBuf::from("/"));
        assert_eq!(tilde(&home.join("dev")), "~/dev");
    }
}
