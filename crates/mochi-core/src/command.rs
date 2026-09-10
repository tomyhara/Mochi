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

//! Building the command that starts or resumes a CLI (FR-7.2, FR-7.5, NFR-3.6).
//!
//! Everything Mochi launches is described as a program plus an argument
//! vector. There is no code path that hands a session id, a path or anything
//! else derived from session data to a shell, which is what makes command
//! injection structurally impossible rather than merely unlikely.
//!
//! Quoting exists only for [`CommandSpec::to_display_string`], which produces
//! the text of the "copy command" button (FR-7.4). That string is for a human
//! to paste, never for Mochi to execute.

use std::path::{Path, PathBuf};

/// A process to start: program, arguments, working directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    /// The session's own working directory. Starting anywhere else would give
    /// the agent the wrong repository (FR-7.1).
    pub cwd: PathBuf,
}

impl CommandSpec {
    pub fn new(program: impl Into<String>, args: Vec<String>, cwd: impl Into<PathBuf>) -> Self {
        CommandSpec {
            program: program.into(),
            args,
            cwd: cwd.into(),
        }
    }

    /// Render for a human to copy and paste. Not used to launch anything.
    pub fn to_display_string(&self, style: QuoteStyle) -> String {
        todo!()
    }
}

/// Quoting rules of the shell the user will paste into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuoteStyle {
    Posix,
    PowerShell,
    Cmd,
}

impl QuoteStyle {
    pub fn for_host() -> QuoteStyle {
        todo!()
    }
}

pub fn quote_posix(s: &str) -> String {
    todo!()
}

pub fn quote_powershell(s: &str) -> String {
    todo!()
}

pub fn quote_cmd(s: &str) -> String {
    todo!()
}

/// Terminal application to hand a command to (FR-7.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalKind {
    WindowsTerminal,
    PowerShell,
    Cmd,
    TerminalApp,
    ITerm2,
    /// A user-supplied template. `{cmd}`, `{cwd}` and `{args}` are substituted.
    Custom(String),
}

/// Wrap a command so that the chosen terminal application runs it, still as an
/// argument vector (NFR-3.6).
pub fn external_launch(spec: &CommandSpec, terminal: &TerminalKind) -> crate::Result<CommandSpec> {
    todo!()
}

/// What a caller needs to know to resume one session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeTarget {
    pub native_id: String,
    pub cwd: PathBuf,
    /// Path of the executable, when the user overrode it (FR-1.3).
    pub executable: Option<PathBuf>,
}

impl ResumeTarget {
    pub fn new(native_id: impl Into<String>, cwd: impl Into<PathBuf>) -> Self {
        ResumeTarget {
            native_id: native_id.into(),
            cwd: cwd.into(),
            executable: None,
        }
    }

    pub fn program(&self, default: &str) -> String {
        match &self.executable {
            Some(p) => p.to_string_lossy().into_owned(),
            None => default.to_string(),
        }
    }
}

/// Refuse to launch when the recorded directory is gone (FR-7.6).
pub fn check_cwd_exists(cwd: &Path) -> crate::Result<()> {
    todo!()
}
