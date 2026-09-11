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

//! Colour contrast (NFR-6.2, WCAG 2.1 AA).
//!
//! The interface this replaces had its contrast measured in a browser, on the
//! rendered page. A native window has no such thing to measure, so the check
//! moved to where the colours are decided: every pair the window actually
//! draws text with, in both themes, against the WCAG definition.
//!
//! Scope is the same as the browser test's was — text against the surface
//! behind it. Panel borders are decoration and are not checked; if one ever
//! carries meaning on its own, it needs 3:1 and a line here.

use egui::Color32;
use mochi_gui::theme::{Palette, DARK, LIGHT};

/// Relative luminance, per the WCAG definition.
fn luminance(colour: Color32) -> f64 {
    let channel = |value: u8| {
        let v = value as f64 / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(colour.r()) + 0.7152 * channel(colour.g()) + 0.0722 * channel(colour.b())
}

fn ratio(a: Color32, b: Color32) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    let (light, dark) = if x > y { (x, y) } else { (y, x) };
    (light + 0.05) / (dark + 0.05)
}

/// Every foreground the window uses, against every surface it is drawn on.
fn pairs(palette: Palette) -> Vec<(String, Color32, Color32)> {
    let surfaces = [
        ("background", palette.bg),
        ("surface", palette.surface),
        ("second surface", palette.surface_2),
    ];
    let foregrounds = [
        ("body text", palette.text),
        ("muted text", palette.text_muted),
        ("warnings", palette.danger),
        ("accent", palette.accent),
    ];

    let mut pairs = Vec::new();
    for (surface_name, surface) in surfaces {
        for (text_name, text) in foregrounds {
            pairs.push((format!("{text_name} on the {surface_name}"), text, surface));
        }
    }
    // The one pair that is not text on a surface: a pressed button.
    pairs.push((
        "a button label on the accent".to_string(),
        palette.accent_on,
        palette.accent,
    ));
    pairs
}

#[test]
fn the_light_theme_clears_aa() {
    for (what, text, background) in pairs(LIGHT) {
        let measured = ratio(text, background);
        assert!(
            measured >= 4.5,
            "light: {what} is {measured:.2}:1, below AA's 4.5:1"
        );
    }
}

#[test]
fn the_dark_theme_clears_aa() {
    for (what, text, background) in pairs(DARK) {
        let measured = ratio(text, background);
        assert!(
            measured >= 4.5,
            "dark: {what} is {measured:.2}:1, below AA's 4.5:1"
        );
    }
}

/// The measurement itself, against the values WCAG's own examples give.
#[test]
fn the_ratio_is_the_wcag_one() {
    assert!((ratio(Color32::BLACK, Color32::WHITE) - 21.0).abs() < 0.01);
    assert!((ratio(Color32::WHITE, Color32::WHITE) - 1.0).abs() < 0.01);
}
