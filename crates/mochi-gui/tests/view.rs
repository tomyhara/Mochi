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

//! Grouping and filtering — the decisions the two left-hand panes make, made
//! where they can be checked without a window.

use mochi_core::model::{ParseStatus, Role, ToolId};
use mochi_gui::view::{
    message_view, repositories, scope_exists, scope_title, sections, RepoView, Scope, SessionView,
    Snapshot,
};

/// A fixed "now" for the dated headings: 2026-09-12 12:00 UTC.
const NOW: i64 = 1_789_560_000_000;

const DAY: i64 = 86_400_000;

fn repo(id: i64, name: &str) -> RepoView {
    RepoView {
        id,
        display_name: name.to_string(),
        root_path: format!("/Users/you/code/{name}"),
        remote_url: None,
        is_worktree: false,
    }
}

fn session(id: i64, repo_id: Option<i64>, title: Option<&str>) -> SessionView {
    SessionView {
        id,
        tool: ToolId::ClaudeCode,
        native_id: format!("sess-{id}"),
        repo_id,
        title: title.map(str::to_string),
        model: None,
        cwd: Some("/Users/you/code/alpha".to_string()),
        cwd_exists: true,
        git_branch: None,
        started_at: None,
        updated_at: Some(id * 1000),
        message_count: 3,
        tokens_in: 0,
        tokens_out: 0,
        source_path: format!("/store/{id}.jsonl"),
        source_size: 10,
        parse_status: ParseStatus::Ok,
        parse_error: None,
        resume: Ok("claude --resume sess".to_string()),
    }
}

fn snapshot() -> Snapshot {
    Snapshot {
        masked: true,
        repositories: vec![repo(1, "alpha"), repo(2, "beta")],
        sessions: vec![
            session(10, Some(1), Some("deploy keeps failing")),
            session(11, Some(1), Some("ECONNRESET again")),
            session(12, Some(2), Some("rename the module")),
            session(13, None, Some("scratch")),
        ],
        ..Default::default()
    }
}

/// The repository pane is a list of places, not of sessions: every repository
/// gets one row, with the whole index above them and the two cross-cutting
/// views below.
#[test]
fn the_repository_pane_lists_every_repository_and_the_views_across_them() {
    let snapshot = snapshot();
    let rows = repositories(&snapshot, "");

    assert_eq!(
        rows.iter().map(|row| row.name.as_str()).collect::<Vec<_>>(),
        ["All repositories", "beta", "alpha", "No repository"]
    );
    assert_eq!(rows[0].scope, Scope::All);
    assert_eq!(rows[0].sessions, 4);
    // beta first: its session is the most recent one (FR-4.4).
    assert_eq!(rows[1].scope, Scope::Repo(2));
    assert_eq!(rows[2].sessions, 2);
    assert_eq!(rows[2].path.as_deref(), Some("/Users/you/code/alpha"));
}

/// FR-3.6: a session that belongs to no repository is the one people are
/// usually looking for, so it gets a row of its own rather than vanishing.
#[test]
fn homeless_sessions_get_a_row_of_their_own() {
    let snapshot = snapshot();
    let rows = repositories(&snapshot, "");
    let loose = rows.last().unwrap();

    assert_eq!(loose.scope, Scope::Unassigned);
    assert_eq!(loose.sessions, 1);
    assert_eq!(loose.path, None);

    let listed = sections(&snapshot, Scope::Unassigned, "", NOW);
    let index = listed[0].sessions[0];
    assert_eq!(snapshot.sessions[index].id, 13);
}

/// A session pointing at a repository the index no longer lists must not
/// disappear: it is still readable, and dropping it would look like data loss.
#[test]
fn a_session_whose_repository_is_gone_falls_back_to_no_repository() {
    let mut snapshot = snapshot();
    snapshot
        .sessions
        .push(session(14, Some(99), Some("orphan")));

    let rows = repositories(&snapshot, "");
    let loose = rows.last().unwrap();
    assert_eq!(loose.scope, Scope::Unassigned);
    assert_eq!(loose.sessions, 2);

    let listed = sections(&snapshot, Scope::Unassigned, "", NOW);
    assert_eq!(
        listed.iter().map(|s| s.sessions.len()).sum::<usize>(),
        2,
        "the orphan is not listed with the sessions that have no repository"
    );
}

/// The `Archived` row only exists when something is archived: it is a warning,
/// and a warning that is always there says nothing.
#[test]
fn the_archived_row_appears_only_when_a_transcript_has_been_deleted() {
    let mut snapshot = snapshot();
    assert!(repositories(&snapshot, "")
        .iter()
        .all(|row| row.scope != Scope::Archived));

    snapshot.sessions[0].parse_status = ParseStatus::Archived;
    let rows = repositories(&snapshot, "");
    let archived = rows.last().unwrap();
    assert_eq!(archived.scope, Scope::Archived);
    assert_eq!(archived.sessions, 1);

    // It cuts across repositories rather than replacing one: the session is
    // still listed under alpha as well.
    let under_alpha = sections(&snapshot, Scope::Repo(1), "", NOW);
    assert_eq!(
        under_alpha.iter().map(|s| s.sessions.len()).sum::<usize>(),
        2
    );
}

/// A repository with no sessions keeps its row — that is a fact about the
/// index worth seeing — and the counts say which tools wrote the rest.
#[test]
fn a_row_carries_its_counts_and_an_empty_repository_keeps_one() {
    let mut snapshot = snapshot();
    snapshot.sessions.retain(|s| s.repo_id != Some(2));
    snapshot.sessions[0].tool = ToolId::Codex;

    let rows = repositories(&snapshot, "");
    let beta = rows.iter().find(|row| row.name == "beta").unwrap();
    assert_eq!(beta.sessions, 0);
    assert_eq!(beta.by_tool, []);
    assert_eq!(beta.updated_at, None);

    let alpha = rows.iter().find(|row| row.name == "alpha").unwrap();
    assert_eq!(
        alpha.by_tool,
        [(ToolId::Codex, 1), (ToolId::ClaudeCode, 1)],
        "the tools are counted in a fixed order, and only when they have any"
    );
    assert_eq!(alpha.updated_at, Some(11_000));
}

/// The repository filter is about repositories: it matches a row's name and
/// its path, and it never touches what the session pane is showing.
#[test]
fn the_repository_filter_matches_name_and_path() {
    let snapshot = snapshot();

    let by_name = repositories(&snapshot, "BET");
    assert_eq!(by_name.len(), 1);
    assert_eq!(by_name[0].name, "beta");

    let by_path = repositories(&snapshot, "/users/you/code/alpha");
    assert_eq!(by_path.len(), 1);
    assert_eq!(by_path[0].name, "alpha");

    assert!(repositories(&snapshot, "no such repository").is_empty());
}

/// The session pane shows one repository at a time, newest first, under
/// headings that say how long ago that was.
#[test]
fn the_session_pane_shows_one_scope_under_dated_headings() {
    let mut snapshot = snapshot();
    snapshot.sessions[0].updated_at = Some(NOW - 3_600_000);
    snapshot.sessions[1].updated_at = Some(NOW - DAY);
    snapshot.sessions[2].updated_at = Some(NOW - 3 * DAY);
    snapshot.sessions[3].updated_at = Some(NOW - 400 * DAY);

    let all = sections(&snapshot, Scope::All, "", NOW);
    assert_eq!(
        all.iter().map(|s| s.label).collect::<Vec<_>>(),
        ["Today", "Yesterday", "Previous 7 days", "Older"]
    );

    let alpha = sections(&snapshot, Scope::Repo(1), "", NOW);
    assert_eq!(
        alpha.iter().map(|s| s.label).collect::<Vec<_>>(),
        ["Today", "Yesterday"]
    );
    assert_eq!(alpha[0].sessions.len(), 1);
}

/// A session with no timestamp is not old, it is unknown — so it goes last,
/// under a heading that says so rather than under "Older".
#[test]
fn an_undated_session_is_listed_rather_than_dropped() {
    let mut snapshot = snapshot();
    for session in &mut snapshot.sessions {
        session.updated_at = None;
    }

    let listed = sections(&snapshot, Scope::All, "", NOW);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].label, "No date");
    assert_eq!(listed[0].sessions.len(), 4);
}

/// A clock that has run ahead — or a session written a second into the future
/// — belongs at the top, not in a section of its own.
#[test]
fn a_session_dated_in_the_future_sits_with_todays() {
    let mut snapshot = snapshot();
    snapshot.sessions[0].updated_at = Some(NOW + 5 * DAY);

    let listed = sections(&snapshot, Scope::All, "", NOW);
    assert_eq!(listed[0].label, "Today");
    assert_eq!(snapshot.sessions[listed[0].sessions[0]].id, 10);
}

#[test]
fn the_session_filter_matches_title_id_and_tool() {
    let snapshot = snapshot();
    let count = |filter: &str| {
        sections(&snapshot, Scope::All, filter, NOW)
            .iter()
            .map(|s| s.sessions.len())
            .sum::<usize>()
    };

    assert_eq!(count("econnreset"), 1);
    assert_eq!(count("sess-12"), 1);
    assert_eq!(count("claude"), 4);
    assert_eq!(count("nothing at all"), 0);
}

/// A rescan can merge two repositories or drop one. The pane has to notice,
/// rather than sit there pointed at a repository that is not in the index.
#[test]
fn a_scope_says_whether_the_index_still_has_it() {
    let mut snapshot = snapshot();
    assert!(scope_exists(&snapshot, Scope::Repo(1)));
    assert!(!scope_exists(&snapshot, Scope::Repo(99)));
    // The three views across repositories are always available.
    assert!(scope_exists(&snapshot, Scope::Archived));

    assert_eq!(
        scope_title(&snapshot, Scope::Repo(1)),
        (
            "alpha".to_string(),
            Some("/Users/you/code/alpha".to_string())
        )
    );
    assert_eq!(
        scope_title(&snapshot, Scope::All),
        ("All repositories".to_string(), None)
    );

    snapshot.repositories.retain(|repo| repo.id != 1);
    assert!(!scope_exists(&snapshot, Scope::Repo(1)));
}

#[test]
fn a_session_with_no_title_is_labelled_by_the_id_a_user_would_type() {
    let untitled = session(20, None, None);
    assert_eq!(untitled.label(), "sess-20");

    let blank = session(21, None, Some("   "));
    assert_eq!(blank.label(), "sess-21");
}

/// The transcript cuts long entries and says how much it cut. The count is
/// taken once, here, rather than by walking the body on every repaint — and
/// characters are not bytes, so it has to be a count of characters.
#[test]
fn an_entry_carries_the_length_of_each_of_its_bodies() {
    let view = message_view(
        0,
        Role::ToolResult,
        "日本語".to_string(),
        None,
        None,
        Some("{\"text\":\"日本語\"}".to_string()),
    );

    assert_eq!(view.content_chars, 3);
    assert_eq!(view.raw_chars, 14);

    // An entry Mochi understood keeps no raw form, and counts nothing for it.
    let plain = message_view(1, Role::User, "hello".to_string(), None, None, None);
    assert_eq!(plain.content_chars, 5);
    assert_eq!(plain.raw_chars, 0);
}

/// FR-4.2: the row has to say why a session is odd, not just list it.
#[test]
fn rows_carry_their_warnings() {
    let mut archived = session(30, None, Some("old"));
    archived.parse_status = ParseStatus::Archived;
    assert_eq!(archived.notes(), ["archived"]);

    let mut gone = session(31, None, Some("moved"));
    gone.cwd_exists = false;
    assert_eq!(gone.notes(), ["no working directory"]);

    let mut both = session(32, None, Some("worst case"));
    both.parse_status = ParseStatus::Failed;
    both.cwd_exists = false;
    assert_eq!(both.notes(), ["unreadable", "no working directory"]);

    // A session with no recorded directory is not missing one.
    let mut never = session(33, None, Some("no cwd"));
    never.cwd = None;
    never.cwd_exists = false;
    assert!(never.notes().is_empty());
}
