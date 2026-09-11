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

//! What the window draws, as plain data.
//!
//! The panels never touch the index: they read this, which the worker thread
//! builds (`worker.rs`). Keeping the two apart is what makes the interesting
//! decisions — which sessions a search matches, how repositories are grouped,
//! whether a session can be resumed — testable without a window, and it is
//! what stops a slow query from being run inside a paint.
//!
//! Text that came out of a session file arrives here already masked, because
//! masking belongs where the files are read (NFR-3.3). A `Snapshot` is
//! therefore exactly as safe to show as its `masked` flag says.

use std::path::PathBuf;

use mochi_core::index::{IndexStats, SessionRecord};
use mochi_core::model::{ParseStatus, Role, ToolId};

/// Everything the window has loaded, apart from transcripts.
///
/// Transcripts are fetched per session on selection rather than held here: a
/// user with ten thousand sessions has gigabytes of them, and the previous
/// shell's habit of shipping the lot as one document is exactly what NFR-1.7
/// (400MB) rules out.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub masked: bool,
    pub repositories: Vec<RepoView>,
    /// Most recently touched first (FR-4.4).
    pub sessions: Vec<SessionView>,
    pub stats: IndexStats,
    pub index_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoView {
    pub id: i64,
    pub display_name: String,
    pub root_path: String,
    pub remote_url: Option<String>,
    pub is_worktree: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionView {
    pub id: i64,
    pub tool: ToolId,
    pub native_id: String,
    pub repo_id: Option<i64>,
    pub title: Option<String>,
    pub model: Option<String>,
    pub cwd: Option<String>,
    pub cwd_exists: bool,
    pub git_branch: Option<String>,
    pub started_at: Option<i64>,
    pub updated_at: Option<i64>,
    pub message_count: i64,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub source_path: String,
    pub source_size: i64,
    pub parse_status: ParseStatus,
    pub parse_error: Option<String>,
    /// The command, or why there is not one.
    pub resume: Result<String, String>,
}

impl SessionView {
    /// What to call this session in a list. A session that never got a title
    /// is still a session, and its native id is what the user would type.
    pub fn label(&self) -> &str {
        match self.title.as_deref() {
            Some(title) if !title.trim().is_empty() => title,
            _ => &self.native_id,
        }
    }

    /// The short warnings that belong next to a row (FR-4.2).
    pub fn notes(&self) -> Vec<&'static str> {
        let mut notes = Vec::new();
        match self.parse_status {
            ParseStatus::Archived => notes.push("archived"),
            ParseStatus::Failed => notes.push("unreadable"),
            ParseStatus::Partial => notes.push("partial"),
            ParseStatus::Ok => {}
        }
        if !self.cwd_exists && self.cwd.is_some() {
            notes.push("no working directory");
        }
        notes
    }
}

/// Build a view from an index record and the resume answer for it.
pub fn session_view(
    record: SessionRecord,
    title: Option<String>,
    resume: Result<String, String>,
) -> SessionView {
    SessionView {
        id: record.id,
        tool: record.tool,
        native_id: record.native_id,
        repo_id: record.repo_id,
        title,
        model: record.model,
        cwd: record.cwd,
        cwd_exists: record.cwd_exists,
        git_branch: record.git_branch,
        started_at: record.started_at,
        updated_at: record.updated_at,
        message_count: record.message_count,
        tokens_in: record.tokens_in,
        tokens_out: record.tokens_out,
        source_path: record.source_path,
        source_size: record.source_size,
        parse_status: record.parse_status,
        parse_error: record.parse_error,
        resume,
    }
}

/// One transcript entry, masked and ready to draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageView {
    pub seq: i64,
    pub role: Role,
    pub content: String,
    pub tool_name: Option<String>,
    pub timestamp: Option<i64>,
    pub raw: Option<String>,
}

/// A full-text match, for the search results pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HitView {
    pub session_id: i64,
    pub tool: ToolId,
    pub repo_display: Option<String>,
    pub session_title: Option<String>,
    pub message_seq: i64,
    pub timestamp: Option<i64>,
    /// Already masked by the index, with the match markers still in it.
    pub snippet: String,
}

/// A repository heading in the sidebar, with the sessions under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// Stable across redraws, so an open group stays open.
    pub key: String,
    pub name: String,
    pub path: Option<String>,
    /// Indices into [`Snapshot::sessions`], keeping the snapshot's order.
    pub sessions: Vec<usize>,
}

/// Group the sessions by repository, keeping only those the filter matches.
///
/// Sessions whose working directory is gone, or that were never in a
/// repository, are grouped separately rather than dropped: FR-3.6 is explicit
/// that they stay visible, and they are usually the ones being looked for.
///
/// With no filter, a repository with no sessions still gets a heading — it is
/// a fact about the index worth seeing. With a filter, empty groups would be
/// noise, so they go.
pub fn groups(snapshot: &Snapshot, filter: &str) -> Vec<Group> {
    let needle = filter.trim().to_lowercase();
    let mut by_repo: Vec<(i64, Vec<usize>)> = snapshot
        .repositories
        .iter()
        .map(|repo| (repo.id, Vec::new()))
        .collect();
    let mut loose: Vec<usize> = Vec::new();

    for (index, session) in snapshot.sessions.iter().enumerate() {
        if !matches(session, &needle) {
            continue;
        }
        match session
            .repo_id
            .and_then(|id| by_repo.iter_mut().find(|(repo_id, _)| *repo_id == id))
        {
            Some((_, list)) => list.push(index),
            // A session pointing at a repository the index no longer lists is
            // still readable, so it goes with the rest of the homeless.
            None => loose.push(index),
        }
    }

    let mut groups = Vec::new();
    for (repo, (_, sessions)) in snapshot.repositories.iter().zip(by_repo) {
        if sessions.is_empty() && !needle.is_empty() {
            continue;
        }
        groups.push(Group {
            key: format!("repo-{}", repo.id),
            name: repo.display_name.clone(),
            path: Some(repo.root_path.clone()),
            sessions,
        });
    }
    if !loose.is_empty() {
        groups.push(Group {
            key: "unassigned".to_string(),
            name: "No repository".to_string(),
            path: None,
            sessions: loose,
        });
    }
    groups
}

/// Does this session match what was typed?
///
/// Deliberately only the things shown on the row — title, session id, tool.
/// Searching inside transcripts is the index's job and has its own pane,
/// because it is a query, not a filter, and it returns lines rather than
/// sessions.
fn matches(session: &SessionView, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    session.label().to_lowercase().contains(needle)
        || session.native_id.to_lowercase().contains(needle)
        || session.tool.as_str().contains(needle)
        || session.tool.display_name().to_lowercase().contains(needle)
}
