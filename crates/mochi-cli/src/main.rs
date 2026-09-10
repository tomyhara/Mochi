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

//! `mochi` — the command line indexer.
//!
//! This is the milestone-1 deliverable: everything the desktop app will need,
//! driven from a terminal so the core can be exercised before there is a UI.
//! It reads session stores, builds the index, and prints what it found. It
//! never launches a CLI and never writes to a session file.

use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};

use mochi_core::adapter::{self, EnvSource};
use mochi_core::command::{QuoteStyle, ResumeTarget};
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

    #[command(subcommand)]
    command: Command,
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
struct ResumeArgs {
    /// Session id, as shown by `mochi sessions`.
    id: i64,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("mochi: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Command::Doctor => doctor(&cli.global),
        Command::Scan(args) => scan(&cli.global, args),
        Command::Repos => repos(&cli.global),
        Command::Sessions(args) => sessions(&cli.global, args),
        Command::Search(args) => search(&cli.global, args),
        Command::Resume(args) => resume(&cli.global, args),
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
