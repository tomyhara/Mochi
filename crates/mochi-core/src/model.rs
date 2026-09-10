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

//! Normalised data model shared by every adapter and by the index.
//!
//! The shapes here follow `doc/requirements.md` §6. Adapters translate each
//! CLI's private format into these types; nothing downstream of an adapter
//! needs to know which tool a session came from.

use std::path::PathBuf;

/// The three CLIs Mochi supports. Deliberately a closed set (NS-7, Q-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ToolId {
    ClaudeCode,
    Codex,
    OpenCode,
}

impl ToolId {
    pub const ALL: [ToolId; 3] = [ToolId::ClaudeCode, ToolId::Codex, ToolId::OpenCode];

    /// Stable identifier used in the database and on the command line.
    pub fn as_str(self) -> &'static str {
        match self {
            ToolId::ClaudeCode => "claude_code",
            ToolId::Codex => "codex",
            ToolId::OpenCode => "opencode",
        }
    }

    /// Human-readable name for the UI (English only, FR-9.3).
    pub fn display_name(self) -> &'static str {
        match self {
            ToolId::ClaudeCode => "Claude Code",
            ToolId::Codex => "Codex CLI",
            ToolId::OpenCode => "OpenCode",
        }
    }

    pub fn parse(s: &str) -> Option<ToolId> {
        match s.trim().to_ascii_lowercase().replace(['-', ' '], "_").as_str() {
            "claude_code" | "claude" | "claudecode" => Some(ToolId::ClaudeCode),
            "codex" | "codex_cli" => Some(ToolId::Codex),
            "opencode" | "open_code" => Some(ToolId::OpenCode),
            _ => None,
        }
    }
}

impl std::fmt::Display for ToolId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Role of a single transcript entry (FR-5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    User,
    Assistant,
    ToolCall,
    ToolResult,
    System,
    Thinking,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::ToolCall => "tool_call",
            Role::ToolResult => "tool_result",
            Role::System => "system",
            Role::Thinking => "thinking",
        }
    }

    pub fn parse(s: &str) -> Option<Role> {
        match s {
            "user" => Some(Role::User),
            "assistant" => Some(Role::Assistant),
            "tool_call" => Some(Role::ToolCall),
            "tool_result" => Some(Role::ToolResult),
            "system" => Some(Role::System),
            "thinking" => Some(Role::Thinking),
            _ => None,
        }
    }
}

/// One entry of a transcript.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub seq: i64,
    pub role: Role,
    /// Flattened text used for display and for the full-text index.
    pub content: String,
    /// The original JSON for this entry, kept so that anything Mochi failed to
    /// understand can still be shown verbatim (FR-2.8, FR-5.8).
    pub raw: Option<String>,
    /// Milliseconds since the Unix epoch, when the source recorded one.
    pub timestamp: Option<i64>,
    /// Tool name for `ToolCall` / `ToolResult` entries.
    pub tool_name: Option<String>,
    /// Native identifier of this entry, when the format has one.
    pub native_id: Option<String>,
    /// Parent entry, used for branches and subagents (FR-5.7).
    pub parent_id: Option<String>,
    pub tokens_in: Option<i64>,
    pub tokens_out: Option<i64>,
}

impl Message {
    pub fn new(seq: i64, role: Role, content: impl Into<String>) -> Self {
        Message {
            seq,
            role,
            content: content.into(),
            raw: None,
            timestamp: None,
            tool_name: None,
            native_id: None,
            parent_id: None,
            tokens_in: None,
            tokens_out: None,
        }
    }
}

/// How well a session could be read (FR-2.7, FR-2.11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseStatus {
    /// Every line was understood.
    Ok,
    /// The session is readable but something was skipped — typically an
    /// incomplete trailing line on a file the CLI is still appending to
    /// (FR-2.6), or lines in a shape this version does not know.
    Partial,
    /// Nothing usable could be read. Isolated to this session (NFR-2.2).
    Failed,
    /// The source file is gone but the transcript survives in the index
    /// (FR-2.11). This is the state that protects against a CLI's own
    /// retention policy deleting history (R-4, P-5).
    Archived,
}

impl ParseStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ParseStatus::Ok => "ok",
            ParseStatus::Partial => "partial",
            ParseStatus::Failed => "failed",
            ParseStatus::Archived => "archived",
        }
    }

    pub fn parse(s: &str) -> Option<ParseStatus> {
        match s {
            "ok" => Some(ParseStatus::Ok),
            "partial" => Some(ParseStatus::Partial),
            "failed" => Some(ParseStatus::Failed),
            "archived" => Some(ParseStatus::Archived),
            _ => None,
        }
    }
}

/// A session file found on disk, before it is parsed. Carries just enough to
/// decide whether re-parsing is needed (FR-2.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRef {
    pub tool: ToolId,
    pub source_path: PathBuf,
    pub source_size: u64,
    /// Milliseconds since the Unix epoch.
    pub source_mtime: i64,
}

/// The result of reading one session.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedSession {
    pub tool: ToolId,
    /// The CLI's own session identifier — the one `resume` takes. Always read
    /// from the file's contents, never from its name (R-8).
    pub native_id: String,
    /// Working directory exactly as the tool recorded it (FR-3.1).
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub title: Option<String>,
    pub model: Option<String>,
    /// Milliseconds since the Unix epoch.
    pub started_at: Option<i64>,
    pub updated_at: Option<i64>,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub messages: Vec<Message>,
    pub parse_status: ParseStatus,
    pub parse_error: Option<String>,
    /// Whatever the file said about its own format version, for R-1 triage.
    pub schema_version: Option<String>,
    pub source_path: PathBuf,
    pub source_size: u64,
    pub source_mtime: i64,
}

impl ParsedSession {
    pub fn empty(tool: ToolId, native_id: impl Into<String>) -> Self {
        ParsedSession {
            tool,
            native_id: native_id.into(),
            cwd: None,
            git_branch: None,
            title: None,
            model: None,
            started_at: None,
            updated_at: None,
            tokens_in: 0,
            tokens_out: 0,
            messages: Vec::new(),
            parse_status: ParseStatus::Ok,
            parse_error: None,
            schema_version: None,
            source_path: PathBuf::new(),
            source_size: 0,
            source_mtime: 0,
        }
    }

    pub fn message_count(&self) -> usize {
        self.messages.len()
    }
}
