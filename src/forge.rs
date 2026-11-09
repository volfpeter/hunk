//! Where a repository lives on the web, as a remote's URL says.

/// A repository on a forge.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Hosted {
    /// `https`, unless the remote's URL says `http`.
    scheme: &'static str,
    /// With the port of a web URL. An ssh port is not the web's.
    pub host: String,
    /// The repository's path on the host, like `owner/project`, without `.git`.
    pub path: String,
}

impl Hosted {
    /// The repository at `url`, unless the URL has no host, as a path on this machine doesn't,
    /// or its path has fewer than two parts.
    pub fn parse(url: &str) -> Option<Hosted> {
        let (scheme, authority, path) = match url.split_once("://") {
            Some((scheme, rest)) => {
                let (authority, path) = rest.split_once('/')?;
                (scheme, authority, path)
            }
            // `[user@]host:path`, as ssh takes it.
            None => {
                let (authority, path) = url.split_once(':').filter(|(authority, _)| !authority.contains('/'))?;
                ("ssh", authority, path)
            }
        };
        let host = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
        let web = matches!(scheme, "http" | "https");
        let host = if web { host } else { host.split(':').next().unwrap_or_default() };
        let path = path.trim_matches('/');
        let path = path.strip_suffix(".git").unwrap_or(path);
        let parts = path.split('/').filter(|part| !part.is_empty()).count();
        let scheme = if scheme == "http" { "http" } else { "https" };
        (!host.is_empty() && parts >= 2).then(|| Hosted { scheme, host: host.to_owned(), path: path.to_owned() })
    }

    /// `<user-or-org>/<project>`: the last two parts of the path.
    pub fn name(&self) -> String {
        let mut parts = self.path.rsplit('/').filter(|part| !part.is_empty());
        let (project, owner) = (parts.next().unwrap_or_default(), parts.next().unwrap_or_default());
        format!("{owner}/{project}")
    }

    /// Whether it is on GitHub. Any other host is taken to be Forgejo or Gitea.
    pub fn is_github(&self) -> bool {
        self.host == "github.com"
    }

    /// The forge's form for a pull request of `branch` into `base`, or into the default branch
    /// without one. GitHub, Forgejo and Gitea all take this address.
    pub fn compare_url(&self, base: Option<&str>, branch: &str) -> String {
        let compared = match base {
            Some(base) => format!("{}...{}", encode(base), encode(branch)),
            None => encode(branch),
        };
        format!("{}://{}/{}/compare/{compared}", self.scheme, self.host, self.path)
    }
}

/// `name` fit for a URL's path, its slashes kept.
fn encode(name: &str) -> String {
    name.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => char::from(byte).to_string(),
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn web(url: &str) -> Option<String> {
        Hosted::parse(url).map(|hosted| hosted.compare_url(None, "b"))
    }

    #[test]
    fn a_remote_on_a_host_has_a_place_on_the_web() {
        assert_eq!(web("git@github.com:owner/repo.git").as_deref(), Some("https://github.com/owner/repo/compare/b"));
        assert_eq!(web("github.com:owner/repo").as_deref(), Some("https://github.com/owner/repo/compare/b"));
        assert_eq!(web("ssh://git@host.example:2222/group/sub/repo.git").as_deref(), Some("https://host.example/group/sub/repo/compare/b"));
        assert_eq!(web("http://user:secret@host.example:3000/owner/repo/").as_deref(), Some("http://host.example:3000/owner/repo/compare/b"));
        assert_eq!(web("file:///srv/git/repo.git"), None);
        assert_eq!(web("/srv/git/repo.git"), None);
        assert_eq!(web("../repo"), None);
        assert_eq!(web("https://host.example/repo"), None);
    }

    #[test]
    fn a_name_is_the_last_two_parts_of_the_path() {
        assert_eq!(Hosted::parse("ssh://git@host.example:2222/group/sub/repo.git").unwrap().name(), "sub/repo");
        assert_eq!(Hosted::parse("https://codeberg.org/org/thing/").unwrap().name(), "org/thing");
    }

    #[test]
    fn compares_a_branch_with_its_base_escaping_what_a_url_cannot_hold() {
        let hosted = Hosted::parse("git@codeberg.org:org/thing.git").unwrap();
        assert!(!hosted.is_github());
        assert_eq!(hosted.compare_url(Some("main"), "user/fix#2"), "https://codeberg.org/org/thing/compare/main...user/fix%232");
    }
}
