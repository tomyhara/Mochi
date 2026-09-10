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

//! Claude Code adapter, against golden files (NFR-5.2).

mod support;

use std::path::PathBuf;

use mochi_core::adapter::claude_code::{is_sidelined, ClaudeCodeAdapter};
use mochi_core::adapter::{EnvSource, ToolAdapter};
use mochi_core::model::{ParseStatus, Role, SessionRef, ToolId};

fn home() -> PathBuf {
    support::golden_home("claude_code")
}

fn refs() -> Vec<SessionRef> {
    let env = EnvSource::with_home(home());
    let adapter = ClaudeCodeAdapter;
    let root = adapter
        .data_roots(&env)
        .into_iter()
        .next()
        .expect("a data root");
    let mut found = adapter.discover(&root).expect("discover");
    found.sort_by(|a, b| a.source_path.cmp(&b.source_path));
    found
}

fn parse_named(name: &str) -> mochi_core::ParsedSession {
    let session = refs()
        .into_iter()
        .find(|r| {
            r.source_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(name)
        })
        .unwrap_or_else(|| panic!("no fixture starting with {name}"));
    ClaudeCodeAdapter.parse(&session).expect("parse")
}

#[test]
fn the_data_root_follows_claude_config_dir() {
    // FR-1.4: the environment variable wins over the default location.
    let adapter = ClaudeCodeAdapter;
    let default = adapter.data_roots(&EnvSource::with_home("/home/user"));
    assert_eq!(default[0], PathBuf::from("/home/user/.claude"));

    let mut env = EnvSource::with_home("/home/user");
    env.set("CLAUDE_CONFIG_DIR", "/elsewhere/claude");
    let overridden = adapter.data_roots(&env);
    assert_eq!(overridden[0], PathBuf::from("/elsewhere/claude"));
}

#[test]
fn discovery_finds_transcripts_and_skips_the_sidelined_ones() {
    let found = refs();
    let names: Vec<String> = found
        .iter()
        .map(|r| {
            r.source_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();

    assert_eq!(found.len(), 4, "found {names:?}");
    assert!(names.iter().all(|n| n.ends_with(".jsonl")));
    assert!(
        !names
            .iter()
            .any(|n| n.contains("orphaned") || n.contains("superseded")),
        "sidelined transcripts would bury the real sessions: {names:?}"
    );
    assert!(found.iter().all(|r| r.tool == ToolId::ClaudeCode));
    assert!(found.iter().all(|r| r.source_size > 0));
}

#[test]
fn sidelined_file_names_are_recognised() {
    assert!(is_sidelined("abc.orphaned-1756713600000-a1b2.jsonl"));
    assert!(is_sidelined("abc.jsonl.superseded-1756713600000"));
    assert!(!is_sidelined("11111111-2222-3333-4444-555555555555.jsonl"));
}

#[test]
fn a_complete_session_parses_into_a_transcript() {
    let parsed = parse_named("11111111");

    assert_eq!(parsed.tool, ToolId::ClaudeCode);
    assert_eq!(parsed.native_id, "11111111-2222-3333-4444-555555555555");
    assert_eq!(parsed.parse_status, ParseStatus::Ok);
    assert_eq!(parsed.git_branch.as_deref(), Some("main"));
    assert_eq!(parsed.model.as_deref(), Some("claude-opus-5"));
    assert_eq!(parsed.schema_version.as_deref(), Some("1.0.60"));

    let roles: Vec<Role> = parsed.messages.iter().map(|m| m.role).collect();
    assert_eq!(
        roles,
        vec![
            Role::User,
            Role::Thinking,
            Role::Assistant,
            Role::ToolCall,
            Role::ToolResult,
            Role::Assistant,
        ]
    );

    assert!(parsed.messages[0].content.contains("ECONNRESET"));
    assert_eq!(parsed.messages[3].tool_name.as_deref(), Some("Read"));
    assert!(
        parsed.messages[3].content.contains("src/fetch.ts"),
        "tool arguments belong in the searchable text: {:?}",
        parsed.messages[3].content
    );
    assert!(parsed.messages[4]
        .content
        .contains("export async function get"));

    // Sequence numbers are dense and ordered so the viewer can index into them.
    let seqs: Vec<i64> = parsed.messages.iter().map(|m| m.seq).collect();
    assert_eq!(seqs, (0..seqs.len() as i64).collect::<Vec<_>>());
}

#[test]
fn the_working_directory_comes_from_the_file_not_the_directory_name() {
    // The trap from doc/tool-integration.md §1.3. The project directory is
    // named "-Users-you-code-my-repo"; turning the dashes back into
    // separators would give /Users/you/code/my/repo, a directory that does
    // not exist. Only the cwd field is authoritative (FR-3.1).
    let parsed = parse_named("11111111");
    assert_eq!(parsed.cwd.as_deref(), Some("/Users/you/code/my-repo"));
}

#[test]
fn the_title_prefers_the_summary_the_tool_wrote() {
    // FR-4.3: use the tool's own title when it has one.
    let parsed = parse_named("11111111");
    assert_eq!(
        parsed.title.as_deref(),
        Some("Retry idempotent fetches on ECONNRESET")
    );
}

#[test]
fn the_title_falls_back_to_the_first_user_prompt() {
    let parsed = parse_named("33333333");
    assert_eq!(parsed.title.as_deref(), Some("hello"));
}

#[test]
fn token_counts_are_summed_over_the_session() {
    let parsed = parse_named("11111111");
    assert_eq!(parsed.tokens_in, 1240 + 2100);
    assert_eq!(parsed.tokens_out, 86 + 220);
}

#[test]
fn timestamps_are_read_as_epoch_milliseconds() {
    let parsed = parse_named("11111111");
    // 2026-09-01T10:00:00Z .. 2026-09-01T10:00:12Z
    assert_eq!(parsed.started_at, Some(1_788_256_800_000));
    assert_eq!(parsed.updated_at, Some(1_788_256_812_000));
    assert!(parsed.updated_at >= parsed.started_at);
}

#[test]
fn a_half_written_last_line_is_skipped_not_fatal() {
    // FR-2.6: the CLI appends while Mochi reads.
    let parsed = parse_named("22222222");
    assert_eq!(parsed.parse_status, ParseStatus::Partial);
    assert_eq!(parsed.native_id, "22222222-2222-3333-4444-555555555555");
    assert_eq!(
        parsed.messages.len(),
        2,
        "the two complete lines still come through"
    );
    assert!(
        parsed.parse_error.is_some(),
        "the reason should be visible in the UI"
    );
    assert_eq!(parsed.git_branch.as_deref(), Some("feature/retry"));
}

#[test]
fn unknown_event_types_are_kept_as_raw_json() {
    // FR-2.8 / R-1: a newer CLI writes shapes this build has never seen. It
    // must still list, and the unknown content must still be readable.
    let parsed = parse_named("33333333");
    assert_eq!(parsed.parse_status, ParseStatus::Partial);
    assert_eq!(parsed.schema_version.as_deref(), Some("2.5.0-next"));

    let raws: Vec<&str> = parsed
        .messages
        .iter()
        .filter_map(|m| m.raw.as_deref())
        .collect();
    assert!(
        raws.iter().any(|r| r.contains("checkpoint_v3")),
        "the unknown event should survive verbatim: {raws:?}"
    );
    assert!(
        parsed
            .messages
            .iter()
            .any(|m| m.content.contains("Still here.")),
        "a broken line in the middle must not truncate the rest of the file"
    );
}

#[test]
fn parsing_never_touches_the_source_file() {
    // FR-2.5, NFR-3.5 and item 9 of the definition of done.
    let before = support::fingerprint(&home());
    for session in refs() {
        let _ = ClaudeCodeAdapter.parse(&session);
    }
    assert_eq!(support::fingerprint(&home()), before);
}

#[test]
fn a_missing_file_is_an_error_about_that_file_only() {
    let session = SessionRef {
        tool: ToolId::ClaudeCode,
        source_path: home().join(".claude/projects/nope/nope.jsonl"),
        source_size: 0,
        source_mtime: 0,
    };
    assert!(ClaudeCodeAdapter.parse(&session).is_err());
}
