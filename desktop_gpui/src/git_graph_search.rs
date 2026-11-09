//! Bounded in-memory Git Graph search. Never modifies graph history,
//! Git refs or source-control selections. Rows retain their original IDs.
use crate::git_history::{GitBranchRow, GitCommitRow, GitTagRow};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitGraphSearchScope {
    All,
    Branch,
    Tag,
    Author,
    Sha,
    Message,
    Commit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitGraphSearch {
    pub scope: GitGraphSearchScope,
    pub terms: Vec<String>,
}

impl GitGraphSearch {
    pub fn parse(raw: &str) -> Self {
        let trimmed = raw.trim();
        let lower = trimmed.to_lowercase();
        let (scope, rest) = [
            ("branch:", GitGraphSearchScope::Branch),
            ("tag:", GitGraphSearchScope::Tag),
            ("author:", GitGraphSearchScope::Author),
            ("sha:", GitGraphSearchScope::Sha),
            ("message:", GitGraphSearchScope::Message),
            ("commit:", GitGraphSearchScope::Commit),
        ]
        .into_iter()
        .find_map(|(prefix, scope)| lower.strip_prefix(prefix).map(|rest| (scope, rest)))
        .unwrap_or((GitGraphSearchScope::All, lower.as_str()));
        Self {
            scope,
            terms: rest
                .split_whitespace()
                .take(8)
                .map(|term| term.chars().take(128).collect::<String>())
                .collect(),
        }
    }

    pub fn is_active(&self) -> bool {
        self.scope != GitGraphSearchScope::All || !self.terms.is_empty()
    }

    fn contains_all(&self, fields: &[&str]) -> bool {
        self.terms.iter().all(|term| {
            fields
                .iter()
                .any(|field| field.to_lowercase().contains(term))
        })
    }

    pub fn branch(&self, row: &GitBranchRow) -> bool {
        if !matches!(
            self.scope,
            GitGraphSearchScope::All | GitGraphSearchScope::Branch
        ) {
            return false;
        }
        self.contains_all(&[
            &row.name,
            &row.tip,
            if row.remote { "remote" } else { "local" },
        ])
    }

    pub fn tag(&self, row: &GitTagRow) -> bool {
        if !matches!(
            self.scope,
            GitGraphSearchScope::All | GitGraphSearchScope::Tag
        ) {
            return false;
        }
        self.contains_all(&[&row.name, &row.tip, &row.oid])
    }

    pub fn commit(&self, row: &GitCommitRow) -> bool {
        match self.scope {
            GitGraphSearchScope::Branch | GitGraphSearchScope::Tag => false,
            GitGraphSearchScope::Author => self.contains_all(&[&row.author]),
            GitGraphSearchScope::Sha => self.contains_all(&[&row.oid, &row.short]),
            GitGraphSearchScope::Message => self.contains_all(&[&row.subject]),
            GitGraphSearchScope::All | GitGraphSearchScope::Commit => {
                let mut fields = vec![
                    row.oid.as_str(),
                    row.short.as_str(),
                    row.subject.as_str(),
                    row.author.as_str(),
                ];
                fields.extend(row.labels.iter().map(String::as_str));
                self.contains_all(&fields)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn branch() -> GitBranchRow {
        GitBranchRow {
            name: "Feature/Cleanup".into(),
            current: false,
            tip: "abcdef12".into(),
            remote: false,
            favorite: false,
        }
    }
    fn tag() -> GitTagRow {
        GitTagRow {
            name: "v1.2.3".into(),
            tip: "d00dbeef".into(),
            oid: "d00dbeef0123456789".into(),
        }
    }
    fn commit() -> GitCommitRow {
        GitCommitRow {
            oid: "abcdef1234567890".into(),
            short: "abcdef12".into(),
            subject: "Fix staged diff rendering".into(),
            author: "Alice Chen".into(),
            parents: 1,
            parent_ids: vec![],
            labels: vec!["refs/heads/main".into()],
        }
    }

    #[test]
    fn unfiltered_keeps_all_rows() {
        let search = GitGraphSearch::parse("");
        assert!(!search.is_active());
        assert!(search.branch(&branch()));
        assert!(search.tag(&tag()));
        assert!(search.commit(&commit()));
    }

    #[test]
    fn case_insensitive_terms_can_match_subject_author_hash_and_refs() {
        assert!(GitGraphSearch::parse("staged ALICE").commit(&commit()));
        assert!(GitGraphSearch::parse("CLEANUP").branch(&branch()));
        assert!(GitGraphSearch::parse("v1.2").tag(&tag()));
        assert!(GitGraphSearch::parse("d00db").tag(&tag()));
        assert!(GitGraphSearch::parse("refs/heads/main").commit(&commit()));
        assert!(GitGraphSearch::parse("missing").commit(&commit()) == false);
    }

    #[test]
    fn prefix_scopes_restrict_which_rows_are_visible() {
        let branch_query = GitGraphSearch::parse("branch:cleanup");
        assert!(branch_query.branch(&branch()));
        assert!(!branch_query.commit(&commit()));
        assert!(!branch_query.tag(&tag()));
        let tag_query = GitGraphSearch::parse("tag:v1");
        assert!(tag_query.tag(&tag()));
        assert!(!tag_query.branch(&branch()));
        let author_query = GitGraphSearch::parse("author:alice");
        assert!(author_query.commit(&commit()));
        assert!(!author_query.branch(&branch()));
        let sha_query = GitGraphSearch::parse("sha:ABCDEF12");
        assert!(sha_query.commit(&commit()));
        assert!(!sha_query.tag(&tag()));
        assert!(GitGraphSearch::parse("message:staged diff").commit(&commit()));
        assert!(!GitGraphSearch::parse("message:alice").commit(&commit()));
        assert!(GitGraphSearch::parse("commit:alice").commit(&commit()));
    }

    #[test]
    fn prefix_without_terms_only_shows_requested_category() {
        assert!(GitGraphSearch::parse("tag:").tag(&tag()));
        assert!(!GitGraphSearch::parse("tag:").commit(&commit()));
        assert!(GitGraphSearch::parse("branch:").branch(&branch()));
    }
}
