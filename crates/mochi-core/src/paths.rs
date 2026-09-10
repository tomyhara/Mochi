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

//! Path normalisation (FR-3.5, NFR-6.4).
//!
//! Two strings can name the same directory and still differ byte for byte:
//! a drive letter's case, a separator, a macOS `/var` symlink, an NFD vs NFC
//! encoded Japanese folder name. Repository identity (FR-3.3) is decided by
//! comparing paths, so every comparison goes through [`normalize`] first.

/// Which platform's path rules to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathStyle {
    Windows,
    Unix,
}

/// Normalisation rules. Split out from the host so the awkward cases can be
/// unit tested on any CI runner (NFR-5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NormalizeOptions {
    pub style: PathStyle,
    /// Fold case when building the comparison key.
    pub case_insensitive: bool,
    /// Rewrite the macOS `/var`, `/tmp` and `/etc` symlinks to their real
    /// `/private/...` targets.
    pub resolve_mac_private: bool,
}

impl NormalizeOptions {
    pub fn windows() -> Self {
        todo!()
    }
    pub fn macos() -> Self {
        todo!()
    }
    pub fn linux() -> Self {
        todo!()
    }
    /// Rules matching the operating system this build runs on.
    pub fn for_host() -> Self {
        todo!()
    }
}

/// A path in both the form to show a user and the form to compare.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NormalizedPath {
    display: String,
    key: String,
}

impl NormalizedPath {
    /// Cleaned up, but still readable: original case, native separators.
    pub fn display(&self) -> &str {
        &self.display
    }
    /// Canonical form. Equal keys mean the same location.
    pub fn key(&self) -> &str {
        &self.key
    }
}

/// Guess whether a string is a Windows path from its shape.
pub fn detect_style(input: &str) -> PathStyle {
    todo!()
}

pub fn normalize(input: &str, opts: NormalizeOptions) -> NormalizedPath {
    todo!()
}

/// Normalise using the rules of the running platform, guessing the style of
/// the input when it clearly belongs to the other one (an index built on one
/// machine can be read on another).
pub fn normalize_host(input: &str) -> NormalizedPath {
    todo!()
}

pub fn same_path(a: &str, b: &str, opts: NormalizeOptions) -> bool {
    todo!()
}
