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

//! Where Mochi keeps its own files (FR-10.1).
//!
//! Windows and macOS have their own conventions and Mochi follows them.
//! `~/.config` is deliberately not used on either: it is an XDG convention
//! that belongs to neither platform, and putting a database there would leave
//! it outside the directories users back up and migrate.
//!
//! This is written out rather than taken from a crate because the rule is four
//! lines long, it is a stated requirement, and the obvious crate for the job
//! pulls in an MPL-licensed dependency that the project's licence policy
//! rejects (NFR-4b.2).

use std::path::{Path, PathBuf};

/// Application name used for the directory on every platform.
pub const APP_NAME: &str = "Mochi";

/// File name of the index inside the data directory.
pub const INDEX_FILE: &str = "index.sqlite3";

/// The platforms whose conventions differ. Named rather than taken from `cfg!`
/// so the rules can be tested on any runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Windows,
    MacOs,
    /// Linux and the rest. Not a supported target (Q-6), but the code should
    /// do something sensible for contributors who build there.
    Other,
}

impl Platform {
    pub fn host() -> Platform {
        if cfg!(windows) {
            Platform::Windows
        } else if cfg!(target_os = "macos") {
            Platform::MacOs
        } else {
            Platform::Other
        }
    }
}

/// Directory for Mochi's own data, worked out from explicit inputs.
///
/// `app_data` is `%APPDATA%` and `xdg_data_home` is `$XDG_DATA_HOME`; either
/// may be absent.
pub fn data_dir_for(
    platform: Platform,
    home: &Path,
    app_data: Option<&str>,
    xdg_data_home: Option<&str>,
) -> PathBuf {
    match platform {
        Platform::Windows => match app_data.filter(|value| !value.is_empty()) {
            Some(roaming) => PathBuf::from(roaming).join(APP_NAME),
            None => home.join("AppData").join("Roaming").join(APP_NAME),
        },
        Platform::MacOs => home
            .join("Library")
            .join("Application Support")
            .join(APP_NAME),
        Platform::Other => match xdg_data_home.filter(|value| !value.is_empty()) {
            Some(share) => PathBuf::from(share).join("mochi"),
            None => home.join(".local").join("share").join("mochi"),
        },
    }
}

/// Directory for Mochi's own data on this machine.
pub fn data_dir() -> Option<PathBuf> {
    let home = home_dir()?;
    Some(data_dir_for(
        Platform::host(),
        &home,
        std::env::var("APPDATA").ok().as_deref(),
        std::env::var("XDG_DATA_HOME").ok().as_deref(),
    ))
}

/// Default location of the index database.
pub fn index_path() -> Option<PathBuf> {
    Some(data_dir()?.join(INDEX_FILE))
}

/// The user's home directory.
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
}
