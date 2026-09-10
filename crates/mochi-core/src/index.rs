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

//! The local index (FR-2.2, FR-6, NFR-1.3).
//!
//! SQLite with an FTS5 table over message text. The index is a cache that can
//! always be rebuilt from the session files, with one exception that matters:
//! once a CLI deletes its own transcript, Mochi's copy is the only one left
//! (FR-2.11, R-4), so sessions are never dropped just because their file went
//! away.
//!
//! The database holds the same secrets as the session files and is created
//! with owner-only permissions (NFR-3.4).

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::model::{Message, ParseStatus, ToolId};
use crate::Result;

/// Marks the start of a matched span inside [`SearchHit::snippet`].
///
/// Deliberately a control character rather than markup: the snippet is shown
/// in a UI that must not be able to interpret session content as anything but
/// text (NFR-3.7).
pub const SNIPPET_START: &str = "\u{2}";
/// Marks the end of a matched span inside [`SearchHit::snippet`].
pub const SNIPPET_END: &str = "\u{3}";

/// A repository row (`doc/requirements.md` §6).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RepositoryRecord {
    pub id: i64,
    pub identity_key: String,
    pub display_name: String,
    pub root_path: String,
    pub path_key: String,
    pub remote_url: Option<String>,
    pub root_commit: Option<String>,
    pub is_worktree: bool,
    pub parent_repo_id: Option<i64>,
    pub hidden: bool,
    pub color_label: Option<String>,
}

/// A session row.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionRecord {
    pub id: i64,
    pub tool: ToolId,
    pub native_id: String,
    pub repo_id: Option<i64>,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub title: Option<String>,
    pub model: Option<String>,
    pub started_at: Option<i64>,
    pub updated_at: Option<i64>,
    pub message_count: i64,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub status: SessionStatus,
    pub source_path: String,
    pub source_size: i64,
    pub source_mtime: i64,
    pub schema_version: Option<String>,
    pub parse_status: ParseStatus,
    /// Whether the recorded working directory still exists. Independent of
    /// `parse_status` (FR-7.6).
    pub cwd_exists: bool,
    pub parse_error: Option<String>,
}

/// Whether the session is being worked on right now (FR-4.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    Running,
    Finished,
    Unknown,
}

impl SessionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            SessionStatus::Running => "running",
            SessionStatus::Finished => "finished",
            SessionStatus::Unknown => "unknown",
        }
    }

    pub fn parse(s: &str) -> Option<SessionStatus> {
        match s {
            "running" => Some(SessionStatus::Running),
            "finished" => Some(SessionStatus::Finished),
            "unknown" => Some(SessionStatus::Unknown),
            _ => None,
        }
    }
}

/// Filters for the session list (FR-4.4, FR-4.5).
#[derive(Debug, Clone, Default)]
pub struct SessionQuery {
    pub repo_id: Option<i64>,
    pub tool: Option<ToolId>,
    pub parse_status: Option<ParseStatus>,
    /// Inclusive lower bound on `updated_at`, milliseconds since the epoch.
    pub since: Option<i64>,
    pub until: Option<i64>,
    pub order: SessionOrder,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SessionOrder {
    #[default]
    UpdatedDesc,
    StartedDesc,
    MessagesDesc,
    SizeDesc,
}

/// A full-text query (FR-6).
#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    pub text: String,
    pub repo_id: Option<i64>,
    pub session_id: Option<i64>,
    pub tool: Option<ToolId>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub session_id: i64,
    pub tool: ToolId,
    pub repo_display: Option<String>,
    pub session_title: Option<String>,
    pub message_seq: i64,
    pub role: String,
    pub timestamp: Option<i64>,
    /// Text around the match, with the matched terms wrapped in the markers
    /// the caller asked for.
    pub snippet: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IndexStats {
    pub repositories: i64,
    pub sessions: i64,
    pub messages: i64,
    pub archived_sessions: i64,
    pub failed_sessions: i64,
    pub total_source_bytes: i64,
}

/// The open index.
pub struct Index {
    conn: Connection,
    path: Option<PathBuf>,
}

impl Index {
    /// Open, creating and migrating as needed.
    pub fn open(path: &Path) -> Result<Index> {
        todo!()
    }

    pub fn open_in_memory() -> Result<Index> {
        todo!()
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Insert or update a repository, keyed on its identity (FR-3.3).
    pub fn upsert_repository(&mut self, repo: &RepositoryRecord) -> Result<i64> {
        todo!()
    }

    /// Insert or update a session, keyed on tool plus native id.
    pub fn upsert_session(&mut self, session: &SessionRecord) -> Result<i64> {
        todo!()
    }

    /// Replace a session's transcript, keeping the full-text index in step.
    pub fn replace_messages(&mut self, session_id: i64, messages: &[Message]) -> Result<()> {
        todo!()
    }

    pub fn messages(&self, session_id: i64) -> Result<Vec<Message>> {
        todo!()
    }

    pub fn session_by_source(&self, source_path: &str) -> Result<Option<SessionRecord>> {
        todo!()
    }

    pub fn session(&self, id: i64) -> Result<Option<SessionRecord>> {
        todo!()
    }

    pub fn list_sessions(&self, query: &SessionQuery) -> Result<Vec<SessionRecord>> {
        todo!()
    }

    pub fn list_repositories(&self) -> Result<Vec<RepositoryRecord>> {
        todo!()
    }

    /// Full-text search over message content (FR-6.1).
    pub fn search(&self, query: &SearchQuery) -> Result<Vec<SearchHit>> {
        todo!()
    }

    /// Mark a session whose source file has gone. The transcript stays
    /// readable (FR-2.11).
    pub fn mark_archived(&mut self, session_id: i64) -> Result<()> {
        todo!()
    }

    pub fn stats(&self) -> Result<IndexStats> {
        todo!()
    }
}
