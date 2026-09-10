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

//! End to end: from a home directory full of session files to a searchable
//! index (FR-2.x, FR-3.x, FR-6.1, NFR-2.2, NFR-3.5).

mod support;

use std::fs;
use std::path::{Path, PathBuf};

use mochi_core::adapter::EnvSource;
use mochi_core::index::{Index, SearchQuery, SessionQuery};
use mochi_core::model::{ParseStatus, ToolId};
use mochi_core::repo::{GitCli, NoGit};
use mochi_core::scan::{ScanOptions, Scanner};

/// A home directory holding every tool's fixtures, ready to be modified.
fn staged_home() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    for tool in ["claude_code", "codex", "opencode"] {
        support::copy_tree(&support::golden_home(tool), tmp.path());
    }
    tmp
}

fn scan_into(index: &mut Index, home: &Path, options: ScanOptions) -> mochi_core::scan::ScanReport {
    Scanner::new(EnvSource::with_home(home), &NoGit, options)
        .run(index)
        .unwrap()
}

/// Write a Claude Code transcript whose working directory is `cwd`.
fn write_session(home: &Path, project: &str, id: &str, cwd: &Path, text: &str) -> PathBuf {
    let dir = home.join(".claude/projects").join(project);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{id}.jsonl"));
    let line = serde_json::json!({
        "type": "user",
        "uuid": format!("{id}-0001"),
        "parentUuid": serde_json::Value::Null,
        "sessionId": id,
        "timestamp": "2026-09-05T09:00:00.000Z",
        "cwd": cwd.to_string_lossy(),
        "version": "1.0.60",
        "message": { "role": "user", "content": text }
    });
    fs::write(&path, format!("{line}\n")).unwrap();
    path
}

#[test]
fn discovery_is_ordered_so_that_indexing_is_reproducible() {
    // Row ids are assigned in the order sessions are inserted, and a caller
    // stores those ids to remember which session was open. If discovery
    // followed directory iteration order, the same store would produce
    // different ids on a different machine — which is exactly what a fixture
    // regenerated in CI caught.
    let home = staged_home();
    let env = EnvSource::with_home(home.path());

    for adapter in mochi_core::adapter::all() {
        for root in adapter.data_roots(&env) {
            let Ok(found) = adapter.discover(&root) else {
                continue;
            };
            let paths: Vec<_> = found.iter().map(|session| &session.source_path).collect();
            let mut sorted = paths.clone();
            sorted.sort();
            assert_eq!(paths, sorted, "{} returned {root:?} unsorted", adapter.id());
        }
    }
}

#[test]
fn a_first_scan_indexes_every_tool() {
    let home = staged_home();
    let mut index = Index::open_in_memory().unwrap();
    let report = scan_into(&mut index, home.path(), ScanOptions::default());

    assert_eq!(report.discovered, 9, "4 Claude Code + 3 Codex + 2 OpenCode");
    assert_eq!(report.parsed, 9);
    assert_eq!(report.unchanged, 0);
    assert_eq!(report.failed, 0, "{:?}", report.errors);

    let sessions = index.list_sessions(&SessionQuery::default()).unwrap();
    assert_eq!(sessions.len(), 9);
    for tool in ToolId::ALL {
        assert!(
            sessions.iter().any(|s| s.tool == tool),
            "{tool} produced no sessions"
        );
    }
}

#[test]
fn search_crosses_tools() {
    // UC-02 / FR-6.1: the whole point of the product. The Claude Code and the
    // Codex session both mention it, in different formats, in different stores.
    let home = staged_home();
    let mut index = Index::open_in_memory().unwrap();
    scan_into(&mut index, home.path(), ScanOptions::default());

    let hits = index
        .search(&SearchQuery {
            text: "ECONNRESET".into(),
            ..Default::default()
        })
        .unwrap();
    let tools: std::collections::BTreeSet<_> = hits.iter().map(|h| h.tool).collect();
    assert!(
        tools.contains(&ToolId::ClaudeCode) && tools.contains(&ToolId::Codex),
        "hits came from {tools:?}"
    );
}

#[test]
fn a_second_scan_reparses_nothing() {
    // FR-2.3: size and modification time decide, so a rescan of ten thousand
    // untouched sessions costs a directory walk and nothing more.
    let home = staged_home();
    let mut index = Index::open_in_memory().unwrap();
    scan_into(&mut index, home.path(), ScanOptions::default());

    let second = scan_into(&mut index, home.path(), ScanOptions::default());
    assert_eq!(second.parsed, 0);
    assert_eq!(second.unchanged, 9);
    assert_eq!(
        index.list_sessions(&SessionQuery::default()).unwrap().len(),
        9
    );
}

#[test]
fn a_changed_file_is_reparsed() {
    let home = staged_home();
    let repo_dir = home.path().join("code/live-repo");
    fs::create_dir_all(&repo_dir).unwrap();
    let path = write_session(
        home.path(),
        "-code-live",
        "aaaa1111",
        &repo_dir,
        "first turn",
    );

    let mut index = Index::open_in_memory().unwrap();
    scan_into(&mut index, home.path(), ScanOptions::default());

    let mut content = fs::read_to_string(&path).unwrap();
    content.push_str(&format!(
        "{}\n",
        serde_json::json!({
            "type": "assistant",
            "uuid": "aaaa1111-0002",
            "sessionId": "aaaa1111",
            "timestamp": "2026-09-05T09:00:30.000Z",
            "cwd": repo_dir.to_string_lossy(),
            "message": { "role": "assistant", "content": [{"type": "text", "text": "second turn"}] }
        })
    ));
    fs::write(&path, content).unwrap();

    let after = scan_into(&mut index, home.path(), ScanOptions::default());
    assert_eq!(after.parsed, 1, "only the file that grew");
    assert_eq!(
        index
            .search(&SearchQuery {
                text: "second turn".into(),
                ..Default::default()
            })
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn a_forced_scan_reparses_everything() {
    // FR-2.10: the escape hatch for when a parser changed.
    let home = staged_home();
    let mut index = Index::open_in_memory().unwrap();
    scan_into(&mut index, home.path(), ScanOptions::default());

    let forced = scan_into(
        &mut index,
        home.path(),
        ScanOptions {
            force: true,
            ..Default::default()
        },
    );
    assert_eq!(forced.parsed, 9);
    assert_eq!(forced.unchanged, 0);
}

#[test]
fn scanning_can_be_limited_to_one_tool() {
    // FR-1.5: a tool the user turned off is not scanned at all.
    let home = staged_home();
    let mut index = Index::open_in_memory().unwrap();
    let report = scan_into(
        &mut index,
        home.path(),
        ScanOptions {
            tools: vec![ToolId::Codex],
            ..Default::default()
        },
    );

    assert_eq!(report.discovered, 3);
    assert!(index
        .list_sessions(&SessionQuery::default())
        .unwrap()
        .iter()
        .all(|s| s.tool == ToolId::Codex));
}

#[test]
fn a_deleted_source_file_becomes_an_archived_session() {
    // FR-2.11, R-4, P-5: Claude Code deletes its own transcripts after 30 days
    // by default. After that Mochi holds the only copy, so the row and its
    // content must survive the file.
    let home = staged_home();
    let mut index = Index::open_in_memory().unwrap();
    scan_into(&mut index, home.path(), ScanOptions::default());

    let victim = home.path().join(
        ".claude/projects/-Users-you-code-my-repo/11111111-2222-3333-4444-555555555555.jsonl",
    );
    assert!(victim.exists());
    fs::remove_file(&victim).unwrap();

    let report = scan_into(&mut index, home.path(), ScanOptions::default());
    assert_eq!(report.archived, 1);
    assert_eq!(report.discovered, 8);

    let session = index
        .list_sessions(&SessionQuery::default())
        .unwrap()
        .into_iter()
        .find(|s| s.native_id == "11111111-2222-3333-4444-555555555555")
        .expect("the session is still listed");
    assert_eq!(session.parse_status, ParseStatus::Archived);
    assert!(
        !index.messages(session.id).unwrap().is_empty(),
        "content survives the file"
    );
    assert_eq!(
        index
            .search(&SearchQuery {
                text: "idempotent".into(),
                session_id: Some(session.id),
                ..Default::default()
            })
            .unwrap()
            .len(),
        2,
        "an archived session is still searchable: the thinking block and the reply both mention it"
    );
}

#[test]
fn an_unreadable_file_is_isolated() {
    // FR-2.7, NFR-2.2: one bad file, one bad row, everything else fine.
    let home = staged_home();
    let dir = home.path().join(".claude/projects/-broken");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("99999999-0000-0000-0000-000000000000.jsonl"),
        b"\x00\x01\x02 not a transcript",
    )
    .unwrap();

    let mut index = Index::open_in_memory().unwrap();
    let report = scan_into(&mut index, home.path(), ScanOptions::default());

    assert_eq!(report.discovered, 10);
    assert!(report.failed <= 1);
    assert!(
        index.list_sessions(&SessionQuery::default()).unwrap().len() >= 9,
        "the healthy sessions still made it in"
    );
    let broken = index
        .list_sessions(&SessionQuery::default())
        .unwrap()
        .into_iter()
        .find(|s| s.source_path.contains("-broken"))
        .expect("the unreadable session is listed, not hidden");
    assert!(
        matches!(
            broken.parse_status,
            ParseStatus::Failed | ParseStatus::Partial
        ),
        "got {:?}",
        broken.parse_status
    );
    assert!(
        broken.parse_error.is_some(),
        "FR-2.7 wants the reason on screen"
    );
}

#[test]
fn a_missing_working_directory_is_recorded_not_hidden() {
    // FR-3.6 and FR-7.6: the fixtures point at /Users/you/code/my-repo, which
    // does not exist on this machine. The sessions still list; they just
    // cannot be resumed.
    let home = staged_home();
    let mut index = Index::open_in_memory().unwrap();
    scan_into(&mut index, home.path(), ScanOptions::default());

    let sessions = index.list_sessions(&SessionQuery::default()).unwrap();
    let orphan = sessions
        .iter()
        .find(|s| s.native_id == "11111111-2222-3333-4444-555555555555")
        .unwrap();
    assert!(!orphan.cwd_exists);
    assert_eq!(
        orphan.repo_id, None,
        "no repository, not a wrong repository"
    );
    assert_eq!(orphan.cwd.as_deref(), Some("/Users/you/code/my-repo"));
}

#[test]
fn sessions_in_a_real_repository_are_grouped_by_it() {
    if !support::git_available() {
        eprintln!("skipping: git not on PATH");
        return;
    }
    let home = staged_home();
    let repo_dir = home.path().join("code/grouped-repo");
    support::init_repo(&repo_dir, Some("git@github.com:example/grouped-repo.git"));

    write_session(
        home.path(),
        "-code-grouped-repo",
        "bbbb1111",
        &repo_dir,
        "first session here",
    );
    write_session(
        home.path(),
        "-code-grouped-repo",
        "bbbb2222",
        &repo_dir,
        "second session here",
    );
    // A worktree of the same repository resolves back to it (FR-3.2, FR-3.4).
    let nested = repo_dir.join("src/deep");
    fs::create_dir_all(&nested).unwrap();
    write_session(
        home.path(),
        "-code-grouped-nested",
        "bbbb3333",
        &nested,
        "third session here",
    );

    let mut index = Index::open_in_memory().unwrap();
    Scanner::new(
        EnvSource::with_home(home.path()),
        &GitCli,
        ScanOptions::default(),
    )
    .run(&mut index)
    .unwrap();

    let repos = index.list_repositories().unwrap();
    assert_eq!(
        repos.len(),
        1,
        "one repository, not one per directory: {repos:?}"
    );
    assert_eq!(repos[0].display_name, "grouped-repo");
    assert_eq!(
        repos[0].remote_url.as_deref(),
        Some("github.com/example/grouped-repo")
    );
    assert!(repos[0].root_commit.is_some());

    let grouped = index
        .list_sessions(&SessionQuery {
            repo_id: Some(repos[0].id),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        grouped.len(),
        3,
        "including the one started in a subdirectory"
    );
    assert!(grouped.iter().all(|s| s.cwd_exists));
}

#[test]
fn a_scan_never_writes_to_a_session_file() {
    // Item 9 of the MVP definition of done, checked over every fixture at once.
    let home = staged_home();
    let before = support::fingerprint(home.path());

    let mut index = Index::open_in_memory().unwrap();
    scan_into(&mut index, home.path(), ScanOptions::default());
    scan_into(
        &mut index,
        home.path(),
        ScanOptions {
            force: true,
            ..Default::default()
        },
    );

    assert_eq!(
        support::fingerprint(home.path()),
        before,
        "a scan modified the store"
    );
}
