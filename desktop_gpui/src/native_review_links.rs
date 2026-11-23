//! Read-only provider browser link for the Native PR sidebar.
//! Classic's full review controller handles API auth, list/compose and reviews;
//! GPUI intentionally only deep-links to the repository's hosted PR page.

fn safe_repo_path(value: &str) -> Option<&str> {
    let path = value
        .trim_matches('/')
        .strip_suffix(".git")
        .unwrap_or(value.trim_matches('/'));
    let segments: Vec<_> = path.split('/').collect();
    if segments.len() < 2 || segments.len() > 12 {
        return None;
    }
    for segment in segments {
        if segment.is_empty()
            || matches!(segment, "." | "..")
            || !segment
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
        {
            return None;
        }
    }
    Some(path)
}

fn safe_host(value: &str) -> Option<&str> {
    let host = value.trim();
    if host.is_empty()
        || host.contains(['@', '/', '\\', '?', '#', '%'])
        || host.chars().any(|ch| ch.is_whitespace() || ch.is_control())
    {
        return None;
    }
    let hostname = host.split_once(':').map_or(host, |(name, _)| name);
    if !hostname.contains('.')
        || hostname.starts_with('.')
        || hostname.ends_with('.')
        || !hostname
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-'))
    {
        return None;
    }
    if let Some((_, port)) = host.split_once(':') {
        if port.parse::<u16>().ok().filter(|port| *port != 0).is_none() {
            return None;
        }
    }
    Some(host)
}

/// Accept HTTPS, standard SSH and scp-style Git remotes. Never forward
/// arbitrary schemes, credentials, fragments or a remote's query string.
fn repository_host_and_path(remote: &str) -> Option<(&str, &str)> {
    let remote = remote.trim();
    let (host, path) = if let Some(remainder) = remote.strip_prefix("https://") {
        remainder.split_once('/')?
    } else if let Some(remainder) = remote.strip_prefix("ssh://git@") {
        let (authority, path) = remainder.split_once('/')?;
        safe_host(authority)?; // Validate an SSH port before removing it.
        let host = authority
            .split_once(':')
            .map_or(authority, |(host, _)| host);
        (host, path)
    } else if let Some(remainder) = remote.strip_prefix("git@") {
        remainder.split_once(':')?
    } else {
        return None;
    };
    let host = safe_host(host)?;
    let path = safe_repo_path(path)?;
    Some((host, path))
}

pub fn repository_home_url(remote: &str) -> Option<String> {
    let (host, path) = repository_host_and_path(remote)?;
    Some(format!("https://{host}/{path}"))
}

pub fn repository_pull_requests_url(remote: &str) -> Option<String> {
    let (host, path) = repository_host_and_path(remote)?;
    let is_gitlab = host.eq_ignore_ascii_case("gitlab.com")
        || host.to_ascii_lowercase().starts_with("gitlab.")
        || host.to_ascii_lowercase().contains(".gitlab.");
    let suffix = if is_gitlab {
        "/-/merge_requests"
    } else {
        "/pulls"
    };
    Some(format!("https://{host}/{path}{suffix}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidebar_link_opens_the_repository_home_and_rejects_credentials() {
        assert_eq!(
            repository_home_url("git@github.com:org/worktree.git").as_deref(),
            Some("https://github.com/org/worktree"),
        );
        assert_eq!(
            repository_home_url("https://token@github.com/org/worktree"),
            None
        );
    }

    #[test]
    fn github_gitea_and_gitlab_https_ssh_remotes() {
        assert_eq!(
            repository_pull_requests_url("https://github.com/a/project.git").as_deref(),
            Some("https://github.com/a/project/pulls"),
        );
        assert_eq!(
            repository_pull_requests_url("git@git.neo-ai.one:neo/alera.git").as_deref(),
            Some("https://git.neo-ai.one/neo/alera/pulls"),
        );
        assert_eq!(
            repository_pull_requests_url("ssh://git@gitlab.example.com:2222/group/sub/repo.git")
                .as_deref(),
            Some("https://gitlab.example.com/group/sub/repo/-/merge_requests"),
        );
    }

    #[test]
    fn disallow_injected_credentials_protocol_and_traversal() {
        for bad in [
            "file:///temp/repo",
            "http://host.example/team/repo",
            "https://token@github.com/org/repo",
            "https://github.com/org/../evil",
            "https://github.com/org/repo?token=secret",
            "git@host.example:org/%2e%2e",
            "git@host.example:org/repo#fragment",
            "https://localhost/repo/name",
            "ssh://git@host.example:wrong/repo/name",
        ] {
            assert_eq!(repository_pull_requests_url(bad), None, "remote={bad}");
        }
    }

    #[test]
    fn reject_missing_repo_segments_and_unknown_local_remotes() {
        for remote in [
            "",
            "git@host.example:repo.git",
            "../repo",
            "https://host.example/repo.git",
            "C:\\repos\\project",
            "ssh://git@host.example",
        ] {
            assert_eq!(
                repository_pull_requests_url(remote),
                None,
                "remote={remote}"
            );
        }
    }
}
