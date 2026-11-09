//! Running `gh`, GitHub's command line: the one place that spawns it.

use std::io;
use std::process::{Command, Stdio};

use serde::Deserialize;

use crate::forge::Hosted;

/// A pull request on GitHub.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub status: Status,
    pub url: String,
}

/// Where a pull request stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Open,
    Draft,
    Merged,
    Closed,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Open => "Open",
            Status::Draft => "Draft",
            Status::Merged => "Merged",
            Status::Closed => "Closed",
        }
    }
}

/// A pull request as `gh pr list --json` prints it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Listed {
    number: u64,
    title: String,
    state: String,
    is_draft: bool,
    url: String,
    is_cross_repository: bool,
}

/// The pull request from `branch` of `hosted` into that repository: the open one if there is,
/// else the newest.
pub fn pull_request(hosted: &Hosted, branch: &str) -> Result<Option<PullRequest>, String> {
    let repo = format!("{}/{}", hosted.host, hosted.path);
    let fields = "number,title,state,isDraft,url,isCrossRepository";
    let listed = run(&["pr", "list", &format!("--repo={repo}"), &format!("--head={branch}"), "--state=all", "--limit=20", &format!("--json={fields}")])?;
    pick(&listed)
}

/// Of the pull requests `gh pr list` printed, newest first, the open one if there is, else the
/// newest. Another repository's branch of the same name doesn't count.
fn pick(listed: &str) -> Result<Option<PullRequest>, String> {
    let listed: Vec<Listed> = serde_json::from_str(listed).map_err(|error| format!("gh printed what isn't a list of pull requests: {error}"))?;
    let own: Vec<PullRequest> = listed.into_iter().filter(|listed| !listed.is_cross_repository).map(PullRequest::from).collect();
    let open = own.iter().position(|found| matches!(found.status, Status::Open | Status::Draft)).unwrap_or(0);
    Ok(own.into_iter().nth(open))
}

impl From<Listed> for PullRequest {
    fn from(listed: Listed) -> PullRequest {
        let status = match listed.state.as_str() {
            "OPEN" if listed.is_draft => Status::Draft,
            "OPEN" => Status::Open,
            "MERGED" => Status::Merged,
            _ => Status::Closed,
        };
        PullRequest { number: listed.number, title: listed.title, status, url: listed.url }
    }
}

/// Runs `gh` without a terminal, so it never prompts, and returns what it printed.
fn run(args: &[&str]) -> Result<String, String> {
    let output = Command::new("gh")
        .args(args)
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .output()
        .map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => "GitHub's command line, gh, isn't installed.".to_owned(),
            _ => format!("gh: {error}"),
        })?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let said = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    Err(if said.is_empty() { format!("gh failed: {}", output.status) } else { said })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listed(number: u64, state: &str, draft: bool, cross: bool) -> String {
        format!(r#"{{"number":{number},"title":"t","state":"{state}","isDraft":{draft},"url":"u","isCrossRepository":{cross}}}"#)
    }

    fn picked(listed: &[String]) -> Option<(u64, Status)> {
        pick(&format!("[{}]", listed.join(","))).unwrap().map(|found| (found.number, found.status))
    }

    #[test]
    fn picks_the_open_pull_request_else_the_newest_of_this_repository() {
        assert_eq!(picked(&[]), None);
        assert_eq!(picked(&[listed(3, "MERGED", false, false), listed(2, "OPEN", true, false)]), Some((2, Status::Draft)));
        assert_eq!(picked(&[listed(3, "OPEN", false, true), listed(2, "CLOSED", false, false), listed(1, "MERGED", false, false)]), Some((2, Status::Closed)));
        assert!(pick("GraphQL: no").is_err());
    }
}
