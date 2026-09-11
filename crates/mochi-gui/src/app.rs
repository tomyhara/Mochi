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

//! The window: layout 1b, "transcript first" (FR-9.1).
//!
//! Repositories and their sessions on the left, the transcript in the middle,
//! what the session is and what can be done with it on the right — the same
//! three columns doc/ui-spec.md describes, drawn by Mochi itself rather than
//! by a browser.
//!
//! Nothing in here reads a file or a database. The panels draw a [`Snapshot`]
//! and send [`Request`]s; `worker.rs` does the rest. That is what keeps a
//! scan of a hundred thousand sessions from freezing the window.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::text::LayoutJob;
use egui::{Align, FontId, Key, Layout, Modifiers, RichText, TextFormat};

use mochi_core::index::{SNIPPET_END, SNIPPET_START};
use mochi_core::model::{ParseStatus, Role, ToolId};

use crate::format::{bytes, clock, elide_middle, one_line, thousands, timestamp};
use crate::theme::Palette;
use crate::view::{groups, HitView, MessageView, SessionView, Snapshot};
use crate::worker::{Request, Response, Worker};

/// How many sessions a repository shows before it has to be opened (FR-9.1).
const COLLAPSED: usize = 5;

/// How much of one entry is drawn before the reader has to ask for the rest.
///
/// A tool result can be a megabyte of log. Laying all of it out costs more
/// than reading it does, and the cut is shown rather than made quietly.
const ENTRY_CHARS: usize = 4_000;

/// How many entries are drawn at once, extended by a button.
const PAGE: usize = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Transcript,
    Tools,
    Diffs,
    Raw,
    Search,
}

impl Tab {
    const ALL: [Tab; 5] = [
        Tab::Transcript,
        Tab::Tools,
        Tab::Diffs,
        Tab::Raw,
        Tab::Search,
    ];

    fn label(self) -> &'static str {
        match self {
            Tab::Transcript => "Transcript",
            Tab::Tools => "Tool calls",
            Tab::Diffs => "Diffs",
            Tab::Raw => "Raw JSONL",
            Tab::Search => "Search",
        }
    }
}

/// Follow the operating system unless the user says otherwise (FR-9.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThemeChoice {
    System,
    Light,
    Dark,
}

impl ThemeChoice {
    fn next(self) -> ThemeChoice {
        match self {
            ThemeChoice::System => ThemeChoice::Light,
            ThemeChoice::Light => ThemeChoice::Dark,
            ThemeChoice::Dark => ThemeChoice::System,
        }
    }

    fn label(self) -> &'static str {
        match self {
            ThemeChoice::System => "Theme: OS",
            ThemeChoice::Light => "Theme: Light",
            ThemeChoice::Dark => "Theme: Dark",
        }
    }

    fn preference(self) -> egui::ThemePreference {
        match self {
            ThemeChoice::System => egui::ThemePreference::System,
            ThemeChoice::Light => egui::ThemePreference::Light,
            ThemeChoice::Dark => egui::ThemePreference::Dark,
        }
    }
}

pub struct App {
    worker: Worker,
    snapshot: Snapshot,
    /// The transcript in hand, and whose it is. One session at a time, so that
    /// a big index does not become a big process (NFR-1.7).
    transcript: Option<(i64, Vec<MessageView>)>,
    awaiting_transcript: bool,
    selected: Option<i64>,
    filter: String,
    query: String,
    hits: Vec<HitView>,
    tab: Tab,
    opened: HashSet<String>,
    expanded: HashSet<i64>,
    shown: usize,
    theme: ThemeChoice,
    reveal: bool,
    busy: Option<&'static str>,
    status: Option<String>,
    error: Option<String>,
    copied: Option<Instant>,
    font: Option<PathBuf>,
    focus_search: bool,
    started: bool,
}

impl App {
    pub fn new(ctx: &egui::Context, options: crate::Options) -> App {
        crate::theme::install(ctx);
        let font = crate::fonts::install(ctx);

        let reveal = options.reveal_secrets;
        let repaint = ctx.clone();
        let worker = Worker::spawn(
            options.resolved_index_path(),
            options.home.clone(),
            move || repaint.request_repaint(),
        );
        worker.send(Request::Load {
            reveal_secrets: reveal,
        });

        App {
            worker,
            snapshot: Snapshot::default(),
            transcript: None,
            awaiting_transcript: false,
            selected: None,
            filter: String::new(),
            query: String::new(),
            hits: Vec::new(),
            tab: Tab::Transcript,
            opened: HashSet::new(),
            expanded: HashSet::new(),
            shown: PAGE,
            theme: ThemeChoice::System,
            reveal,
            busy: Some("Reading the index…"),
            status: None,
            error: None,
            copied: None,
            font,
            focus_search: false,
            started: false,
        }
    }

    fn session(&self) -> Option<&SessionView> {
        let id = self.selected?;
        self.snapshot.sessions.iter().find(|s| s.id == id)
    }

    fn select(&mut self, id: i64) {
        if self.selected == Some(id) {
            return;
        }
        self.selected = Some(id);
        self.transcript = None;
        self.expanded.clear();
        self.shown = PAGE;
        self.awaiting_transcript = true;
        self.worker.send(Request::Messages {
            session_id: id,
            reveal_secrets: self.reveal,
        });
    }

    /// Read everything again, masked or not.
    ///
    /// Unmasking is a re-read rather than a display toggle: the window should
    /// never be holding a secret it is only visually hiding (NFR-3.3).
    fn set_reveal(&mut self, reveal: bool) {
        self.reveal = reveal;
        self.transcript = None;
        self.hits.clear();
        self.busy = Some("Reading the index…");
        self.worker.send(Request::Load {
            reveal_secrets: reveal,
        });
        if let Some(id) = self.selected {
            self.awaiting_transcript = true;
            self.worker.send(Request::Messages {
                session_id: id,
                reveal_secrets: reveal,
            });
        }
        if !self.query.trim().is_empty() {
            self.worker.send(Request::Search {
                text: self.query.clone(),
                reveal_secrets: reveal,
            });
        }
    }

    fn drain(&mut self) {
        while let Some(response) = self.worker.try_recv() {
            match response {
                Response::Busy(text) => self.busy = Some(text),
                Response::Scanned(summary) => self.status = Some(summary),
                Response::Loaded(snapshot) => {
                    self.busy = None;
                    self.error = None;
                    self.snapshot = *snapshot;
                    // Open what the user was doing last (FR-4.4), but never
                    // move the selection out from under them on a rescan.
                    let still_there = self
                        .selected
                        .is_some_and(|id| self.snapshot.sessions.iter().any(|s| s.id == id));
                    if !still_there {
                        self.selected = None;
                        if let Some(first) = self.snapshot.sessions.first().map(|s| s.id) {
                            self.select(first);
                        }
                    } else if let Some(id) = self.selected {
                        // A rescan can have changed what is in the session the
                        // user is reading, so whatever is on screen is stale.
                        if self.transcript.is_none() && !self.awaiting_transcript {
                            self.awaiting_transcript = true;
                            self.worker.send(Request::Messages {
                                session_id: id,
                                reveal_secrets: self.reveal,
                            });
                        }
                    }
                }
                Response::Messages {
                    session_id,
                    messages,
                } => {
                    // A late answer for a session the user has already left.
                    if self.selected == Some(session_id) {
                        self.transcript = Some((session_id, messages));
                        self.awaiting_transcript = false;
                    }
                }
                Response::Hits { text, hits } => {
                    if text == self.query {
                        self.hits = hits;
                    }
                }
                Response::Failed(message) => {
                    self.busy = None;
                    self.awaiting_transcript = false;
                    self.error = Some(message);
                }
            }
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::K)) {
            self.focus_search = true;
        }
        if ctx.input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::R)) {
            self.rescan();
        }
    }

    fn rescan(&mut self) {
        self.busy = Some("Scanning your session stores…");
        self.status = None;
        // What is on screen was read before the scan; it is answered again
        // when the fresh snapshot arrives.
        self.transcript = None;
        self.worker.send(Request::Rescan {
            reveal_secrets: self.reveal,
        });
    }

    fn run_search(&mut self) {
        self.tab = Tab::Search;
        self.hits.clear();
        if self.query.trim().is_empty() {
            return;
        }
        self.worker.send(Request::Search {
            text: self.query.clone(),
            reveal_secrets: self.reveal,
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if !self.started {
            self.started = true;
            ctx.set_theme(self.theme.preference());
        }
        self.drain();
        self.shortcuts(&ctx);

        let palette = Palette::of(ctx.theme());

        self.topbar(ui, palette);
        self.statusbar(ui, palette);
        self.sidebar(ui, palette);
        self.rail(ui, palette);
        self.centre(ui, palette);

        // The copy confirmation fades by itself, so the window has to come
        // back even if nobody moves the mouse.
        if let Some(at) = self.copied {
            if at.elapsed() < Duration::from_secs(2) {
                ctx.request_repaint_after(Duration::from_millis(250));
            } else {
                self.copied = None;
            }
        }
    }
}

impl App {
    fn topbar(&mut self, ui: &mut egui::Ui, palette: Palette) {
        egui::Panel::top("topbar")
            .exact_size(52.0)
            .frame(
                egui::Frame::default()
                    .fill(palette.surface)
                    .inner_margin(egui::Margin::symmetric(12, 8)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    let session = self.session().cloned();
                    let repository = session.as_ref().and_then(|s| {
                        s.repo_id
                            .and_then(|id| self.snapshot.repositories.iter().find(|r| r.id == id))
                    });

                    ui.label(
                        RichText::new(
                            repository
                                .map(|r| r.display_name.clone())
                                .unwrap_or_else(|| "No repository".to_string()),
                        )
                        .strong(),
                    );
                    ui.label(RichText::new("/").color(palette.text_muted));
                    ui.label(
                        RichText::new(
                            session
                                .as_ref()
                                .map(|s| s.tool.display_name())
                                .unwrap_or("—"),
                        )
                        .color(palette.text_muted),
                    );
                    ui.label(RichText::new("/").color(palette.text_muted));
                    ui.label(
                        RichText::new(
                            session
                                .as_ref()
                                .map(|s| one_line(s.label(), 48))
                                .unwrap_or_else(|| "—".to_string()),
                        )
                        .color(palette.text_muted),
                    );

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button(self.theme.label()).clicked() {
                            self.theme = self.theme.next();
                            ui.ctx().set_theme(self.theme.preference());
                        }

                        let enabled = self.busy.is_none();
                        if ui
                            .add_enabled(enabled, egui::Button::new("Rescan"))
                            .on_hover_text("Read your session stores again (Ctrl+R)")
                            .clicked()
                        {
                            self.rescan();
                        }

                        // Nothing can launch a session yet: an interactive CLI
                        // needs somewhere to be interactive, and that is the
                        // integrated terminal (FR-7.8, milestone 3). Copying
                        // the command is the working path.
                        ui.add_enabled(false, egui::Button::new("Resume"))
                            .on_disabled_hover_text(
                                "Starting a session needs the integrated terminal, which is not \
                                 built yet. Use Copy command.",
                            );

                        let command = session.as_ref().and_then(|s| s.resume.as_ref().ok());
                        let copied = self.copied.is_some();
                        let label = if copied { "Copied" } else { "Copy command" };
                        if ui
                            .add_enabled(command.is_some(), egui::Button::new(label))
                            .clicked()
                        {
                            if let Some(command) = command {
                                ui.ctx().copy_text(command.clone());
                                self.copied = Some(Instant::now());
                            }
                        }

                        ui.add_space(8.0);
                        let hint = if cfg!(target_os = "macos") {
                            "⌘K"
                        } else {
                            "Ctrl+K"
                        };
                        ui.label(RichText::new(hint).color(palette.text_muted).size(11.0));
                        let search = ui.add(
                            egui::TextEdit::singleline(&mut self.query)
                                .hint_text("Search transcripts")
                                .desired_width(260.0),
                        );
                        if self.focus_search {
                            self.focus_search = false;
                            search.request_focus();
                        }
                        if search.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                            self.run_search();
                        }
                    });
                });
            });
    }

    fn statusbar(&mut self, ui: &mut egui::Ui, palette: Palette) {
        egui::Panel::bottom("status")
            .exact_size(26.0)
            .frame(
                egui::Frame::default()
                    .fill(palette.surface)
                    .inner_margin(egui::Margin::symmetric(12, 4)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    if let Some(error) = &self.error {
                        ui.label(
                            RichText::new(format!("⚠ {error}"))
                                .color(palette.danger)
                                .size(12.0),
                        );
                    } else if let Some(busy) = self.busy {
                        ui.add(egui::Spinner::new().size(12.0));
                        ui.label(RichText::new(busy).color(palette.text_muted).size(12.0));
                    } else {
                        let stats = &self.snapshot.stats;
                        ui.label(
                            RichText::new(format!(
                                "{} repositories · {} sessions · {} messages · {} on disk",
                                thousands(stats.repositories),
                                thousands(stats.sessions),
                                thousands(stats.messages),
                                bytes(stats.total_source_bytes),
                            ))
                            .color(palette.text_muted)
                            .size(12.0),
                        );
                        if let Some(status) = &self.status {
                            ui.label(RichText::new("·").color(palette.text_muted).size(12.0));
                            ui.label(RichText::new(status).color(palette.text_muted).size(12.0));
                        }
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let index = self
                            .snapshot
                            .index_path
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "no index location".to_string());
                        let font = match &self.font {
                            Some(path) => format!("font: {}", path.display()),
                            None => "font: egui's own (no CJK face found)".to_string(),
                        };
                        ui.label(
                            RichText::new(elide_middle(&index, 52))
                                .color(palette.text_muted)
                                .size(12.0),
                        )
                        .on_hover_text(format!("{index}\n{font}"));
                    });
                });
            });
    }

    fn sidebar(&mut self, ui: &mut egui::Ui, palette: Palette) {
        egui::Panel::left("sidebar")
            .resizable(true)
            .default_size(300.0)
            .size_range(220.0..=460.0)
            .frame(
                egui::Frame::default()
                    .fill(palette.bg)
                    .inner_margin(egui::Margin::same(10)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Repositories").strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if !self.filter.is_empty() && ui.small_button("clear").clicked() {
                            self.filter.clear();
                        }
                    });
                });
                ui.add_space(6.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter)
                        .hint_text("Filter sessions")
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(8.0);

                let groups = groups(&self.snapshot, &self.filter);
                if groups.is_empty() {
                    ui.label(
                        RichText::new(if self.snapshot.sessions.is_empty() {
                            "Nothing indexed yet. Rescan to look again."
                        } else {
                            "Nothing matches that filter."
                        })
                        .color(palette.text_muted),
                    );
                    return;
                }

                egui::ScrollArea::vertical()
                    .id_salt("sidebar-scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for group in groups {
                            let open = self.opened.contains(&group.key);
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(one_line(&group.name, 26)).strong());
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(
                                        RichText::new(group.sessions.len().to_string())
                                            .color(palette.text_muted)
                                            .size(11.0),
                                    );
                                });
                            });
                            if let Some(path) = &group.path {
                                ui.label(
                                    RichText::new(elide_middle(path, 38))
                                        .color(palette.text_muted)
                                        .size(11.0),
                                )
                                .on_hover_text(path);
                            }

                            let limit = if open {
                                group.sessions.len()
                            } else {
                                COLLAPSED.min(group.sessions.len())
                            };
                            for index in &group.sessions[..limit] {
                                let session = &self.snapshot.sessions[*index];
                                let selected = self.selected == Some(session.id);
                                let job = row(session, palette, ui);
                                if ui.selectable_label(selected, job).clicked() {
                                    let id = session.id;
                                    self.select(id);
                                    self.tab = Tab::Transcript;
                                }
                            }

                            let hidden = group.sessions.len() - limit;
                            if hidden > 0 {
                                if ui.small_button(format!("Show {hidden} more")).clicked() {
                                    self.opened.insert(group.key.clone());
                                }
                            } else if open
                                && group.sessions.len() > COLLAPSED
                                && ui.small_button("Show fewer").clicked()
                            {
                                self.opened.remove(&group.key);
                            }
                            ui.add_space(10.0);
                        }
                    });
            });
    }

    fn rail(&mut self, ui: &mut egui::Ui, palette: Palette) {
        egui::Panel::right("rail")
            .resizable(true)
            .default_size(330.0)
            .size_range(260.0..=520.0)
            .frame(
                egui::Frame::default()
                    .fill(palette.bg)
                    .inner_margin(egui::Margin::same(12)),
            )
            .show(ui, |ui| {
                let Some(session) = self.session().cloned() else {
                    ui.label(RichText::new("Session").strong());
                    ui.label(RichText::new("Nothing selected.").color(palette.text_muted));
                    return;
                };
                let repository = session
                    .repo_id
                    .and_then(|id| self.snapshot.repositories.iter().find(|r| r.id == id))
                    .cloned();

                egui::ScrollArea::vertical()
                    .id_salt("rail-scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for notice in notices(&session) {
                            notice_box(ui, palette, &notice);
                        }

                        heading(ui, "Session");
                        field(ui, palette, "Tool", session.tool.display_name());
                        field(ui, palette, "Session id", &session.native_id);
                        if let Some(model) = &session.model {
                            field(ui, palette, "Model", model);
                        }
                        if let Some(branch) = &session.git_branch {
                            field(ui, palette, "Branch", branch);
                        }
                        field(ui, palette, "Messages", &thousands(session.message_count));
                        field(
                            ui,
                            palette,
                            "Tokens",
                            &format!(
                                "{} in · {} out",
                                thousands(session.tokens_in),
                                thousands(session.tokens_out)
                            ),
                        );
                        field(ui, palette, "Started", &timestamp(session.started_at));
                        field(ui, palette, "Updated", &timestamp(session.updated_at));

                        heading(ui, "Location");
                        field(
                            ui,
                            palette,
                            "Repository",
                            repository
                                .as_ref()
                                .map(|r| r.display_name.as_str())
                                .unwrap_or("None"),
                        );
                        field(
                            ui,
                            palette,
                            "Working directory",
                            session.cwd.as_deref().unwrap_or("Not recorded"),
                        );
                        field(ui, palette, "Source file", &session.source_path);
                        field(ui, palette, "Size", &bytes(session.source_size));

                        heading(ui, "Secrets");
                        ui.label(
                            RichText::new(if self.snapshot.masked {
                                "Credentials are masked in what you see."
                            } else {
                                "Credentials are shown in full."
                            })
                            .color(palette.text_muted)
                            .size(12.0),
                        );
                        ui.add_space(6.0);
                        let busy = self.busy.is_some();
                        let label = if busy {
                            "Reading…"
                        } else if self.snapshot.masked {
                            "Reveal secrets"
                        } else {
                            "Mask secrets"
                        };
                        if ui
                            .add_enabled(!busy, egui::Button::new(label))
                            .on_hover_text(
                                "Unmasking re-reads the index, so the window never holds a secret \
                                 it is only pretending to hide.",
                            )
                            .clicked()
                        {
                            let reveal = self.snapshot.masked;
                            self.set_reveal(reveal);
                        }

                        heading(ui, "Resume");
                        match &session.resume {
                            Ok(command) => {
                                // Shown rather than offered as a second
                                // button: copying already lives in the header
                                // (FR-9.1b), and seeing the command is what
                                // tells you it will start in the right
                                // directory (FR-7.1).
                                ui.add(
                                    egui::Label::new(RichText::new(command).monospace().size(12.0))
                                        .wrap(),
                                );
                            }
                            Err(reason) => {
                                ui.label(
                                    RichText::new(format!("Cannot resume: {reason}."))
                                        .color(palette.text_muted)
                                        .size(12.0),
                                );
                            }
                        }
                    });
            });
    }

    fn centre(&mut self, ui: &mut egui::Ui, palette: Palette) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(palette.bg)
                    .inner_margin(egui::Margin::same(0)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    for tab in Tab::ALL {
                        let label = if tab == Tab::Search && !self.hits.is_empty() {
                            format!("{} ({})", tab.label(), self.hits.len())
                        } else {
                            tab.label().to_string()
                        };
                        if ui.selectable_label(self.tab == tab, label).clicked() {
                            self.tab = tab;
                        }
                    }
                });
                ui.separator();

                egui::ScrollArea::vertical()
                    .id_salt("centre-scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add_space(10.0);
                        match self.tab {
                            Tab::Search => self.search_pane(ui, palette),
                            Tab::Diffs => empty(
                                ui,
                                palette,
                                "Diffs are not extracted yet",
                                &[
                                    "File changes are shown as the tool output that produced \
                                     them, under Tool calls.",
                                    "Reading edits back out as a diff is FR-5.6, and is not built.",
                                ],
                            ),
                            tab => self.transcript_pane(ui, palette, tab),
                        }
                        ui.add_space(20.0);
                    });
            });
    }

    fn transcript_pane(&mut self, ui: &mut egui::Ui, palette: Palette, tab: Tab) {
        if self.session().is_none() {
            empty(
                ui,
                palette,
                "No session selected",
                &["Pick a session from the sidebar to read it."],
            );
            return;
        }

        let Some((_, messages)) = &self.transcript else {
            ui.horizontal(|ui| {
                ui.add_space(24.0);
                ui.add(egui::Spinner::new().size(14.0));
                ui.label(RichText::new("Reading the transcript…").color(palette.text_muted));
            });
            return;
        };

        let selection: Vec<&MessageView> = match tab {
            Tab::Tools => messages
                .iter()
                .filter(|m| matches!(m.role, Role::ToolCall | Role::ToolResult))
                .collect(),
            Tab::Raw => messages.iter().filter(|m| m.raw.is_some()).collect(),
            _ => messages.iter().collect(),
        };

        if selection.is_empty() {
            let session = self.session().expect("checked above");
            match tab {
                Tab::Tools => empty(
                    ui,
                    palette,
                    "No tool calls",
                    &["This session was a conversation; the agent ran nothing."],
                ),
                Tab::Raw => empty(
                    ui,
                    palette,
                    "Nothing kept in raw form",
                    &[
                        "Raw JSON is kept for entries Mochi did not fully understand, so that a \
                         format change never hides content. Every entry here was understood.",
                    ],
                ),
                _ => empty(
                    ui,
                    palette,
                    "This session has no messages",
                    &[if session.parse_status == ParseStatus::Failed {
                        "Its file could not be read. The reason is in the panel on the right."
                    } else {
                        "It was started but nothing was recorded before it ended."
                    }],
                ),
            }
            return;
        }

        let total = selection.len();
        let shown = self.shown.min(total);
        let mut expand: Option<i64> = None;
        let mut collapse: Option<i64> = None;

        for message in &selection[..shown] {
            let expanded = self.expanded.contains(&message.seq);
            match entry(ui, palette, message, tab == Tab::Raw, expanded) {
                Some(true) => expand = Some(message.seq),
                Some(false) => collapse = Some(message.seq),
                None => {}
            }
        }
        if let Some(seq) = expand {
            self.expanded.insert(seq);
        }
        if let Some(seq) = collapse {
            self.expanded.remove(&seq);
        }

        if shown < total {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(24.0);
                if ui
                    .button(format!("Show more ({} of {total} shown)", shown))
                    .clicked()
                {
                    self.shown += PAGE;
                }
            });
        }
    }

    fn search_pane(&mut self, ui: &mut egui::Ui, palette: Palette) {
        if self.query.trim().is_empty() {
            empty(
                ui,
                palette,
                "Search every session at once",
                &[
                    "Type in the box at the top and press Enter. The index searches inside every \
                     message of every tool, including text without spaces between the words.",
                ],
            );
            return;
        }
        if self.hits.is_empty() {
            empty(
                ui,
                palette,
                "No matches",
                &["Nothing in the index contains that."],
            );
            return;
        }

        let mut open: Option<i64> = None;
        for hit in &self.hits {
            ui.horizontal_top(|ui| {
                ui.add_space(20.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(short(hit.tool))
                                .monospace()
                                .size(10.0)
                                .color(palette.text_muted),
                        );
                        ui.label(
                            RichText::new(
                                hit.repo_display
                                    .clone()
                                    .unwrap_or_else(|| "No repository".to_string()),
                            )
                            .strong()
                            .size(12.0),
                        );
                        ui.label(
                            RichText::new(one_line(
                                hit.session_title.as_deref().unwrap_or("(no title)"),
                                40,
                            ))
                            .color(palette.text_muted)
                            .size(12.0),
                        );
                        ui.label(
                            RichText::new(timestamp(hit.timestamp))
                                .color(palette.text_muted)
                                .monospace()
                                .size(11.0),
                        );
                    });
                    ui.add(egui::Label::new(snippet(&hit.snippet, palette)).wrap());
                    if ui.small_button("Open this session").clicked() {
                        open = Some(hit.session_id);
                    }
                });
            });
            ui.add_space(12.0);
        }
        if let Some(id) = open {
            self.select(id);
            self.tab = Tab::Transcript;
        }
    }
}

/// One transcript entry. Returns `Some(true)` to expand it, `Some(false)` to
/// fold it back up.
fn entry(
    ui: &mut egui::Ui,
    palette: Palette,
    message: &MessageView,
    raw: bool,
    expanded: bool,
) -> Option<bool> {
    let body = if raw {
        message.raw.as_deref().unwrap_or_default()
    } else {
        message.content.as_str()
    };
    let mut toggled = None;

    ui.horizontal_top(|ui| {
        ui.add_space(12.0);
        // The meta column, right-aligned against the body, as in the mock.
        ui.allocate_ui_with_layout(egui::vec2(92.0, 0.0), Layout::top_down(Align::Max), |ui| {
            ui.label(RichText::new(role_label(message.role)).strong().size(12.0));
            ui.label(
                RichText::new(clock(message.timestamp))
                    .monospace()
                    .size(11.0)
                    .color(palette.text_muted),
            );
            if let Some(tool) = &message.tool_name {
                ui.label(
                    RichText::new(one_line(tool, 14))
                        .monospace()
                        .size(11.0)
                        .color(palette.text_muted),
                );
            }
        });
        ui.add_space(12.0);

        ui.vertical(|ui| {
            let count = body.chars().count();
            let cut = !expanded && count > ENTRY_CHARS;
            let text: String = if cut {
                body.chars().take(ENTRY_CHARS).collect()
            } else {
                body.to_string()
            };

            let monospace = raw || matches!(message.role, Role::ToolCall | Role::ToolResult);
            let rich = if monospace {
                RichText::new(text).monospace().size(12.5)
            } else {
                RichText::new(text).size(14.0)
            }
            .color(palette.role_text(message.role));

            if body.trim().is_empty() {
                ui.label(
                    RichText::new("(no text)")
                        .italics()
                        .color(palette.text_muted),
                );
            } else if monospace {
                egui::Frame::default()
                    .fill(palette.surface)
                    .stroke(egui::Stroke::new(1.0, palette.border))
                    .corner_radius(8)
                    .inner_margin(egui::Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        ui.add(egui::Label::new(rich).wrap());
                    });
            } else {
                ui.add(egui::Label::new(rich).wrap());
            }

            if cut {
                if ui
                    .small_button(format!("Show all {} characters", thousands(count as i64)))
                    .clicked()
                {
                    toggled = Some(true);
                }
            } else if expanded && count > ENTRY_CHARS && ui.small_button("Show less").clicked() {
                toggled = Some(false);
            }
        });
    });
    ui.add_space(16.0);
    toggled
}

/// The sidebar row: a tool tag, the title, and any warnings.
fn row(session: &SessionView, palette: Palette, ui: &egui::Ui) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.append(
        short(session.tool),
        0.0,
        TextFormat {
            font_id: FontId::monospace(10.0),
            color: palette.text_muted,
            valign: Align::Center,
            ..Default::default()
        },
    );
    job.append(
        &one_line(session.label(), 34),
        8.0,
        TextFormat {
            font_id: FontId::proportional(13.0),
            color: palette.text,
            valign: Align::Center,
            ..Default::default()
        },
    );
    for note in session.notes() {
        job.append(
            note,
            8.0,
            TextFormat {
                font_id: FontId::proportional(10.0),
                color: palette.danger,
                valign: Align::Center,
                ..Default::default()
            },
        );
    }
    job.wrap.max_width = ui.available_width();
    job.wrap.max_rows = 1;
    job
}

/// Turn the index's match markers into something egui can colour.
///
/// Public so that it can be checked without a window: the markers come from
/// SQLite's snippet function, and a malformed one must not lose the text
/// around it.
pub fn snippet(text: &str, palette: Palette) -> LayoutJob {
    let mut job = LayoutJob::default();
    let plain = TextFormat {
        font_id: FontId::monospace(12.0),
        color: palette.text_muted,
        ..Default::default()
    };
    let hit = TextFormat {
        font_id: FontId::monospace(12.0),
        color: palette.text,
        background: palette.accent.linear_multiply(0.35),
        ..Default::default()
    };

    let one = text.replace(['\n', '\r'], " ");
    let mut rest = one.as_str();
    while let Some(start) = rest.find(SNIPPET_START) {
        job.append(&rest[..start], 0.0, plain.clone());
        rest = &rest[start + SNIPPET_START.len()..];
        match rest.find(SNIPPET_END) {
            Some(end) => {
                job.append(&rest[..end], 0.0, hit.clone());
                rest = &rest[end + SNIPPET_END.len()..];
            }
            // A marker the index opened and never closed: show the remainder
            // rather than dropping it.
            None => break,
        }
    }
    job.append(rest, 0.0, plain);
    job
}

fn short(tool: ToolId) -> &'static str {
    match tool {
        ToolId::ClaudeCode => "CC",
        ToolId::Codex => "CDX",
        ToolId::OpenCode => "OC",
    }
}

fn role_label(role: Role) -> &'static str {
    match role {
        Role::User => "You",
        Role::Assistant => "Assistant",
        Role::Thinking => "Thinking",
        Role::ToolCall => "Tool call",
        Role::ToolResult => "Tool result",
        Role::System => "System",
    }
}

/// The warnings that belong at the top of the rail, in full sentences.
fn notices(session: &SessionView) -> Vec<String> {
    let mut out = Vec::new();
    match session.parse_status {
        ParseStatus::Archived => out.push(
            "The original transcript has been deleted by the tool that wrote it. What you are \
             reading is Mochi's copy, which is now the only one."
                .to_string(),
        ),
        ParseStatus::Partial => {
            if let Some(error) = &session.parse_error {
                out.push(format!("Read with problems: {error}"));
            }
        }
        ParseStatus::Failed => out.push(format!(
            "This session could not be read: {}",
            session.parse_error.as_deref().unwrap_or("unknown reason")
        )),
        ParseStatus::Ok => {}
    }
    if !session.cwd_exists && session.cwd.is_some() {
        out.push(
            "Working directory missing. You can still read this session, but it cannot be \
             resumed."
                .to_string(),
        );
    }
    out
}

fn notice_box(ui: &mut egui::Ui, palette: Palette, text: &str) {
    egui::Frame::default()
        .fill(palette.surface_2)
        .stroke(egui::Stroke::new(1.0, palette.border))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.add(egui::Label::new(RichText::new(text).size(12.0).color(palette.text)).wrap());
        });
    ui.add_space(8.0);
}

fn heading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(12.0);
    ui.label(RichText::new(text).strong());
    ui.add_space(4.0);
}

fn field(ui: &mut egui::Ui, palette: Palette, label: &str, value: &str) {
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(egui::vec2(108.0, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.label(RichText::new(label).color(palette.text_muted).size(12.0));
        });
        ui.add(egui::Label::new(RichText::new(value).size(12.5).color(palette.text)).wrap());
    });
    ui.add_space(4.0);
}

/// An empty state that says what to do next, not just that there is nothing
/// here (FR-9.6).
fn empty(ui: &mut egui::Ui, palette: Palette, title: &str, lines: &[&str]) {
    ui.add_space(40.0);
    ui.vertical_centered(|ui| {
        ui.allocate_ui_with_layout(egui::vec2(520.0, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.label(RichText::new(title).strong().size(15.0));
            ui.add_space(6.0);
            for line in lines {
                ui.add(
                    egui::Label::new(RichText::new(*line).color(palette.text_muted).size(13.0))
                        .wrap(),
                );
                ui.add_space(4.0);
            }
        });
    });
}
