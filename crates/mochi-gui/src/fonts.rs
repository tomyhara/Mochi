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

//! Text that is not Latin.
//!
//! egui ships with fonts that cover Latin and little else, and Mochi's whole
//! job is showing you what you actually said to an agent — which for a large
//! part of its users is Japanese. Tofu boxes would make the window useless
//! (FR-5.1, and NFR-6.4, which asks that CJK text be handled properly rather
//! than approximately).
//!
//! Rather than bundle a CJK font — several megabytes, against a product whose
//! point is one small self-contained file — the window borrows the one the
//! operating system already has. It is added *after* egui's own fonts, so
//! Latin text still looks like egui and only the glyphs it lacks come from the
//! system face. If nothing is found the window still runs: Latin is fine and
//! the rest is tofu, which is worse than a missing font but better than a
//! refusal to start.

use std::path::PathBuf;
use std::sync::Arc;

use mochi_core::config::Platform;

/// Read a face from `MOCHI_FONT` instead of searching. Also how a contributor
/// checks the fallback path without uninstalling their fonts.
pub const OVERRIDE_ENV: &str = "MOCHI_FONT";

/// Faces to try, best first.
///
/// Each platform's list starts with the face that platform's own applications
/// use for Japanese, then widens to the pan-CJK ones, so a machine set up for
/// Chinese or Korean still gets readable text.
pub fn candidates(platform: Platform) -> Vec<PathBuf> {
    let paths: &[&str] = match platform {
        Platform::Windows => &[
            r"C:\Windows\Fonts\YuGothM.ttc",
            r"C:\Windows\Fonts\YuGothR.ttc",
            r"C:\Windows\Fonts\meiryo.ttc",
            r"C:\Windows\Fonts\msgothic.ttc",
            r"C:\Windows\Fonts\malgun.ttf",
            r"C:\Windows\Fonts\simsun.ttc",
        ],
        Platform::MacOs => &[
            "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
            "/System/Library/Fonts/ヒラギノ丸ゴ ProN W4.ttc",
            "/System/Library/Fonts/Hiragino Sans GB.ttc",
            "/System/Library/Fonts/AppleSDGothicNeo.ttc",
            "/System/Library/Fonts/PingFang.ttc",
            "/Library/Fonts/Arial Unicode.ttf",
        ],
        Platform::Other => &[
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJKjp-Regular.otf",
            "/usr/share/fonts/truetype/fonts-japanese-gothic.ttf",
            "/usr/share/fonts/opentype/ipafont-gothic/ipag.ttf",
        ],
    };
    paths.iter().map(PathBuf::from).collect()
}

/// The face this machine will use, and the bytes of it.
fn find() -> Option<(PathBuf, Vec<u8>)> {
    let explicit = std::env::var_os(OVERRIDE_ENV)
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty());

    let wanted = explicit
        .into_iter()
        .chain(candidates(Platform::host()))
        .collect::<Vec<_>>();

    for path in wanted {
        // A font is a file like any other: unreadable, truncated or replaced
        // by something that is not a font. Every one of those is a reason to
        // try the next candidate, not to fail.
        if let Ok(bytes) = std::fs::read(&path) {
            if !bytes.is_empty() {
                return Some((path, bytes));
            }
        }
    }
    None
}

/// Add the system face to both families, as a fallback.
///
/// Returns what was used, for the diagnostics view to show — "why is this
/// tofu" is otherwise unanswerable from inside the window.
pub fn install(ctx: &egui::Context) -> Option<PathBuf> {
    let (path, bytes) = find()?;

    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "system-cjk".to_string(),
        Arc::new(egui::FontData::from_owned(bytes)),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("system-cjk".to_string());
    }
    ctx.set_fonts(fonts);
    Some(path)
}
