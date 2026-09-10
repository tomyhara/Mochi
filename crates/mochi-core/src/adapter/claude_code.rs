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

use crate::adapter::{EnvSource, ToolAdapter};
use crate::command::{CommandSpec, ResumeTarget};
use crate::model::{ParsedSession, SessionRef, ToolId};
use crate::Result;

#[derive(Debug, Default, Clone, Copy)]
pub struct ClaudeCodeAdapter;

impl ToolAdapter for ClaudeCodeAdapter {
    fn id(&self) -> ToolId {
        ToolId::ClaudeCode
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
        "claude"
    }
}

/// Whether a file name is one of the sidelined transcripts Claude Code leaves
/// behind (`.orphaned-*`, `.superseded-*`). Hidden by default so they do not
/// bury the real sessions.
pub fn is_sidelined(file_name: &str) -> bool {
    todo!()
}
