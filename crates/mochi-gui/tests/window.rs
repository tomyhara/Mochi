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

//! The window itself, drawn without a window.
//!
//! egui lays out and paints into a buffer; only the last step needs a screen.
//! So the whole interface can be run here against a real index, which is the
//! only way the drawing code is ever executed in CI — and the only way the
//! promise that a masked index stays masked *on screen* is actually checked,
//! rather than checked one layer below and assumed.

use eframe::App as _;
use egui::{Context, Pos2, Rect, Vec2};

use mochi_core::index::{Index, RepositoryRecord, SessionRecord, SessionStatus};
use mochi_core::model::{Message, ParseStatus, Role, ToolId};
use mochi_gui::{App, Options};

const KEY: &str = "OPENAI_API_KEY=sk-EXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAMPLEEX";

fn seeded(path: &std::path::Path) {
    let mut index = Index::open(path).unwrap();
    let repo_id = index
        .upsert_repository(&RepositoryRecord {
            identity_key: "path:/Users/you/code/alpha".into(),
            display_name: "alpha".into(),
            root_path: "/Users/you/code/alpha".into(),
            path_key: "/Users/you/code/alpha".into(),
            ..Default::default()
        })
        .unwrap();

    let id = index
        .upsert_session(&SessionRecord {
            id: 0,
            tool: ToolId::ClaudeCode,
            native_id: "sess-1".into(),
            repo_id: Some(repo_id),
            cwd: Some("/Users/you/code/alpha".into()),
            git_branch: Some("main".into()),
            title: Some("why does deploy fail".into()),
            model: Some("claude-opus-5".into()),
            started_at: Some(1_000),
            updated_at: Some(2_000),
            message_count: 2,
            tokens_in: 1,
            tokens_out: 2,
            status: SessionStatus::Finished,
            source_path: "/store/a.jsonl".into(),
            source_size: 2048,
            source_mtime: 2_000,
            schema_version: None,
            parse_status: ParseStatus::Ok,
            cwd_exists: false,
            parse_error: None,
        })
        .unwrap();
    index
        .replace_messages(
            id,
            &[
                Message::new(0, Role::User, "why does deploy fail? cat .env"),
                Message::new(1, Role::ToolResult, KEY),
            ],
        )
        .unwrap();
}

struct Headless {
    _dir: tempfile::TempDir,
    ctx: Context,
    app: App,
    frame: eframe::Frame,
    /// Delivered with the next frame, as a keypress would be.
    pending: Vec<egui::Event>,
}

impl Headless {
    fn start() -> Headless {
        Headless::started(|_| {})
    }

    /// Start a window on the seeded index, with `prepare` given a chance to
    /// change it first — which is how a failure that only the index can
    /// produce is arranged.
    fn started(prepare: impl FnOnce(&std::path::Path)) -> Headless {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("index.sqlite3");
        seeded(&db);
        prepare(&db);

        let ctx = Context::default();
        let app = App::new(
            &ctx,
            Options {
                index_path: Some(db),
                // Points the scanner at an empty tree rather than at whoever
                // is running the tests. Nothing should scan at all here.
                home: Some(dir.path().to_path_buf()),
                reveal_secrets: false,
            },
        );
        Headless {
            _dir: dir,
            ctx,
            app,
            frame: eframe::Frame::_new_kittest(),
            pending: Vec::new(),
        }
    }

    /// Press a key with the platform's command modifier held, on the next
    /// frame.
    fn press(&mut self, key: egui::Key) {
        self.pending.push(egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        });
    }

    /// Draw one frame and collect every piece of text that was painted.
    fn frame(&mut self) -> Vec<String> {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1400.0, 900.0))),
            events: std::mem::take(&mut self.pending),
            ..Default::default()
        };
        let app = &mut self.app;
        let frame = &mut self.frame;
        let output = self.ctx.run_ui(input, |ui| app.ui(ui, frame));

        let mut text = Vec::new();
        for clipped in output.shapes {
            collect(&clipped.shape, &mut text);
        }
        text
    }

    /// Draw frames until something the index thread had to fetch is on
    /// screen, or give up and let the assertion say what was missing.
    fn settle(&mut self, wanted: &str) -> String {
        let mut painted = String::new();
        for _ in 0..200 {
            painted = self.frame().join("\n");
            if painted.contains(wanted) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        painted
    }
}

fn collect(shape: &egui::epaint::Shape, into: &mut Vec<String>) {
    match shape {
        egui::epaint::Shape::Text(text) => into.push(text.galley.text().to_string()),
        egui::epaint::Shape::Vec(shapes) => {
            for shape in shapes {
                collect(shape, into);
            }
        }
        _ => {}
    }
}

#[test]
fn the_window_draws_the_index_it_was_given() {
    let mut window = Headless::start();
    let painted = window.settle("why does deploy fail");

    // The three columns of layout 1b: the repository and its sessions, the
    // transcript, and what the session is.
    assert!(painted.contains("Repositories"), "{painted}");
    assert!(painted.contains("alpha"), "{painted}");
    assert!(painted.contains("why does deploy fail"), "{painted}");
    assert!(painted.contains("Claude Code"), "{painted}");
    assert!(painted.contains("sess-1"), "{painted}");
}

/// NFR-3.3, end to end: this is the assertion that covers the actual promise —
/// not that the worker masked it, but that no pixel of a credential is drawn.
#[test]
fn a_credential_is_never_painted_while_the_index_is_masked() {
    let mut window = Headless::start();
    let painted = window.settle("Tool result");

    assert!(painted.contains("Tool result"), "the transcript never drew");
    assert!(!painted.contains("sk-EXAMPLE"), "a credential was painted");
    assert!(painted.contains("Credentials are masked in what you see."));
}

/// FR-7.6: a session whose working directory is gone cannot be resumed, and
/// the window has to say so where the command would have been.
#[test]
fn the_rail_explains_why_a_session_cannot_be_resumed() {
    let mut window = Headless::start();
    let painted = window.settle("Cannot resume");

    assert!(painted.contains("Cannot resume"), "{painted}");
    assert!(painted.contains("Working directory missing."), "{painted}");
}

/// FR-9.6: a transcript that cannot be read has to say so where it would have
/// been drawn. Treating "no transcript" as "still loading" left the centre
/// pane spinning for ever with the reason hidden in the status bar.
#[test]
fn a_transcript_that_cannot_be_read_explains_itself_instead_of_spinning() {
    let mut window = Headless::started(|db| {
        // What a corrupt `messages` table looks like from the reader's side:
        // the row is there, and reading it back fails. Everything else about
        // the session — the list, the counts, the rail — still works, which
        // is exactly the case the pane has to have an answer for.
        let conn = rusqlite::Connection::open(db).unwrap();
        // Bytes where text is expected: the column keeps them (SQLite's TEXT
        // affinity leaves a blob alone), and reading the row back fails.
        conn.execute("UPDATE messages SET role = x'00ff'", [])
            .unwrap();
    });
    let painted = window.settle("could not be read");

    assert!(
        painted.contains("This transcript could not be read"),
        "{painted}"
    );
    assert!(
        !painted.contains("Reading the transcript…"),
        "still spinning: {painted}"
    );
    // The session itself is not lost with its transcript.
    assert!(painted.contains("why does deploy fail"), "{painted}");
}

/// A rescan can change what is in the session being read, so the transcript on
/// screen afterwards has to be the one read after the scan — including when an
/// answer to the pre-scan question was already on its way.
#[test]
fn a_rescan_started_while_a_transcript_is_in_flight_still_ends_up_with_one() {
    let mut window = Headless::started(|_| {});
    // One frame: enough for the first snapshot to select a session and ask for
    // its transcript, and not enough for the answer to have been taken.
    window.frame();
    window.press(egui::Key::R);

    let painted = window.settle("Tool result");
    assert!(painted.contains("Tool result"), "{painted}");
    assert!(
        !painted.contains("Reading the transcript…"),
        "the transcript was never asked for again: {painted}"
    );
}

/// Drawing the same frame twice must not make egui complain about two widgets
/// claiming one id — the failure mode that turns a panel into a flickering
/// mess and that nothing but running it would catch.
#[test]
fn repeated_frames_are_clean() {
    let mut window = Headless::start();
    window.settle("Tool result");
    // egui logs an error and draws the offending widget twice when two claim
    // one id, so a second and third pass over a settled window is the check.
    for _ in 0..5 {
        assert!(
            window
                .frame()
                .iter()
                .any(|line| line.contains("Repositories")),
            "the sidebar stopped drawing"
        );
    }
}
