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

//! Path normalisation (FR-3.5, NFR-5.3, NFR-6.4).
//!
//! Every case here is a way two strings can name one directory. Repository
//! grouping is only as good as this module.

use mochi_core::paths::{detect_style, normalize, same_path, NormalizeOptions, PathStyle};

fn win() -> NormalizeOptions {
    NormalizeOptions::windows()
}
fn mac() -> NormalizeOptions {
    NormalizeOptions::macos()
}
fn linux() -> NormalizeOptions {
    NormalizeOptions::linux()
}

#[test]
fn windows_drive_letter_case_does_not_matter() {
    let a = normalize(r"c:\Users\you\code\app", win());
    let b = normalize(r"C:\Users\you\code\app", win());
    assert_eq!(a.key(), b.key());
    // The display form settles on the conventional upper-case drive letter.
    assert_eq!(a.display(), r"C:\Users\you\code\app");
}

#[test]
fn windows_separators_are_interchangeable() {
    let a = normalize(r"C:\Users\you\code\app", win());
    let b = normalize("C:/Users/you/code/app", win());
    assert_eq!(a.key(), b.key());
    assert_eq!(a.display(), b.display());
}

#[test]
fn windows_paths_are_case_insensitive() {
    assert!(same_path(r"C:\Users\You\Code\App", r"c:\users\you\code\app", win()));
}

#[test]
fn unix_paths_are_case_sensitive() {
    assert!(!same_path("/home/user/App", "/home/user/app", linux()));
    assert!(same_path("/home/user/app", "/home/user/app", linux()));
}

#[test]
fn macos_is_case_insensitive_by_default() {
    // The default volume format folds case, which is why two sessions written
    // as /Users/you/Code and /Users/you/code are the same repository.
    assert!(same_path("/Users/you/Code/app", "/Users/you/code/app", mac()));
}

#[test]
fn long_path_prefix_is_stripped() {
    // NFR-2.6: Mochi uses the \\?\ prefix to get past MAX_PATH, but it must
    // never leak into a comparison or into the UI.
    let a = normalize(r"\\?\C:\Users\you\code\app", win());
    let b = normalize(r"C:\Users\you\code\app", win());
    assert_eq!(a.key(), b.key());
    assert_eq!(a.display(), r"C:\Users\you\code\app");
}

#[test]
fn unc_paths_survive_normalisation() {
    let a = normalize(r"\\server\share\code\app", win());
    assert_eq!(a.display(), r"\\server\share\code\app");
    assert!(same_path(r"\\SERVER\share\code\app", r"\\server\share\code\app", win()));
}

#[test]
fn trailing_separators_are_dropped_but_roots_survive() {
    assert_eq!(normalize("/home/user/app/", linux()).display(), "/home/user/app");
    assert_eq!(normalize("/home/user/app//", linux()).display(), "/home/user/app");
    assert_eq!(normalize("/", linux()).display(), "/");
    assert_eq!(normalize(r"C:\", win()).display(), r"C:\");
    assert_eq!(normalize("C:", win()).display(), r"C:\");
}

#[test]
fn dot_segments_are_resolved_lexically() {
    assert!(same_path("/home/user/code/../code/app/.", "/home/user/code/app", linux()));
    assert_eq!(normalize("/a/b/../../c", linux()).display(), "/c");
    // Going above the root cannot escape it.
    assert_eq!(normalize("/../..", linux()).display(), "/");
}

#[test]
fn macos_private_symlinks_are_resolved() {
    // /var, /tmp and /etc are symlinks into /private on macOS, so the same
    // directory shows up under both names depending on who reported it.
    assert!(same_path("/var/folders/zz/session", "/private/var/folders/zz/session", mac()));
    assert!(same_path("/tmp/build", "/private/tmp/build", mac()));
    assert_eq!(normalize("/var/folders/zz", mac()).display(), "/private/var/folders/zz");
    // A directory that merely starts with those letters is left alone.
    assert_eq!(normalize("/variant/data", mac()).display(), "/variant/data");
}

#[test]
fn linux_keeps_var_as_var() {
    assert_eq!(normalize("/var/log/app", linux()).display(), "/var/log/app");
    assert!(!same_path("/var/log/app", "/private/var/log/app", linux()));
}

#[test]
fn unicode_normalisation_forms_compare_equal() {
    // NFR-6.4: macOS hands back decomposed file names, Windows composed ones.
    // The same Japanese directory must not become two repositories.
    let composed = "/Users/you/コード/がぎぐ";
    let decomposed = "/Users/you/コード/か\u{3099}き\u{3099}く\u{3099}";
    assert_ne!(composed, decomposed, "fixture must actually differ byte for byte");
    assert!(same_path(composed, decomposed, mac()));
    assert_eq!(
        normalize(decomposed, mac()).display(),
        composed,
        "the display form should be composed, which is what users type"
    );
}

#[test]
fn non_ascii_and_spaces_survive_display() {
    let p = "/Users/you/私の コード/app (v2)";
    assert_eq!(normalize(p, mac()).display(), p);
    let w = r"C:\Users\you\私の コード\app";
    assert_eq!(normalize(w, win()).display(), w);
}

#[test]
fn emoji_paths_do_not_panic() {
    let p = "/Users/you/📁 projects/app";
    assert_eq!(normalize(p, mac()).display(), p);
    assert!(same_path(p, "/Users/you/📁 projects/app/", mac()));
}

#[test]
fn style_detection() {
    assert_eq!(detect_style(r"C:\Users\you"), PathStyle::Windows);
    assert_eq!(detect_style("C:/Users/you"), PathStyle::Windows);
    assert_eq!(detect_style(r"\\server\share"), PathStyle::Windows);
    assert_eq!(detect_style(r"\\?\C:\x"), PathStyle::Windows);
    assert_eq!(detect_style("/Users/you"), PathStyle::Unix);
    assert_eq!(detect_style("/home/user"), PathStyle::Unix);
    assert_eq!(detect_style("relative/path"), PathStyle::Unix);
}

#[test]
fn empty_input_is_not_a_panic() {
    assert_eq!(normalize("", linux()).display(), "");
    assert_eq!(normalize("", linux()).key(), "");
}

#[test]
fn keys_are_stable_across_repeated_normalisation() {
    let once = normalize(r"c:/Users/YOU/code/app/", win());
    let twice = normalize(once.display(), win());
    assert_eq!(once.key(), twice.key());
    assert_eq!(once.display(), twice.display());
}
