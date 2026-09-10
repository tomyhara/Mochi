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

//! Codex CLI adapter, against golden files (NFR-5.2).

mod support;

use std::path::PathBuf;

use mochi_core::adapter::codex::CodexAdapter;
use mochi_core::adapter::{EnvSource, ToolAdapter};
use mochi_core::model::{ParseStatus, Role, SessionRef, ToolId};

fn home() -> PathBuf {
    support::golden_home("codex")
}

fn refs() -> Vec<SessionRef> {
    let env = EnvSource::with_home(home());
    let root = CodexAdapter
        .data_roots(&env)
        .into_iter()
        .next()
        .expect("a data root");
    let mut found = CodexAdapter.discover(&root).expect("discover");
    found.sort_by(|a, b| a.source_path.cmp(&b.source_path));
    found
}

fn parse_containing(fragment: &str) -> mochi_core::ParsedSession {
    let session = refs()
        .into_iter()
        .find(|r| r.source_path.to_string_lossy().contains(fragment))
        .unwrap_or_else(|| panic!("no fixture whose name contains {fragment}"));
    CodexAdapter.parse(&session).expect("parse")
}

#[test]
fn the_data_root_follows_codex_home() {
    let default = CodexAdapter.data_roots(&EnvSource::with_home("/home/user"));
    assert_eq!(default[0], PathBuf::from("/home/user/.codex"));

    let mut env = EnvSource::with_home("/home/user");
    env.set("CODEX_HOME", "/elsewhere/codex");
    assert_eq!(
        CodexAdapter.data_roots(&env)[0],
        PathBuf::from("/elsewhere/codex")
    );
}

#[test]
fn discovery_walks_the_date_directories() {
    // Codex files sit under sessions/YYYY/MM/DD, so discovery has to recurse
    // rather than list one directory.
    let found = refs();
    assert_eq!(
        found.len(),
        3,
        "{:?}",
        found.iter().map(|r| &r.source_path).collect::<Vec<_>>()
    );
    assert!(found.iter().all(|r| r.tool == ToolId::Codex));
    assert!(found
        .iter()
        .all(|r| r.source_path.to_string_lossy().contains("2026")));
}

#[test]
fn a_rollout_parses_into_a_transcript() {
    let parsed = parse_containing("99999999");

    assert_eq!(parsed.native_id, "019242aa-1111-7bbb-8ccc-000000000001");
    assert_eq!(parsed.cwd.as_deref(), Some("/Users/you/code/my-repo"));
    assert_eq!(parsed.git_branch.as_deref(), Some("main"));
    assert_eq!(parsed.schema_version.as_deref(), Some("0.31.0"));
    assert_eq!(parsed.parse_status, ParseStatus::Ok);

    let roles: Vec<Role> = parsed.messages.iter().map(|m| m.role).collect();
    assert_eq!(
        roles,
        vec![
            Role::User,
            Role::Thinking,
            Role::ToolCall,
            Role::ToolResult,
            Role::Assistant
        ]
    );
    assert_eq!(parsed.messages[2].tool_name.as_deref(), Some("shell"));
    assert!(parsed.messages[2].content.contains("rg -n ECONNRESET"));
    assert!(parsed.messages[3].content.contains("read ECONNRESET"));
    assert!(parsed.messages[4].content.contains("pool keeps sockets"));
}

#[test]
fn the_session_id_comes_from_inside_the_file() {
    // R-8: the uuid in the file name is not authoritative, and this fixture is
    // a file the user renamed. Resuming on the file name would fail.
    let parsed = parse_containing("renamed-by-the-user");
    assert_eq!(parsed.native_id, "019242bb-2222-7bbb-8ccc-000000000002");
    assert!(!parsed.native_id.contains("renamed"));
}

#[test]
fn token_usage_comes_from_the_count_event() {
    let parsed = parse_containing("99999999");
    assert_eq!(parsed.tokens_in, 5400);
    assert_eq!(parsed.tokens_out, 318);
}

#[test]
fn the_title_is_taken_from_the_first_prompt() {
    let parsed = parse_containing("99999999");
    assert_eq!(
        parsed.title.as_deref(),
        Some("The nightly job dies with ECONNRESET. Find out why.")
    );
}

#[test]
fn timestamps_are_epoch_milliseconds() {
    let parsed = parse_containing("99999999");
    assert_eq!(parsed.started_at, Some(1_788_265_800_000));
    assert!(parsed.updated_at.unwrap() > parsed.started_at.unwrap());
}

#[test]
fn a_rollout_without_metadata_still_lists() {
    // FR-2.7: one damaged file is not allowed to hide the others, and the
    // session must still be visible with whatever could be read.
    let parsed = parse_containing("00000000-0000-0000-0000-000000000000");
    assert_eq!(parsed.parse_status, ParseStatus::Partial);
    assert!(parsed.parse_error.is_some());
    assert!(
        !parsed.native_id.is_empty(),
        "without an id from the file, fall back to something stable so the row still exists"
    );
    assert!(parsed
        .messages
        .iter()
        .any(|m| m.content.contains("orphan turn")));
}

#[test]
fn resuming_a_session_without_an_id_is_refused() {
    // Better to disable the button than to run `codex resume` with a guess.
    let parsed = parse_containing("00000000-0000-0000-0000-000000000000");
    assert!(
        parsed.native_id.starts_with("unknown:"),
        "an id Mochi invented must be marked as such, got {}",
        parsed.native_id
    );
}

#[test]
fn parsing_never_touches_the_source_file() {
    let before = support::fingerprint(&home());
    for session in refs() {
        let _ = CodexAdapter.parse(&session);
    }
    assert_eq!(support::fingerprint(&home()), before);
}
