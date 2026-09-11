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

//! The Mochi window.
//!
//! A native interface: Mochi draws its own widgets with `egui` and asks the
//! operating system for nothing but a window and a GPU surface. There is no
//! web view, no bundled browser and no HTML — which is what lets the whole
//! application, window and command line together, ship as one executable file
//! that runs with nothing installed alongside it.
//!
//! That is a change from doc/requirements.md §7.1, which chose Tauri. R-10
//! reserved the right to change the shell, and this is that change; §7.1
//! records why. Neither `mochi-core` nor anything it decides moved.
//!
//! The window is deliberately thin. Everything that reads a file, opens the
//! index or decides anything lives in `mochi-core`; `worker.rs` runs it on its
//! own thread; `app.rs` draws the answers.

pub mod app;
pub mod fonts;
pub mod format;
pub mod theme;
pub mod view;
pub mod worker;

pub use app::App;

/// What the window was told on the command line.
///
/// All three are the command line tool's own global flags, honoured here for
/// the same reasons: to point an experiment at a throwaway index, to browse a
/// fixture tree instead of a home directory, and to open with secrets already
/// shown.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Index database. Defaults to the OS application data directory.
    pub index_path: Option<std::path::PathBuf>,
    /// Look for session stores under here instead of the user's home.
    pub home: Option<std::path::PathBuf>,
    /// Start with credentials shown rather than masked.
    pub reveal_secrets: bool,
}

impl Options {
    /// Where the index lives, falling back to this platform's convention
    /// (FR-10.1).
    pub fn resolved_index_path(&self) -> Option<std::path::PathBuf> {
        self.index_path
            .clone()
            .or_else(mochi_core::config::index_path)
    }
}

/// The window's icon, carried inside the executable so that there is still
/// only one file to copy about.
const ICON: &[u8] = include_bytes!("../icons/128x128@2x.png");

/// Open the window. Returns when it closes.
pub fn run(options: Options) -> Result<(), String> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Mochi")
        .with_app_id("com.tomyhara.mochi")
        .with_inner_size([1400.0, 900.0])
        .with_min_inner_size([900.0, 600.0]);

    // A window with no icon is not worth refusing to open for.
    if let Ok(icon) = eframe::icon_data::from_png_bytes(ICON) {
        viewport = viewport.with_icon(icon);
    }

    eframe::run_native(
        "Mochi",
        eframe::NativeOptions {
            viewport,
            ..Default::default()
        },
        Box::new(move |cc| Ok(Box::new(App::new(&cc.egui_ctx, options)))),
    )
    .map_err(|error| format!("the Mochi window could not be started: {error}"))
}
