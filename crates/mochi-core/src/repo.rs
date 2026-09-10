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
pub fn find_repo_root(start: &Path) -> Option<RepoRoot> {
    todo!()
}

/// Strip everything that varies between two ways of writing the same remote:
/// scheme, credentials, port, `.git` suffix, trailing slash, host case, and
/// the `git@host:path` form.
pub fn normalize_remote_url(url: &str) -> String {
    todo!()
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
        todo!()
    }
}

/// Whether two working trees should be grouped together.
///
/// `bundle_clones` is the user's setting from FR-3.4: with it off, two clones
/// of the same remote stay separate entries.
pub fn same_repository(a: &RepoIdentity, b: &RepoIdentity, bundle_clones: bool) -> bool {
    todo!()
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

impl GitProbe for GitCli {
    fn facts(&self, root: &Path) -> GitFacts {
        todo!()
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
    todo!()
}
