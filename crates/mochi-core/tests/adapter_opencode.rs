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

//! OpenCode adapter, against golden files (NFR-5.2).
//!
//! OpenCode changed its storage layout between versions, so the adapter has to
//! say which layout it is looking at before it reads anything
//! (doc/tool-integration.md §3.2, R-2).

mod support;

use std::fs;
use std::path::PathBuf;

use mochi_core::adapter::opencode::{detect_storage, OpenCodeAdapter, Storage};
use mochi_core::adapter::{EnvSource, ToolAdapter};
use mochi_core::model::{ParseStatus, Role, SessionRef, ToolId};

fn home() -> PathBuf {
    support::golden_home("opencode")
}

fn root() -> PathBuf {
    home().join(".local/share/opencode")
}

fn refs() -> Vec<SessionRef> {
    let mut found = OpenCodeAdapter.discover(&root()).expect("discover");
    found.sort_by(|a, b| a.source_path.cmp(&b.source_path));
    found
}

#[test]
fn the_data_root_is_the_same_on_every_platform() {
    // Note for Windows: OpenCode uses ~/.local/share there too, not %APPDATA%.
    let roots = OpenCodeAdapter.data_roots(&EnvSource::with_home("/home/user"));
    assert_eq!(roots[0], PathBuf::from("/home/user/.local/share/opencode"));

    let mut env = EnvSource::with_home("/home/user");
    env.set("XDG_DATA_HOME", "/elsewhere/share");
    assert_eq!(
        OpenCodeAdapter.data_roots(&env)[0],
        PathBuf::from("/elsewhere/share/opencode")
    );
}

#[test]
fn the_storage_layout_is_detected_before_reading() {
    assert_eq!(detect_storage(&root()), Storage::JsonFiles);

    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(detect_storage(tmp.path()), Storage::Missing);

    fs::write(tmp.path().join("opencode.db"), b"SQLite format 3\0").unwrap();
    assert_eq!(
        detect_storage(tmp.path()),
        Storage::Sqlite,
        "a newer install keeps sessions in SQLite"
    );
}

#[test]
fn the_sqlite_layout_reports_itself_rather_than_being_guessed_at() {
    // The schema has not been verified against a real install yet
    // (doc/tool-integration.md §5). Reading it on a guess would produce a
    // confidently wrong session list, so the adapter says so instead.
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("opencode.db"), b"SQLite format 3\0").unwrap();

    let err = OpenCodeAdapter.discover(tmp.path()).unwrap_err();
    let message = err.to_string();
    assert!(message.contains("SQLite"), "unhelpful message: {message}");
}

#[test]
fn discovery_lists_one_entry_per_session() {
    let found = refs();
    assert_eq!(found.len(), 2, "{:?}", found.iter().map(|r| &r.source_path).collect::<Vec<_>>());
    assert!(found.iter().all(|r| r.tool == ToolId::OpenCode));
}

#[test]
fn a_session_is_assembled_from_its_message_and_part_files() {
    let session = refs()
        .into_iter()
        .find(|r| r.source_path.to_string_lossy().contains("ses_EXAMPLE01"))
        .expect("fixture session");
    let parsed = OpenCodeAdapter.parse(&session).expect("parse");

    assert_eq!(parsed.native_id, "ses_EXAMPLE01");
    assert_eq!(parsed.cwd.as_deref(), Some("/Users/you/code/my-repo"));
    assert_eq!(parsed.title.as_deref(), Some("Trim the docker image"));
    assert_eq!(parsed.model.as_deref(), Some("claude-opus-5"));
    assert_eq!(parsed.schema_version.as_deref(), Some("0.5.29"));
    assert_eq!(parsed.parse_status, ParseStatus::Ok);
    assert_eq!(parsed.started_at, Some(1_756_900_000_000));
    assert_eq!(parsed.updated_at, Some(1_756_900_600_000));
    assert_eq!(parsed.tokens_in, 3200);
    assert_eq!(parsed.tokens_out, 410);

    let roles: Vec<Role> = parsed.messages.iter().map(|m| m.role).collect();
    assert_eq!(
        roles,
        vec![Role::User, Role::Thinking, Role::ToolCall, Role::ToolResult, Role::Assistant],
        "parts must come out in the order they were written, across messages"
    );
    assert!(parsed.messages[0].content.contains("1.2GB"));
    assert_eq!(parsed.messages[2].tool_name.as_deref(), Some("bash"));
    assert!(parsed.messages[2].content.contains("docker image ls"));
    assert!(parsed.messages[3].content.contains("1.19GB"));
}

#[test]
fn a_session_with_no_messages_is_still_a_session() {
    let session = refs()
        .into_iter()
        .find(|r| r.source_path.to_string_lossy().contains("ses_EXAMPLE02"))
        .expect("fixture session");
    let parsed = OpenCodeAdapter.parse(&session).expect("parse");
    assert_eq!(parsed.native_id, "ses_EXAMPLE02");
    assert!(parsed.messages.is_empty());
    assert_eq!(parsed.cwd.as_deref(), Some("/Users/you/code/scratch-pad"));
}

#[test]
fn parsing_never_touches_the_source_files() {
    let before = support::fingerprint(&home());
    for session in refs() {
        let _ = OpenCodeAdapter.parse(&session);
    }
    assert_eq!(support::fingerprint(&home()), before);
}

#[test]
fn the_auth_file_is_never_read() {
    // doc/tool-integration.md §3.2: auth.json holds provider credentials.
    // Nothing in Mochi has any reason to open it.
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("opencode");
    support::copy_tree(&root(), &store);
    fs::write(store.join("auth.json"), r#"{"anthropic":{"key":"sk-EXAMPLE"}}"#).unwrap();

    let found = OpenCodeAdapter.discover(&store).unwrap();
    assert!(
        !found.iter().any(|r| r.source_path.to_string_lossy().contains("auth.json")),
        "auth.json must never be treated as a session"
    );
    for session in &found {
        let parsed = OpenCodeAdapter.parse(session).unwrap();
        assert!(!parsed.messages.iter().any(|m| m.content.contains("sk-EXAMPLE")));
    }
}
