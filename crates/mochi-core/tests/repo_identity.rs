// Copyright 2026 The Mochi Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Repository resolution and identity (FR-3.1 – FR-3.5).

mod support;

use std::fs;

use mochi_core::paths::{normalize_host, NormalizeOptions};
use mochi_core::repo::{
    find_repo_root, identify, normalize_remote_url, same_repository, GitCli, GitProbe, NoGit,
    RepoIdentity,
};

fn ident(commit: Option<&str>, remote: Option<&str>, path: &str) -> RepoIdentity {
    RepoIdentity {
        root_commit: commit.map(str::to_string),
        remote_url: remote.map(str::to_string),
        path_key: normalize_host(path).key().to_string(),
    }
}

#[test]
fn remote_urls_in_every_form_normalise_alike() {
    let expected = "github.com/example/my-repo";
    for url in [
        "https://github.com/example/my-repo.git",
        "https://github.com/example/my-repo",
        "https://github.com/example/my-repo/",
        "git@github.com:example/my-repo.git",
        "ssh://git@github.com/example/my-repo.git",
        "ssh://git@github.com:22/example/my-repo.git",
        "git://github.com/example/my-repo.git",
        "https://GitHub.com/example/my-repo.git",
    ] {
        assert_eq!(normalize_remote_url(url), expected, "for {url}");
    }
}

#[test]
fn credentials_never_survive_url_normalisation() {
    // A remote can carry a token. It must not end up in the index, in a log or
    // on screen (NFR-3.3, NFR-5.5).
    let normalised = normalize_remote_url(
        "https://someone:ghp_EXAMPLEEXAMPLEEXAMPLE@github.com/example/my-repo.git",
    );
    assert_eq!(normalised, "github.com/example/my-repo");
    assert!(!normalised.contains("ghp_"));
    assert!(!normalised.contains("someone"));
}

#[test]
fn path_case_in_remote_is_preserved() {
    // Only the host is case-insensitive; a path can be case sensitive.
    assert_eq!(
        normalize_remote_url("https://example.com/Team/Repo.git"),
        "example.com/Team/Repo"
    );
}

#[test]
fn identity_prefers_the_first_commit() {
    let id = ident(Some("abc123"), Some("github.com/example/r"), "/home/user/r");
    assert_eq!(id.key(), "commit:abc123");
}

#[test]
fn identity_falls_back_to_remote_then_path() {
    let by_remote = ident(None, Some("github.com/example/r"), "/home/user/r");
    assert_eq!(by_remote.key(), "remote:github.com/example/r");

    let by_path = ident(None, None, "/home/user/r");
    assert_eq!(by_path.key(), "path:/home/user/r");
}

#[test]
fn a_moved_clone_is_still_the_same_repository() {
    // FR-3.3: this is the whole point — the directory moved, the history did not.
    let before = ident(
        Some("abc123"),
        Some("github.com/example/r"),
        "/home/user/old/r",
    );
    let after = ident(
        Some("abc123"),
        Some("github.com/example/r"),
        "/home/user/new/r",
    );
    assert!(same_repository(&before, &after, true));
}

#[test]
fn two_clones_bundle_only_when_the_user_asked_for_it() {
    // FR-3.4: same history, two checkouts. Bundling is a setting.
    let a = ident(Some("abc123"), Some("github.com/example/r"), "/home/user/a");
    let b = ident(Some("abc123"), Some("github.com/example/r"), "/home/user/b");
    assert!(same_repository(&a, &b, true));
    assert!(!same_repository(&a, &b, false));
}

#[test]
fn unrelated_repositories_never_merge() {
    let a = ident(Some("aaa"), Some("github.com/example/a"), "/home/user/a");
    let b = ident(Some("bbb"), Some("github.com/example/b"), "/home/user/b");
    assert!(!same_repository(&a, &b, true));

    // Same path evidence only, different paths.
    let c = ident(None, None, "/home/user/a");
    let d = ident(None, None, "/home/user/b");
    assert!(!same_repository(&c, &d, true));
}

#[test]
fn a_fork_with_a_shared_first_commit_still_groups_by_commit() {
    // Deliberate: a fork shares its root commit, and grouping the two together
    // is the behaviour FR-3.3 asks for. Recorded here so a change is a choice.
    let upstream = ident(
        Some("abc123"),
        Some("github.com/example/r"),
        "/home/user/upstream",
    );
    let fork = ident(Some("abc123"), Some("github.com/me/r"), "/home/user/fork");
    assert!(same_repository(&upstream, &fork, true));
}

#[test]
fn repo_root_is_found_from_a_nested_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("project");
    fs::create_dir_all(root.join(".git")).unwrap();
    let nested = root.join("src/deep/deeper");
    fs::create_dir_all(&nested).unwrap();

    let found = find_repo_root(&nested).expect("repository root");
    assert_eq!(
        normalize_host(&found.root.to_string_lossy()).key(),
        normalize_host(&root.to_string_lossy()).key()
    );
    assert!(!found.is_worktree);
    assert_eq!(found.git_dir, found.common_dir);
}

#[test]
fn a_worktree_points_back_at_the_main_repository() {
    // FR-3.2: in a worktree, .git is a file holding "gitdir: <path>".
    let tmp = tempfile::tempdir().unwrap();
    let main = tmp.path().join("main");
    let git_dir = main.join(".git");
    fs::create_dir_all(git_dir.join("worktrees/feature")).unwrap();
    fs::write(git_dir.join("worktrees/feature/commondir"), "../..\n").unwrap();

    let wt = tmp.path().join("feature");
    fs::create_dir_all(&wt).unwrap();
    fs::write(
        wt.join(".git"),
        format!("gitdir: {}\n", git_dir.join("worktrees/feature").display()),
    )
    .unwrap();

    let found = find_repo_root(&wt).expect("worktree root");
    assert!(found.is_worktree, "a .git file means a linked worktree");
    assert_eq!(
        normalize_host(&found.common_dir.to_string_lossy()).key(),
        normalize_host(&git_dir.to_string_lossy()).key(),
        "the worktree must resolve back to the main .git directory"
    );
}

#[test]
fn a_directory_outside_any_repository_has_no_root() {
    let tmp = tempfile::tempdir().unwrap();
    let plain = tmp.path().join("just-a-folder");
    fs::create_dir_all(&plain).unwrap();
    assert!(find_repo_root(&plain).is_none());
    assert!(identify(&plain, &NoGit).is_none());
}

#[test]
fn a_vanished_directory_has_no_root() {
    // FR-3.6: the session survives its working directory. It goes into the
    // "no repository" group rather than being dropped.
    let missing = std::path::Path::new("/definitely/not/here/at/all");
    assert!(find_repo_root(missing).is_none());
}

#[test]
fn git_facts_come_from_a_real_repository() {
    if !support::git_available() {
        eprintln!("skipping: git not on PATH");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("real-repo");
    let root_commit = support::init_repo(&repo, Some("git@github.com:example/real-repo.git"));

    let facts = GitCli.facts(&repo);
    assert_eq!(facts.root_commit.as_deref(), Some(root_commit.as_str()));
    assert_eq!(
        facts.remote_url.as_deref(),
        Some("github.com/example/real-repo")
    );
    assert_eq!(facts.branch.as_deref(), Some("main"));

    let (root, identity) = identify(&repo, &GitCli).expect("identified");
    assert!(!root.is_worktree);
    assert_eq!(identity.key(), format!("commit:{root_commit}"));
}

#[test]
fn identify_works_without_git_installed() {
    // NFR-2: no git binary is a degraded mode, not a failure.
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("project");
    fs::create_dir_all(repo.join(".git")).unwrap();

    let (_, identity) = identify(&repo, &NoGit).expect("identified by path alone");
    assert!(identity.root_commit.is_none());
    assert!(identity.key().starts_with("path:"));
}

#[test]
fn identity_path_keys_use_normalised_paths() {
    let a = ident(None, None, "/home/user/app/");
    let b = ident(None, None, "/home/user/./app");
    assert_eq!(a.key(), b.key());
    let _ = NormalizeOptions::for_host();
}
