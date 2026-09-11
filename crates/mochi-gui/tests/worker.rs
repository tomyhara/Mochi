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

//! The index thread, exercised without a window.
//!
//! These are the parts of the window that can get a user's secrets wrong or
//! show them somebody else's session, so they are checked against a real
//! index rather than a stub.

use std::time::Duration;

use mochi_core::index::{Index, RepositoryRecord, SessionRecord, SessionStatus};
use mochi_core::model::{Message, ParseStatus, Role, ToolId};
use mochi_gui::view::Snapshot;
use mochi_gui::worker::{Request, Response, Worker};

const KEY: &str = "OPENAI_API_KEY=sk-EXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAMPLEEX";
const WAIT: Duration = Duration::from_secs(20);

/// An index on disk with one repository, one session and two messages, one of
/// which contains something that must never be shown by accident.
fn seeded(path: &std::path::Path) {
    let mut index = Index::open(path).unwrap();
    let repo_id = index
        .upsert_repository(&RepositoryRecord {
            identity_key: "path:/Users/you/code/alpha".into(),
            display_name: "alpha".into(),
            root_path: "/Users/you/code/alpha".into(),
            path_key: "/Users/you/code/alpha".into(),
            ..Default::default()
        })
        .unwrap();

    let session = SessionRecord {
        id: 0,
        tool: ToolId::ClaudeCode,
        native_id: "sess-1".into(),
        repo_id: Some(repo_id),
        cwd: Some("/Users/you/code/alpha".into()),
        git_branch: Some("main".into()),
        title: Some(format!("why does deploy fail, {KEY}")),
        model: Some("claude-opus-5".into()),
        started_at: Some(1_000),
        updated_at: Some(2_000),
        message_count: 2,
        tokens_in: 1,
        tokens_out: 2,
        status: SessionStatus::Finished,
        source_path: "/store/a.jsonl".into(),
        source_size: 2048,
        source_mtime: 2_000,
        schema_version: None,
        parse_status: ParseStatus::Ok,
        cwd_exists: false,
        parse_error: None,
    };
    let id = index.upsert_session(&session).unwrap();
    index
        .replace_messages(
            id,
            &[
                Message::new(0, Role::User, "why does deploy fail? cat .env"),
                Message::new(1, Role::ToolResult, KEY),
            ],
        )
        .unwrap();
}

struct Fixture {
    _dir: tempfile::TempDir,
    worker: Worker,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("index.sqlite3");
    seeded(&db);
    // `home` points at the empty fixture tree rather than at whoever is
    // running the tests. Nothing here should scan at all — the index is not
    // empty — and this is what proves it if that ever changes.
    let worker = Worker::spawn(Some(db), Some(dir.path().to_path_buf()), || {});
    Fixture { _dir: dir, worker }
}

fn load(fixture: &Fixture, reveal_secrets: bool) -> Snapshot {
    fixture.worker.send(Request::Load { reveal_secrets });
    loop {
        match fixture.worker.recv_timeout(WAIT).expect("no answer") {
            Response::Loaded(snapshot) => return *snapshot,
            Response::Busy(_) => continue,
            Response::Scanned(_) => panic!("a populated index should not have been rescanned"),
            other => panic!("unexpected answer: {other:?}"),
        }
    }
}

#[test]
fn loading_answers_with_repositories_and_sessions() {
    let fixture = fixture();
    let snapshot = load(&fixture, false);

    assert!(snapshot.masked);
    assert_eq!(snapshot.repositories.len(), 1);
    assert_eq!(snapshot.repositories[0].display_name, "alpha");
    assert_eq!(snapshot.sessions.len(), 1);
    assert_eq!(snapshot.stats.messages, 2);
    assert!(snapshot.index_path.is_some());
}

/// NFR-3.3. A title is read out of a session file like anything else, so it
/// can carry a key just as a message can.
#[test]
fn titles_are_masked_unless_the_reader_asks() {
    let fixture = fixture();

    let masked = load(&fixture, false);
    let title = masked.sessions[0].title.clone().unwrap();
    assert!(!title.contains("sk-EXAMPLE"), "a key reached the window");
    assert!(title.starts_with("why does deploy fail"));

    let revealed = load(&fixture, true);
    assert!(!revealed.masked);
    assert!(revealed.sessions[0].title.clone().unwrap().contains(KEY));
}

#[test]
fn transcripts_are_masked_unless_the_reader_asks() {
    let fixture = fixture();
    let id = load(&fixture, false).sessions[0].id;

    fixture.worker.send(Request::Messages {
        session_id: id,
        reveal_secrets: false,
    });
    let Some(Response::Messages { messages, .. }) = fixture.worker.recv_timeout(WAIT) else {
        panic!("no transcript came back");
    };
    assert_eq!(messages.len(), 2);
    assert!(!messages[1].content.contains("sk-EXAMPLE"));

    fixture.worker.send(Request::Messages {
        session_id: id,
        reveal_secrets: true,
    });
    let Some(Response::Messages { messages, .. }) = fixture.worker.recv_timeout(WAIT) else {
        panic!("no transcript came back");
    };
    assert_eq!(messages[1].content, KEY);
}

#[test]
fn searching_answers_with_the_query_it_was_given() {
    let fixture = fixture();
    load(&fixture, false);

    fixture.worker.send(Request::Search {
        text: "deploy".into(),
        reveal_secrets: false,
    });
    let Some(Response::Hits { text, hits }) = fixture.worker.recv_timeout(WAIT) else {
        panic!("no hits came back");
    };
    // The window throws away answers to questions it has stopped asking, so
    // the query has to come back with them.
    assert_eq!(text, "deploy");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].repo_display.as_deref(), Some("alpha"));
    assert!(hits[0].snippet.to_lowercase().contains("deploy"));
}

/// A snippet is cut out of the message text, so a snippet of unmasked text can
/// show half a key even when the window asked for masking.
#[test]
fn search_snippets_are_masked_too() {
    let fixture = fixture();
    load(&fixture, false);

    fixture.worker.send(Request::Search {
        text: "OPENAI_API_KEY".into(),
        reveal_secrets: false,
    });
    let Some(Response::Hits { hits, .. }) = fixture.worker.recv_timeout(WAIT) else {
        panic!("no hits came back");
    };
    assert!(!hits.is_empty());
    for hit in hits {
        assert!(
            !hit.snippet.contains("sk-EXAMPLE"),
            "a key reached the window"
        );
    }
}

/// FR-7.6 / FR-2.11: the rail has to say why a session cannot be resumed, and
/// the reason travels with the session rather than being worked out twice.
#[test]
fn a_session_whose_directory_is_gone_carries_the_reason_instead_of_a_command() {
    let fixture = fixture();
    let snapshot = load(&fixture, false);

    match &snapshot.sessions[0].resume {
        Ok(command) => panic!("expected no command, got {command}"),
        Err(reason) => assert!(reason.contains("no longer exists"), "{reason}"),
    }
}

/// An index that cannot be opened is the first thing a user sees when
/// something is wrong, so the answer has to name the file rather than fail
/// silently.
#[test]
fn an_unusable_index_is_reported_rather_than_hidden() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("not-a-directory");
    std::fs::write(&db, b"this is not a database").unwrap();

    let worker = Worker::spawn(Some(db.join("index.sqlite3")), None, || {});
    worker.send(Request::Load {
        reveal_secrets: false,
    });

    loop {
        match worker.recv_timeout(WAIT).expect("no answer") {
            Response::Failed(message) => {
                assert!(message.contains("index"), "{message}");
                break;
            }
            Response::Busy(_) => continue,
            other => panic!("expected a failure, got {other:?}"),
        }
    }
}
