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

//! Codex CLI adapter.
//!
//! Layout (see `doc/tool-integration.md` §2):
//! `<root>/sessions/YYYY/MM/DD/rollout-<timestamp>-<uuid>.jsonl`. The first
//! line carries session metadata; the rest are conversation and tool events.
//!
//! The uuid in the file name is not authoritative — files can be renamed, and
//! `codex resume` takes the id recorded inside the file (R-8).

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::adapter::util::{
    compact_json, first_string, flatten_text, read_jsonl, session_ref, timestamp_ms,
    title_from_prompt, Line,
};
use crate::adapter::{EnvSource, ToolAdapter};
use crate::command::{CommandSpec, ResumeTarget};
use crate::model::{Message, ParseStatus, ParsedSession, Role, SessionRef, ToolId};
use crate::{Error, Result};

#[derive(Debug, Default, Clone, Copy)]
pub struct CodexAdapter;

impl ToolAdapter for CodexAdapter {
    fn id(&self) -> ToolId {
        ToolId::Codex
    }

    fn data_roots(&self, env: &EnvSource) -> Vec<PathBuf> {
        if let Some(dir) = env.var("CODEX_HOME") {
            return vec![PathBuf::from(dir)];
        }
        vec![env.home().join(".codex")]
    }

    fn discover(&self, root: &Path) -> Result<Vec<SessionRef>> {
        let sessions = root.join("sessions");
        if !sessions.is_dir() {
            return Ok(Vec::new());
        }

        let mut found = Vec::new();
        // The tree is sessions/YYYY/MM/DD, and new date directories appear
        // while Mochi is running, so this has to recurse rather than list.
        for entry in walkdir::WalkDir::new(&sessions)
            .into_iter()
            .filter_map(std::result::Result::ok)
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy();
            if !name.starts_with("rollout-") || !name.ends_with(".jsonl") {
                continue;
            }
            if let Ok(session) = session_ref(ToolId::Codex, entry.path()) {
                found.push(session);
            }
        }
        found.sort_by(|a, b| a.source_path.cmp(&b.source_path));
        Ok(found)
    }

    fn parse(&self, session: &SessionRef) -> Result<ParsedSession> {
        let lines = read_jsonl(&session.source_path)?;
        let mut out = ParsedSession::empty(ToolId::Codex, String::new());
        out.source_path = session.source_path.clone();
        out.source_size = session.source_size;
        out.source_mtime = session.source_mtime;

        let mut problems: Vec<String> = Vec::new();
        let mut first_prompt: Option<String> = None;
        let mut seq = 0i64;

        for line in &lines {
            let value = match line {
                Line::Parsed(value) => value,
                Line::Broken { complete } => {
                    problems.push(if *complete {
                        "a line could not be read as JSON".to_string()
                    } else {
                        "the last line was still being written and was skipped".to_string()
                    });
                    continue;
                }
            };

            let timestamp = value.get("timestamp").and_then(timestamp_ms);
            if let Some(ts) = timestamp {
                out.started_at = Some(out.started_at.map_or(ts, |current: i64| current.min(ts)));
                out.updated_at = Some(out.updated_at.map_or(ts, |current: i64| current.max(ts)));
            }

            let kind = value.get("type").and_then(Value::as_str).unwrap_or("");
            let payload = value.get("payload").unwrap_or(value);

            match kind {
                "session_meta" | "session_info" => {
                    read_meta(payload, &mut out);
                    continue;
                }
                "turn_context" => {
                    if out.model.is_none() {
                        out.model = first_string(payload, &["model"]).map(str::to_string);
                    }
                    if out.cwd.is_none() {
                        out.cwd = first_string(payload, &["cwd"]).map(str::to_string);
                    }
                    continue;
                }
                "event_msg" => {
                    read_token_count(payload, &mut out);
                    continue;
                }
                _ => {}
            }

            let item_type = payload.get("type").and_then(Value::as_str).unwrap_or("");
            let push = |mut entry: Message, out: &mut ParsedSession, seq: &mut i64| {
                entry.seq = *seq;
                entry.timestamp = timestamp;
                out.messages.push(entry);
                *seq += 1;
            };

            match item_type {
                "message" => {
                    let role = match payload
                        .get("role")
                        .and_then(Value::as_str)
                        .unwrap_or("user")
                    {
                        "assistant" => Role::Assistant,
                        "system" | "developer" => Role::System,
                        _ => Role::User,
                    };
                    let text = payload.get("content").map(flatten_text).unwrap_or_default();
                    if role == Role::User && first_prompt.is_none() {
                        first_prompt = Some(text.clone());
                    }
                    push(Message::new(0, role, text), &mut out, &mut seq);
                }
                "reasoning" => {
                    let text = payload
                        .get("summary")
                        .or_else(|| payload.get("content"))
                        .map(flatten_text)
                        .unwrap_or_default();
                    push(Message::new(0, Role::Thinking, text), &mut out, &mut seq);
                }
                "function_call" | "local_shell_call" | "custom_tool_call" => {
                    let mut entry = Message::new(
                        0,
                        Role::ToolCall,
                        payload
                            .get("arguments")
                            .or_else(|| payload.get("action"))
                            .or_else(|| payload.get("input"))
                            .map(compact_json)
                            .unwrap_or_default(),
                    );
                    entry.tool_name = first_string(payload, &["name", "tool"]).map(str::to_string);
                    entry.native_id = first_string(payload, &["call_id"]).map(str::to_string);
                    entry.raw = Some(payload.to_string());
                    push(entry, &mut out, &mut seq);
                }
                "function_call_output" | "local_shell_call_output" | "custom_tool_call_output" => {
                    let mut entry = Message::new(
                        0,
                        Role::ToolResult,
                        payload
                            .get("output")
                            .or_else(|| payload.get("result"))
                            .map(flatten_text)
                            .unwrap_or_default(),
                    );
                    entry.parent_id = first_string(payload, &["call_id"]).map(str::to_string);
                    entry.raw = Some(payload.to_string());
                    push(entry, &mut out, &mut seq);
                }
                other => {
                    // FR-2.8: unknown shapes stay visible as raw JSON rather
                    // than being dropped or crashing the parse.
                    let mut entry = Message::new(0, Role::System, flatten_text(payload));
                    entry.raw = Some(value.to_string());
                    push(entry, &mut out, &mut seq);
                    problems.push(format!("unrecognised item type {other:?}"));
                }
            }
        }

        if out.native_id.is_empty() {
            // R-8: with no id in the file there is nothing safe to resume, so
            // the id is marked as invented and resume_command refuses it.
            let stem = session
                .source_path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "session".to_string());
            out.native_id = format!("unknown:{stem}");
            problems.push("no session id was recorded in the file".to_string());
        }

        out.title = first_prompt.as_deref().map(title_from_prompt);

        if !problems.is_empty() {
            problems.dedup();
            out.parse_status = if out.messages.is_empty() {
                ParseStatus::Failed
            } else {
                ParseStatus::Partial
            };
            out.parse_error = Some(problems.join("; "));
        }
        if lines.is_empty() {
            out.parse_status = ParseStatus::Failed;
            out.parse_error = Some("the file held no readable lines".to_string());
        }

        Ok(out)
    }

    fn resume_command(&self, target: &ResumeTarget) -> Result<CommandSpec> {
        if target.native_id.starts_with("unknown:") {
            return Err(Error::invalid(
                "this rollout has no session id recorded in it, so it cannot be resumed",
            ));
        }
        Ok(CommandSpec::new(
            target.program(self.executable_name()),
            vec!["resume".to_string(), target.native_id.clone()],
            target.cwd.clone(),
        ))
    }

    fn new_session_command(&self, cwd: &Path, executable: Option<&Path>) -> Result<CommandSpec> {
        let program = executable
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.executable_name().to_string());
        Ok(CommandSpec::new(program, Vec::new(), cwd))
    }

    fn executable_name(&self) -> &'static str {
        "codex"
    }
}

/// The first line of a rollout. Field names have moved between versions, so
/// several spellings are accepted for each (R-1).
fn read_meta(payload: &Value, out: &mut ParsedSession) {
    if let Some(id) = first_string(payload, &["id", "session_id", "sessionId", "uuid"]) {
        out.native_id = id.to_string();
    }
    if out.cwd.is_none() {
        out.cwd =
            first_string(payload, &["cwd", "workdir", "working_directory"]).map(str::to_string);
    }
    if out.schema_version.is_none() {
        out.schema_version =
            first_string(payload, &["cli_version", "version", "codex_version"]).map(str::to_string);
    }
    if out.model.is_none() {
        out.model = first_string(payload, &["model"]).map(str::to_string);
    }
    if let Some(git) = payload.get("git") {
        if out.git_branch.is_none() {
            out.git_branch = first_string(git, &["branch"]).map(str::to_string);
        }
    }
}

/// Token totals arrive as an event carrying a running total, so the last one
/// wins rather than accumulating.
fn read_token_count(payload: &Value, out: &mut ParsedSession) {
    if payload.get("type").and_then(Value::as_str) != Some("token_count") {
        return;
    }
    let Some(usage) = payload
        .get("info")
        .and_then(|info| info.get("total_token_usage").or(Some(info)))
    else {
        return;
    };
    if let Some(input) = usage.get("input_tokens").and_then(Value::as_i64) {
        out.tokens_in = input;
    }
    if let Some(output) = usage.get("output_tokens").and_then(Value::as_i64) {
        out.tokens_out = output;
    }
}
