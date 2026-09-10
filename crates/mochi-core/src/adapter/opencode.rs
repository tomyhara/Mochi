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

use std::path::{Path, PathBuf};

use crate::adapter::{EnvSource, ToolAdapter};
use crate::command::{CommandSpec, ResumeTarget};
use crate::model::{ParsedSession, SessionRef, ToolId};
use crate::Result;

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
    todo!()
}

impl ToolAdapter for OpenCodeAdapter {
    fn id(&self) -> ToolId {
        ToolId::OpenCode
    }

    fn data_roots(&self, env: &EnvSource) -> Vec<PathBuf> {
        todo!()
    }

    fn discover(&self, root: &Path) -> Result<Vec<SessionRef>> {
        todo!()
    }

    fn parse(&self, session: &SessionRef) -> Result<ParsedSession> {
        todo!()
    }

    fn resume_command(&self, target: &ResumeTarget) -> Result<CommandSpec> {
        todo!()
    }

    fn new_session_command(&self, cwd: &Path, executable: Option<&Path>) -> Result<CommandSpec> {
        todo!()
    }

    fn executable_name(&self) -> &'static str {
        "opencode"
    }
}
