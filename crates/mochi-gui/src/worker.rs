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

//! The thread that owns the index.
//!
//! Everything that touches SQLite or the filesystem happens here, on one
//! thread, and answers arrive as messages. The window therefore never blocks
//! on a scan of a hundred thousand session files (NFR-1.1), and the index —
//! which is a `rusqlite` connection and deliberately not `Sync` — has exactly
//! one owner.
//!
//! Masking happens here too, on the way out, so the window never holds a
//! secret it is only pretending to hide (NFR-3.3). Revealing is a re-read,
//! not a display toggle.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use mochi_core::adapter::EnvSource;
use mochi_core::command::QuoteStyle;
use mochi_core::export;
use mochi_core::index::{Index, SearchQuery, SessionOrder, SessionQuery};
use mochi_core::mask::Masker;
use mochi_core::repo::GitCli;
use mochi_core::scan::{ScanOptions, Scanner};

use crate::view::{session_view, HitView, MessageView, RepoView, Snapshot};

/// What the window asks for.
#[derive(Debug, Clone)]
pub enum Request {
    /// Read repositories and sessions. Scans first if the index is empty,
    /// because an application that opens to an empty list and no explanation
    /// is worse than one that takes a moment.
    Load { reveal_secrets: bool },
    /// Re-read the session stores, then reload.
    Rescan { reveal_secrets: bool },
    /// The transcript of one session.
    Messages {
        session_id: i64,
        reveal_secrets: bool,
    },
    /// Full-text search across every indexed message (FR-6.1).
    Search { text: String, reveal_secrets: bool },
}

/// What comes back.
#[derive(Debug, Clone)]
pub enum Response {
    /// Something long has started; the text says what.
    Busy(&'static str),
    Loaded(Box<Snapshot>),
    Scanned(String),
    Messages {
        session_id: i64,
        messages: Vec<MessageView>,
    },
    Hits {
        text: String,
        hits: Vec<HitView>,
    },
    Failed(String),
}

/// A handle to the index thread.
pub struct Worker {
    requests: Sender<Request>,
    responses: Receiver<Response>,
}

/// The way back to the window: a sender that wakes it.
///
/// An idle window does not repaint on its own, so a message sent without a
/// wake is a message nobody sees until the mouse moves — which for a progress
/// message means the status bar describing the wrong thing for as long as the
/// work takes. Wrapping the sender is what makes that impossible to forget:
/// there is no way to send without waking.
struct Outbox {
    sender: Sender<Response>,
    wake: Box<dyn Fn() + Send>,
}

impl Outbox {
    /// Send one answer. `false` means the window is gone, which is how the
    /// thread is asked to stop.
    fn send(&self, response: Response) -> bool {
        if self.sender.send(response).is_err() {
            return false;
        }
        (self.wake)();
        true
    }
}

impl Worker {
    /// Start the thread. `wake` is called for every answer sent — progress
    /// included — so that a window sitting idle repaints instead of waiting
    /// for a mouse move.
    pub fn spawn(
        index_path: Option<PathBuf>,
        home: Option<PathBuf>,
        wake: impl Fn() + Send + 'static,
    ) -> Worker {
        let (requests, inbox) = mpsc::channel::<Request>();
        let (outbox, responses) = mpsc::channel::<Response>();

        thread::Builder::new()
            .name("mochi-index".to_string())
            .spawn(move || {
                let mut state = State {
                    index_path,
                    home,
                    index: None,
                };
                let outbox = Outbox {
                    sender: outbox,
                    wake: Box::new(wake),
                };
                // Ends when the window drops its sender, which is how the
                // thread is asked to stop.
                while let Ok(request) = inbox.recv() {
                    if !state.handle(request, &outbox) {
                        return;
                    }
                }
            })
            .expect("the index thread could not be started");

        Worker {
            requests,
            responses,
        }
    }

    /// Ask for something. Ignores the error: a dead index thread shows up as
    /// the absence of an answer, which the window already has to handle.
    pub fn send(&self, request: Request) {
        let _ = self.requests.send(request);
    }

    /// Wait for one answer.
    ///
    /// The window never uses this — it asks once per frame with
    /// [`Worker::try_recv`] and draws whatever has arrived. It exists so that
    /// the protocol can be exercised without a window.
    pub fn recv_timeout(&self, timeout: std::time::Duration) -> Option<Response> {
        self.responses.recv_timeout(timeout).ok()
    }

    /// Take one answer, if there is one waiting.
    ///
    /// A dead index thread reads as "nothing waiting": the window has to cope
    /// with an answer that never comes anyway, and there is nothing useful it
    /// could do differently.
    pub fn try_recv(&self) -> Option<Response> {
        self.responses.try_recv().ok()
    }
}

struct State {
    index_path: Option<PathBuf>,
    home: Option<PathBuf>,
    index: Option<Index>,
}

impl State {
    /// Answer one request. `false` means the window has gone away.
    fn handle(&mut self, request: Request, out: &Outbox) -> bool {
        match request {
            Request::Load { reveal_secrets } => {
                out.send(Response::Busy("Reading the index…"))
                    && self.loading(reveal_secrets, false, out)
            }
            Request::Rescan { reveal_secrets } => {
                out.send(Response::Busy("Scanning your session stores…"))
                    && self.loading(reveal_secrets, true, out)
            }
            Request::Messages {
                session_id,
                reveal_secrets,
            } => match self.messages(session_id, reveal_secrets) {
                Ok(messages) => out.send(Response::Messages {
                    session_id,
                    messages,
                }),
                Err(error) => out.send(Response::Failed(error)),
            },
            Request::Search {
                text,
                reveal_secrets,
            } => match self.search(&text, reveal_secrets) {
                Ok(hits) => out.send(Response::Hits { text, hits }),
                Err(error) => out.send(Response::Failed(error)),
            },
        }
    }

    /// [`State::load`], with the failure reported rather than returned.
    fn loading(&mut self, reveal_secrets: bool, force_scan: bool, out: &Outbox) -> bool {
        match self.load(reveal_secrets, force_scan, out) {
            Ok(connected) => connected,
            Err(error) => out.send(Response::Failed(error)),
        }
    }

    fn index(&mut self) -> Result<&mut Index, String> {
        if self.index.is_none() {
            let path = self.index_path.clone().ok_or_else(|| {
                "could not work out this system's application data directory".to_string()
            })?;
            let index = Index::open(&path)
                .map_err(|error| format!("opening the index at {}: {error}", path.display()))?;
            self.index = Some(index);
        }
        Ok(self.index.as_mut().expect("just opened"))
    }

    /// Read everything but the transcripts, scanning first when asked to or
    /// when there is nothing to show.
    fn load(
        &mut self,
        reveal_secrets: bool,
        force_scan: bool,
        out: &Outbox,
    ) -> Result<bool, String> {
        let env = match &self.home {
            Some(home) => EnvSource::with_home(home),
            None => EnvSource::from_process(),
        };

        let index = self.index()?;
        let empty = index.stats().map_err(|e| e.to_string())?.sessions == 0;

        if force_scan || empty {
            // A first run reads every session file on the machine, which can
            // take minutes. Saying "reading the index" for that long would be
            // a lie about what is happening.
            if !out.send(Response::Busy("Scanning your session stores…")) {
                return Ok(false);
            }
            let report = Scanner::new(env, &GitCli, ScanOptions::default())
                .run(index)
                .map_err(|error| format!("scanning: {error}"))?;
            if !out.send(Response::Scanned(format!(
                "{} session(s) found: {} read, {} unchanged, {} archived, {} unreadable",
                report.discovered, report.parsed, report.unchanged, report.archived, report.failed
            ))) {
                return Ok(false);
            }
        }

        let index_path = self.index_path.clone();
        let index = self.index()?;
        let masker = Masker::new();
        let style = QuoteStyle::for_host();

        let repositories = index
            .list_repositories()
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|repo| RepoView {
                id: repo.id,
                display_name: repo.display_name,
                root_path: repo.root_path,
                remote_url: repo.remote_url,
                is_worktree: repo.is_worktree,
            })
            .collect();

        let sessions = index
            .list_sessions(&SessionQuery {
                order: SessionOrder::UpdatedDesc,
                limit: Some(u32::MAX),
                ..Default::default()
            })
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|record| {
                let resume = export::resume_command(&record, style);
                // A title comes out of the session file like any other text,
                // so it can carry a key just as a message can.
                let title = record.title.as_deref().map(|title| {
                    if reveal_secrets {
                        title.to_string()
                    } else {
                        masker.mask(title).text
                    }
                });
                session_view(record, title, resume)
            })
            .collect();

        let stats = index.stats().map_err(|error| error.to_string())?;

        Ok(out.send(Response::Loaded(Box::new(Snapshot {
            masked: !reveal_secrets,
            repositories,
            sessions,
            stats,
            index_path,
        }))))
    }

    fn messages(
        &mut self,
        session_id: i64,
        reveal_secrets: bool,
    ) -> Result<Vec<MessageView>, String> {
        let index = self.index()?;
        let masker = Masker::new();
        let mask = |text: &str| -> String {
            if reveal_secrets {
                text.to_string()
            } else {
                masker.mask(text).text
            }
        };

        Ok(index
            .messages(session_id)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|message| MessageView {
                seq: message.seq,
                role: message.role,
                content: mask(&message.content),
                tool_name: message.tool_name,
                timestamp: message.timestamp,
                raw: message.raw.as_deref().map(mask),
            })
            .collect())
    }

    fn search(&mut self, text: &str, reveal_secrets: bool) -> Result<Vec<HitView>, String> {
        if text.trim().is_empty() {
            return Ok(Vec::new());
        }
        let index = self.index()?;
        Ok(index
            .search(&SearchQuery {
                text: text.to_string(),
                repo_id: None,
                session_id: None,
                tool: None,
                limit: Some(200),
                reveal_secrets,
            })
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|hit| HitView {
                session_id: hit.session_id,
                tool: hit.tool,
                repo_display: hit.repo_display,
                session_title: hit.session_title,
                message_seq: hit.message_seq,
                timestamp: hit.timestamp,
                snippet: hit.snippet,
            })
            .collect())
    }
}
