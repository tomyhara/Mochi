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

//! The document the interface reads (`mochi export`, and the desktop shell's
//! `load_index`).
//!
//! This lives in the core rather than in either front end because both produce
//! it, and two implementations of one contract drift. `ui/src/types.ts` is the
//! other half of it.
//!
//! Field names are camelCase and written out by hand rather than derived from
//! the internal types: the interface should not have to change every time a
//! column moves, and the contract is easier to review when it is in one place
//! you can read top to bottom.

use serde_json::{json, Value};

use crate::adapter;
use crate::command::{QuoteStyle, ResumeTarget};
use crate::index::{Index, SessionQuery, SessionRecord};
use crate::mask::Masker;
use crate::model::ParseStatus;
use crate::Result;

/// Version of the document shape. Bump it when a change would make an older
/// interface misread a newer file.
pub const SCHEMA: i64 = 1;

#[derive(Debug, Clone, Copy)]
pub struct ExportOptions {
    /// Show secrets in the exported content.
    ///
    /// Phrased so the safe value is the default: an export is a file that gets
    /// committed, attached to a bug report and passed around (FR-8.5), and the
    /// session logs it comes from routinely hold API keys (NFR-3.3).
    pub reveal_secrets: bool,
    /// How to quote the resume command for display. The command is text for a
    /// human to read or paste — nothing runs it (NFR-3.6).
    pub quote_style: QuoteStyle,
}

impl Default for ExportOptions {
    fn default() -> Self {
        ExportOptions {
            reveal_secrets: false,
            quote_style: QuoteStyle::for_host(),
        }
    }
}

/// Build the whole document: every repository, every session, every message.
pub fn document(index: &Index, options: ExportOptions) -> Result<Value> {
    let masker = Masker::new();
    let mask = |text: &str| -> String {
        if options.reveal_secrets {
            text.to_string()
        } else {
            masker.mask(text).text
        }
    };

    let repositories: Vec<Value> = index
        .list_repositories()?
        .into_iter()
        .map(|repo| {
            json!({
                "id": repo.id,
                "displayName": repo.display_name,
                "rootPath": repo.root_path,
                "remoteUrl": repo.remote_url,
                "rootCommit": repo.root_commit,
                "isWorktree": repo.is_worktree,
                "hidden": repo.hidden,
            })
        })
        .collect();

    let mut sessions = Vec::new();
    for session in index.list_sessions(&SessionQuery {
        limit: Some(u32::MAX),
        ..Default::default()
    })? {
        let messages: Vec<Value> = index
            .messages(session.id)?
            .into_iter()
            .map(|message| {
                json!({
                    "seq": message.seq,
                    "role": message.role.as_str(),
                    "content": mask(&message.content),
                    "toolName": message.tool_name,
                    "timestamp": message.timestamp,
                    "raw": message.raw.as_deref().map(&mask),
                })
            })
            .collect();

        sessions.push(json!({
            "id": session.id,
            "tool": session.tool.as_str(),
            "nativeId": session.native_id,
            "repoId": session.repo_id,
            "title": session.title.as_deref().map(&mask),
            "model": session.model,
            "cwd": session.cwd,
            "cwdExists": session.cwd_exists,
            "gitBranch": session.git_branch,
            "startedAt": session.started_at,
            "updatedAt": session.updated_at,
            "messageCount": session.message_count,
            "tokensIn": session.tokens_in,
            "tokensOut": session.tokens_out,
            "status": session.status.as_str(),
            "parseStatus": session.parse_status.as_str(),
            "parseError": session.parse_error,
            "sourcePath": session.source_path,
            "sourceSize": session.source_size,
            "resume": resume(&session, options.quote_style),
            "messages": messages,
        }));
    }

    Ok(json!({
        "schema": SCHEMA,
        "masked": !options.reveal_secrets,
        "repositories": repositories,
        "sessions": sessions,
    }))
}

/// Whether a session can be resumed, and the command if it can.
///
/// The reasons matter as much as the command: FR-7.6 and FR-2.11 both end with
/// a session the user can still read but cannot restart, and the interface has
/// to say which it is rather than showing a button that fails.
pub fn resume(session: &SessionRecord, style: QuoteStyle) -> Value {
    let unavailable =
        |reason: &str| json!({ "available": false, "command": null, "reason": reason });

    if session.parse_status == ParseStatus::Archived {
        return unavailable("the original transcript is gone, so there is nothing to resume");
    }
    let Some(cwd) = session.cwd.as_deref() else {
        return unavailable("this session recorded no working directory");
    };
    if !session.cwd_exists {
        return unavailable("the working directory no longer exists");
    }

    let target = ResumeTarget::new(&session.native_id, std::path::PathBuf::from(cwd));
    match adapter::for_tool(session.tool).resume_command(&target) {
        Ok(spec) => json!({
            "available": true,
            "command": spec.to_display_string(style),
            "reason": null,
        }),
        Err(error) => unavailable(&error.to_string()),
    }
}
