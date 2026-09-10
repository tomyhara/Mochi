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

//! Turning what is on disk into index rows (FR-2.3, FR-2.9, FR-3.x).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::adapter::{EnvSource, ToolAdapter};
use crate::index::{Index, RepositoryRecord, SessionQuery, SessionRecord, SessionStatus};
use crate::model::{ParseStatus, ParsedSession, ToolId};
use crate::paths::normalize_host;
use crate::repo::{identify, GitProbe};
use crate::Result;

/// What one scan did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScanReport {
    pub discovered: usize,
    /// Sessions read because they were new or had changed.
    pub parsed: usize,
    /// Sessions left alone because size and mtime matched the index (FR-2.3).
    pub unchanged: usize,
    /// Sessions whose source file has disappeared since the last scan.
    pub archived: usize,
    /// Sessions that could not be read at all. Isolated, never fatal.
    pub failed: usize,
    pub repositories: usize,
    pub errors: Vec<ScanError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanError {
    pub tool: ToolId,
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct ScanOptions {
    /// Only these tools, or all of them when empty (FR-1.5).
    pub tools: Vec<ToolId>,
    /// Re-read every session even if it looks unchanged (FR-2.10).
    pub force: bool,
    /// Group worktrees and second clones with their origin (FR-3.4).
    pub bundle_clones: bool,
}

impl Default for ScanOptions {
    fn default() -> Self {
        ScanOptions {
            tools: Vec::new(),
            force: false,
            bundle_clones: true,
        }
    }
}

/// Runs a scan against an index.
pub struct Scanner<'a> {
    adapters: Vec<Box<dyn ToolAdapter>>,
    env: EnvSource,
    probe: &'a dyn GitProbe,
    options: ScanOptions,
}

impl<'a> Scanner<'a> {
    pub fn new(env: EnvSource, probe: &'a dyn GitProbe, options: ScanOptions) -> Scanner<'a> {
        let adapters = crate::adapter::all()
            .into_iter()
            .filter(|adapter| options.tools.is_empty() || options.tools.contains(&adapter.id()))
            .collect();
        Scanner {
            adapters,
            env,
            probe,
            options,
        }
    }

    /// Discover, parse what changed, resolve repositories, write rows.
    ///
    /// A failure on one session is recorded in the report and the scan carries
    /// on (FR-2.7, NFR-2.2).
    pub fn run(&self, index: &mut Index) -> Result<ScanReport> {
        let mut report = ScanReport::default();
        let mut repos = RepoCache::new(self.probe, self.options.bundle_clones);
        let mut seen: HashMap<ToolId, HashSet<String>> = HashMap::new();

        for adapter in &self.adapters {
            let tool = adapter.id();
            let found = seen.entry(tool).or_default();

            for root in adapter.data_roots(&self.env) {
                let discovered = match adapter.discover(&root) {
                    Ok(discovered) => discovered,
                    Err(error) => {
                        // A whole store Mochi cannot read — an OpenCode
                        // install on the newer SQLite layout, say. Recorded and
                        // stepped over; the other tools still index.
                        report.errors.push(ScanError {
                            tool,
                            path: root.clone(),
                            message: error.to_string(),
                        });
                        continue;
                    }
                };

                for session in discovered {
                    let source = path_key(&session.source_path);
                    if !found.insert(source.clone()) {
                        // Two roots pointing at the same tree.
                        continue;
                    }
                    report.discovered += 1;

                    if !self.options.force && is_unchanged(index, &source, &session)? {
                        report.unchanged += 1;
                        continue;
                    }

                    match adapter.parse(&session) {
                        Ok(parsed) => {
                            if parsed.parse_status == ParseStatus::Failed {
                                report.failed += 1;
                                if let Some(reason) = parsed.parse_error.as_deref() {
                                    report.errors.push(ScanError {
                                        tool,
                                        path: session.source_path.clone(),
                                        message: reason.to_string(),
                                    });
                                }
                            }
                            self.store(index, &parsed, &mut repos)?;
                            report.parsed += 1;
                        }
                        Err(error) => {
                            // The file could not be opened at all. Record it
                            // rather than letting it stop the scan.
                            report.failed += 1;
                            report.errors.push(ScanError {
                                tool,
                                path: session.source_path.clone(),
                                message: error.to_string(),
                            });
                        }
                    }
                }
            }
        }

        report.archived = self.archive_missing(index, &seen)?;
        report.repositories = index.list_repositories()?.len();
        Ok(report)
    }

    /// Write one parsed session and its transcript.
    fn store(
        &self,
        index: &mut Index,
        parsed: &ParsedSession,
        repos: &mut RepoCache,
    ) -> Result<()> {
        let cwd_path = parsed.cwd.as_deref().map(PathBuf::from);
        let cwd_exists = cwd_path.as_deref().map(Path::is_dir).unwrap_or(false);

        let repo_id = match (&cwd_path, cwd_exists) {
            // FR-3.6: a working directory that is gone leaves the session in
            // the "no repository" group rather than guessing at an ancestor.
            (Some(cwd), true) => repos.resolve(cwd, index)?,
            _ => None,
        };

        let record = SessionRecord {
            id: 0,
            tool: parsed.tool,
            native_id: parsed.native_id.clone(),
            repo_id,
            cwd: parsed.cwd.clone(),
            git_branch: parsed.git_branch.clone(),
            title: parsed.title.clone(),
            model: parsed.model.clone(),
            started_at: parsed.started_at,
            updated_at: parsed.updated_at.or(parsed.started_at),
            message_count: parsed.messages.len() as i64,
            tokens_in: parsed.tokens_in,
            tokens_out: parsed.tokens_out,
            // Live-session detection is the integrated terminal's job
            // (FR-7.8e); until it exists nothing here can honestly claim a
            // session is running.
            status: SessionStatus::Unknown,
            source_path: path_key(&parsed.source_path),
            source_size: parsed.source_size as i64,
            source_mtime: parsed.source_mtime,
            schema_version: parsed.schema_version.clone(),
            parse_status: parsed.parse_status,
            cwd_exists,
            parse_error: parsed.parse_error.clone(),
        };

        let session_id = index.upsert_session(&record)?;
        index.replace_messages(session_id, &parsed.messages)?;
        Ok(())
    }

    /// Sessions the index knows about whose file is no longer there.
    ///
    /// Only tools that were actually scanned are considered, so turning a tool
    /// off does not archive its history, and each file is checked directly:
    /// a store that failed to list must not archive everything inside it.
    fn archive_missing(
        &self,
        index: &mut Index,
        seen: &HashMap<ToolId, HashSet<String>>,
    ) -> Result<usize> {
        let mut archived = 0;
        for (tool, found) in seen {
            let known = index.list_sessions(&SessionQuery {
                tool: Some(*tool),
                limit: Some(u32::MAX),
                ..Default::default()
            })?;
            for session in known {
                if session.parse_status == ParseStatus::Archived {
                    continue;
                }
                if found.contains(&session.source_path) {
                    continue;
                }
                if Path::new(&session.source_path).exists() {
                    continue;
                }
                index.mark_archived(session.id)?;
                archived += 1;
            }
        }
        Ok(archived)
    }
}

/// Whether the indexed copy is still current (FR-2.3).
fn is_unchanged(index: &Index, source: &str, session: &crate::model::SessionRef) -> Result<bool> {
    let Some(existing) = index.session_by_source(source)? else {
        return Ok(false);
    };
    if existing.parse_status == ParseStatus::Archived {
        // The file came back. Read it again.
        return Ok(false);
    }
    Ok(existing.source_size == session.source_size as i64
        && existing.source_mtime == session.source_mtime)
}

/// Resolves working directories to repository rows, remembering answers.
///
/// Without the cache every session in a repository would spawn its own `git`
/// processes, which for ten thousand sessions is the difference between a
/// scan and a coffee break.
struct RepoCache<'a> {
    probe: &'a dyn GitProbe,
    bundle_clones: bool,
    by_directory: HashMap<String, Option<i64>>,
}

impl<'a> RepoCache<'a> {
    fn new(probe: &'a dyn GitProbe, bundle_clones: bool) -> RepoCache<'a> {
        RepoCache {
            probe,
            bundle_clones,
            by_directory: HashMap::new(),
        }
    }

    fn resolve(&mut self, cwd: &Path, index: &mut Index) -> Result<Option<i64>> {
        let cache_key = normalize_host(&cwd.to_string_lossy()).key().to_string();
        if let Some(cached) = self.by_directory.get(&cache_key) {
            return Ok(*cached);
        }

        let resolved = match identify(cwd, self.probe) {
            None => None,
            Some((root, identity)) => {
                // FR-3.4: with bundling off, two checkouts of one history stay
                // separate, so identity has to fall back to the path.
                let identity_key = if self.bundle_clones {
                    identity.key()
                } else {
                    format!("path:{}", identity.path_key)
                };
                let display = normalize_host(&root.root.to_string_lossy());
                let record = RepositoryRecord {
                    id: 0,
                    identity_key,
                    display_name: repository_name(&root.root),
                    root_path: display.display().to_string(),
                    path_key: identity.path_key.clone(),
                    remote_url: identity.remote_url.clone(),
                    root_commit: identity.root_commit.clone(),
                    is_worktree: root.is_worktree,
                    parent_repo_id: None,
                    hidden: false,
                    color_label: None,
                };
                Some(index.upsert_repository(&record)?)
            }
        };

        self.by_directory.insert(cache_key, resolved);
        Ok(resolved)
    }
}

/// The last component of the path, which is what a user calls the repository.
fn repository_name(root: &Path) -> String {
    root.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.to_string_lossy().into_owned())
}

/// The form of a path used as a database key: normalised for comparison but
/// still openable.
fn path_key(path: &Path) -> String {
    normalize_host(&path.to_string_lossy())
        .display()
        .to_string()
}
