//! Running `gh`, GitHub's command line: the one place that spawns it.

use std::io::{self, Write};
use std::process::{Command, Stdio};

use rune_kit::diff::Side;
use serde::{Deserialize, Serialize};

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
    let listed = run(&["pr", "list", &format!("--repo={repo}"), &format!("--head={branch}"), "--state=all", "--limit=20", &format!("--json={fields}")], None)?;
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

/// What a review says of a pull request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    RequestChanges,
    Comment,
    Approve,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Verdict::RequestChanges => "Request changes",
            Verdict::Comment => "Comment",
            Verdict::Approve => "Approve",
        }
    }

    fn event(self) -> &'static str {
        match self {
            Verdict::RequestChanges => "REQUEST_CHANGES",
            Verdict::Comment => "COMMENT",
            Verdict::Approve => "APPROVE",
        }
    }
}

/// A review comment on lines `start` to `end` of `side` of a pull request's diff, 1-based and
/// inclusive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineComment {
    pub path: String,
    pub side: Side,
    pub start: u32,
    pub end: u32,
    pub body: String,
}

/// A review as GitHub's API takes it.
#[derive(Serialize)]
struct ReviewRequest<'a> {
    event: &'static str,
    #[serde(skip_serializing_if = "str::is_empty")]
    body: &'a str,
    comments: Vec<ReviewComment<'a>>,
}

#[derive(Serialize)]
struct ReviewComment<'a> {
    path: &'a str,
    side: &'static str,
    line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_side: Option<&'static str>,
    body: &'a str,
}

impl<'a> From<&'a LineComment> for ReviewComment<'a> {
    fn from(comment: &'a LineComment) -> ReviewComment<'a> {
        let side = match comment.side {
            Side::Old => "LEFT",
            Side::New => "RIGHT",
        };
        let range = comment.start < comment.end;
        ReviewComment { path: &comment.path, side, line: comment.end, start_line: range.then_some(comment.start), start_side: range.then_some(side), body: &comment.body }
    }
}

/// The diff of pull request `number` of `hosted`, as GitHub shows it.
pub fn pull_request_diff(hosted: &Hosted, number: u64) -> Result<String, String> {
    run(&["pr", "diff", &number.to_string(), &format!("--repo={}/{}", hosted.host, hosted.path)], None)
}

/// Sends a review of pull request `number` of `hosted` saying `verdict`, with `summary` as its
/// text unless it is empty, and `comments` on their lines. GitHub takes all of it or none.
pub fn submit_review(hosted: &Hosted, number: u64, verdict: Verdict, summary: &str, comments: &[LineComment]) -> Result<(), String> {
    let input = review_request(verdict, summary, comments);
    let endpoint = format!("repos/{}/pulls/{number}/reviews", hosted.path);
    run(&["api", &format!("--hostname={}", hosted.host), "--method=POST", &endpoint, "--input=-"], Some(&input)).map(drop)
}

fn review_request(verdict: Verdict, summary: &str, comments: &[LineComment]) -> String {
    let request = ReviewRequest { event: verdict.event(), body: summary, comments: comments.iter().map(ReviewComment::from).collect() };
    serde_json::to_string(&request).expect("a review is always JSON")
}

/// Runs `gh` without a terminal, so it never prompts, with `input` on its stdin, and returns what
/// it printed.
fn run(args: &[&str], input: Option<&str>) -> Result<String, String> {
    let failed = |error: io::Error| match error.kind() {
        io::ErrorKind::NotFound => "GitHub's command line, gh, isn't installed.".to_owned(),
        _ => format!("gh: {error}"),
    };
    let mut child = Command::new("gh")
        .args(args)
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("NO_COLOR", "1")
        .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(failed)?;
    // Dropping stdin once written tells gh the input ended.
    if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
        stdin.write_all(input.as_bytes()).map_err(failed)?;
    }
    let output = child.wait_with_output().map_err(failed)?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let said = refusal(&String::from_utf8_lossy(&output.stdout)).unwrap_or_else(|| String::from_utf8_lossy(&output.stderr).trim().to_owned());
    Err(if said.is_empty() { format!("gh failed: {}", output.status) } else { said })
}

/// Why GitHub's API refused a request, from the errors in the body `gh api` printed. Its stderr
/// only gives the status.
fn refusal(printed: &str) -> Option<String> {
    let body: serde_json::Value = serde_json::from_str(printed).ok()?;
    let errors: Vec<&str> = body["errors"].as_array()?.iter().filter_map(|error| error.as_str().or_else(|| error["message"].as_str())).filter(|error| !error.is_empty()).collect();
    (!errors.is_empty()).then(|| errors.join("\n"))
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

    #[test]
    fn a_refusal_gives_github_s_reasons() {
        let refused = r#"{"message":"Unprocessable Entity","errors":["Can not approve your own pull request",{"field":"line","message":"Line must be part of the diff"}],"status":"422"}"#;
        assert_eq!(refusal(refused).as_deref(), Some("Can not approve your own pull request\nLine must be part of the diff"));
        assert_eq!(refusal(r#"{"message":"Not Found"}"#), None);
        assert_eq!(refusal(r#"{"errors":[""]}"#), None);
        assert_eq!(refusal(""), None);
    }

    #[test]
    fn a_review_sends_ranges_with_their_start_and_leaves_an_empty_summary_out() {
        let comment = |start, end, side| LineComment { path: "a.rs".into(), side, start, end, body: "b".into() };
        let sent = review_request(Verdict::Approve, "", &[comment(3, 3, Side::New), comment(1, 2, Side::Old)]);
        let expected = r#"{"event":"APPROVE","comments":[{"path":"a.rs","side":"RIGHT","line":3,"body":"b"},{"path":"a.rs","side":"LEFT","line":2,"start_line":1,"start_side":"LEFT","body":"b"}]}"#;
        assert_eq!(sent, expected);
    }
}
