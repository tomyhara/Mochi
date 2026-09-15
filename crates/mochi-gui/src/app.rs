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

//! The window: "transcript first" (mock `1b`) with the left-hand selection of
//! mock `1a` (FR-9.1).
//!
//! Four columns. Repositories, then the sessions of the one picked, then the
//! transcript, then what the session is and what can be done with it. The two
//! narrow panes on the left were one nested list until it turned out that a
//! tree of repositories with sessions folded inside them reads as a single
//! undifferentiated list: nothing on screen says which rows are the things you
//! choose between and which are the things you open. Mock `1a` had already
//! answered that — one pane per question — so the left-hand side is now its,
//! while the wide transcript and the metadata rail stay `1b`'s.
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

use crate::format::{
    bytes, clock, date, elide_middle, hour_minute, now_ms, one_line, relative, thousands, timestamp,
};
use crate::theme::Palette;
use crate::view::{
    repositories, scope_exists, scope_title, sections, HitView, MessageView, RepoRow, Scope,
    Section, SessionView, Snapshot,
};
use crate::worker::{About, Request, Response, Worker};

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

/// Which reading of the index a transcript belongs to.
///
/// A rescan can change what is in the session being read, so a transcript read
/// before it is stale whether or not it arrived before it. Counting the
/// readings is what tells the two apart: an answer to a question asked of the
/// old index is recognisable as such, rather than merely as an answer that
/// happens to be present.
type Generation = u64;

/// A transcript being read, or read.
///
/// One session at a time, so that a big index does not become a big process
/// (NFR-1.7).
struct Transcript {
    session_id: i64,
    generation: Generation,
    /// The entries, or why there are none. A transcript that could not be read
    /// is an answer about this session, and the pane that was going to show it
    /// is where the reason belongs (FR-9.6).
    body: Result<Vec<MessageView>, String>,
}

/// A transcript that has been asked for and not yet answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Awaiting {
    session_id: i64,
    generation: Generation,
}

/// What the two left-hand panes are showing, and what it was built from.
///
/// Both walk every session, and both are drawn on every frame — including
/// every frame of typing in a filter box and of scrolling. On an index of the
/// size NFR-1 is written for that is work worth not repeating while its answer
/// cannot have changed.
struct Listing {
    /// Which snapshot this was built from, by number.
    snapshot: u64,
    repo_filter: String,
    filter: String,
    scope: Scope,
    /// When it was built. The dated headings are relative to it, so a window
    /// left open across midnight has to be told.
    now: i64,
    repos: Vec<RepoRow>,
    sections: Vec<Section>,
    /// How many sessions the sections hold between them.
    listed: usize,
}

pub struct App {
    worker: Worker,
    snapshot: Snapshot,
    /// How many snapshots the window has taken. Numbering them is how work
    /// done on one — the sidebar's grouping — is recognised as still standing,
    /// without comparing ten thousand sessions to find out.
    snapshots: u64,
    listing: Option<Listing>,
    transcript: Option<Transcript>,
    awaiting: Option<Awaiting>,
    /// Bumped whenever what has been read stops being what is true: a rescan,
    /// or a change of masking.
    generation: Generation,
    selected: Option<i64>,
    /// Which repository the session pane is showing.
    scope: Scope,
    /// Narrows the repository pane, by name and path.
    repo_filter: String,
    /// Narrows the session pane, by title, id and tool.
    filter: String,
    query: String,
    hits: Vec<HitView>,
    tab: Tab,
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
            snapshots: 0,
            listing: None,
            transcript: None,
            awaiting: None,
            generation: 0,
            selected: None,
            scope: Scope::All,
            repo_filter: String::new(),
            filter: String::new(),
            query: String::new(),
            hits: Vec::new(),
            tab: Tab::Transcript,
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

    /// What the left-hand panes show, built again only if something it was
    /// built from has changed since it last was.
    ///
    /// Taken out of the window rather than borrowed from it, because drawing a
    /// row can change the selection or the scope, and both change the window
    /// while the rows are still being drawn. The caller puts it back.
    fn listing(&mut self) -> Listing {
        let now = now_ms();
        match self.listing.take() {
            Some(listing)
                if listing.snapshot == self.snapshots
                    && listing.repo_filter == self.repo_filter
                    && listing.filter == self.filter
                    && listing.scope == self.scope
                    // "Today" stops meaning today at midnight, and a window
                    // left open overnight would go on saying it.
                    && (now - listing.now).abs() < 60_000 =>
            {
                listing
            }
            _ => {
                let sections = sections(&self.snapshot, self.scope, &self.filter, now);
                Listing {
                    snapshot: self.snapshots,
                    repo_filter: self.repo_filter.clone(),
                    filter: self.filter.clone(),
                    scope: self.scope,
                    now,
                    repos: repositories(&self.snapshot, &self.repo_filter),
                    listed: sections.iter().map(|section| section.sessions.len()).sum(),
                    sections,
                }
            }
        }
    }

    fn select(&mut self, id: i64) {
        if self.selected == Some(id) {
            return;
        }
        self.selected = Some(id);
        self.transcript = None;
        self.expanded.clear();
        self.shown = PAGE;
        self.ensure_transcript();
    }

    /// Ask for the selected session's transcript unless the one in hand, or
    /// the one already asked for, is both its own and current.
    ///
    /// Everything that invalidates a transcript — selecting another session,
    /// rescanning, unmasking — goes through here, so there is one answer to
    /// "is what is on screen still true", rather than one per caller.
    fn ensure_transcript(&mut self) {
        let Some(id) = self.selected else {
            return;
        };
        let current = |session_id: i64, generation: Generation| {
            session_id == id && generation == self.generation
        };
        let in_hand = self
            .transcript
            .as_ref()
            .is_some_and(|t| current(t.session_id, t.generation));
        let asked = self
            .awaiting
            .is_some_and(|a| current(a.session_id, a.generation));
        if in_hand || asked {
            return;
        }

        self.awaiting = Some(Awaiting {
            session_id: id,
            generation: self.generation,
        });
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
        // What is on screen was read with the other masking, so it is not an
        // answer to the question now being asked.
        self.generation += 1;
        self.worker.send(Request::Load {
            reveal_secrets: reveal,
        });
        self.ensure_transcript();
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
                    self.snapshots += 1;
                    // A rescan can merge two repositories into one, or drop
                    // the one being shown. A pane pointed at a repository that
                    // is no longer there would just look empty.
                    if !scope_exists(&self.snapshot, self.scope) {
                        self.scope = Scope::All;
                    }
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
                    } else {
                        // A rescan can have changed what is in the session the
                        // user is reading, so whatever was read before it is
                        // stale — including an answer that arrived while the
                        // scan was running.
                        self.ensure_transcript();
                    }
                }
                Response::Messages {
                    session_id,
                    messages,
                } => self.answered(session_id, Ok(messages)),
                Response::Hits { text, hits } => {
                    if text == self.query {
                        self.hits = hits;
                    }
                }
                Response::Failed { about, message } => match about {
                    About::Transcript(session_id) => self.answered(session_id, Err(message)),
                    // The index itself, or a search: neither belongs to one
                    // session, so the status bar is where it is said.
                    About::Index | About::Search => {
                        self.busy = None;
                        self.error = Some(message);
                    }
                },
            }
        }
    }

    /// Take a transcript, or the reason there is not one.
    ///
    /// Dropped unless it answers the question actually being asked: a session
    /// the user has already left, or a reading of the index that a rescan has
    /// since replaced, is not an answer at all.
    fn answered(&mut self, session_id: i64, body: Result<Vec<MessageView>, String>) {
        let Some(asked) = self.awaiting.filter(|a| a.session_id == session_id) else {
            return;
        };
        self.awaiting = None;
        if asked.generation != self.generation {
            // Asked before a rescan or a change of masking. Ask again, now.
            self.ensure_transcript();
            return;
        }
        self.transcript = Some(Transcript {
            session_id,
            generation: asked.generation,
            body,
        });
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::K)) {
            self.focus_search = true;
        }
        // Guarded like the Rescan button: every press during a scan would
        // otherwise queue another whole scan behind the one running.
        if ctx.input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::R))
            && self.busy.is_none()
        {
            self.rescan();
        }
    }

    fn rescan(&mut self) {
        self.busy = Some("Scanning your session stores…");
        self.status = None;
        // What is on screen was read before the scan, and so is any answer
        // still in flight: the session's file can grow while the scan runs.
        // Both are asked for again when the fresh snapshot arrives.
        self.transcript = None;
        self.generation += 1;
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
        // Held while the rows are drawn — clicking one calls back into the
        // window — and put back afterwards, so that the next frame is free
        // unless something it was built from has changed.
        let listing = self.listing();
        self.repositories_pane(ui, palette, &listing);
        self.sessions_pane(ui, palette, &listing);
        self.listing = Some(listing);
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

    /// The first pane: which repository (FR-9.1, mock `1a`).
    ///
    /// One row per repository, plus the three views that cut across them: the
    /// whole index, the sessions that belong to no repository, and the ones
    /// whose original file the tool deleted. Picking a row is the only thing
    /// this pane does — it never opens a session — which is the distinction
    /// the nested sidebar could not make.
    fn repositories_pane(&mut self, ui: &mut egui::Ui, palette: Palette, listing: &Listing) {
        egui::Panel::left("repositories")
            .resizable(true)
            .default_size(236.0)
            .size_range(180.0..=360.0)
            .frame(
                egui::Frame::default()
                    .fill(palette.bg)
                    .inner_margin(egui::Margin::same(10)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Repositories").strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        // What is listed, not what the index holds: next to a
                        // filtered list, the total would be a lie about the
                        // rows underneath it.
                        let listed = listing
                            .repos
                            .iter()
                            .filter(|repo| matches!(repo.scope, Scope::Repo(_)))
                            .count();
                        ui.label(
                            RichText::new(thousands(listed as i64))
                                .color(palette.text_muted)
                                .size(11.0),
                        );
                    });
                });
                ui.add_space(6.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.repo_filter)
                        .hint_text("Filter repositories")
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(8.0);

                if listing.repos.is_empty() {
                    ui.label(
                        RichText::new(if self.snapshot.sessions.is_empty() {
                            "Nothing indexed yet. Rescan to look again."
                        } else {
                            "No repository matches that filter."
                        })
                        .color(palette.text_muted)
                        .size(12.0),
                    );
                    return;
                }

                egui::ScrollArea::vertical()
                    .id_salt("repositories-scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for repo in &listing.repos {
                            if self.repository_row(ui, palette, repo, listing.now) {
                                self.scope = repo.scope;
                            }
                        }
                    });
            });
    }

    /// One row of the repository pane: what it is called, how many sessions it
    /// holds, where it is, and how those sessions split between the tools.
    fn repository_row(
        &self,
        ui: &mut egui::Ui,
        palette: Palette,
        repo: &RepoRow,
        now: i64,
    ) -> bool {
        let selected = self.scope == repo.scope;
        card(ui, palette, selected, ("repo", repo.scope), |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(one_line(&repo.name, 22)).strong().size(13.0));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(thousands(repo.sessions as i64))
                            .color(palette.text_muted)
                            .size(11.0),
                    );
                });
            });
            if let Some(path) = &repo.path {
                ui.label(
                    RichText::new(elide_middle(path, 32))
                        .color(palette.text_muted)
                        .size(11.0),
                )
                .on_hover_text(path);
            }
            // What the sessions were written by, and when the last one was —
            // the two things that tell one repository from another at a
            // glance.
            let mut meta: Vec<String> = repo
                .by_tool
                .iter()
                .map(|(tool, count)| format!("{} {}", short(*tool).to_lowercase(), count))
                .collect();
            if repo.sessions > 0 {
                meta.push(relative(repo.updated_at, now));
            }
            if !meta.is_empty() {
                ui.label(
                    RichText::new(meta.join(" · "))
                        .color(palette.text_muted)
                        .size(10.5),
                );
            }
        })
    }

    /// The second pane: which session (FR-9.1, mock `1a`).
    ///
    /// Only the sessions of the repository the pane next door has picked, in
    /// dated sections, each row saying when it was, how long it is and which
    /// branch it was on — so that choosing between two sessions of the same
    /// repository is possible without opening both.
    fn sessions_pane(&mut self, ui: &mut egui::Ui, palette: Palette, listing: &Listing) {
        egui::Panel::left("sessions")
            .resizable(true)
            .default_size(324.0)
            .size_range(240.0..=520.0)
            .frame(
                egui::Frame::default()
                    .fill(palette.surface)
                    .inner_margin(egui::Margin::same(10)),
            )
            .show(ui, |ui| {
                let (title, path) = scope_title(&self.snapshot, self.scope);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(one_line(&title, 24)).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!(
                                "{} {}",
                                thousands(listing.listed as i64),
                                if listing.listed == 1 {
                                    "session"
                                } else {
                                    "sessions"
                                }
                            ))
                            .color(palette.text_muted)
                            .size(11.0),
                        );
                    });
                });
                if let Some(path) = &path {
                    ui.label(
                        RichText::new(elide_middle(path, 44))
                            .color(palette.text_muted)
                            .size(11.0),
                    )
                    .on_hover_text(path);
                }
                ui.add_space(6.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter)
                        .hint_text("Filter sessions")
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(8.0);

                if listing.sections.is_empty() {
                    // Which nothing it is: an empty index, a filter that
                    // matched none of it, or a repository that really has no
                    // sessions are three different things to do next about
                    // (FR-9.6).
                    let reason = if !self.filter.trim().is_empty() {
                        "Nothing here matches that filter."
                    } else if self.snapshot.sessions.is_empty() {
                        "Nothing indexed yet. Rescan to look again."
                    } else {
                        match self.scope {
                            Scope::Repo(_) => "This repository has no sessions in the index.",
                            Scope::Unassigned => "Every session belongs to a repository.",
                            Scope::Archived => "No tool has deleted a transcript Mochi has read.",
                            Scope::All => "Nothing to show.",
                        }
                    };
                    ui.label(RichText::new(reason).color(palette.text_muted).size(12.0));
                    return;
                }

                egui::ScrollArea::vertical()
                    .id_salt("sessions-scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for section in &listing.sections {
                            section_heading(ui, palette, section.label);
                            for index in &section.sessions {
                                let session = &self.snapshot.sessions[*index];
                                let selected = self.selected == Some(session.id);
                                let today = section.is_today();
                                let clicked =
                                    card(ui, palette, selected, ("session", session.id), |ui| {
                                        ui.add(egui::Label::new(row(session, palette, ui)).wrap());
                                        ui.label(
                                            RichText::new(meta(session, today))
                                                .color(palette.text_muted)
                                                .size(10.5),
                                        );
                                    });
                                if clicked {
                                    let id = session.id;
                                    self.select(id);
                                    self.tab = Tab::Transcript;
                                }
                            }
                            ui.add_space(4.0);
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

        let selected = self.selected.expect("a session is selected");
        let ready = self
            .transcript
            .as_ref()
            .is_some_and(|transcript| transcript.session_id == selected);
        if !ready {
            // Nothing in hand. Either the answer is on its way, or something
            // dropped the question — a stale answer, a rescan — in which case
            // asking again here is what keeps the spinner from being a lie.
            self.ensure_transcript();
            ui.horizontal(|ui| {
                ui.add_space(24.0);
                ui.add(egui::Spinner::new().size(14.0));
                ui.label(RichText::new("Reading the transcript…").color(palette.text_muted));
            });
            return;
        }

        let messages = match &self.transcript.as_ref().expect("checked above").body {
            Ok(messages) => messages,
            // FR-9.6: the reason belongs where the transcript would have been.
            // The status bar is for what has gone wrong with the window, and a
            // session that loads fine next would clear it anyway.
            Err(reason) => {
                let reason = format!("The index answered: {reason}");
                empty(
                    ui,
                    palette,
                    "This transcript could not be read",
                    &[
                        &reason,
                        "The session itself is still listed, and the panel on the right says where \
                         its file is. Rescan to read it again.",
                    ],
                );
                return;
            }
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
    // The character count travels with the entry rather than being worked out
    // here: this runs for every entry on screen on every repaint, and a tool
    // result can be a megabyte long.
    let (body, count) = if raw {
        (
            message.raw.as_deref().unwrap_or_default(),
            message.raw_chars,
        )
    } else {
        (message.content.as_str(), message.content_chars)
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
            let cut = !expanded && count > ENTRY_CHARS;
            // Where to cut costs the length of the cut, not the length of the
            // entry: `chars().take(…).collect()` walks the same distance but
            // builds the string a character at a time.
            let text: &str = if cut {
                let end = body
                    .char_indices()
                    .nth(ENTRY_CHARS)
                    .map(|(at, _)| at)
                    .unwrap_or(body.len());
                &body[..end]
            } else {
                body
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

/// One clickable row of a left-hand pane, however many lines it draws.
///
/// egui's `selectable_label` is one line of one string, which is what forced
/// the old sidebar to say so little about each session. This is the same
/// bargain made the other way: draw whatever the row needs, then make the
/// whole block behave like one control.
fn card<K: std::hash::Hash + std::fmt::Debug>(
    ui: &mut egui::Ui,
    palette: Palette,
    selected: bool,
    id: K,
    add: impl FnOnce(&mut egui::Ui),
) -> bool {
    let fill = if selected {
        palette.surface_2
    } else {
        egui::Color32::TRANSPARENT
    };
    let inner = egui::Frame::default()
        .fill(fill)
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 3.0;
            add(ui);
        });

    let rect = inner.response.rect;
    let response = ui.interact(rect, ui.make_persistent_id(id), egui::Sense::click());
    if selected {
        // A bar down the edge as well as the fill: rows here are two and three
        // lines tall, and at that size a slightly lighter background is easy
        // to miss.
        ui.painter().rect_filled(
            egui::Rect::from_min_size(rect.min, egui::vec2(2.0, rect.height())),
            1.0,
            palette.accent,
        );
    } else if response.hovered() {
        ui.painter().rect_stroke(
            rect,
            6.0,
            egui::Stroke::new(1.0, palette.border),
            egui::StrokeKind::Inside,
        );
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    ui.add_space(2.0);
    response.clicked()
}

/// A dated heading in the session pane.
fn section_heading(ui: &mut egui::Ui, palette: Palette, label: &str) {
    ui.add_space(6.0);
    ui.label(
        RichText::new(label.to_uppercase())
            .color(palette.text_muted)
            .size(10.0)
            .monospace(),
    );
    ui.add_space(3.0);
}

/// The second line of a session row: when, how long, and on which branch.
///
/// Under a heading that already says which day it is, the clock alone is
/// enough; anywhere else the date has to be there.
fn meta(session: &SessionView, today: bool) -> String {
    let mut parts = vec![if today {
        hour_minute(session.updated_at)
    } else {
        date(session.updated_at)
    }];
    parts.push(format!("{} msgs", thousands(session.message_count)));
    if let Some(branch) = &session.git_branch {
        parts.push(one_line(branch, 20));
    }
    parts.join(" · ")
}

/// The session row's first line: a tool tag, the title, and any warnings.
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
        &one_line(session.label(), 44),
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
