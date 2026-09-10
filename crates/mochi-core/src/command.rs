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

use crate::{Error, Result};

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
        let quote = match style {
            QuoteStyle::Posix => quote_posix,
            QuoteStyle::PowerShell => quote_powershell,
            QuoteStyle::Cmd => quote_cmd,
        };

        let mut command = quote(&self.program);
        for arg in &self.args {
            command.push(' ');
            command.push_str(&quote(arg));
        }

        let cwd = quote(&self.cwd.to_string_lossy());
        match style {
            QuoteStyle::Posix => format!("cd {cwd} && {command}"),
            QuoteStyle::PowerShell => format!("cd {cwd}; {command}"),
            QuoteStyle::Cmd => format!("cd /d {cwd} && {command}"),
        }
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
        if cfg!(windows) {
            QuoteStyle::PowerShell
        } else {
            QuoteStyle::Posix
        }
    }
}

/// Characters that need no quoting anywhere. Everything outside ASCII is
/// included: a Japanese directory name is ordinary text, not a metacharacter
/// (NFR-6.5).
fn needs_quoting(s: &str, extra_safe: &str) -> bool {
    s.is_empty()
        || s.chars().any(|c| {
            !(c.is_ascii_alphanumeric()
                || !c.is_ascii()
                || "._/@+,=-".contains(c)
                || extra_safe.contains(c))
        })
}

pub fn quote_posix(s: &str) -> String {
    if !needs_quoting(s, ":%") {
        return s.to_string();
    }
    // Single quotes protect everything except a single quote, which has to be
    // closed, escaped and reopened.
    format!("'{}'", s.replace('\'', r"'\''"))
}

pub fn quote_powershell(s: &str) -> String {
    if !needs_quoting(s, r":\") {
        return s.to_string();
    }
    // A single-quoted PowerShell string expands nothing; a quote is doubled.
    format!("'{}'", s.replace('\'', "''"))
}

pub fn quote_cmd(s: &str) -> String {
    if !needs_quoting(s, r":\") {
        return s.to_string();
    }
    // cmd.exe has no way to quote a literal `%`: it is expanded even inside
    // double quotes. Callers render this text for a human, who will see the
    // percent signs and can fix them; Mochi never executes it.
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// Terminal application to hand a command to (FR-7.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalKind {
    WindowsTerminal,
    PowerShell,
    Cmd,
    TerminalApp,
    ITerm2,
    /// A user-supplied argument template, e.g.
    /// `kitty --directory {cwd} -- {program} {args}`. It is split on
    /// whitespace *before* substitution, so a directory containing spaces
    /// stays one argument.
    Custom(String),
}

/// Wrap a command so that the chosen terminal application runs it, still as an
/// argument vector (NFR-3.6).
///
/// Terminal.app, iTerm2, cmd.exe and PowerShell are missing on purpose. Each
/// can only be handed a command as a single string, which means building shell
/// text out of session data — exactly what NFR-3.6 forbids. Doing it safely
/// needs the launcher design that comes with the integrated terminal in
/// milestone 3 (R-10). Until then the integrated terminal is the default
/// (FR-7.8a) and "copy command" (FR-7.4) covers the rest.
pub fn external_launch(spec: &CommandSpec, terminal: &TerminalKind) -> Result<CommandSpec> {
    match terminal {
        TerminalKind::WindowsTerminal => {
            let mut args = vec![
                "-d".to_string(),
                spec.cwd.to_string_lossy().into_owned(),
                spec.program.clone(),
            ];
            args.extend(spec.args.iter().cloned());
            Ok(CommandSpec::new("wt.exe", args, spec.cwd.clone()))
        }
        TerminalKind::Custom(template) => expand_template(template, spec),
        TerminalKind::TerminalApp => Err(unsupported("Terminal.app")),
        TerminalKind::ITerm2 => Err(unsupported("iTerm2")),
        TerminalKind::Cmd => Err(unsupported("cmd.exe")),
        TerminalKind::PowerShell => Err(unsupported("PowerShell")),
    }
}

fn unsupported(name: &str) -> Error {
    Error::invalid(format!(
        "launching through {name} is not supported yet: it would require building a shell \
         command line out of session data. Use the integrated terminal, or copy the command."
    ))
}

fn expand_template(template: &str, spec: &CommandSpec) -> Result<CommandSpec> {
    let mut tokens: Vec<String> = Vec::new();
    for token in template.split_whitespace() {
        match token {
            "{cwd}" => tokens.push(spec.cwd.to_string_lossy().into_owned()),
            "{program}" => tokens.push(spec.program.clone()),
            "{args}" => tokens.extend(spec.args.iter().cloned()),
            other => tokens.push(other.to_string()),
        }
    }

    if tokens.is_empty() {
        return Err(Error::invalid("the terminal template is empty"));
    }
    let program = tokens.remove(0);
    Ok(CommandSpec::new(program, tokens, spec.cwd.clone()))
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
pub fn check_cwd_exists(cwd: &Path) -> Result<()> {
    if cwd.is_dir() {
        Ok(())
    } else {
        Err(Error::invalid(format!(
            "working directory {} no longer exists",
            cwd.display()
        )))
    }
}
