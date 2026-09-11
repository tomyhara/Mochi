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

//! Which face the window borrows for the text egui cannot draw.

use mochi_core::config::Platform;
use mochi_gui::app::snippet;
use mochi_gui::fonts::{candidates, install, OVERRIDE_ENV};
use mochi_gui::theme::LIGHT;

/// Every platform has to offer something, or Japanese transcripts are tofu on
/// that platform and nobody finds out until a user says so (NFR-6.4).
#[test]
fn every_platform_has_somewhere_to_look() {
    for platform in [Platform::Windows, Platform::MacOs, Platform::Other] {
        let found = candidates(platform);
        assert!(!found.is_empty(), "{platform:?} has no candidate faces");
        // `is_absolute` answers for the machine running the test rather than
        // for the platform being described, so the shape is checked by hand.
        for path in &found {
            let text = path.display().to_string();
            assert!(
                text.starts_with('/') || text.starts_with(r"C:\"),
                "{platform:?}: {text} is not a full path"
            );
        }
    }
}

#[test]
fn the_windows_list_starts_with_the_face_windows_uses_for_japanese() {
    let first = candidates(Platform::Windows)[0].display().to_string();
    assert!(first.contains("YuGoth"), "{first}");
}

#[test]
fn the_macos_list_starts_with_hiragino() {
    let first = candidates(Platform::MacOs)[0].display().to_string();
    assert!(first.contains("ヒラギノ"), "{first}");
}

/// epaint does not report a face it cannot parse — it panics on the first
/// frame, once the face is installed. So an unparsable candidate has to be
/// recognised before it is installed, or a corrupt system font (or a
/// `MOCHI_FONT` pointing at something that is not a font) stops the window from
/// opening at all.
///
/// This is the only test that touches `MOCHI_FONT`, which is process-wide.
#[test]
fn a_file_that_is_not_a_font_is_skipped_rather_than_installed() {
    let dir = tempfile::tempdir().unwrap();
    let not_a_font = dir.path().join("not-a-font.ttf");
    std::fs::write(&not_a_font, b"127.0.0.1 localhost\n").unwrap();
    std::env::set_var(OVERRIDE_ENV, &not_a_font);

    let ctx = egui::Context::default();
    let installed = install(&ctx);
    std::env::remove_var(OVERRIDE_ENV);
    assert_ne!(
        installed.as_deref(),
        Some(not_a_font.as_path()),
        "a file that is not a font was installed as one"
    );

    // Laying out text is where epaint would have panicked, so the window's
    // first frame is the assertion.
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::Vec2::new(400.0, 300.0),
        )),
        ..Default::default()
    };
    let painted = ctx.run_ui(input, |ui| {
        ui.label("日本語 and Latin");
    });
    assert!(!painted.shapes.is_empty(), "nothing was drawn");
}

#[test]
fn a_search_snippet_keeps_all_of_its_text() {
    let job = snippet("before \u{2}match\u{3} after", LIGHT);
    assert_eq!(job.text, "before match after");
    // Three runs: the plain text either side, and the match between them.
    assert_eq!(job.sections.len(), 3);
}

/// SQLite writes the markers, so they are as trustworthy as any other input.
/// An unterminated one must not swallow the rest of the line.
#[test]
fn an_unterminated_marker_does_not_lose_the_rest() {
    let job = snippet("before \u{2}match and then some", LIGHT);
    assert_eq!(job.text, "before match and then some");
}

#[test]
fn a_snippet_with_no_markers_is_still_shown() {
    let job = snippet("nothing to highlight", LIGHT);
    assert_eq!(job.text, "nothing to highlight");
    assert_eq!(job.sections.len(), 1);
}
