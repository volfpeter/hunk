//! Switching branches: what the branch switcher offers for what is typed.

use crate::overview::{Entry, RemoteBranch};

/// How many branches the switcher offers at once.
const SHOWN: usize = 10;

/// Something the switcher offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BranchChoice {
    Local(Entry),
    Remote(RemoteBranch),
    /// A new branch of this name, from `HEAD`.
    Create(String),
}

/// What the switcher offers for what has been typed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BranchOffer {
    /// Local branches, then those only on a remote, then a new branch.
    pub choices: Vec<BranchChoice>,
    /// How many more branches match than are offered.
    pub more: usize,
}

impl BranchOffer {
    /// The branches whose names hold every word `typed`, ignoring case, and a new branch named
    /// `typed` when no branch has that name and git could take it as one.
    pub fn new(typed: &str, local: &[Entry], remote: &[RemoteBranch]) -> BranchOffer {
        let words: Vec<String> = typed.split_whitespace().map(str::to_lowercase).collect();
        let matches = |name: &str| {
            let name = name.to_lowercase();
            words.iter().all(|word| name.contains(word))
        };
        let local_matches = local.iter().filter(|branch| matches(&branch.id)).cloned().map(BranchChoice::Local);
        let remote_matches = remote.iter().filter(|branch| matches(&branch.id())).cloned().map(BranchChoice::Remote);
        let mut choices: Vec<BranchChoice> = local_matches.chain(remote_matches).collect();
        let more = choices.len().saturating_sub(SHOWN);
        choices.truncate(SHOWN);

        let name = typed.trim();
        let taken = local.iter().any(|branch| branch.id == name) || remote.iter().any(|branch| branch.name == name || branch.id() == name);
        if !name.is_empty() && !name.contains(char::is_whitespace) && !taken {
            choices.push(BranchChoice::Create(name.to_owned()));
        }
        BranchOffer { choices, more }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(name: &str) -> Entry {
        Entry { id: name.to_owned(), title: name.to_owned(), detail: String::new() }
    }

    fn remote(remote: &str, name: &str) -> RemoteBranch {
        RemoteBranch { remote: remote.to_owned(), name: name.to_owned(), age: String::new() }
    }

    fn names(offer: &BranchOffer) -> Vec<String> {
        let name = |choice: &BranchChoice| match choice {
            BranchChoice::Local(branch) => branch.id.clone(),
            BranchChoice::Remote(branch) => branch.id(),
            BranchChoice::Create(name) => format!("+{name}"),
        };
        offer.choices.iter().map(name).collect()
    }

    #[test]
    fn offers_local_branches_then_remote_ones_filtered_by_every_word() {
        let (locals, remotes) = ([local("main"), local("fix/Login")], [remote("origin", "fix/logout"), remote("upstream", "docs")]);

        assert_eq!(names(&BranchOffer::new("", &locals, &remotes)), ["main", "fix/Login", "origin/fix/logout", "upstream/docs"]);
        assert_eq!(names(&BranchOffer::new("LOG fix", &locals, &remotes)), ["fix/Login", "origin/fix/logout"]);
        assert_eq!(names(&BranchOffer::new("ori", &locals, &remotes)), ["origin/fix/logout", "+ori"]);
    }

    #[test]
    fn offers_a_new_branch_only_for_a_name_no_branch_has() {
        let (locals, remotes) = ([local("main")], [remote("origin", "feature")]);

        assert_eq!(names(&BranchOffer::new(" main ", &locals, &remotes)), ["main"]);
        assert_eq!(names(&BranchOffer::new("feature", &locals, &remotes)), ["origin/feature"]);
        assert_eq!(names(&BranchOffer::new("origin/feature", &locals, &remotes)), ["origin/feature"]);
        assert_eq!(names(&BranchOffer::new("feat", &locals, &remotes)), ["origin/feature", "+feat"]);
    }

    #[test]
    fn a_long_list_is_cut_but_still_offers_a_new_branch() {
        let locals: Vec<Entry> = (0..SHOWN + 3).map(|n| local(&format!("b{n}"))).collect();

        let offer = BranchOffer::new("b", &locals, &[]);

        assert_eq!((offer.choices.len(), offer.more), (SHOWN + 1, 3));
        assert_eq!(offer.choices.last(), Some(&BranchChoice::Create("b".to_owned())));
    }
}
