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

use std::collections::{HashMap, HashSet};
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
    /// How many characters `content` has, and `raw` after it.
    ///
    /// Counted here, once, because the transcript cuts long entries and says
    /// how much it cut: counting from the string itself would walk every byte
    /// of a megabyte-long tool result on every repaint, for every entry on
    /// screen. [`message_view`] is what keeps these honest.
    pub content_chars: usize,
    pub raw_chars: usize,
}

/// Build a transcript entry from its already-masked text.
pub fn message_view(
    seq: i64,
    role: Role,
    content: String,
    tool_name: Option<String>,
    timestamp: Option<i64>,
    raw: Option<String>,
) -> MessageView {
    MessageView {
        content_chars: content.chars().count(),
        raw_chars: raw.as_deref().map(|raw| raw.chars().count()).unwrap_or(0),
        seq,
        role,
        content,
        tool_name,
        timestamp,
        raw,
    }
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

/// Which slice of the index the session list is showing (FR-9.1).
///
/// The repository pane picks one of these and the session pane shows it. They
/// are a closed set rather than "a repository id or nothing" because two of
/// the useful views cut across repositories: the sessions that belong to none,
/// and the ones whose original file the tool has since deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    /// Everything in the index.
    All,
    /// One repository, by its index id.
    Repo(i64),
    /// Sessions that belong to no repository the index lists (FR-3.6).
    Unassigned,
    /// Sessions Mochi's copy has outlived (`ParseStatus::Archived`).
    Archived,
}

/// One row of the repository pane.
///
/// It carries its own counts because the pane is drawn on every frame and the
/// counts are a walk over every session: doing it here, once per snapshot,
/// is what keeps scrolling cheap on the index NFR-1 is written for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoRow {
    pub scope: Scope,
    pub name: String,
    /// The repository's root, for the rows that have one.
    pub path: Option<String>,
    /// How many sessions the row would list.
    pub sessions: usize,
    /// How they split between the tools, in a fixed order, tools with none
    /// left out.
    pub by_tool: Vec<(ToolId, usize)>,
    /// The most recently touched session in the row.
    pub updated_at: Option<i64>,
}

/// A dated heading in the session pane, with the sessions under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub label: &'static str,
    /// Indices into [`Snapshot::sessions`], keeping the snapshot's order —
    /// which is most recently touched first (FR-4.4).
    pub sessions: Vec<usize>,
}

impl Section {
    /// Is this the section whose heading already says which day it is?
    ///
    /// The rows under it show the clock alone; everywhere else they have to
    /// carry the date.
    pub fn is_today(&self) -> bool {
        self.label == SECTIONS[0]
    }
}

/// The headings, newest first. `Undated` is last because a session with no
/// timestamp is not old, it is unknown.
const SECTIONS: [&str; 6] = [
    "Today",
    "Yesterday",
    "Previous 7 days",
    "Previous 30 days",
    "Older",
    "No date",
];

const DAY: i64 = 86_400_000;

/// The rows of the repository pane, most recently used first.
///
/// Recency rather than the alphabet: the repository you were in five minutes
/// ago is the one you are looking for, and the filter box is how you find any
/// other. Repositories with no sessions keep their row — that is a fact about
/// the index worth seeing — and sort to the bottom by name.
///
/// `filter` matches a row's name and its path, and nothing else: it is a way
/// of finding a repository, not of searching sessions, which is the pane next
/// to it and the index behind that.
pub fn repositories(snapshot: &Snapshot, filter: &str) -> Vec<RepoRow> {
    let needle = filter.trim().to_lowercase();
    // Looked up by id rather than searched for: an index of ten thousand
    // sessions over hundreds of repositories would otherwise cost the product
    // of the two (NFR-1).
    let mut slots: HashMap<i64, usize> = HashMap::with_capacity(snapshot.repositories.len());
    for (slot, repo) in snapshot.repositories.iter().enumerate() {
        slots.entry(repo.id).or_insert(slot);
    }

    let mut counts: Vec<Counts> = vec![Counts::default(); snapshot.repositories.len()];
    let mut all = Counts::default();
    let mut loose = Counts::default();
    let mut archived = Counts::default();

    for session in &snapshot.sessions {
        all.add(session);
        match session.repo_id.and_then(|id| slots.get(&id)) {
            Some(slot) => counts[*slot].add(session),
            // A session pointing at a repository the index no longer lists is
            // still readable, so it goes with the rest of the homeless.
            None => loose.add(session),
        }
        if session.parse_status == ParseStatus::Archived {
            archived.add(session);
        }
    }

    let mut repos: Vec<RepoRow> = snapshot
        .repositories
        .iter()
        .zip(counts)
        .map(|(repo, counts)| {
            counts.row(
                Scope::Repo(repo.id),
                repo.display_name.clone(),
                Some(repo.root_path.clone()),
            )
        })
        .collect();
    repos.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    let mut rows = Vec::with_capacity(repos.len() + 3);
    rows.push(all.row(Scope::All, "All repositories".to_string(), None));
    rows.append(&mut repos);
    if loose.sessions > 0 {
        rows.push(loose.row(Scope::Unassigned, "No repository".to_string(), None));
    }
    if archived.sessions > 0 {
        rows.push(archived.row(Scope::Archived, "Archived".to_string(), None));
    }

    rows.retain(|row| {
        needle.is_empty()
            || row.name.to_lowercase().contains(&needle)
            || row
                .path
                .as_deref()
                .is_some_and(|path| path.to_lowercase().contains(&needle))
    });
    rows
}

/// The sessions of one scope, under dated headings, newest first.
///
/// `now` is passed in rather than read here so that the headings can be
/// checked without waiting a day for one.
pub fn sections(snapshot: &Snapshot, scope: Scope, filter: &str, now: i64) -> Vec<Section> {
    let needle = filter.trim().to_lowercase();
    let known: HashSet<i64> = snapshot.repositories.iter().map(|repo| repo.id).collect();

    let mut buckets: Vec<Vec<usize>> = vec![Vec::new(); SECTIONS.len()];
    for (index, session) in snapshot.sessions.iter().enumerate() {
        if !in_scope(session, scope, &known) || !matches(session, &needle) {
            continue;
        }
        buckets[bucket(session.updated_at, now)].push(index);
    }

    SECTIONS
        .iter()
        .zip(buckets)
        .filter(|(_, sessions)| !sessions.is_empty())
        .map(|(label, sessions)| Section { label, sessions })
        .collect()
}

/// What the session pane calls the scope it is showing, and the path under it.
pub fn scope_title(snapshot: &Snapshot, scope: Scope) -> (String, Option<String>) {
    match scope {
        Scope::All => ("All repositories".to_string(), None),
        Scope::Unassigned => ("No repository".to_string(), None),
        Scope::Archived => ("Archived".to_string(), None),
        Scope::Repo(id) => match snapshot.repositories.iter().find(|repo| repo.id == id) {
            Some(repo) => (repo.display_name.clone(), Some(repo.root_path.clone())),
            // The repository went away under the selection — a rescan can do
            // that — and saying so is better than drawing a blank header.
            None => ("Repository is gone".to_string(), None),
        },
    }
}

/// Is this scope still something the index can show?
///
/// A rescan can drop the repository the session pane is pointed at, and a
/// pane pointed at nothing should fall back to everything rather than look
/// empty.
pub fn scope_exists(snapshot: &Snapshot, scope: Scope) -> bool {
    match scope {
        Scope::Repo(id) => snapshot.repositories.iter().any(|repo| repo.id == id),
        _ => true,
    }
}

/// Which heading a session belongs under.
fn bucket(updated_at: Option<i64>, now: i64) -> usize {
    let Some(at) = updated_at else {
        return 5;
    };
    // Whole days, so that "yesterday" means the day before this one rather
    // than "between 24 and 48 hours ago".
    let today = now.div_euclid(DAY);
    match today - at.div_euclid(DAY) {
        // A timestamp in the future is a clock that moved, not a session from
        // tomorrow: it belongs at the top, with today's.
        days if days <= 0 => 0,
        1 => 1,
        days if days < 7 => 2,
        days if days < 30 => 3,
        _ => 4,
    }
}

fn in_scope(session: &SessionView, scope: Scope, known: &HashSet<i64>) -> bool {
    match scope {
        Scope::All => true,
        Scope::Repo(id) => session.repo_id == Some(id),
        Scope::Unassigned => session.repo_id.is_none_or(|id| !known.contains(&id)),
        Scope::Archived => session.parse_status == ParseStatus::Archived,
    }
}

/// The counts one repository row is built from.
#[derive(Debug, Clone, Copy, Default)]
struct Counts {
    sessions: usize,
    by_tool: [usize; 3],
    updated_at: Option<i64>,
}

impl Counts {
    fn add(&mut self, session: &SessionView) {
        self.sessions += 1;
        self.by_tool[slot(session.tool)] += 1;
        self.updated_at = self.updated_at.max(session.updated_at);
    }

    fn row(self, scope: Scope, name: String, path: Option<String>) -> RepoRow {
        RepoRow {
            scope,
            name,
            path,
            sessions: self.sessions,
            by_tool: TOOLS
                .iter()
                .enumerate()
                .filter(|(slot, _)| self.by_tool[*slot] > 0)
                .map(|(slot, tool)| (*tool, self.by_tool[slot]))
                .collect(),
            updated_at: self.updated_at,
        }
    }
}

/// The tools, in the order a row lists them.
const TOOLS: [ToolId; 3] = [ToolId::Codex, ToolId::ClaudeCode, ToolId::OpenCode];

fn slot(tool: ToolId) -> usize {
    match tool {
        ToolId::Codex => 0,
        ToolId::ClaudeCode => 1,
        ToolId::OpenCode => 2,
    }
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
