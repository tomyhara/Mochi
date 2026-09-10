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

//! The index and its full-text search (FR-2.2, FR-2.11, FR-6, NFR-1.3, NFR-6.4).

use mochi_core::index::{
    Index, RepositoryRecord, SearchQuery, SessionOrder, SessionQuery, SessionRecord, SessionStatus,
    SNIPPET_END, SNIPPET_START,
};
use mochi_core::model::{Message, ParseStatus, Role, ToolId};

fn repo(name: &str, path: &str) -> RepositoryRecord {
    RepositoryRecord {
        identity_key: format!("path:{path}"),
        display_name: name.to_string(),
        root_path: path.to_string(),
        path_key: path.to_string(),
        ..Default::default()
    }
}

fn session(tool: ToolId, native_id: &str, repo_id: Option<i64>) -> SessionRecord {
    SessionRecord {
        id: 0,
        tool,
        native_id: native_id.to_string(),
        repo_id,
        cwd: Some("/Users/you/code/my-repo".into()),
        git_branch: Some("main".into()),
        title: Some(format!("session {native_id}")),
        model: Some("claude-opus-5".into()),
        started_at: Some(1_000),
        updated_at: Some(2_000),
        message_count: 0,
        tokens_in: 0,
        tokens_out: 0,
        status: SessionStatus::Finished,
        source_path: format!("/store/{native_id}.jsonl"),
        source_size: 1024,
        source_mtime: 2_000,
        schema_version: None,
        parse_status: ParseStatus::Ok,
        cwd_exists: true,
        parse_error: None,
    }
}

fn msg(seq: i64, role: Role, content: &str) -> Message {
    Message::new(seq, role, content)
}

fn seeded() -> (Index, i64, i64) {
    let mut index = Index::open_in_memory().unwrap();
    let repo_id = index
        .upsert_repository(&repo("my-repo", "/Users/you/code/my-repo"))
        .unwrap();
    let session_id = index
        .upsert_session(&session(ToolId::ClaudeCode, "sess-1", Some(repo_id)))
        .unwrap();
    index
        .replace_messages(
            session_id,
            &[
                msg(0, Role::User, "the nightly job dies with ECONNRESET"),
                msg(
                    1,
                    Role::Assistant,
                    "the pool keeps sockets longer than the gateway does",
                ),
                msg(
                    2,
                    Role::ToolResult,
                    "logs/nightly.log:118: Error: read ECONNRESET",
                ),
            ],
        )
        .unwrap();
    (index, repo_id, session_id)
}

#[test]
fn a_fresh_index_is_empty_but_usable() {
    let index = Index::open_in_memory().unwrap();
    assert_eq!(index.stats().unwrap().sessions, 0);
    assert!(index
        .list_sessions(&SessionQuery::default())
        .unwrap()
        .is_empty());
    assert!(index
        .search(&SearchQuery {
            text: "anything".into(),
            ..Default::default()
        })
        .unwrap()
        .is_empty());
}

#[test]
fn repositories_are_keyed_on_identity_not_on_row_order() {
    let mut index = Index::open_in_memory().unwrap();
    let first = index
        .upsert_repository(&repo("my-repo", "/Users/you/code/my-repo"))
        .unwrap();

    let mut renamed = repo("Billing", "/Users/you/code/my-repo");
    renamed.display_name = "Billing".into();
    let second = index.upsert_repository(&renamed).unwrap();

    assert_eq!(
        first, second,
        "the same identity must update, not duplicate"
    );
    let repos = index.list_repositories().unwrap();
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0].display_name, "Billing");
}

#[test]
fn sessions_are_keyed_on_tool_and_native_id() {
    let mut index = Index::open_in_memory().unwrap();
    let a = index
        .upsert_session(&session(ToolId::ClaudeCode, "same-id", None))
        .unwrap();
    let b = index
        .upsert_session(&session(ToolId::ClaudeCode, "same-id", None))
        .unwrap();
    let c = index
        .upsert_session(&session(ToolId::Codex, "same-id", None))
        .unwrap();

    assert_eq!(a, b);
    assert_ne!(a, c, "two tools may use the same id for different sessions");
    assert_eq!(index.stats().unwrap().sessions, 2);
}

#[test]
fn a_session_round_trips_through_the_index() {
    let (index, repo_id, session_id) = seeded();
    let stored = index.session(session_id).unwrap().expect("session");

    assert_eq!(stored.native_id, "sess-1");
    assert_eq!(stored.tool, ToolId::ClaudeCode);
    assert_eq!(stored.repo_id, Some(repo_id));
    assert_eq!(
        stored.message_count, 3,
        "the count follows the messages that were stored"
    );
    assert_eq!(stored.cwd.as_deref(), Some("/Users/you/code/my-repo"));
    assert!(stored.cwd_exists);

    let messages = index.messages(session_id).unwrap();
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0].role, Role::User);
    assert_eq!(
        messages[2].content,
        "logs/nightly.log:118: Error: read ECONNRESET"
    );
}

#[test]
fn full_text_search_finds_a_term_across_a_session() {
    let (index, _, session_id) = seeded();
    let hits = index
        .search(&SearchQuery {
            text: "ECONNRESET".into(),
            ..Default::default()
        })
        .unwrap();

    assert_eq!(hits.len(), 2, "two messages mention it");
    assert!(hits.iter().all(|h| h.session_id == session_id));
    assert!(hits[0].snippet.contains(SNIPPET_START) && hits[0].snippet.contains(SNIPPET_END));
    assert!(
        hits.iter()
            .any(|h| h.repo_display.as_deref() == Some("my-repo")),
        "FR-6.3 wants the repository name beside the hit"
    );
    assert!(hits.iter().any(|h| h.role == "tool_result"));
}

#[test]
fn search_snippets_are_masked_before_they_are_cut() {
    // NFR-3.3, and the reason the order matters: a snippet taken out of the
    // raw text and masked afterwards shows however much of the key fitted
    // inside the window. Masking has to happen first.
    let mut index = Index::open_in_memory().unwrap();
    let session_id = index
        .upsert_session(&session(ToolId::ClaudeCode, "leak", None))
        .unwrap();
    index
        .replace_messages(
            session_id,
            &[msg(
                0,
                Role::ToolResult,
                "deploy failed. OPENAI_API_KEY=sk-EXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAMPLEEX is set in .env",
            )],
        )
        .unwrap();

    let hits = index
        .search(&SearchQuery {
            text: "OPENAI_API_KEY".into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert!(
        !hits[0].snippet.contains("sk-EXAMPLE"),
        "the snippet leaked the key: {}",
        hits[0].snippet
    );
    assert!(
        hits[0].snippet.contains("redacted"),
        "the reader should see that something was removed: {}",
        hits[0].snippet
    );

    // The toggle still works for someone who needs to see the real value.
    let revealed = index
        .search(&SearchQuery {
            text: "OPENAI_API_KEY".into(),
            reveal_secrets: true,
            ..Default::default()
        })
        .unwrap();
    assert!(revealed[0].snippet.contains("sk-EXAMPLE"));
}

#[test]
fn a_masked_hit_still_gets_a_useful_snippet() {
    // The matched term can disappear into the masked span. The row must still
    // come back with something readable rather than an empty line.
    let mut index = Index::open_in_memory().unwrap();
    let session_id = index
        .upsert_session(&session(ToolId::Codex, "inside", None))
        .unwrap();
    index
        .replace_messages(
            session_id,
            &[msg(
                0,
                Role::User,
                "token check: ghp_EXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAM",
            )],
        )
        .unwrap();

    let hits = index
        .search(&SearchQuery {
            text: "ghp_EXAMPLEEXAMPLE".into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert!(
        !hits[0].snippet.contains("ghp_EXAMPLE"),
        "got {}",
        hits[0].snippet
    );
    assert!(!hits[0].snippet.trim().is_empty());
}

#[test]
fn search_is_case_insensitive() {
    let (index, _, _) = seeded();
    let lower = index
        .search(&SearchQuery {
            text: "econnreset".into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(lower.len(), 2);
}

#[test]
fn search_finds_japanese_without_spaces() {
    // NFR-6.4 ③: the UI is English, the content is not. A word-boundary
    // tokeniser would find nothing here.
    let mut index = Index::open_in_memory().unwrap();
    let session_id = index
        .upsert_session(&session(ToolId::Codex, "jp", None))
        .unwrap();
    index
        .replace_messages(
            session_id,
            &[msg(
                0,
                Role::User,
                "接続がリセットされました。リトライ間隔を2秒にします。",
            )],
        )
        .unwrap();

    for term in ["リセット", "リトライ間隔", "接続"] {
        let hits = index
            .search(&SearchQuery {
                text: term.into(),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(hits.len(), 1, "searching for {term} found {hits:?}");
    }
}

#[test]
fn search_can_be_narrowed_to_a_repository_a_session_or_a_tool() {
    let (mut index, repo_id, session_id) = seeded();
    let other_repo = index
        .upsert_repository(&repo("other", "/Users/you/code/other"))
        .unwrap();
    let other = index
        .upsert_session(&session(ToolId::Codex, "sess-2", Some(other_repo)))
        .unwrap();
    index
        .replace_messages(other, &[msg(0, Role::User, "another ECONNRESET report")])
        .unwrap();

    let all = index
        .search(&SearchQuery {
            text: "ECONNRESET".into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(all.len(), 3);

    let by_repo = index
        .search(&SearchQuery {
            text: "ECONNRESET".into(),
            repo_id: Some(repo_id),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(by_repo.len(), 2);

    let by_session = index
        .search(&SearchQuery {
            text: "ECONNRESET".into(),
            session_id: Some(other),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(by_session.len(), 1);

    let by_tool = index
        .search(&SearchQuery {
            text: "ECONNRESET".into(),
            tool: Some(ToolId::Codex),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(by_tool.len(), 1);
    assert_eq!(by_tool[0].session_id, other);
    let _ = session_id;
}

#[test]
fn a_phrase_search_is_not_an_and_search() {
    // FR-6.6
    let (index, _, _) = seeded();
    let phrase = index
        .search(&SearchQuery {
            text: "\"keeps sockets\"".into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(phrase.len(), 1);

    let nonsense = index
        .search(&SearchQuery {
            text: "\"sockets keeps\"".into(),
            ..Default::default()
        })
        .unwrap();
    assert!(nonsense.is_empty(), "word order matters in a quoted phrase");
}

#[test]
fn search_input_cannot_break_the_query() {
    // Whatever a user types is a search term, never query syntax that errors
    // or reaches further than it should.
    let (index, _, _) = seeded();
    for text in [
        "ECONNRESET*",
        "a\"b",
        "(",
        "NEAR/",
        "'; drop table sessions; --",
        "",
    ] {
        let result = index.search(&SearchQuery {
            text: text.into(),
            ..Default::default()
        });
        assert!(result.is_ok(), "query {text:?} failed: {:?}", result.err());
    }
    assert_eq!(
        index.stats().unwrap().sessions,
        1,
        "the table is still there"
    );
}

#[test]
fn replacing_messages_replaces_what_is_searchable() {
    let (mut index, _, session_id) = seeded();
    index
        .replace_messages(
            session_id,
            &[msg(0, Role::User, "completely different content")],
        )
        .unwrap();

    assert!(index
        .search(&SearchQuery {
            text: "ECONNRESET".into(),
            ..Default::default()
        })
        .unwrap()
        .is_empty());
    assert_eq!(
        index
            .search(&SearchQuery {
                text: "completely".into(),
                ..Default::default()
            })
            .unwrap()
            .len(),
        1
    );
    assert_eq!(index.messages(session_id).unwrap().len(), 1);
    assert_eq!(index.session(session_id).unwrap().unwrap().message_count, 1);
}

#[test]
fn session_listing_filters_and_orders() {
    let mut index = Index::open_in_memory().unwrap();
    let repo_id = index.upsert_repository(&repo("r", "/r")).unwrap();

    let mut old = session(ToolId::ClaudeCode, "old", Some(repo_id));
    old.updated_at = Some(1_000);
    old.message_count = 50;
    let mut recent = session(ToolId::Codex, "recent", Some(repo_id));
    recent.updated_at = Some(9_000);
    recent.message_count = 2;
    let mut elsewhere = session(ToolId::Codex, "elsewhere", None);
    elsewhere.updated_at = Some(5_000);

    index.upsert_session(&old).unwrap();
    index.upsert_session(&recent).unwrap();
    index.upsert_session(&elsewhere).unwrap();

    let by_recency = index.list_sessions(&SessionQuery::default()).unwrap();
    let ids: Vec<&str> = by_recency.iter().map(|s| s.native_id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["recent", "elsewhere", "old"],
        "most recently touched first (FR-4.4)"
    );

    let in_repo = index
        .list_sessions(&SessionQuery {
            repo_id: Some(repo_id),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(in_repo.len(), 2);

    let codex_only = index
        .list_sessions(&SessionQuery {
            tool: Some(ToolId::Codex),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(codex_only.len(), 2);

    let since = index
        .list_sessions(&SessionQuery {
            since: Some(5_000),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(since.len(), 2);

    let busiest = index
        .list_sessions(&SessionQuery {
            order: SessionOrder::MessagesDesc,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(busiest[0].native_id, "old");

    let limited = index
        .list_sessions(&SessionQuery {
            limit: Some(1),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(limited.len(), 1);
}

#[test]
fn an_archived_session_keeps_its_transcript() {
    // FR-2.11 and R-4: the CLI deleted its own file after 30 days. Mochi's copy
    // is now the only one, so archiving must never drop content.
    let (mut index, _, session_id) = seeded();
    index.mark_archived(session_id).unwrap();

    let stored = index.session(session_id).unwrap().unwrap();
    assert_eq!(stored.parse_status, ParseStatus::Archived);
    assert_eq!(index.messages(session_id).unwrap().len(), 3);
    assert_eq!(
        index
            .search(&SearchQuery {
                text: "ECONNRESET".into(),
                ..Default::default()
            })
            .unwrap()
            .len(),
        2,
        "an archived session stays searchable"
    );
    assert_eq!(index.stats().unwrap().archived_sessions, 1);
}

#[test]
fn the_index_survives_being_closed_and_reopened() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("index.sqlite3");

    {
        let mut index = Index::open(&path).unwrap();
        let session_id = index
            .upsert_session(&session(ToolId::ClaudeCode, "persist", None))
            .unwrap();
        index
            .replace_messages(session_id, &[msg(0, Role::User, "durable ECONNRESET")])
            .unwrap();
    }

    let index = Index::open(&path).unwrap();
    assert_eq!(index.stats().unwrap().sessions, 1);
    assert_eq!(
        index
            .search(&SearchQuery {
                text: "durable".into(),
                ..Default::default()
            })
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn opening_an_index_twice_in_a_row_migrates_only_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("index.sqlite3");
    Index::open(&path).unwrap();
    Index::open(&path).unwrap();
    assert!(Index::open(&path).is_ok());
}

#[cfg(unix)]
#[test]
fn the_index_file_is_readable_only_by_its_owner() {
    // NFR-3.4: it holds the same secrets as the session files.
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("index.sqlite3");
    Index::open(&path).unwrap();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "index permissions were {mode:o}");
}

#[test]
fn stats_report_what_the_ui_shows() {
    let (mut index, _, _) = seeded();
    let mut failed = session(ToolId::OpenCode, "broken", None);
    failed.parse_status = ParseStatus::Failed;
    failed.parse_error = Some("unreadable".into());
    failed.source_size = 4096;
    index.upsert_session(&failed).unwrap();

    let stats = index.stats().unwrap();
    assert_eq!(stats.sessions, 2);
    assert_eq!(stats.repositories, 1);
    assert_eq!(stats.messages, 3);
    assert_eq!(stats.failed_sessions, 1);
    assert_eq!(stats.total_source_bytes, 1024 + 4096, "FR-8.2 disk usage");
}
