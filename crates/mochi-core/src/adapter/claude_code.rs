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

//! Claude Code adapter.
//!
//! Layout (see `doc/tool-integration.md` §1):
//! `<root>/projects/<project>/<session-id>.jsonl`, one JSON object per line,
//! appended to as the session goes on.
//!
//! The project directory name is the working directory with separators
//! replaced by `-`, which is **not reversible** when the path itself contains
//! a `-`. The working directory is therefore always read from the `cwd` field
//! inside the file, never derived from the directory name.

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
pub struct ClaudeCodeAdapter;

impl ToolAdapter for ClaudeCodeAdapter {
    fn id(&self) -> ToolId {
        ToolId::ClaudeCode
    }

    fn data_roots(&self, env: &EnvSource) -> Vec<PathBuf> {
        if let Some(dir) = env.var("CLAUDE_CONFIG_DIR") {
            return vec![PathBuf::from(dir)];
        }
        vec![env.home().join(".claude")]
    }

    fn discover(&self, root: &Path) -> Result<Vec<SessionRef>> {
        let projects = root.join("projects");
        if !projects.is_dir() {
            return Ok(Vec::new());
        }

        let mut found = Vec::new();
        for entry in walkdir::WalkDir::new(&projects)
            .max_depth(2)
            .into_iter()
            .filter_map(std::result::Result::ok)
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy();
            if !name.ends_with(".jsonl") || is_sidelined(&name) {
                continue;
            }
            match session_ref(ToolId::ClaudeCode, entry.path()) {
                Ok(session) => found.push(session),
                // A file that vanished between the walk and the stat is not an
                // error worth failing the scan for.
                Err(_) => continue,
            }
        }
        Ok(found)
    }

    fn parse(&self, session: &SessionRef) -> Result<ParsedSession> {
        let lines = read_jsonl(&session.source_path)?;
        let mut out = ParsedSession::empty(ToolId::ClaudeCode, String::new());
        out.source_path = session.source_path.clone();
        out.source_size = session.source_size;
        out.source_mtime = session.source_mtime;

        let mut problems: Vec<String> = Vec::new();
        let mut first_prompt: Option<String> = None;
        let mut summary_title: Option<String> = None;
        let mut seq = 0i64;

        for line in &lines {
            let value = match line {
                Line::Parsed(value) => value,
                Line::Broken { complete } => {
                    problems.push(if *complete {
                        "a line could not be read as JSON".to_string()
                    } else {
                        // FR-2.6: the CLI is mid-write. Expected, not damage.
                        "the last line was still being written and was skipped".to_string()
                    });
                    continue;
                }
            };

            if out.native_id.is_empty() {
                if let Some(id) = first_string(value, &["sessionId", "session_id"]) {
                    out.native_id = id.to_string();
                }
            }
            if out.cwd.is_none() {
                // The one field that matters. See the module docs.
                out.cwd = first_string(value, &["cwd"]).map(str::to_string);
            }
            if out.git_branch.is_none() {
                out.git_branch =
                    first_string(value, &["gitBranch", "git_branch"]).map(str::to_string);
            }
            if out.schema_version.is_none() {
                out.schema_version = first_string(value, &["version"]).map(str::to_string);
            }

            if let Some(ts) = value.get("timestamp").and_then(timestamp_ms) {
                out.started_at = Some(out.started_at.map_or(ts, |current: i64| current.min(ts)));
                out.updated_at = Some(out.updated_at.map_or(ts, |current: i64| current.max(ts)));
            }

            let kind = value.get("type").and_then(Value::as_str).unwrap_or("");

            // A summary line is the title the tool wrote for itself (FR-4.3).
            if kind == "summary" {
                if let Some(text) = first_string(value, &["summary"]) {
                    summary_title = Some(text.to_string());
                }
                continue;
            }

            let timestamp = value.get("timestamp").and_then(timestamp_ms);

            let Some(message) = value.get("message") else {
                // Some lines carry text directly; anything else is a shape this
                // build does not know, kept verbatim so it stays readable
                // (FR-2.8).
                if let Some(text) = first_string(value, &["content", "text"]) {
                    let mut entry = Message::new(seq, Role::System, text);
                    entry.timestamp = timestamp;
                    entry.native_id = first_string(value, &["uuid"]).map(str::to_string);
                    out.messages.push(entry);
                    seq += 1;
                } else {
                    let mut entry = Message::new(seq, Role::System, String::new());
                    entry.raw = Some(value.to_string());
                    entry.timestamp = timestamp;
                    entry.native_id = first_string(value, &["uuid"]).map(str::to_string);
                    out.messages.push(entry);
                    seq += 1;
                    problems.push(format!("unrecognised entry type {kind:?}"));
                }
                continue;
            };

            let speaker = match message.get("role").and_then(Value::as_str).unwrap_or(kind) {
                "assistant" => Role::Assistant,
                _ => Role::User,
            };

            if out.model.is_none() {
                out.model = first_string(message, &["model"]).map(str::to_string);
            }
            if let Some(usage) = message.get("usage") {
                // cache_read_input_tokens is deliberately excluded: it is not
                // billed the same way and adding it would overstate the total.
                out.tokens_in += usage
                    .get("input_tokens")
                    .and_then(Value::as_i64)
                    .unwrap_or(0);
                out.tokens_out += usage
                    .get("output_tokens")
                    .and_then(Value::as_i64)
                    .unwrap_or(0);
            }

            let uuid = first_string(value, &["uuid"]).map(str::to_string);
            let parent = first_string(value, &["parentUuid", "parent_uuid"]).map(str::to_string);

            let push = |mut entry: Message, out: &mut ParsedSession, seq: &mut i64| {
                entry.seq = *seq;
                entry.timestamp = timestamp;
                entry.native_id = uuid.clone();
                entry.parent_id = parent.clone();
                out.messages.push(entry);
                *seq += 1;
            };

            match message.get("content") {
                Some(Value::String(text)) => {
                    if speaker == Role::User && first_prompt.is_none() {
                        first_prompt = Some(text.clone());
                    }
                    push(Message::new(0, speaker, text.clone()), &mut out, &mut seq);
                }
                Some(Value::Array(blocks)) => {
                    for block in blocks {
                        let block_type = block.get("type").and_then(Value::as_str).unwrap_or("");
                        match block_type {
                            "text" => {
                                let text = flatten_text(block);
                                if speaker == Role::User && first_prompt.is_none() {
                                    first_prompt = Some(text.clone());
                                }
                                push(Message::new(0, speaker, text), &mut out, &mut seq);
                            }
                            "thinking" | "redacted_thinking" => {
                                push(
                                    Message::new(0, Role::Thinking, flatten_text(block)),
                                    &mut out,
                                    &mut seq,
                                );
                            }
                            "tool_use" => {
                                let mut entry = Message::new(
                                    0,
                                    Role::ToolCall,
                                    block.get("input").map(compact_json).unwrap_or_default(),
                                );
                                entry.tool_name =
                                    first_string(block, &["name"]).map(str::to_string);
                                entry.raw = Some(block.to_string());
                                push(entry, &mut out, &mut seq);
                            }
                            "tool_result" => {
                                let mut entry = Message::new(
                                    0,
                                    Role::ToolResult,
                                    block.get("content").map(flatten_text).unwrap_or_default(),
                                );
                                entry.raw = Some(block.to_string());
                                push(entry, &mut out, &mut seq);
                            }
                            other => {
                                let mut entry = Message::new(0, speaker, flatten_text(block));
                                entry.raw = Some(block.to_string());
                                push(entry, &mut out, &mut seq);
                                problems.push(format!("unrecognised content block {other:?}"));
                            }
                        }
                    }
                }
                Some(other) => {
                    let mut entry = Message::new(0, speaker, flatten_text(other));
                    entry.raw = Some(other.to_string());
                    push(entry, &mut out, &mut seq);
                }
                None => {}
            }
        }

        if out.native_id.is_empty() {
            // The file name is the session id in this tool's layout, so it is
            // a reasonable fallback — but only as a fallback.
            out.native_id = session
                .source_path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "unknown".to_string());
        }

        out.title = summary_title.or_else(|| first_prompt.as_deref().map(title_from_prompt));

        if !problems.is_empty() {
            out.parse_status = if out.messages.is_empty() {
                ParseStatus::Failed
            } else {
                ParseStatus::Partial
            };
            problems.dedup();
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
                "this session has no id recorded in its file, so it cannot be resumed",
            ));
        }
        Ok(CommandSpec::new(
            target.program(self.executable_name()),
            vec!["--resume".to_string(), target.native_id.clone()],
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
        "claude"
    }
}

/// Whether a file name is one of the sidelined transcripts Claude Code leaves
/// behind (`.orphaned-*`, `.superseded-*`). Hidden by default so they do not
/// bury the real sessions.
pub fn is_sidelined(file_name: &str) -> bool {
    file_name.contains(".orphaned-") || file_name.contains(".superseded-")
}
