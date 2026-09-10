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

//! OpenCode adapter (file storage).
//!
//! OpenCode has moved its storage from JSON files to SQLite, so the layout
//! depends on the installed version (`doc/tool-integration.md` §3.2). This
//! adapter reads the JSON layout:
//!
//! ```text
//! <root>/storage/session/**/<session-id>.json
//! <root>/storage/message/<session-id>/<message-id>.json
//! <root>/storage/part/<session-id>/<message-id>/<part-id>.json
//! ```
//!
//! When it finds `opencode.db` instead, it reports that through
//! [`detect_storage`] rather than guessing at an unverified schema. The
//! recommended long-term source for OpenCode is its local HTTP API (§3.3);
//! that work is tracked for milestone 0.
//!
//! `auth.json` sits in the same tree and holds provider credentials. Nothing
//! here has any reason to open it, and nothing does.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::adapter::util::{
    compact_json, file_stats, first_string, flatten_text, read_json, timestamp_ms,
};
use crate::adapter::{EnvSource, ToolAdapter};
use crate::command::{CommandSpec, ResumeTarget};
use crate::model::{Message, ParseStatus, ParsedSession, Role, SessionRef, ToolId};
use crate::{Error, Result};

#[derive(Debug, Default, Clone, Copy)]
pub struct OpenCodeAdapter;

/// Which storage layout a root uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Storage {
    /// `storage/session`, `storage/message`, `storage/part`.
    JsonFiles,
    /// `opencode.db`. Schema not yet verified against a real install.
    Sqlite,
    Missing,
}

/// Look at a root and report which layout it holds.
pub fn detect_storage(root: &Path) -> Storage {
    if root.join("storage").join("session").is_dir() {
        return Storage::JsonFiles;
    }
    if root.join("opencode.db").is_file() {
        return Storage::Sqlite;
    }
    Storage::Missing
}

impl ToolAdapter for OpenCodeAdapter {
    fn id(&self) -> ToolId {
        ToolId::OpenCode
    }

    fn data_roots(&self, env: &EnvSource) -> Vec<PathBuf> {
        // Note for Windows: OpenCode uses ~/.local/share there too, not
        // %APPDATA% (doc/tool-integration.md §3.1).
        if let Some(dir) = env.var("XDG_DATA_HOME") {
            return vec![PathBuf::from(dir).join("opencode")];
        }
        vec![env.home().join(".local").join("share").join("opencode")]
    }

    fn discover(&self, root: &Path) -> Result<Vec<SessionRef>> {
        match detect_storage(root) {
            Storage::Missing => return Ok(Vec::new()),
            Storage::Sqlite => {
                // Reading a schema nobody has checked would produce a
                // confidently wrong session list, which is worse than an
                // honest gap. See doc/tool-integration.md §5.
                return Err(Error::invalid(
                    "this OpenCode install keeps sessions in SQLite (opencode.db), which Mochi \
                     cannot read yet. The verified route is the local HTTP API; support is \
                     tracked for milestone 0.",
                ));
            }
            Storage::JsonFiles => {}
        }

        let session_dir = root.join("storage").join("session");
        let mut found = Vec::new();
        for entry in walkdir::WalkDir::new(&session_dir)
            .into_iter()
            .filter_map(std::result::Result::ok)
        {
            if !entry.file_type().is_file() {
                continue;
            }
            if entry.path().extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Some(id) = entry
                .path()
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
            else {
                continue;
            };
            // A session is spread over three directories, so change detection
            // has to look at all of them, not just the index file (FR-2.3).
            let (size, mtime) = aggregate_stats(root, &id, entry.path());
            found.push(SessionRef {
                tool: ToolId::OpenCode,
                source_path: entry.path().to_path_buf(),
                source_size: size,
                source_mtime: mtime,
            });
        }
        Ok(found)
    }

    fn parse(&self, session: &SessionRef) -> Result<ParsedSession> {
        let info = read_json(&session.source_path)?;
        let id = first_string(&info, &["id"])
            .map(str::to_string)
            .or_else(|| {
                session
                    .source_path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "unknown".to_string());

        let mut out = ParsedSession::empty(ToolId::OpenCode, id.clone());
        out.source_path = session.source_path.clone();
        out.source_size = session.source_size;
        out.source_mtime = session.source_mtime;
        out.title = first_string(&info, &["title"]).map(str::to_string);
        out.cwd = first_string(&info, &["directory", "cwd", "path"]).map(str::to_string);
        out.schema_version = first_string(&info, &["version"]).map(str::to_string);
        if let Some(time) = info.get("time") {
            out.started_at = time.get("created").and_then(timestamp_ms);
            out.updated_at = time
                .get("updated")
                .and_then(timestamp_ms)
                .or(out.started_at);
        }

        let Some(root) = storage_root(&session.source_path) else {
            out.parse_status = ParseStatus::Partial;
            out.parse_error =
                Some("the session file is not inside a storage/session directory".into());
            return Ok(out);
        };

        let mut problems: Vec<String> = Vec::new();
        let mut seq = 0i64;

        for (message_id, message) in read_messages(&root, &id, &mut problems) {
            let speaker = match message
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("user")
            {
                "assistant" => Role::Assistant,
                "system" => Role::System,
                _ => Role::User,
            };
            if out.model.is_none() {
                out.model =
                    first_string(&message, &["modelID", "model_id", "model"]).map(str::to_string);
            }
            if let Some(tokens) = message.get("tokens") {
                out.tokens_in += tokens.get("input").and_then(Value::as_i64).unwrap_or(0);
                out.tokens_out += tokens.get("output").and_then(Value::as_i64).unwrap_or(0);
            }
            let timestamp = message
                .get("time")
                .and_then(|t| t.get("created"))
                .and_then(timestamp_ms);

            if out.cwd.is_none() {
                out.cwd = message
                    .get("path")
                    .and_then(|p| first_string(p, &["cwd", "root"]))
                    .map(str::to_string);
            }

            for part in read_parts(&root, &id, &message_id, &mut problems) {
                let part_type = part.get("type").and_then(Value::as_str).unwrap_or("");
                match part_type {
                    "text" => {
                        let mut entry = Message::new(seq, speaker, flatten_text(&part));
                        entry.timestamp = timestamp;
                        entry.parent_id = Some(message_id.clone());
                        out.messages.push(entry);
                        seq += 1;
                    }
                    "reasoning" => {
                        let mut entry = Message::new(seq, Role::Thinking, flatten_text(&part));
                        entry.timestamp = timestamp;
                        entry.parent_id = Some(message_id.clone());
                        out.messages.push(entry);
                        seq += 1;
                    }
                    "tool" => {
                        let tool_name = first_string(&part, &["tool"]).map(str::to_string);
                        let state = part.get("state");

                        let mut call = Message::new(
                            seq,
                            Role::ToolCall,
                            state
                                .and_then(|s| s.get("input"))
                                .map(compact_json)
                                .unwrap_or_default(),
                        );
                        call.tool_name = tool_name.clone();
                        call.timestamp = timestamp;
                        call.native_id =
                            first_string(&part, &["callID", "call_id"]).map(str::to_string);
                        call.raw = Some(part.to_string());
                        out.messages.push(call);
                        seq += 1;

                        let mut result = Message::new(
                            seq,
                            Role::ToolResult,
                            state
                                .and_then(|s| s.get("output"))
                                .map(flatten_text)
                                .unwrap_or_default(),
                        );
                        result.tool_name = tool_name;
                        result.timestamp = timestamp;
                        result.parent_id =
                            first_string(&part, &["callID", "call_id"]).map(str::to_string);
                        out.messages.push(result);
                        seq += 1;
                    }
                    // step-start and step-finish are bookkeeping, not content.
                    "step-start" | "step-finish" | "snapshot" | "patch" => {}
                    other => {
                        let mut entry = Message::new(seq, speaker, flatten_text(&part));
                        entry.raw = Some(part.to_string());
                        entry.timestamp = timestamp;
                        out.messages.push(entry);
                        seq += 1;
                        problems.push(format!("unrecognised part type {other:?}"));
                    }
                }
            }
        }

        if out.title.is_none() {
            out.title = out
                .messages
                .iter()
                .find(|m| m.role == Role::User)
                .map(|m| crate::adapter::util::title_from_prompt(&m.content));
        }

        if !problems.is_empty() {
            problems.dedup();
            out.parse_status = ParseStatus::Partial;
            out.parse_error = Some(problems.join("; "));
        }

        Ok(out)
    }

    fn resume_command(&self, target: &ResumeTarget) -> Result<CommandSpec> {
        // The exact flag is unverified against a real install
        // (doc/tool-integration.md §3.5). It is isolated here, which is the
        // point of the adapter split.
        Ok(CommandSpec::new(
            target.program(self.executable_name()),
            vec!["--session".to_string(), target.native_id.clone()],
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
        "opencode"
    }
}

/// Walk back from `<root>/storage/session/.../id.json` to `<root>`.
fn storage_root(session_file: &Path) -> Option<PathBuf> {
    let mut current = session_file.parent();
    while let Some(dir) = current {
        if dir.file_name().and_then(|n| n.to_str()) == Some("storage") {
            return dir.parent().map(Path::to_path_buf);
        }
        current = dir.parent();
    }
    None
}

/// Messages of one session, ordered by creation time then by id so that two
/// messages written in the same millisecond still come out in a stable order.
fn read_messages(
    root: &Path,
    session_id: &str,
    problems: &mut Vec<String>,
) -> Vec<(String, Value)> {
    let dir = root.join("storage").join("message").join(session_id);
    let mut out = read_json_dir(&dir, problems);
    out.sort_by(|a, b| {
        let created = |value: &Value| {
            value
                .get("time")
                .and_then(|t| t.get("created"))
                .and_then(timestamp_ms)
                .unwrap_or(0)
        };
        created(&a.1)
            .cmp(&created(&b.1))
            .then_with(|| a.0.cmp(&b.0))
    });
    out
}

/// Parts of one message, ordered by id.
fn read_parts(
    root: &Path,
    session_id: &str,
    message_id: &str,
    problems: &mut Vec<String>,
) -> Vec<Value> {
    let dir = root
        .join("storage")
        .join("part")
        .join(session_id)
        .join(message_id);
    let mut entries = read_json_dir(&dir, problems);
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries.into_iter().map(|(_, value)| value).collect()
}

fn read_json_dir(dir: &Path, problems: &mut Vec<String>) -> Vec<(String, Value)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.filter_map(std::result::Result::ok) {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let id = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        match read_json(&path) {
            Ok(value) => out.push((id, value)),
            // FR-2.7: one unreadable part, not one unreadable session.
            Err(error) => problems.push(error.to_string()),
        }
    }
    out
}

/// Total size and newest modification time across a session's three
/// directories.
fn aggregate_stats(root: &Path, session_id: &str, session_file: &Path) -> (u64, i64) {
    let (mut size, mut mtime) = file_stats(session_file).unwrap_or((0, 0));
    for sub in ["message", "part"] {
        let dir = root.join("storage").join(sub).join(session_id);
        for entry in walkdir::WalkDir::new(&dir)
            .into_iter()
            .filter_map(std::result::Result::ok)
        {
            if !entry.file_type().is_file() {
                continue;
            }
            if let Ok((file_size, file_mtime)) = file_stats(entry.path()) {
                size += file_size;
                mtime = mtime.max(file_mtime);
            }
        }
    }
    (size, mtime)
}
