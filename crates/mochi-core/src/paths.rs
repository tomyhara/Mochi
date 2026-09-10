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
//!
//! Normalisation here is purely lexical. It never touches the filesystem,
//! because most of the paths Mochi compares refer to directories that are on
//! another machine, or no longer exist at all.

use unicode_normalization::UnicodeNormalization;

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
        NormalizeOptions {
            style: PathStyle::Windows,
            case_insensitive: true,
            resolve_mac_private: false,
        }
    }

    pub fn macos() -> Self {
        // The default volume format folds case. A case-sensitive volume exists
        // but is rare, and treating two spellings as one repository is the
        // safer error of the two: the alternative splits a user's history.
        NormalizeOptions {
            style: PathStyle::Unix,
            case_insensitive: true,
            resolve_mac_private: true,
        }
    }

    pub fn linux() -> Self {
        NormalizeOptions {
            style: PathStyle::Unix,
            case_insensitive: false,
            resolve_mac_private: false,
        }
    }

    /// Rules matching the operating system this build runs on.
    pub fn for_host() -> Self {
        #[cfg(windows)]
        {
            NormalizeOptions::windows()
        }
        #[cfg(target_os = "macos")]
        {
            NormalizeOptions::macos()
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            NormalizeOptions::linux()
        }
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
    let bytes = input.as_bytes();
    if input.starts_with(r"\\") || input.starts_with("//?/") {
        return PathStyle::Windows;
    }
    // A drive letter: "C:", "C:\" or "C:/".
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return PathStyle::Windows;
    }
    if input.contains('\\') && !input.contains('/') {
        return PathStyle::Windows;
    }
    PathStyle::Unix
}

pub fn normalize(input: &str, opts: NormalizeOptions) -> NormalizedPath {
    if input.is_empty() {
        return NormalizedPath {
            display: String::new(),
            key: String::new(),
        };
    }

    // Compose first. macOS reports file names decomposed, Windows composed;
    // the same Japanese directory must not become two repositories (NFR-6.4).
    let composed: String = input.nfc().collect();

    match opts.style {
        PathStyle::Windows => normalize_windows(&composed, opts),
        PathStyle::Unix => normalize_unix(&composed, opts),
    }
}

/// Normalise using the rules of the running platform, guessing the style of
/// the input when it clearly belongs to the other one (an index built on one
/// machine can be read on another).
pub fn normalize_host(input: &str) -> NormalizedPath {
    let host = NormalizeOptions::for_host();
    let style = detect_style(input);
    if style == host.style {
        return normalize(input, host);
    }
    // The path came from the other kind of machine. Use that platform's rules
    // so it at least normalises consistently with itself.
    let opts = match style {
        PathStyle::Windows => NormalizeOptions::windows(),
        PathStyle::Unix => NormalizeOptions {
            style: PathStyle::Unix,
            case_insensitive: host.case_insensitive,
            resolve_mac_private: host.resolve_mac_private,
        },
    };
    normalize(input, opts)
}

pub fn same_path(a: &str, b: &str, opts: NormalizeOptions) -> bool {
    normalize(a, opts).key == normalize(b, opts).key
}

fn normalize_windows(input: &str, opts: NormalizeOptions) -> NormalizedPath {
    let mut rest = input.replace('/', "\\");

    // \\?\ and \\?\UNC\ exist to get past MAX_PATH (NFR-2.6). They are an API
    // detail and must never reach a comparison or the screen.
    if let Some(stripped) = rest.strip_prefix(r"\\?\UNC\") {
        rest = format!(r"\\{stripped}");
    } else if let Some(stripped) = rest.strip_prefix(r"\\?\") {
        rest = stripped.to_string();
    }

    let (prefix, remainder) = if let Some(after) = rest.strip_prefix(r"\\") {
        // UNC: \\server\share\... . Server and share belong to the prefix.
        let mut parts = after.splitn(3, '\\');
        let server = parts.next().unwrap_or("");
        let share = parts.next().unwrap_or("");
        let tail = parts.next().unwrap_or("");
        (format!(r"\\{server}\{share}"), tail.to_string())
    } else if rest.len() >= 2
        && rest.as_bytes()[0].is_ascii_alphabetic()
        && rest.as_bytes()[1] == b':'
    {
        let drive = rest[..1].to_ascii_uppercase();
        let tail = rest[2..].trim_start_matches('\\').to_string();
        (format!("{drive}:"), tail)
    } else {
        (String::new(), rest.trim_start_matches('\\').to_string())
    };

    let segments = resolve_segments(&remainder, '\\');
    let joined = segments.join("\\");

    let display = if prefix.is_empty() {
        joined.clone()
    } else if joined.is_empty() {
        // A bare root: "C:" displays as "C:\", a UNC share as-is.
        if prefix.starts_with(r"\\") {
            prefix.clone()
        } else {
            format!("{prefix}\\")
        }
    } else {
        format!("{prefix}\\{joined}")
    };

    let key_source = if prefix.is_empty() {
        joined
    } else {
        format!("{prefix}/{joined}")
    };
    let key = key_source.replace('\\', "/");
    let key = if opts.case_insensitive {
        fold_case(&key)
    } else {
        key
    };

    NormalizedPath { display, key }
}

fn normalize_unix(input: &str, opts: NormalizeOptions) -> NormalizedPath {
    let absolute = input.starts_with('/');
    let segments = resolve_segments(input, '/');
    let mut joined = segments.join("/");

    if absolute {
        joined = format!("/{joined}");
    }

    if opts.resolve_mac_private {
        joined = resolve_mac_private(&joined);
    }

    let display = if joined.is_empty() {
        String::new()
    } else {
        joined
    };
    let key = if opts.case_insensitive {
        fold_case(&display)
    } else {
        display.clone()
    };

    NormalizedPath { display, key }
}

/// `/var`, `/tmp` and `/etc` are symlinks into `/private` on macOS, so the same
/// directory turns up under both names depending on who reported it.
fn resolve_mac_private(path: &str) -> String {
    for link in ["/var", "/tmp", "/etc"] {
        if path == link {
            return format!("/private{link}");
        }
        if let Some(rest) = path.strip_prefix(link) {
            if rest.starts_with('/') {
                return format!("/private{path}");
            }
        }
    }
    path.to_string()
}

/// Split on `sep`, drop empty and `.` segments, and apply `..` lexically.
/// `..` above the root is dropped rather than escaping it.
fn resolve_segments(path: &str, sep: char) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for segment in path.split(sep) {
        match segment {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            other => out.push(other.to_string()),
        }
    }
    out
}

/// Lower-case for comparison. `to_lowercase` handles the non-ASCII cases that
/// a byte-wise fold would get wrong, which matters for the paths in NFR-6.4.
fn fold_case(s: &str) -> String {
    s.to_lowercase()
}
