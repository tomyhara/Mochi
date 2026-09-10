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

use crate::mask::Masker;
use crate::model::{Message, ParseStatus, Role, ToolId};
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
    /// Show secrets in the snippet instead of masking them (NFR-3.3).
    ///
    /// Phrased so that the default is the safe one. Snippets are built after
    /// masking rather than before, because a snippet that cuts an API key in
    /// half still shows half an API key.
    pub reveal_secrets: bool,
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

/// Current schema version. Bumping it runs the migrations below.
const SCHEMA_VERSION: i32 = 1;

const SCHEMA: &str = r#"
CREATE TABLE repositories (
    id             INTEGER PRIMARY KEY,
    identity_key   TEXT NOT NULL UNIQUE,
    display_name   TEXT NOT NULL,
    root_path      TEXT NOT NULL,
    path_key       TEXT NOT NULL,
    remote_url     TEXT,
    root_commit    TEXT,
    is_worktree    INTEGER NOT NULL DEFAULT 0,
    parent_repo_id INTEGER REFERENCES repositories(id),
    hidden         INTEGER NOT NULL DEFAULT 0,
    color_label    TEXT
);

CREATE TABLE sessions (
    id             INTEGER PRIMARY KEY,
    tool           TEXT NOT NULL,
    native_id      TEXT NOT NULL,
    repo_id        INTEGER REFERENCES repositories(id),
    cwd            TEXT,
    git_branch     TEXT,
    title          TEXT,
    model          TEXT,
    started_at     INTEGER,
    updated_at     INTEGER,
    message_count  INTEGER NOT NULL DEFAULT 0,
    tokens_in      INTEGER NOT NULL DEFAULT 0,
    tokens_out     INTEGER NOT NULL DEFAULT 0,
    status         TEXT NOT NULL DEFAULT 'unknown',
    source_path    TEXT NOT NULL,
    source_size    INTEGER NOT NULL DEFAULT 0,
    source_mtime   INTEGER NOT NULL DEFAULT 0,
    schema_version TEXT,
    parse_status   TEXT NOT NULL DEFAULT 'ok',
    cwd_exists     INTEGER NOT NULL DEFAULT 0,
    parse_error    TEXT,
    UNIQUE (tool, native_id)
);

CREATE INDEX sessions_repo    ON sessions (repo_id);
CREATE INDEX sessions_updated ON sessions (updated_at DESC);
CREATE INDEX sessions_source  ON sessions (source_path);

CREATE TABLE messages (
    id         INTEGER PRIMARY KEY,
    session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    seq        INTEGER NOT NULL,
    role       TEXT NOT NULL,
    content    TEXT NOT NULL,
    raw        TEXT,
    timestamp  INTEGER,
    tool_name  TEXT,
    native_id  TEXT,
    parent_id  TEXT,
    tokens_in  INTEGER,
    tokens_out INTEGER
);

CREATE INDEX messages_session ON messages (session_id, seq);

-- trigram rather than the default word tokeniser: it is the one that finds a
-- substring inside a run of Japanese with no spaces in it (NFR-6.4), and it
-- still matches English words. The cost is that a query shorter than three
-- characters cannot use the index; search() falls back to a scan for those.
CREATE VIRTUAL TABLE messages_fts USING fts5(
    content,
    content='messages',
    content_rowid='id',
    tokenize='trigram'
);

CREATE TRIGGER messages_fts_insert AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, content) VALUES (new.id, new.content);
END;

CREATE TRIGGER messages_fts_delete AFTER DELETE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, content)
    VALUES ('delete', old.id, old.content);
END;

CREATE TRIGGER messages_fts_update AFTER UPDATE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, content)
    VALUES ('delete', old.id, old.content);
    INSERT INTO messages_fts(rowid, content) VALUES (new.id, new.content);
END;
"#;

/// How many hits or rows to return when the caller does not say.
const DEFAULT_LIMIT: u32 = 200;

/// The open index.
pub struct Index {
    conn: Connection,
    path: Option<PathBuf>,
    masker: Masker,
}

impl Index {
    /// Open, creating and migrating as needed.
    pub fn open(path: &Path) -> Result<Index> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                std::fs::create_dir_all(parent).map_err(|e| crate::Error::io(parent, e))?;
            }
        }
        // Create the file with owner-only permissions before SQLite touches
        // it. SQLite copies the database file's mode onto its journal and WAL
        // files, so getting this right once covers all three (NFR-3.4).
        if !path.exists() {
            create_private_file(path)?;
        }

        let conn = Connection::open(path)?;
        let mut index = Index {
            conn,
            path: Some(path.to_path_buf()),
            masker: Masker::new(),
        };
        index.prepare()?;
        Ok(index)
    }

    pub fn open_in_memory() -> Result<Index> {
        let conn = Connection::open_in_memory()?;
        let mut index = Index {
            conn,
            path: None,
            masker: Masker::new(),
        };
        index.prepare()?;
        Ok(index)
    }

    fn prepare(&mut self) -> Result<()> {
        self.conn.pragma_update(None, "foreign_keys", "ON")?;
        // The index is a cache that can be rebuilt from the session files, so
        // durability matters less than not stalling the UI mid-scan.
        self.conn.pragma_update(None, "journal_mode", "WAL")?;
        self.conn.pragma_update(None, "synchronous", "NORMAL")?;

        let version: i32 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap_or(0);

        if version == 0 {
            self.conn.execute_batch(SCHEMA)?;
            self.conn
                .pragma_update(None, "user_version", SCHEMA_VERSION)?;
        } else if version > SCHEMA_VERSION {
            return Err(crate::Error::invalid(format!(
                "this index was written by a newer version of Mochi (schema {version}); \
                 delete it to have it rebuilt from your session files"
            )));
        }
        Ok(())
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Insert or update a repository, keyed on its identity (FR-3.3).
    pub fn upsert_repository(&mut self, repo: &RepositoryRecord) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO repositories
                 (identity_key, display_name, root_path, path_key, remote_url, root_commit,
                  is_worktree, parent_repo_id, hidden, color_label)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT (identity_key) DO UPDATE SET
                 display_name   = excluded.display_name,
                 root_path      = excluded.root_path,
                 path_key       = excluded.path_key,
                 remote_url     = COALESCE(excluded.remote_url, repositories.remote_url),
                 root_commit    = COALESCE(excluded.root_commit, repositories.root_commit),
                 is_worktree    = excluded.is_worktree,
                 parent_repo_id = COALESCE(excluded.parent_repo_id, repositories.parent_repo_id),
                 hidden         = excluded.hidden,
                 color_label    = COALESCE(excluded.color_label, repositories.color_label)",
            rusqlite::params![
                repo.identity_key,
                repo.display_name,
                repo.root_path,
                repo.path_key,
                repo.remote_url,
                repo.root_commit,
                repo.is_worktree as i64,
                repo.parent_repo_id,
                repo.hidden as i64,
                repo.color_label,
            ],
        )?;
        Ok(self.conn.query_row(
            "SELECT id FROM repositories WHERE identity_key = ?1",
            [&repo.identity_key],
            |row| row.get(0),
        )?)
    }

    /// Insert or update a session, keyed on tool plus native id.
    pub fn upsert_session(&mut self, session: &SessionRecord) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO sessions
                 (tool, native_id, repo_id, cwd, git_branch, title, model, started_at, updated_at,
                  message_count, tokens_in, tokens_out, status, source_path, source_size,
                  source_mtime, schema_version, parse_status, cwd_exists, parse_error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                     ?18, ?19, ?20)
             ON CONFLICT (tool, native_id) DO UPDATE SET
                 repo_id        = excluded.repo_id,
                 cwd            = excluded.cwd,
                 git_branch     = excluded.git_branch,
                 title          = excluded.title,
                 model          = excluded.model,
                 started_at     = excluded.started_at,
                 updated_at     = excluded.updated_at,
                 message_count  = excluded.message_count,
                 tokens_in      = excluded.tokens_in,
                 tokens_out     = excluded.tokens_out,
                 status         = excluded.status,
                 source_path    = excluded.source_path,
                 source_size    = excluded.source_size,
                 source_mtime   = excluded.source_mtime,
                 schema_version = excluded.schema_version,
                 parse_status   = excluded.parse_status,
                 cwd_exists     = excluded.cwd_exists,
                 parse_error    = excluded.parse_error",
            rusqlite::params![
                session.tool.as_str(),
                session.native_id,
                session.repo_id,
                session.cwd,
                session.git_branch,
                session.title,
                session.model,
                session.started_at,
                session.updated_at,
                session.message_count,
                session.tokens_in,
                session.tokens_out,
                session.status.as_str(),
                session.source_path,
                session.source_size,
                session.source_mtime,
                session.schema_version,
                session.parse_status.as_str(),
                session.cwd_exists as i64,
                session.parse_error,
            ],
        )?;
        Ok(self.conn.query_row(
            "SELECT id FROM sessions WHERE tool = ?1 AND native_id = ?2",
            rusqlite::params![session.tool.as_str(), session.native_id],
            |row| row.get(0),
        )?)
    }

    /// Replace a session's transcript, keeping the full-text index in step.
    pub fn replace_messages(&mut self, session_id: i64, messages: &[Message]) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM messages WHERE session_id = ?1", [session_id])?;
        {
            let mut insert = tx.prepare(
                "INSERT INTO messages
                     (session_id, seq, role, content, raw, timestamp, tool_name, native_id,
                      parent_id, tokens_in, tokens_out)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )?;
            for message in messages {
                insert.execute(rusqlite::params![
                    session_id,
                    message.seq,
                    message.role.as_str(),
                    message.content,
                    message.raw,
                    message.timestamp,
                    message.tool_name,
                    message.native_id,
                    message.parent_id,
                    message.tokens_in,
                    message.tokens_out,
                ])?;
            }
        }
        tx.execute(
            "UPDATE sessions SET message_count = ?2 WHERE id = ?1",
            rusqlite::params![session_id, messages.len() as i64],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn messages(&self, session_id: i64) -> Result<Vec<Message>> {
        let mut statement = self.conn.prepare(
            "SELECT seq, role, content, raw, timestamp, tool_name, native_id, parent_id,
                    tokens_in, tokens_out
             FROM messages WHERE session_id = ?1 ORDER BY seq",
        )?;
        let rows = statement.query_map([session_id], |row| {
            Ok(Message {
                seq: row.get(0)?,
                role: Role::parse(&row.get::<_, String>(1)?).unwrap_or(Role::System),
                content: row.get(2)?,
                raw: row.get(3)?,
                timestamp: row.get(4)?,
                tool_name: row.get(5)?,
                native_id: row.get(6)?,
                parent_id: row.get(7)?,
                tokens_in: row.get(8)?,
                tokens_out: row.get(9)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn session_by_source(&self, source_path: &str) -> Result<Option<SessionRecord>> {
        self.query_session("WHERE source_path = ?1", rusqlite::params![source_path])
    }

    pub fn session(&self, id: i64) -> Result<Option<SessionRecord>> {
        self.query_session("WHERE id = ?1", rusqlite::params![id])
    }

    fn query_session(
        &self,
        filter: &str,
        params: &[&dyn rusqlite::ToSql],
    ) -> Result<Option<SessionRecord>> {
        let sql = format!("{SESSION_COLUMNS} {filter} LIMIT 1");
        let mut statement = self.conn.prepare(&sql)?;
        let mut rows = statement.query(params)?;
        match rows.next()? {
            Some(row) => Ok(Some(read_session(row)?)),
            None => Ok(None),
        }
    }

    pub fn list_sessions(&self, query: &SessionQuery) -> Result<Vec<SessionRecord>> {
        let mut where_parts: Vec<String> = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(repo_id) = query.repo_id {
            params.push(Box::new(repo_id));
            where_parts.push(format!("repo_id = ?{}", params.len()));
        }
        if let Some(tool) = query.tool {
            params.push(Box::new(tool.as_str().to_string()));
            where_parts.push(format!("tool = ?{}", params.len()));
        }
        if let Some(status) = query.parse_status {
            params.push(Box::new(status.as_str().to_string()));
            where_parts.push(format!("parse_status = ?{}", params.len()));
        }
        if let Some(since) = query.since {
            params.push(Box::new(since));
            where_parts.push(format!(
                "COALESCE(updated_at, started_at, 0) >= ?{}",
                params.len()
            ));
        }
        if let Some(until) = query.until {
            params.push(Box::new(until));
            where_parts.push(format!(
                "COALESCE(updated_at, started_at, 0) <= ?{}",
                params.len()
            ));
        }

        let filter = if where_parts.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_parts.join(" AND "))
        };
        let order = match query.order {
            SessionOrder::UpdatedDesc => "COALESCE(updated_at, started_at, 0) DESC, id DESC",
            SessionOrder::StartedDesc => "COALESCE(started_at, updated_at, 0) DESC, id DESC",
            SessionOrder::MessagesDesc => "message_count DESC, id DESC",
            SessionOrder::SizeDesc => "source_size DESC, id DESC",
        };
        let sql = format!(
            "{SESSION_COLUMNS} {filter} ORDER BY {order} LIMIT {}",
            query.limit.unwrap_or(DEFAULT_LIMIT)
        );

        let mut statement = self.conn.prepare(&sql)?;
        let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let mut rows = statement.query(refs.as_slice())?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(read_session(row)?);
        }
        Ok(out)
    }

    pub fn list_repositories(&self) -> Result<Vec<RepositoryRecord>> {
        let mut statement = self.conn.prepare(
            "SELECT id, identity_key, display_name, root_path, path_key, remote_url, root_commit,
                    is_worktree, parent_repo_id, hidden, color_label
             FROM repositories ORDER BY display_name COLLATE NOCASE",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(RepositoryRecord {
                id: row.get(0)?,
                identity_key: row.get(1)?,
                display_name: row.get(2)?,
                root_path: row.get(3)?,
                path_key: row.get(4)?,
                remote_url: row.get(5)?,
                root_commit: row.get(6)?,
                is_worktree: row.get::<_, i64>(7)? != 0,
                parent_repo_id: row.get(8)?,
                hidden: row.get::<_, i64>(9)? != 0,
                color_label: row.get(10)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Full-text search over message content (FR-6.1).
    pub fn search(&self, query: &SearchQuery) -> Result<Vec<SearchHit>> {
        let text = query.text.trim();
        if text.is_empty() {
            return Ok(Vec::new());
        }

        let terms = split_terms(text);
        if terms.is_empty() {
            return Ok(Vec::new());
        }

        // The trigram index cannot serve a term shorter than three characters,
        // and a two-character Japanese word is a perfectly ordinary search. Do
        // it the slow way rather than returning nothing.
        if terms.iter().any(|term| term.chars().count() < 3) {
            return self.search_scan(query, &terms);
        }
        self.search_fts(query, &terms)
    }

    fn search_fts(&self, query: &SearchQuery, terms: &[String]) -> Result<Vec<SearchHit>> {
        // Every term becomes a quoted FTS5 string, so nothing a user types is
        // read as query syntax.
        let match_expression = terms
            .iter()
            .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" ");

        let (filter, mut params) = search_filters(query, 2);
        let sql = format!(
            "SELECT s.id, s.tool, r.display_name, s.title, m.seq, m.role, m.timestamp, m.content
             FROM messages_fts
             JOIN messages  m ON m.id = messages_fts.rowid
             JOIN sessions  s ON s.id = m.session_id
             LEFT JOIN repositories r ON r.id = s.repo_id
             WHERE messages_fts MATCH ?1 {filter}
             ORDER BY rank, s.id, m.seq
             LIMIT {}",
            query.limit.unwrap_or(DEFAULT_LIMIT)
        );

        let mut bound: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(match_expression)];
        bound.append(&mut params);

        let mut statement = self.conn.prepare(&sql)?;
        let refs: Vec<&dyn rusqlite::ToSql> = bound.iter().map(|p| p.as_ref()).collect();
        let mut rows = statement.query(refs.as_slice())?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let mut hit = read_hit(row)?;
            self.finish_hit(&mut hit, terms, query.reveal_secrets);
            out.push(hit);
        }
        Ok(out)
    }

    /// Fallback for terms too short for the trigram index.
    fn search_scan(&self, query: &SearchQuery, terms: &[String]) -> Result<Vec<SearchHit>> {
        let (filter, mut params) = search_filters(query, terms.len() + 1);
        let like_clauses: Vec<String> = (1..=terms.len())
            .map(|position| format!("m.content LIKE ?{position} ESCAPE '\\'"))
            .collect();
        let sql = format!(
            "SELECT s.id, s.tool, r.display_name, s.title, m.seq, m.role, m.timestamp, m.content
             FROM messages m
             JOIN sessions s ON s.id = m.session_id
             LEFT JOIN repositories r ON r.id = s.repo_id
             WHERE {} {filter}
             ORDER BY s.id, m.seq
             LIMIT {}",
            like_clauses.join(" AND "),
            query.limit.unwrap_or(DEFAULT_LIMIT)
        );

        let mut bound: Vec<Box<dyn rusqlite::ToSql>> = terms
            .iter()
            .map(|term| Box::new(format!("%{}%", escape_like(term))) as Box<dyn rusqlite::ToSql>)
            .collect();
        bound.append(&mut params);

        let mut statement = self.conn.prepare(&sql)?;
        let refs: Vec<&dyn rusqlite::ToSql> = bound.iter().map(|p| p.as_ref()).collect();
        let mut rows = statement.query(refs.as_slice())?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let mut hit = read_hit(row)?;
            self.finish_hit(&mut hit, terms, query.reveal_secrets);
            out.push(hit);
        }
        Ok(out)
    }

    /// Turn a hit's full message content into the snippet a caller displays.
    ///
    /// Masking happens first and the snippet is cut out of the masked text, so
    /// there is no window in which a truncated secret can be shown (NFR-3.3).
    fn finish_hit(&self, hit: &mut SearchHit, terms: &[String], reveal_secrets: bool) {
        if !reveal_secrets {
            hit.snippet = self.masker.mask(&hit.snippet).text;
            hit.session_title = hit
                .session_title
                .as_deref()
                .map(|title| self.masker.mask(title).text);
        }
        hit.snippet = highlight(&hit.snippet, terms);
    }

    /// Mark a session whose source file has gone. The transcript stays
    /// readable (FR-2.11).
    pub fn mark_archived(&mut self, session_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET parse_status = 'archived', status = 'finished' WHERE id = ?1",
            [session_id],
        )?;
        Ok(())
    }

    pub fn stats(&self) -> Result<IndexStats> {
        Ok(IndexStats {
            repositories: self.count("SELECT COUNT(*) FROM repositories")?,
            sessions: self.count("SELECT COUNT(*) FROM sessions")?,
            messages: self.count("SELECT COUNT(*) FROM messages")?,
            archived_sessions: self
                .count("SELECT COUNT(*) FROM sessions WHERE parse_status = 'archived'")?,
            failed_sessions: self
                .count("SELECT COUNT(*) FROM sessions WHERE parse_status = 'failed'")?,
            total_source_bytes: self.count("SELECT COALESCE(SUM(source_size), 0) FROM sessions")?,
        })
    }

    fn count(&self, sql: &str) -> Result<i64> {
        Ok(self.conn.query_row(sql, [], |row| row.get(0))?)
    }
}

const SESSION_COLUMNS: &str = "SELECT id, tool, native_id, repo_id, cwd, git_branch, title, model,
        started_at, updated_at, message_count, tokens_in, tokens_out, status, source_path,
        source_size, source_mtime, schema_version, parse_status, cwd_exists, parse_error
    FROM sessions";

fn read_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionRecord> {
    Ok(SessionRecord {
        id: row.get(0)?,
        tool: ToolId::parse(&row.get::<_, String>(1)?).unwrap_or(ToolId::ClaudeCode),
        native_id: row.get(2)?,
        repo_id: row.get(3)?,
        cwd: row.get(4)?,
        git_branch: row.get(5)?,
        title: row.get(6)?,
        model: row.get(7)?,
        started_at: row.get(8)?,
        updated_at: row.get(9)?,
        message_count: row.get(10)?,
        tokens_in: row.get(11)?,
        tokens_out: row.get(12)?,
        status: SessionStatus::parse(&row.get::<_, String>(13)?).unwrap_or(SessionStatus::Unknown),
        source_path: row.get(14)?,
        source_size: row.get(15)?,
        source_mtime: row.get(16)?,
        schema_version: row.get(17)?,
        parse_status: ParseStatus::parse(&row.get::<_, String>(18)?).unwrap_or(ParseStatus::Ok),
        cwd_exists: row.get::<_, i64>(19)? != 0,
        parse_error: row.get(20)?,
    })
}

fn read_hit(row: &rusqlite::Row<'_>) -> rusqlite::Result<SearchHit> {
    Ok(SearchHit {
        session_id: row.get(0)?,
        tool: ToolId::parse(&row.get::<_, String>(1)?).unwrap_or(ToolId::ClaudeCode),
        repo_display: row.get(2)?,
        session_title: row.get(3)?,
        message_seq: row.get(4)?,
        role: row.get(5)?,
        timestamp: row.get(6)?,
        snippet: row.get(7)?,
    })
}

/// Filters shared by both search paths, numbered from `first_parameter`.
fn search_filters(
    query: &SearchQuery,
    first_parameter: usize,
) -> (String, Vec<Box<dyn rusqlite::ToSql>>) {
    let mut clauses = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    let mut position = first_parameter;

    if let Some(repo_id) = query.repo_id {
        params.push(Box::new(repo_id));
        clauses.push(format!("AND s.repo_id = ?{position}"));
        position += 1;
    }
    if let Some(session_id) = query.session_id {
        params.push(Box::new(session_id));
        clauses.push(format!("AND s.id = ?{position}"));
        position += 1;
    }
    if let Some(tool) = query.tool {
        params.push(Box::new(tool.as_str().to_string()));
        clauses.push(format!("AND s.tool = ?{position}"));
    }
    (clauses.join(" "), params)
}

/// Split a query into terms, honouring double-quoted phrases (FR-6.6).
fn split_terms(text: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for character in text.chars() {
        match character {
            '"' => {
                in_quotes = !in_quotes;
                if !in_quotes && !current.trim().is_empty() {
                    terms.push(current.trim().to_string());
                    current.clear();
                }
            }
            c if c.is_whitespace() && !in_quotes => {
                if !current.trim().is_empty() {
                    terms.push(current.trim().to_string());
                }
                current.clear();
            }
            c => current.push(c),
        }
    }
    if !current.trim().is_empty() {
        terms.push(current.trim().to_string());
    }
    terms
}

fn escape_like(term: &str) -> String {
    term.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// Cut a snippet around the first term that appears in `content`, marking the
/// match with [`SNIPPET_START`] and [`SNIPPET_END`].
///
/// A term can be absent even though the row matched: masking may have replaced
/// the text it was found in. The snippet then falls back to the opening of the
/// message, which is still useful and never shows what was hidden.
fn highlight(content: &str, terms: &[String]) -> String {
    const CONTEXT: usize = 60;
    let lowered = content.to_lowercase();
    let found = terms
        .iter()
        .filter_map(|term| {
            let needle = term.to_lowercase();
            lowered.find(&needle).map(|index| (index, needle))
        })
        .min_by_key(|(index, _)| *index);

    let Some((byte_index, needle)) = found else {
        let head: String = content.chars().take(CONTEXT * 2).collect();
        return if head.chars().count() < content.chars().count() {
            format!("{head}…")
        } else {
            head
        };
    };

    let start = floor_boundary(content, byte_index.saturating_sub(CONTEXT));
    let end = ceil_boundary(
        content,
        (byte_index + needle.len() + CONTEXT).min(content.len()),
    );
    let matched_end = byte_index + needle.len();

    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.push_str(&content[start..byte_index]);
    out.push_str(SNIPPET_START);
    out.push_str(&content[byte_index..matched_end]);
    out.push_str(SNIPPET_END);
    out.push_str(&content[matched_end..end]);
    if end < content.len() {
        out.push('…');
    }
    out
}

fn floor_boundary(text: &str, mut index: usize) -> usize {
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn ceil_boundary(text: &str, mut index: usize) -> usize {
    while index < text.len() && !text.is_char_boundary(index) {
        index += 1;
    }
    index
}

#[cfg(unix)]
fn create_private_file(path: &Path) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)
        .map_err(|e| crate::Error::io(path, e))?;
    Ok(())
}

#[cfg(not(unix))]
fn create_private_file(path: &Path) -> Result<()> {
    // On Windows the file inherits the ACL of the profile directory it lives
    // in, which is already restricted to the user (NFR-3.4). Setting an
    // explicit ACL needs a platform crate; it belongs with the installer work.
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|e| crate::Error::io(path, e))?;
    Ok(())
}
