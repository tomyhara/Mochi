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

//! Grouping and filtering — the decisions the sidebar makes, made where they
//! can be checked without a window.

use mochi_core::model::{ParseStatus, ToolId};
use mochi_gui::view::{groups, RepoView, SessionView, Snapshot};

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

#[test]
fn sessions_sit_under_their_repository() {
    let snapshot = snapshot();
    let groups = groups(&snapshot, "");

    assert_eq!(
        groups.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(),
        ["alpha", "beta", "No repository"]
    );
    assert_eq!(groups[0].sessions.len(), 2);
    assert_eq!(groups[1].sessions.len(), 1);
}

/// FR-3.6: a session that belongs to no repository is the one people are
/// usually looking for, so it gets a heading of its own rather than vanishing.
#[test]
fn homeless_sessions_are_still_shown() {
    let snapshot = snapshot();
    let groups = groups(&snapshot, "");
    let loose = groups.last().unwrap();

    assert_eq!(loose.key, "unassigned");
    assert_eq!(loose.path, None);
    assert_eq!(snapshot.sessions[loose.sessions[0]].id, 13);
}

/// A repository the index still lists but whose sessions have all gone is a
/// fact worth seeing — until a filter is typed, when it is just noise.
#[test]
fn an_empty_repository_keeps_its_heading_until_a_filter_is_typed() {
    let mut snapshot = snapshot();
    snapshot.sessions.retain(|s| s.repo_id != Some(2));

    assert_eq!(groups(&snapshot, "").len(), 3);
    let filtered = groups(&snapshot, "deploy");
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].name, "alpha");
}

#[test]
fn the_filter_matches_title_id_and_tool() {
    let snapshot = snapshot();

    assert_eq!(groups(&snapshot, "econnreset")[0].sessions.len(), 1);
    assert_eq!(groups(&snapshot, "sess-12")[0].sessions.len(), 1);
    assert_eq!(
        groups(&snapshot, "claude")
            .iter()
            .map(|g| g.sessions.len())
            .sum::<usize>(),
        4
    );
    assert!(groups(&snapshot, "nothing at all").is_empty());
}

/// A session pointing at a repository the index no longer lists must not
/// disappear: it is still readable, and dropping it would look like data loss.
#[test]
fn a_session_whose_repository_is_gone_falls_back_to_no_repository() {
    let mut snapshot = snapshot();
    snapshot
        .sessions
        .push(session(14, Some(99), Some("orphan")));

    let groups = groups(&snapshot, "");
    let loose = groups.last().unwrap();
    assert_eq!(loose.name, "No repository");
    assert_eq!(loose.sessions.len(), 2);
}

#[test]
fn a_session_with_no_title_is_labelled_by_the_id_a_user_would_type() {
    let untitled = session(20, None, None);
    assert_eq!(untitled.label(), "sess-20");

    let blank = session(21, None, Some("   "));
    assert_eq!(blank.label(), "sess-21");
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
