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

//! Where Mochi stores its own files (FR-10.1).

use std::path::{Path, PathBuf};

use mochi_core::config::{data_dir_for, index_path, Platform, APP_NAME, INDEX_FILE};

/// Compare against a path built the same way, since `join` uses the *host*
/// separator: these tests run on all three CI runners.
fn joined(base: &str, tail: &str) -> PathBuf {
    PathBuf::from(base).join(tail)
}

#[test]
fn windows_uses_the_roaming_application_data_directory() {
    let dir = data_dir_for(
        Platform::Windows,
        Path::new(r"C:\Users\you"),
        Some(r"C:\Users\you\AppData\Roaming"),
        None,
    );
    assert_eq!(dir, joined(r"C:\Users\you\AppData\Roaming", "Mochi"));
}

#[test]
fn windows_falls_back_when_appdata_is_unset() {
    let dir = data_dir_for(Platform::Windows, Path::new(r"C:\Users\you"), None, None);
    assert_eq!(
        dir,
        Path::new(r"C:\Users\you")
            .join("AppData")
            .join("Roaming")
            .join("Mochi")
    );
    let empty = data_dir_for(
        Platform::Windows,
        Path::new(r"C:\Users\you"),
        Some(""),
        None,
    );
    assert_eq!(empty, dir, "an empty variable is the same as an unset one");
}

#[test]
fn macos_uses_application_support() {
    let dir = data_dir_for(Platform::MacOs, Path::new("/Users/you"), None, None);
    assert_eq!(
        dir,
        Path::new("/Users/you")
            .join("Library")
            .join("Application Support")
            .join("Mochi")
    );
}

#[test]
fn xdg_is_never_used_on_a_supported_platform() {
    // FR-10.1 spells this out: ~/.config and $XDG_DATA_HOME belong to neither
    // Windows nor macOS, and a database there would sit outside what users
    // back up and migrate.
    for (platform, home) in [
        (Platform::Windows, r"C:\Users\you"),
        (Platform::MacOs, "/Users/you"),
    ] {
        let dir = data_dir_for(platform, Path::new(home), None, Some("/somewhere/share"));
        let text = dir.to_string_lossy();
        assert!(
            !text.contains("somewhere"),
            "{platform:?} honoured XDG_DATA_HOME: {text}"
        );
        assert!(
            !text.contains(".config"),
            "{platform:?} used ~/.config: {text}"
        );
    }
}

#[test]
fn other_platforms_get_something_sensible() {
    // Linux is not supported, but a contributor building there should not have
    // files land in a strange place (Q-6).
    let dir = data_dir_for(Platform::Other, Path::new("/home/user"), None, None);
    assert_eq!(
        dir,
        Path::new("/home/user")
            .join(".local")
            .join("share")
            .join("mochi")
    );

    let xdg = data_dir_for(
        Platform::Other,
        Path::new("/home/user"),
        None,
        Some("/data"),
    );
    assert_eq!(xdg, joined("/data", "mochi"));
}

#[test]
fn the_index_lives_in_the_data_directory() {
    let path = index_path().expect("a data directory on this machine");
    assert!(path.ends_with(INDEX_FILE));
    assert!(
        path.parent().unwrap().ends_with(APP_NAME) || path.parent().unwrap().ends_with("mochi")
    );
}
