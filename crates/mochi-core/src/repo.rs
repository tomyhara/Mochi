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

//! Resolving a working directory to a git repository, and deciding when two
//! repositories are the same one (FR-3.1 – FR-3.5).

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::paths::normalize_host;

/// Where a working directory's repository lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoRoot {
    /// Root of the working tree.
    pub root: PathBuf,
    /// The `.git` directory for this working tree.
    pub git_dir: PathBuf,
    /// The shared directory a worktree points back at. Equals `git_dir` for an
    /// ordinary clone.
    pub common_dir: PathBuf,
    pub is_worktree: bool,
}

/// Walk up from `start` looking for a working tree. Handles `.git` being a
/// file, which is how `git worktree` and submodules record their real git
/// directory (FR-3.2).
///
/// A directory that does not exist has no repository, even if one of its
/// ancestors does: a session whose working directory was deleted belongs in
/// the "no repository" group, not in whatever repository happens to sit above
/// where it used to be (FR-3.6).
pub fn find_repo_root(start: &Path) -> Option<RepoRoot> {
    if !start.is_dir() {
        return None;
    }

    let mut current = Some(start);
    while let Some(dir) = current {
        let dot_git = dir.join(".git");
        if dot_git.is_dir() {
            return Some(RepoRoot {
                root: dir.to_path_buf(),
                git_dir: dot_git.clone(),
                common_dir: dot_git,
                is_worktree: false,
            });
        }
        if dot_git.is_file() {
            let git_dir = read_gitdir_pointer(&dot_git, dir)?;
            let common_dir = read_common_dir(&git_dir);
            return Some(RepoRoot {
                root: dir.to_path_buf(),
                git_dir,
                common_dir,
                is_worktree: true,
            });
        }
        current = dir.parent();
    }
    None
}

/// A `.git` file holds a single line: `gitdir: <path>`.
fn read_gitdir_pointer(dot_git: &Path, working_tree: &Path) -> Option<PathBuf> {
    let contents = std::fs::read_to_string(dot_git).ok()?;
    let pointer = contents
        .lines()
        .find_map(|line| line.trim().strip_prefix("gitdir:"))?
        .trim();
    if pointer.is_empty() {
        return None;
    }
    let path = PathBuf::from(pointer);
    Some(if path.is_absolute() {
        path
    } else {
        lexical_join(working_tree, &path)
    })
}

/// A linked worktree's git directory holds a `commondir` file pointing at the
/// main repository's `.git`.
fn read_common_dir(git_dir: &Path) -> PathBuf {
    let Ok(contents) = std::fs::read_to_string(git_dir.join("commondir")) else {
        return git_dir.to_path_buf();
    };
    let pointer = contents.trim();
    if pointer.is_empty() {
        return git_dir.to_path_buf();
    }
    let path = PathBuf::from(pointer);
    if path.is_absolute() {
        path
    } else {
        lexical_join(git_dir, &path)
    }
}

/// Join without touching the filesystem, resolving `.` and `..` as text. The
/// paths involved often refer to another machine or to a directory that is
/// gone, so `canonicalize` is not available.
fn lexical_join(base: &Path, relative: &Path) -> PathBuf {
    let combined = base.join(relative);
    PathBuf::from(
        normalize_host(&combined.to_string_lossy())
            .display()
            .to_string(),
    )
}

/// Strip everything that varies between two ways of writing the same remote:
/// scheme, credentials, port, `.git` suffix, trailing slash, host case, and
/// the `git@host:path` form.
pub fn normalize_remote_url(url: &str) -> String {
    let mut rest = url.trim();

    // scheme://
    if let Some(index) = rest.find("://") {
        rest = &rest[index + 3..];
    }

    // Credentials. Anything before the last @ of the authority is identity,
    // not location, and one of them may be a token (NFR-3.3).
    let authority_end = rest.find('/').unwrap_or(rest.len());
    if let Some(at) = rest[..authority_end].rfind('@') {
        rest = &rest[at + 1..];
    }

    let mut path = rest.replace('\\', "/");

    // Split host from path. The scp-like form `host:owner/repo` uses a colon
    // where a URL would use a slash; a numeric part after the colon is a port.
    let (host, tail) = match path.find(['/', ':']) {
        Some(index) => {
            let separator = path.as_bytes()[index];
            let host = path[..index].to_string();
            let mut tail = path[index + 1..].to_string();
            if separator == b':' {
                // Drop a port, keep an scp-style path.
                let digits_end = tail.find('/').unwrap_or(tail.len());
                if !tail[..digits_end].is_empty()
                    && tail[..digits_end].chars().all(|c| c.is_ascii_digit())
                {
                    tail = tail[digits_end..].trim_start_matches('/').to_string();
                }
            }
            (host, tail)
        }
        None => (path.clone(), String::new()),
    };

    path = if tail.is_empty() {
        host.to_lowercase()
    } else {
        format!("{}/{}", host.to_lowercase(), tail.trim_start_matches('/'))
    };

    let path = path.trim_end_matches('/');
    path.strip_suffix(".git")
        .unwrap_or(path)
        .trim_end_matches('/')
        .to_string()
}

/// The evidence Mochi has about which repository something belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RepoIdentity {
    /// Hash of the first commit. The strongest signal: it survives renames,
    /// moves and re-clones.
    pub root_commit: Option<String>,
    /// Normalised `origin` URL.
    pub remote_url: Option<String>,
    /// Normalised absolute path, always present.
    pub path_key: String,
}

impl RepoIdentity {
    /// Identity key, strongest available evidence first (FR-3.3).
    pub fn key(&self) -> String {
        if let Some(commit) = self.root_commit.as_deref().filter(|c| !c.is_empty()) {
            return format!("commit:{commit}");
        }
        if let Some(remote) = self.remote_url.as_deref().filter(|r| !r.is_empty()) {
            return format!("remote:{remote}");
        }
        format!("path:{}", self.path_key)
    }
}

/// Whether two working trees should be grouped together.
///
/// `bundle_clones` is the user's setting from FR-3.4: with it off, two clones
/// of the same remote stay separate entries.
pub fn same_repository(a: &RepoIdentity, b: &RepoIdentity, bundle_clones: bool) -> bool {
    if !bundle_clones {
        return a.path_key == b.path_key;
    }
    a.key() == b.key()
}

/// Facts read out of a repository by shelling out to `git`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitFacts {
    pub root_commit: Option<String>,
    pub remote_url: Option<String>,
    pub branch: Option<String>,
}

/// Reads repository facts. Behind a trait so that tests, and callers that must
/// not block, can substitute something cheaper.
pub trait GitProbe {
    fn facts(&self, root: &Path) -> GitFacts;
}

/// Runs the real `git` binary, with arguments passed as a vector (NFR-3.6).
#[derive(Debug, Default, Clone, Copy)]
pub struct GitCli;

impl GitCli {
    fn run(root: &Path, args: &[&str]) -> Option<String> {
        let output = Command::new("git")
            .args(args)
            .current_dir(root)
            // Never let git stop for a credential prompt during a background
            // scan, and keep its output parseable.
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("LC_ALL", "C")
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    }
}

impl GitProbe for GitCli {
    fn facts(&self, root: &Path) -> GitFacts {
        // A repository can have more than one root commit (an unrelated
        // history was merged in). Sorting makes the choice the same in every
        // clone, which is what identity depends on.
        let root_commit =
            GitCli::run(root, &["rev-list", "--max-parents=0", "HEAD"]).and_then(|out| {
                let mut roots: Vec<&str> = out
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .collect();
                roots.sort_unstable();
                roots.first().map(|s| s.to_string())
            });

        let remote_url = GitCli::run(root, &["remote", "get-url", "origin"])
            .map(|url| normalize_remote_url(&url))
            .filter(|url| !url.is_empty());

        let branch = GitCli::run(root, &["rev-parse", "--abbrev-ref", "HEAD"])
            .filter(|branch| branch != "HEAD");

        GitFacts {
            root_commit,
            remote_url,
            branch,
        }
    }
}

/// A probe that knows nothing. Used when `git` is unavailable; identity then
/// falls back to the path (FR-3.3 ③).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoGit;

impl GitProbe for NoGit {
    fn facts(&self, _root: &Path) -> GitFacts {
        GitFacts::default()
    }
}

/// Resolve the repository for a session's working directory.
///
/// Returns `None` when the directory is not in a repository or no longer
/// exists — those sessions go to the "no repository" group (FR-3.6).
pub fn identify(cwd: &Path, probe: &dyn GitProbe) -> Option<(RepoRoot, RepoIdentity)> {
    let root = find_repo_root(cwd)?;
    let facts = probe.facts(&root.root);
    let identity = RepoIdentity {
        root_commit: facts.root_commit,
        remote_url: facts.remote_url,
        path_key: normalize_host(&root.root.to_string_lossy())
            .key()
            .to_string(),
    };
    Some((root, identity))
}
