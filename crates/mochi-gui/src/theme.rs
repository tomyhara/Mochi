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

//! Colour.
//!
//! The palette is the one the interface already had, carried over unchanged so
//! that the window looks like the thing the mockups and doc/ui-spec.md
//! describe. Every pair here has to clear WCAG AA (NFR-6.2); the values are
//! the ones the browser tests measured, and changing one means measuring it
//! again rather than eyeballing it.

use egui::{Color32, Stroke, Visuals};

/// The nine colours everything else is built from.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// Window background.
    pub bg: Color32,
    /// Cards and panels that sit on the background.
    pub surface: Color32,
    /// The quieter of the two surfaces: headers, inputs, selected rows.
    pub surface_2: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub accent: Color32,
    /// Text drawn on top of the accent.
    pub accent_on: Color32,
    pub danger: Color32,
}

pub const LIGHT: Palette = Palette {
    bg: Color32::from_rgb(0xfa, 0xf9, 0xf5),
    surface: Color32::from_rgb(0xff, 0xff, 0xff),
    surface_2: Color32::from_rgb(0xf1, 0xef, 0xe9),
    border: Color32::from_rgb(0xd3, 0xce, 0xc2),
    text: Color32::from_rgb(0x1b, 0x1d, 0x20),
    text_muted: Color32::from_rgb(0x54, 0x5b, 0x64),
    accent: Color32::from_rgb(0xb4, 0x45, 0x3a),
    accent_on: Color32::from_rgb(0xff, 0xff, 0xff),
    danger: Color32::from_rgb(0x9a, 0x2f, 0x26),
};

pub const DARK: Palette = Palette {
    bg: Color32::from_rgb(0x10, 0x12, 0x14),
    surface: Color32::from_rgb(0x17, 0x1a, 0x1d),
    surface_2: Color32::from_rgb(0x1e, 0x22, 0x27),
    border: Color32::from_rgb(0x33, 0x3a, 0x41),
    text: Color32::from_rgb(0xe8, 0xeb, 0xef),
    text_muted: Color32::from_rgb(0xa7, 0xb0, 0xbb),
    accent: Color32::from_rgb(0xf0, 0x96, 0x8c),
    accent_on: Color32::from_rgb(0x10, 0x12, 0x14),
    danger: Color32::from_rgb(0xf0, 0x96, 0x8c),
};

impl Palette {
    pub fn of(theme: egui::Theme) -> Palette {
        match theme {
            egui::Theme::Light => LIGHT,
            egui::Theme::Dark => DARK,
        }
    }

    /// The colour a transcript entry's text is drawn in.
    ///
    /// Thinking and system entries are the model talking to itself rather than
    /// to the reader, and the interface has always shown them quieter.
    pub fn role_text(&self, role: mochi_core::model::Role) -> Color32 {
        use mochi_core::model::Role;
        match role {
            Role::Thinking | Role::System => self.text_muted,
            _ => self.text,
        }
    }
}

/// egui's own settings, dressed in one of the palettes.
pub fn visuals(palette: Palette, dark: bool) -> Visuals {
    let mut visuals = if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };

    visuals.override_text_color = Some(palette.text);
    visuals.panel_fill = palette.bg;
    visuals.window_fill = palette.surface;
    visuals.extreme_bg_color = palette.surface;
    visuals.faint_bg_color = palette.surface_2;
    visuals.hyperlink_color = palette.accent;
    visuals.window_stroke = Stroke::new(1.0, palette.border);
    visuals.selection.bg_fill = palette.accent.linear_multiply(0.35);
    visuals.selection.stroke = Stroke::new(1.0, palette.text);
    visuals.error_fg_color = palette.danger;
    visuals.warn_fg_color = palette.danger;

    // Widgets: flat, bordered, and quiet until touched. The interface this
    // replaces was a web page, and the thing that made it feel like Mochi was
    // restraint rather than any one colour.
    visuals.widgets.noninteractive.bg_fill = palette.surface;
    visuals.widgets.noninteractive.weak_bg_fill = palette.surface;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, palette.border);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, palette.text_muted);

    visuals.widgets.inactive.bg_fill = palette.surface_2;
    visuals.widgets.inactive.weak_bg_fill = palette.surface_2;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, palette.border);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, palette.text);

    visuals.widgets.hovered.bg_fill = palette.surface;
    visuals.widgets.hovered.weak_bg_fill = palette.surface;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, palette.accent);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, palette.text);

    visuals.widgets.active.bg_fill = palette.accent;
    visuals.widgets.active.weak_bg_fill = palette.accent;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, palette.accent);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, palette.accent_on);

    visuals.widgets.open.bg_fill = palette.surface_2;
    visuals.widgets.open.weak_bg_fill = palette.surface_2;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, palette.border);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, palette.text);

    visuals
}

/// Install both palettes, so that following the operating system (FR-9.2) is
/// a matter of egui switching between them rather than of us repainting.
pub fn install(ctx: &egui::Context) {
    ctx.set_visuals_of(egui::Theme::Light, visuals(LIGHT, false));
    ctx.set_visuals_of(egui::Theme::Dark, visuals(DARK, true));
}
