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

//! The Mochi desktop application.
//!
//! A thin shell: it opens the index, hands the interface the same document
//! `mochi export` produces, and gets out of the way. Everything that decides
//! anything lives in `mochi-core`.
//!
//! Tauri rather than Electron, per the decision in `doc/requirements.md` §7.1 —
//! the load this product actually carries is parsing gigabytes of JSONL and
//! searching ten thousand sessions, which is Rust's side of the argument, and
//! the startup and memory targets (NFR-1.1, NFR-1.7) are set by that choice.
//! R-10 is still open: if the integrated terminal spike cannot be made to work
//! on this stack, the shell changes. Nothing in `ui/` or `mochi-core` would.

// Windows: a GUI application should not also open a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use mochi_core::adapter::EnvSource;
use mochi_core::export::{document, ExportOptions};
use mochi_core::index::Index;
use mochi_core::repo::GitCli;
use mochi_core::scan::{ScanOptions, Scanner};
use tauri::plugin::Builder as PluginBuilder;

/// Bridge the interface expects on `window` (`ui/src/data/desktop.ts`).
///
/// Injected here rather than having the interface reach for Tauri's own
/// global, so the interface keeps one shell-shaped contract instead of one per
/// shell.
const BRIDGE: &str = r#"
window.__MOCHI__ = {
  invoke: (command, args) => window.__TAURI__.core.invoke(command, args ?? {}),
};
"#;

fn open() -> Result<Index, String> {
    let path = mochi_core::config::index_path()
        .ok_or_else(|| "could not work out this system's application data directory".to_string())?;
    Index::open(&path).map_err(|error| format!("opening the index at {}: {error}", path.display()))
}

/// Read everything the window needs.
///
/// On a first run the index is empty, so this scans before answering: an
/// application that opens to an empty list and no explanation would be worse
/// than one that takes a moment.
#[tauri::command]
async fn load_index(reveal_secrets: bool) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut index = open()?;
        if index.stats().map_err(|e| e.to_string())?.sessions == 0 {
            scan_into(&mut index)?;
        }
        // Unmasking happens here, where the files are read, so the window is
        // never holding a secret it is only pretending to hide (NFR-3.3).
        document(
            &index,
            ExportOptions {
                reveal_secrets,
                ..Default::default()
            },
        )
        .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("the index task did not finish: {error}"))?
}

/// Re-read the session stores and answer with the fresh document.
#[tauri::command]
async fn rescan(reveal_secrets: bool) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut index = open()?;
        scan_into(&mut index)?;
        document(
            &index,
            ExportOptions {
                reveal_secrets,
                ..Default::default()
            },
        )
        .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("the scan task did not finish: {error}"))?
}

fn scan_into(index: &mut Index) -> Result<(), String> {
    Scanner::new(EnvSource::from_process(), &GitCli, ScanOptions::default())
        .run(index)
        .map(|_| ())
        .map_err(|error| format!("scanning: {error}"))
}

/// Where the index lives, for the window to show in a diagnostics view.
#[tauri::command]
fn index_location() -> Option<PathBuf> {
    mochi_core::config::index_path()
}

fn main() {
    tauri::Builder::default()
        // Runs in every webview before the page loads, which is what makes the
        // bridge available to the interface's first render.
        .plugin(
            PluginBuilder::<tauri::Wry, ()>::new("mochi-bridge")
                .js_init_script(BRIDGE.to_string())
                .build(),
        )
        .invoke_handler(tauri::generate_handler![load_index, rescan, index_location])
        .run(tauri::generate_context!())
        .expect("the Mochi window could not be started");
}
