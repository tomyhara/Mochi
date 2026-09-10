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

//! Tool adapters (NFR-5.1).
//!
//! Each CLI stores sessions in its own undocumented layout, and those layouts
//! change without notice (R-1). Keeping each one behind [`ToolAdapter`] means a
//! breaking change in one CLI can only break one file, and each parser can be
//! pinned by its own golden files (NFR-5.2).
//!
//! This is an internal seam, not a plugin API — the supported set is fixed at
//! three tools (NS-7).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::command::{CommandSpec, ResumeTarget};
use crate::model::{ParsedSession, SessionRef, ToolId};
use crate::Result;

pub mod claude_code;
pub mod codex;
pub mod opencode;
pub(crate) mod util;

/// The environment an adapter reads its locations from.
///
/// Wrapping the process environment makes the location rules testable and lets
/// a user point Mochi at a different tree (FR-1.4).
#[derive(Debug, Clone)]
pub struct EnvSource {
    home: PathBuf,
    vars: HashMap<String, String>,
}

impl EnvSource {
    /// The real environment of this process.
    pub fn from_process() -> EnvSource {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_default();
        let vars = std::env::vars()
            .filter(|(key, _)| RELEVANT_VARS.contains(&key.as_str()))
            .collect();
        EnvSource { home, vars }
    }

    /// An environment with nothing but a home directory. Used by tests and by
    /// `--home` on the command line.
    pub fn with_home(home: impl Into<PathBuf>) -> EnvSource {
        EnvSource {
            home: home.into(),
            vars: HashMap::new(),
        }
    }

    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) -> &mut Self {
        self.vars.insert(key.into(), value.into());
        self
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn var(&self, key: &str) -> Option<&str> {
        self.vars
            .get(key)
            .map(String::as_str)
            .filter(|v| !v.is_empty())
    }
}

/// The only environment variables an adapter may look at. Listing them keeps
/// the process environment, which can hold credentials, out of Mochi's reach
/// except where a tool documents an override (FR-1.4).
const RELEVANT_VARS: &[&str] = &["CLAUDE_CONFIG_DIR", "CODEX_HOME", "XDG_DATA_HOME"];

/// Everything Mochi needs from one CLI.
pub trait ToolAdapter: Send + Sync {
    fn id(&self) -> ToolId;

    /// Directories that may contain sessions, most specific first. Honours the
    /// tool's own environment overrides (FR-1.4).
    fn data_roots(&self, env: &EnvSource) -> Vec<PathBuf>;

    /// List the sessions under one root without parsing them (FR-2.1).
    ///
    /// Results are ordered by source path. Directory iteration order is
    /// whatever the filesystem feels like returning, which would make the row
    /// ids a scan assigns depend on the machine it ran on — and those ids are
    /// what a caller stores to remember which session was open.
    fn discover(&self, root: &Path) -> Result<Vec<SessionRef>>;

    /// Read one session. Must not fail the caller for anything it can isolate
    /// to this session (FR-2.7).
    fn parse(&self, session: &SessionRef) -> Result<ParsedSession>;

    /// The command that resumes a session (FR-7.2).
    fn resume_command(&self, target: &ResumeTarget) -> Result<CommandSpec>;

    /// The command that starts a fresh session in a directory (FR-7.7).
    fn new_session_command(&self, cwd: &Path, executable: Option<&Path>) -> Result<CommandSpec>;

    /// Name of the executable to look for on `PATH` (FR-1.1).
    fn executable_name(&self) -> &'static str;
}

/// One adapter per supported tool.
pub fn all() -> Vec<Box<dyn ToolAdapter>> {
    vec![
        Box::new(claude_code::ClaudeCodeAdapter),
        Box::new(codex::CodexAdapter),
        Box::new(opencode::OpenCodeAdapter),
    ]
}

pub fn for_tool(tool: ToolId) -> Box<dyn ToolAdapter> {
    match tool {
        ToolId::ClaudeCode => Box::new(claude_code::ClaudeCodeAdapter),
        ToolId::Codex => Box::new(codex::CodexAdapter),
        ToolId::OpenCode => Box::new(opencode::OpenCodeAdapter),
    }
}
