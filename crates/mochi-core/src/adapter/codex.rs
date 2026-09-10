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

use crate::adapter::{EnvSource, ToolAdapter};
use crate::command::{CommandSpec, ResumeTarget};
use crate::model::{ParsedSession, SessionRef, ToolId};
use crate::Result;

#[derive(Debug, Default, Clone, Copy)]
pub struct CodexAdapter;

impl ToolAdapter for CodexAdapter {
    fn id(&self) -> ToolId {
        ToolId::Codex
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
        "codex"
    }
}
