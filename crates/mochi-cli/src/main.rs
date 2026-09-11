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

//! `mochi` — the whole of Mochi, in one executable.
//!
//! Run it with no arguments and it opens the window; run it with a subcommand
//! and it is the command line indexer. One file, because a product whose point
//! is that it works offline and touches nothing should not need an installer
//! to arrive (NFR-4b.7).
//!
//! The command line half reads session stores, builds the index, and prints
//! what it found. It never launches a CLI and never writes to a session file.

// Windows: this is a windowed program, so it is built for the GUI subsystem
// and does not drag a console box along behind the window. `console::attach`
// below borrows the calling terminal's console back for the subcommands that
// print, which is what makes the same file work as a command line tool.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};

use mochi_core::adapter::{self, EnvSource};
use mochi_core::command::{QuoteStyle, ResumeTarget};
use mochi_core::export::ExportOptions;
use mochi_core::index::{
    Index, SearchQuery, SessionOrder, SessionQuery, SessionRecord, SNIPPET_END, SNIPPET_START,
};
use mochi_core::mask::Masker;
use mochi_core::model::{ParseStatus, ToolId};
use mochi_core::repo::{GitCli, GitProbe, NoGit};
use mochi_core::scan::{ScanOptions, Scanner};

#[derive(Parser)]
#[command(
    name = "mochi",
    version,
    about = "Index and search Codex CLI, Claude Code and OpenCode sessions",
    long_about = "Mochi collects the sessions the three agent CLIs leave on this machine, \
                  groups them by repository and makes them searchable.\n\nIt reads only: \
                  session files are never modified, and nothing is sent anywhere."
)]
struct Cli {
    #[command(flatten)]
    global: GlobalArgs,

    /// With no subcommand, Mochi opens its window.
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Args, Clone)]
struct GlobalArgs {
    /// Index database. Defaults to the OS application data directory.
    #[arg(long, global = true, value_name = "PATH")]
    db: Option<PathBuf>,

    /// Look for session stores under this directory instead of your home.
    #[arg(long, global = true, value_name = "PATH")]
    home: Option<PathBuf>,

    /// Show secrets instead of masking them.
    #[arg(long, global = true)]
    no_mask: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Open the Mochi window. The default when nothing else is asked for.
    Ui,
    /// Report which tools and session stores were found.
    Doctor,
    /// Find sessions and bring the index up to date.
    Scan(ScanArgs),
    /// List repositories.
    Repos,
    /// List sessions.
    Sessions(SessionsArgs),
    /// Search across every indexed session.
    Search(SearchArgs),
    /// Print the command that resumes a session. Does not run it.
    Resume(ResumeArgs),
    /// Write the whole index as JSON.
    Export(ExportArgs),
    /// Print the attribution notice for everything Mochi is built from.
    Licenses(LicenseArgs),
}

#[derive(Args)]
struct LicenseArgs {
    /// Also print the full text of the Apache License 2.0.
    #[arg(long)]
    full: bool,
}

#[derive(Args)]
struct ScanArgs {
    /// Only this tool. Repeat for several.
    #[arg(long, value_name = "TOOL")]
    tool: Vec<String>,

    /// Re-read every session, even ones that look unchanged.
    #[arg(long)]
    force: bool,

    /// Keep separate clones of one repository as separate entries.
    #[arg(long)]
    no_bundle_clones: bool,

    /// Skip running git. Repositories are then grouped by path alone.
    #[arg(long)]
    no_git: bool,
}

#[derive(Args)]
struct SessionsArgs {
    #[arg(long, value_name = "TOOL")]
    tool: Option<String>,

    /// Repository id, as shown by `mochi repos`.
    #[arg(long, value_name = "ID")]
    repo: Option<i64>,

    #[arg(long, default_value_t = 20)]
    limit: u32,

    /// updated (default), started, messages or size.
    #[arg(long, default_value = "updated")]
    sort: String,
}

#[derive(Args)]
struct SearchArgs {
    /// What to look for. Quote a phrase to keep the words together.
    query: Vec<String>,

    #[arg(long, value_name = "TOOL")]
    tool: Option<String>,

    #[arg(long, value_name = "ID")]
    repo: Option<i64>,

    #[arg(long, default_value_t = 20)]
    limit: u32,
}

#[derive(Args)]
struct ExportArgs {
    /// Write here instead of to standard output.
    #[arg(long, value_name = "PATH")]
    out: Option<PathBuf>,

    /// Indent the JSON.
    #[arg(long)]
    pretty: bool,
}

#[derive(Args)]
struct ResumeArgs {
    /// Session id, as shown by `mochi sessions`.
    id: i64,
}

fn main() {
    // Which command this is decides where its answer goes, so the arguments
    // are read first. clap's own output — a usage error, `--help` — is an
    // answer to something typed at a prompt and needs a console just as much.
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(reply) => {
            #[cfg(windows)]
            console::attach();
            let _ = reply.print();
            std::process::exit(reply.exit_code());
        }
    };

    // Only the subcommands that print take a console. The window never does:
    // launched from anything that is not a terminal — a shortcut running
    // `mochi.exe --no-mask`, a scheduler, `mochi.exe ui` — it would otherwise
    // find no console to borrow, make one, and leave an empty black box beside
    // the window for the rest of the session.
    #[cfg(windows)]
    if !matches!(cli.command, None | Some(Command::Ui)) {
        console::attach();
    }

    if let Err(error) = run(cli) {
        report(&error);
        std::process::exit(1);
    }
}

/// Say why, somewhere it will be read.
fn report(error: &anyhow::Error) {
    let text = format!("mochi: {error:#}");

    // Windows: a double-clicked GUI-subsystem process has no console, so
    // `eprintln!` writes to an invalid handle and the user is left with a
    // program that started and did nothing visible at all. The window's own
    // failures — no usable GPU adapter, an index that cannot be opened —
    // happen before there is a window to say so in, so they are said here.
    #[cfg(windows)]
    if !console::can_print() && !console::borrow_parent() {
        console::message(&text);
        return;
    }

    eprintln!("{text}");
}

fn run(cli: Cli) -> Result<()> {
    match &cli.command {
        None | Some(Command::Ui) => ui(&cli.global),
        Some(Command::Doctor) => doctor(&cli.global),
        Some(Command::Scan(args)) => scan(&cli.global, args),
        Some(Command::Repos) => repos(&cli.global),
        Some(Command::Sessions(args)) => sessions(&cli.global, args),
        Some(Command::Search(args)) => search(&cli.global, args),
        Some(Command::Resume(args)) => resume(&cli.global, args),
        Some(Command::Export(args)) => export(&cli.global, args),
        Some(Command::Licenses(args)) => licenses(args),
    }
}

/// Open the window.
///
/// The global flags mean the same here as they do anywhere else: `--db` for an
/// index kept away from the real one, `--home` to browse a fixture tree, and
/// `--no-mask` to open with credentials shown.
fn ui(global: &GlobalArgs) -> Result<()> {
    mochi_gui::run(mochi_gui::Options {
        index_path: global.db.clone(),
        home: global.home.clone(),
        reveal_secrets: global.no_mask,
    })
    .map_err(|error| anyhow::anyhow!(error))
}

/// Print into the console that started us, and say things where a double-click
/// can read them.
///
/// Windows gives a GUI program no console of its own, so without this a
/// subcommand would run and say nothing. Written out rather than taken from a
/// crate: it is a handful of calls, and the alternative is a dependency in the
/// shipped binary for a few lines of FFI.
#[cfg(windows)]
mod console {
    /// `ATTACH_PARENT_PROCESS`: the console of whatever started this process.
    const PARENT: u32 = u32::MAX;
    const STD_OUTPUT: u32 = -11i32 as u32;
    const STD_ERROR: u32 = -12i32 as u32;
    const INVALID_HANDLE: isize = -1;
    /// `MB_OK | MB_ICONERROR`.
    const ERROR_BOX: u32 = 0x10;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn AttachConsole(process_id: u32) -> i32;
        fn AllocConsole() -> i32;
        fn GetConsoleWindow() -> isize;
        fn GetStdHandle(which: u32) -> isize;
        fn SetStdHandle(which: u32, handle: isize) -> i32;
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(owner: isize, text: *const u16, caption: *const u16, style: u32) -> i32;
    }

    /// Is there anywhere for a message to go as it stands — a console, or a
    /// file or pipe somebody redirected the output to?
    ///
    /// `false` means printing would write to an invalid handle and be lost,
    /// which for a double-clicked window is the difference between a reason and
    /// a program that did nothing.
    pub fn can_print() -> bool {
        // SAFETY: takes no arguments and returns a window handle or nothing.
        let console = unsafe { GetConsoleWindow() != 0 };
        console || handle_set(STD_ERROR)
    }

    /// Borrow the console of whatever started this process. `false` when there
    /// was none: a double-click, a shortcut, a scheduler.
    pub fn borrow_parent() -> bool {
        keeping_std_handles(|| unsafe { AttachConsole(PARENT) != 0 })
    }

    /// Somewhere to print: the console that started us, or one of our own.
    pub fn attach() {
        if borrow_parent() || handle_set(STD_OUTPUT) {
            // Already going somewhere. A file or a pipe is somewhere, and a
            // console of our own on top of one would be an empty box.
            return;
        }
        keeping_std_handles(|| unsafe { AllocConsole() != 0 });
    }

    /// Is one of the standard handles pointing at anything?
    ///
    /// Windows leaves these empty for a process started from Explorer with
    /// nothing redirected, which is what tells a double-click apart from a
    /// `mochi export > index.json`.
    fn handle_set(which: u32) -> bool {
        // SAFETY: takes an identifier and returns a handle or nothing.
        let handle = unsafe { GetStdHandle(which) };
        handle != 0 && handle != INVALID_HANDLE
    }

    /// Attach a console without letting it take the output away.
    ///
    /// `mochi export > index.json` reaches this holding a file handle for its
    /// standard output. Attaching a console can point `STD_OUTPUT_HANDLE` at
    /// the console screen buffer instead — which would print the JSON to the
    /// terminal and leave the file empty — so whatever was there before is put
    /// back afterwards. In the ordinary case the handles are the console's own
    /// and restoring them changes nothing.
    fn keeping_std_handles(attach: impl FnOnce() -> bool) -> bool {
        // SAFETY: none of these take pointers; each returns a handle or a
        // boolean. `SetStdHandle` is given back a handle this process was
        // holding a moment earlier.
        unsafe {
            let saved = [
                (STD_OUTPUT, GetStdHandle(STD_OUTPUT)),
                (STD_ERROR, GetStdHandle(STD_ERROR)),
            ];
            let attached = attach();
            for (which, handle) in saved {
                if handle != 0 && handle != INVALID_HANDLE {
                    SetStdHandle(which, handle);
                }
            }
            attached
        }
    }

    /// Say something where a double-click can read it.
    pub fn message(text: &str) {
        let text = wide(text);
        let caption = wide("Mochi");
        // SAFETY: both strings are NUL-terminated and outlive the call, which
        // blocks until the box is dismissed.
        unsafe {
            MessageBoxW(0, text.as_ptr(), caption.as_ptr(), ERROR_BOX);
        }
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

fn env_source(global: &GlobalArgs) -> EnvSource {
    match &global.home {
        Some(home) => EnvSource::with_home(home),
        None => EnvSource::from_process(),
    }
}

/// Where the index lives. Windows and macOS conventions, never `~/.config`
/// (FR-10.1).
fn database_path(global: &GlobalArgs) -> Result<PathBuf> {
    if let Some(path) = &global.db {
        return Ok(path.clone());
    }
    mochi_core::config::index_path()
        .context("could not work out this system's application data directory; pass --db")
}

fn open_index(global: &GlobalArgs) -> Result<Index> {
    let path = database_path(global)?;
    Index::open(&path).with_context(|| format!("opening the index at {}", path.display()))
}

fn parse_tool(name: &str) -> Result<ToolId> {
    ToolId::parse(name)
        .with_context(|| format!("unknown tool {name:?}; expected claude_code, codex or opencode"))
}

fn doctor(global: &GlobalArgs) -> Result<()> {
    let env = env_source(global);
    println!("home:  {}", env.home().display());
    println!("index: {}", database_path(global)?.display());
    println!();

    for tool in adapter::all() {
        println!("{}", tool.id().display_name());
        for root in tool.data_roots(&env) {
            let exists = root.is_dir();
            match tool.discover(&root) {
                Ok(found) if exists => {
                    println!("  {}  {} session(s)", root.display(), found.len())
                }
                Ok(_) => println!("  {}  not present", root.display()),
                Err(error) => println!("  {}  unreadable: {error}", root.display()),
            }
        }
    }
    Ok(())
}

fn scan(global: &GlobalArgs, args: &ScanArgs) -> Result<()> {
    let mut tools = Vec::new();
    for name in &args.tool {
        tools.push(parse_tool(name)?);
    }

    let options = ScanOptions {
        tools,
        force: args.force,
        bundle_clones: !args.no_bundle_clones,
    };

    let git_cli = GitCli;
    let no_git = NoGit;
    let probe: &dyn GitProbe = if args.no_git { &no_git } else { &git_cli };

    let mut index = open_index(global)?;
    let report = Scanner::new(env_source(global), probe, options).run(&mut index)?;

    println!(
        "found {} session(s): {} read, {} unchanged, {} archived, {} unreadable",
        report.discovered, report.parsed, report.unchanged, report.archived, report.failed
    );
    println!("{} repositor(y/ies) in the index", report.repositories);

    if !report.errors.is_empty() {
        println!();
        println!("problems ({}):", report.errors.len());
        for error in report.errors.iter().take(20) {
            println!(
                "  [{}] {}: {}",
                error.tool,
                error.path.display(),
                error.message
            );
        }
        if report.errors.len() > 20 {
            println!("  ... and {} more", report.errors.len() - 20);
        }
    }
    Ok(())
}

fn repos(global: &GlobalArgs) -> Result<()> {
    let index = open_index(global)?;
    let repositories = index.list_repositories()?;
    if repositories.is_empty() {
        println!("no repositories yet — run `mochi scan` first");
        return Ok(());
    }

    for repo in repositories {
        let sessions = index.list_sessions(&SessionQuery {
            repo_id: Some(repo.id),
            limit: Some(u32::MAX),
            ..Default::default()
        })?;
        println!(
            "{:>4}  {:<28} {} session(s){}",
            repo.id,
            repo.display_name,
            sessions.len(),
            if repo.is_worktree { "  [worktree]" } else { "" }
        );
        println!("      {}", repo.root_path);
        if let Some(remote) = &repo.remote_url {
            println!("      {remote}");
        }
    }

    let loose = index.list_sessions(&SessionQuery {
        limit: Some(u32::MAX),
        ..Default::default()
    })?;
    let orphans = loose.iter().filter(|s| s.repo_id.is_none()).count();
    if orphans > 0 {
        println!();
        println!("{orphans} session(s) belong to no repository");
    }
    Ok(())
}

fn sessions(global: &GlobalArgs, args: &SessionsArgs) -> Result<()> {
    let order = match args.sort.as_str() {
        "updated" => SessionOrder::UpdatedDesc,
        "started" => SessionOrder::StartedDesc,
        "messages" => SessionOrder::MessagesDesc,
        "size" => SessionOrder::SizeDesc,
        other => {
            anyhow::bail!("unknown sort {other:?}; expected updated, started, messages or size")
        }
    };

    let query = SessionQuery {
        repo_id: args.repo,
        tool: args.tool.as_deref().map(parse_tool).transpose()?,
        order,
        limit: Some(args.limit),
        ..Default::default()
    };

    let index = open_index(global)?;
    let found = index.list_sessions(&query)?;
    if found.is_empty() {
        println!("no sessions — run `mochi scan` first");
        return Ok(());
    }

    let masker = Masker::new();
    let mut out = io::stdout().lock();
    for session in &found {
        let title = session.title.as_deref().unwrap_or("(no title)");
        let title = if global.no_mask {
            title.to_string()
        } else {
            masker.mask(title).text
        };
        writeln!(
            out,
            "{:>5}  {:<12} {:<19} {:>4} msg  {}{}",
            session.id,
            session.tool.as_str(),
            format_time(session.updated_at),
            session.message_count,
            truncate(&title, 60),
            status_note(session),
        )?;
    }
    Ok(())
}

/// The state a user needs to see next to a row: why a session cannot be
/// resumed, or why it looks incomplete (FR-4.2).
fn status_note(session: &SessionRecord) -> String {
    let mut notes = Vec::new();
    match session.parse_status {
        ParseStatus::Archived => notes.push("archived".to_string()),
        ParseStatus::Failed => notes.push("unreadable".to_string()),
        ParseStatus::Partial => notes.push("partial".to_string()),
        ParseStatus::Ok => {}
    }
    if !session.cwd_exists {
        notes.push("working directory missing".to_string());
    }
    if notes.is_empty() {
        String::new()
    } else {
        format!("  [{}]", notes.join(", "))
    }
}

fn search(global: &GlobalArgs, args: &SearchArgs) -> Result<()> {
    let text = args.query.join(" ");
    if text.trim().is_empty() {
        anyhow::bail!("nothing to search for");
    }

    let index = open_index(global)?;
    let hits = index.search(&SearchQuery {
        text,
        repo_id: args.repo,
        session_id: None,
        tool: args.tool.as_deref().map(parse_tool).transpose()?,
        limit: Some(args.limit),
        reveal_secrets: global.no_mask,
    })?;

    if hits.is_empty() {
        println!("no matches");
        return Ok(());
    }

    let colour = io::stdout().is_terminal();
    let mut out = io::stdout().lock();
    for hit in &hits {
        writeln!(
            out,
            "{:>5}  {:<12} {:<19} {}",
            hit.session_id,
            hit.tool.as_str(),
            format_time(hit.timestamp),
            hit.repo_display.as_deref().unwrap_or("(no repository)")
        )?;
        // The snippet was already masked by the index unless --no-mask was
        // passed: it has to be, because a snippet cut out of unmasked text can
        // still show half a key (NFR-3.3).
        writeln!(out, "       {}", render_snippet(&hit.snippet, colour))?;
    }
    Ok(())
}

/// Turn the index's match markers into something readable.
fn render_snippet(snippet: &str, colour: bool) -> String {
    let one_line = snippet.replace(['\n', '\r'], " ");
    if colour {
        one_line
            .replace(SNIPPET_START, "\x1b[1;33m")
            .replace(SNIPPET_END, "\x1b[0m")
    } else {
        one_line
            .replace(SNIPPET_START, "[")
            .replace(SNIPPET_END, "]")
    }
}

fn resume(global: &GlobalArgs, args: &ResumeArgs) -> Result<()> {
    let index = open_index(global)?;
    let session = index
        .session(args.id)?
        .with_context(|| format!("no session with id {}", args.id))?;

    if session.parse_status == ParseStatus::Archived {
        anyhow::bail!(
            "session {} is archived: the original transcript is gone, so there is nothing for \
             the CLI to resume. Its content is still readable in the index.",
            session.id
        );
    }
    let cwd = session
        .cwd
        .as_deref()
        .context("this session has no recorded working directory")?;
    if !session.cwd_exists {
        anyhow::bail!(
            "the working directory {cwd} no longer exists, so this session cannot be resumed"
        );
    }

    let target = ResumeTarget::new(&session.native_id, PathBuf::from(cwd));
    let spec = adapter::for_tool(session.tool).resume_command(&target)?;

    // Printed for a human to copy (FR-7.4). Mochi does not run it: launching
    // belongs to the app, which starts the process from this argument vector
    // rather than from this text (NFR-3.6).
    println!("{}", spec.to_display_string(QuoteStyle::for_host()));
    Ok(())
}

/// Write the whole index as one JSON document.
///
/// The shape lives in `mochi_core::export`, which is also where it is
/// documented as the one format Mochi promises to anything outside it.
fn export(global: &GlobalArgs, args: &ExportArgs) -> Result<()> {
    let index = open_index(global)?;
    let document = mochi_core::export::document(
        &index,
        ExportOptions {
            reveal_secrets: global.no_mask,
            ..Default::default()
        },
    )?;

    let text = if args.pretty {
        serde_json::to_string_pretty(&document)?
    } else {
        serde_json::to_string(&document)?
    };

    match &args.out {
        Some(path) => {
            std::fs::write(path, text + "\n")
                .with_context(|| format!("writing {}", path.display()))?;
            eprintln!("wrote {}", path.display());
        }
        None => println!("{text}"),
    }
    Ok(())
}

/// The attribution Apache-2.0 §4 asks a redistributor to pass on.
///
/// Carried inside the executable rather than shipped beside it: Mochi is one
/// file on purpose, and a licence notice in a second file is a licence notice
/// that gets separated from the binary the first time somebody copies it.
fn licenses(args: &LicenseArgs) -> Result<()> {
    const NOTICE: &str = include_str!("../../../NOTICE");
    const LICENSE: &str = include_str!("../../../LICENSE");

    print!("{NOTICE}");
    if args.full {
        println!();
        print!("{LICENSE}");
    } else {
        println!();
        println!("Mochi itself is under the Apache License 2.0; run `mochi licenses --full`");
        println!("for its text, or see https://github.com/tomyhara/Mochi/blob/main/LICENSE");
    }
    Ok(())
}

fn format_time(milliseconds: Option<i64>) -> String {
    let Some(ms) = milliseconds else {
        return "-".to_string();
    };
    match chrono::DateTime::from_timestamp_millis(ms) {
        Some(time) => time.format("%Y-%m-%d %H:%M:%S").to_string(),
        None => "-".to_string(),
    }
}

fn truncate(text: &str, limit: usize) -> String {
    let cleaned = text.replace(['\n', '\r'], " ");
    if cleaned.chars().count() <= limit {
        return cleaned;
    }
    cleaned
        .chars()
        .take(limit.saturating_sub(1))
        .collect::<String>()
        + "…"
}
