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

//! The document both front ends read (`mochi_core::export`).
//!
//! The interface is written against this shape (`ui/src/types.ts`), so a
//! change here that nobody notices is a change that breaks the window.

use mochi_core::command::QuoteStyle;
use mochi_core::export::{document, ExportOptions, SCHEMA};
use mochi_core::index::{Index, RepositoryRecord, SessionRecord, SessionStatus};
use mochi_core::model::{Message, ParseStatus, Role, ToolId};

fn record(native_id: &str) -> SessionRecord {
    SessionRecord {
        id: 0,
        tool: ToolId::ClaudeCode,
        native_id: native_id.to_string(),
        repo_id: None,
        cwd: Some("/Users/you/code/my-repo".into()),
        git_branch: Some("main".into()),
        title: Some("a session".into()),
        model: Some("claude-opus-5".into()),
        started_at: Some(1_000),
        updated_at: Some(2_000),
        message_count: 0,
        tokens_in: 12,
        tokens_out: 34,
        status: SessionStatus::Finished,
        source_path: "/store/a.jsonl".into(),
        source_size: 2048,
        source_mtime: 2_000,
        schema_version: None,
        parse_status: ParseStatus::Ok,
        cwd_exists: false,
        parse_error: None,
    }
}

fn seeded() -> Index {
    let mut index = Index::open_in_memory().unwrap();
    let repo_id = index
        .upsert_repository(&RepositoryRecord {
            identity_key: "path:/Users/you/code/my-repo".into(),
            display_name: "my-repo".into(),
            root_path: "/Users/you/code/my-repo".into(),
            path_key: "/Users/you/code/my-repo".into(),
            ..Default::default()
        })
        .unwrap();

    let mut session = record("sess-1");
    session.repo_id = Some(repo_id);
    let id = index.upsert_session(&session).unwrap();
    index
        .replace_messages(
            id,
            &[
                Message::new(0, Role::User, "why does deploy fail? cat .env"),
                Message::new(
                    1,
                    Role::ToolResult,
                    "OPENAI_API_KEY=sk-EXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAMPLEEX",
                ),
            ],
        )
        .unwrap();
    index
}

#[test]
fn the_document_carries_every_field_the_interface_reads() {
    let value = document(&seeded(), ExportOptions::default()).unwrap();

    assert_eq!(value["schema"], SCHEMA);
    assert_eq!(value["masked"], true);
    assert_eq!(value["repositories"].as_array().unwrap().len(), 1);

    let repository = &value["repositories"][0];
    for key in ["id", "displayName", "rootPath", "isWorktree", "hidden"] {
        assert!(!repository[key].is_null(), "repository.{key} is missing");
    }

    let session = &value["sessions"][0];
    for key in [
        "id",
        "tool",
        "nativeId",
        "title",
        "model",
        "cwd",
        "cwdExists",
        "gitBranch",
        "startedAt",
        "updatedAt",
        "messageCount",
        "tokensIn",
        "tokensOut",
        "status",
        "parseStatus",
        "sourcePath",
        "sourceSize",
        "resume",
        "messages",
    ] {
        assert!(!session[key].is_null(), "session.{key} is missing");
    }
    // Present but null is the correct answer for these two here.
    assert!(session["repoId"].is_number());
    assert!(session["parseError"].is_null());

    for key in ["seq", "role", "content"] {
        assert!(
            !session["messages"][0][key].is_null(),
            "message.{key} is missing"
        );
    }
}

#[test]
fn secrets_are_masked_unless_the_caller_asks_otherwise() {
    // FR-8.5: an export gets committed, attached to bug reports and passed
    // around, so the default has to be the safe one.
    let index = seeded();

    let masked = document(&index, ExportOptions::default()).unwrap();
    let text = masked.to_string();
    assert!(
        !text.contains("sk-EXAMPLE"),
        "the default export leaked a key"
    );
    assert!(text.contains("redacted:openai_api_key"));

    let revealed = document(
        &index,
        ExportOptions {
            reveal_secrets: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(revealed["masked"], false);
    assert!(revealed.to_string().contains("sk-EXAMPLE"));
}

#[test]
fn a_session_whose_directory_is_gone_reports_why_it_cannot_resume() {
    // FR-7.6: the interface needs the reason, not just a disabled button.
    let value = document(&seeded(), ExportOptions::default()).unwrap();
    let resume = &value["sessions"][0]["resume"];

    assert_eq!(resume["available"], false);
    assert!(resume["command"].is_null());
    assert!(
        resume["reason"]
            .as_str()
            .unwrap()
            .contains("working directory"),
        "unhelpful reason: {resume}"
    );
}

#[test]
fn a_resumable_session_carries_the_command_the_core_would_run() {
    let mut index = Index::open_in_memory().unwrap();
    let directory = tempfile::tempdir().unwrap();

    let mut session = record("sess-live");
    session.cwd = Some(directory.path().to_string_lossy().into_owned());
    session.cwd_exists = true;
    index.upsert_session(&session).unwrap();

    let value = document(
        &index,
        ExportOptions {
            reveal_secrets: false,
            quote_style: QuoteStyle::Posix,
        },
    )
    .unwrap();
    let resume = &value["sessions"][0]["resume"];

    assert_eq!(resume["available"], true);
    let command = resume["command"].as_str().unwrap();
    assert!(command.starts_with("cd "), "got {command}");
    assert!(
        command.ends_with("claude --resume sess-live"),
        "got {command}"
    );
}

#[test]
fn an_archived_session_says_the_transcript_is_gone() {
    // FR-2.11: it is still readable, and that distinction is the whole point.
    let mut index = Index::open_in_memory().unwrap();
    let id = index.upsert_session(&record("sess-archived")).unwrap();
    index
        .replace_messages(id, &[Message::new(0, Role::User, "still here")])
        .unwrap();
    index.mark_archived(id).unwrap();

    let value = document(&index, ExportOptions::default()).unwrap();
    let session = &value["sessions"][0];

    assert_eq!(session["parseStatus"], "archived");
    assert_eq!(session["resume"]["available"], false);
    assert!(session["resume"]["reason"]
        .as_str()
        .unwrap()
        .contains("nothing to resume"));
    assert_eq!(
        session["messages"].as_array().unwrap().len(),
        1,
        "the transcript must survive the file"
    );
}
