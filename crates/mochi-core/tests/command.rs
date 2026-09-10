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

//! Building launch commands (FR-7.1 – FR-7.6, NFR-3.6, NFR-6.5).

use std::path::{Path, PathBuf};

use mochi_core::adapter::{self, ToolAdapter};
use mochi_core::command::{
    check_cwd_exists, external_launch, quote_cmd, quote_posix, quote_powershell, CommandSpec,
    QuoteStyle, ResumeTarget, TerminalKind,
};
use mochi_core::model::ToolId;

fn resume(tool: ToolId, id: &str, cwd: &str) -> CommandSpec {
    adapter::for_tool(tool)
        .resume_command(&ResumeTarget::new(id, PathBuf::from(cwd)))
        .expect("resume command")
}

#[test]
fn each_tool_gets_its_own_resume_command() {
    let cc = resume(ToolId::ClaudeCode, "11111111-2222-3333-4444-555555555555", "/Users/you/code/my-repo");
    assert_eq!(cc.program, "claude");
    assert_eq!(cc.args, vec!["--resume", "11111111-2222-3333-4444-555555555555"]);

    let cx = resume(ToolId::Codex, "019242aa-1111-7bbb-8ccc-000000000001", "/Users/you/code/my-repo");
    assert_eq!(cx.program, "codex");
    assert_eq!(cx.args, vec!["resume", "019242aa-1111-7bbb-8ccc-000000000001"]);

    let oc = resume(ToolId::OpenCode, "ses_EXAMPLE01", "/Users/you/code/my-repo");
    assert_eq!(oc.program, "opencode");
    assert_eq!(oc.args, vec!["--session", "ses_EXAMPLE01"]);
}

#[test]
fn resume_always_carries_the_sessions_own_directory() {
    // FR-7.1: starting the CLI anywhere else points the agent at the wrong
    // repository, which is worse than not starting it.
    let spec = resume(ToolId::ClaudeCode, "abc", "/Users/you/code/my-repo");
    assert_eq!(spec.cwd, Path::new("/Users/you/code/my-repo"));
}

#[test]
fn a_user_supplied_executable_path_is_honoured() {
    // FR-1.3: the CLI is not always on PATH under its plain name.
    let mut target = ResumeTarget::new("abc", PathBuf::from("/Users/you/code/my-repo"));
    target.executable = Some(PathBuf::from("/opt/tools/claude-1.0.60"));
    let spec = adapter::for_tool(ToolId::ClaudeCode).resume_command(&target).unwrap();
    assert_eq!(spec.program, "/opt/tools/claude-1.0.60");
    assert_eq!(spec.args, vec!["--resume", "abc"]);
}

#[test]
fn new_session_commands_take_no_id() {
    for tool in ToolId::ALL {
        let spec = adapter::for_tool(tool)
            .new_session_command(Path::new("/Users/you/code/my-repo"), None)
            .unwrap();
        assert_eq!(spec.cwd, Path::new("/Users/you/code/my-repo"));
        assert!(
            !spec.args.iter().any(|a| a.contains("resume") || a.contains("--session")),
            "{tool} new session should not resume anything: {:?}",
            spec.args
        );
    }
}

#[test]
fn a_hostile_session_id_stays_one_argument() {
    // NFR-3.6: session ids come from files Mochi did not write. This is the
    // test that says an id can never become shell syntax.
    let evil = "abc; rm -rf ~; echo ";
    let spec = resume(ToolId::ClaudeCode, evil, "/Users/you/code/my-repo");
    assert_eq!(spec.args, vec!["--resume", evil]);
    assert_eq!(spec.args.len(), 2, "the id must not be split into several arguments");
}

#[test]
fn a_hostile_id_is_quoted_when_rendered_for_a_human() {
    // FR-7.4: the "copy command" text is pasted into a real shell, so it has
    // to be quoted even though Mochi itself never runs it.
    let spec = resume(ToolId::ClaudeCode, "abc; rm -rf ~", "/Users/you/code/my repo");
    let line = spec.to_display_string(QuoteStyle::Posix);
    assert!(line.contains("'abc; rm -rf ~'"), "got {line}");
    assert!(!line.contains("; rm -rf ~ "), "unquoted metacharacters escaped into {line}");
}

#[test]
fn posix_quoting() {
    assert_eq!(quote_posix("plain"), "plain");
    assert_eq!(quote_posix("/Users/you/code/my-repo"), "/Users/you/code/my-repo");
    assert_eq!(quote_posix("with space"), "'with space'");
    assert_eq!(quote_posix("it's"), r#"'it'\''s'"#);
    assert_eq!(quote_posix("a;b"), "'a;b'");
    assert_eq!(quote_posix("$HOME"), "'$HOME'");
    assert_eq!(quote_posix(""), "''");
    // NFR-6.5: a Japanese path is not a special character.
    assert_eq!(quote_posix("/Users/you/コード"), "/Users/you/コード");
    assert_eq!(quote_posix("/Users/you/私の コード"), "'/Users/you/私の コード'");
}

#[test]
fn powershell_quoting() {
    assert_eq!(quote_powershell("plain"), "plain");
    assert_eq!(quote_powershell(r"C:\Users\you\code"), r"C:\Users\you\code");
    assert_eq!(quote_powershell(r"C:\Users\you\my repo"), r"'C:\Users\you\my repo'");
    assert_eq!(quote_powershell("it's"), "'it''s'");
    assert_eq!(quote_powershell("a;b"), "'a;b'");
    assert_eq!(quote_powershell("$env:PATH"), "'$env:PATH'");
}

#[test]
fn cmd_quoting() {
    assert_eq!(quote_cmd("plain"), "plain");
    assert_eq!(quote_cmd(r"C:\Users\you\my repo"), r#""C:\Users\you\my repo""#);
    assert_eq!(quote_cmd("a&b"), r#""a&b""#);
    assert_eq!(quote_cmd("a%PATH%b"), r#""a%PATH%b""#);
}

#[test]
fn display_string_shows_the_directory_change_first() {
    let spec = resume(ToolId::Codex, "019242aa", "/Users/you/code/my repo");
    let line = spec.to_display_string(QuoteStyle::Posix);
    assert!(line.starts_with("cd '/Users/you/code/my repo' && "), "got {line}");
    assert!(line.ends_with("codex resume 019242aa"), "got {line}");
}

#[test]
fn missing_working_directory_is_refused_before_launch() {
    // FR-7.6: do not try and fail, say so up front.
    let tmp = tempfile::tempdir().unwrap();
    assert!(check_cwd_exists(tmp.path()).is_ok());
    assert!(check_cwd_exists(&tmp.path().join("gone")).is_err());
}

#[test]
fn windows_terminal_is_launched_from_an_argument_vector() {
    let spec = resume(ToolId::ClaudeCode, "abc", r"C:\Users\you\my repo");
    let wrapped = external_launch(&spec, &TerminalKind::WindowsTerminal).unwrap();
    assert_eq!(wrapped.program, "wt.exe");
    assert_eq!(
        wrapped.args,
        vec!["-d", r"C:\Users\you\my repo", "claude", "--resume", "abc"],
        "no argument may be joined into a single string"
    );
}

#[test]
fn a_custom_terminal_template_substitutes_into_an_argument_vector() {
    // FR-7.3 "other": the user gives an argument list, not a shell line, so
    // there is still nothing for a shell to reinterpret.
    let spec = resume(ToolId::Codex, "abc", "/Users/you/code/my repo");
    let template = "kitty --directory {cwd} -- {program} {args}";
    let wrapped = external_launch(&spec, &TerminalKind::Custom(template.into())).unwrap();
    assert_eq!(wrapped.program, "kitty");
    assert_eq!(
        wrapped.args,
        vec!["--directory", "/Users/you/code/my repo", "--", "codex", "resume", "abc"]
    );
}

#[test]
fn terminals_that_would_need_a_shell_string_are_refused_for_now() {
    // Terminal.app, iTerm2, cmd.exe and PowerShell can only be handed a
    // command as text, which is exactly what NFR-3.6 forbids Mochi from
    // building. They are deferred to the milestone-3 terminal work rather than
    // shipped with a quoting bug. Until then the integrated terminal (the
    // default, FR-7.8a) and "copy command" (FR-7.4) cover the need.
    let spec = resume(ToolId::ClaudeCode, "abc", "/Users/you/code/my-repo");
    for kind in [
        TerminalKind::TerminalApp,
        TerminalKind::ITerm2,
        TerminalKind::Cmd,
        TerminalKind::PowerShell,
    ] {
        let err = external_launch(&spec, &kind).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("not supported yet"), "unhelpful message: {message}");
    }
}

#[test]
fn quote_style_for_host_is_defined_everywhere() {
    let style = QuoteStyle::for_host();
    if cfg!(windows) {
        assert_eq!(style, QuoteStyle::PowerShell);
    } else {
        assert_eq!(style, QuoteStyle::Posix);
    }
}
