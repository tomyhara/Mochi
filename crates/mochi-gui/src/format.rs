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

//! Small text helpers shared by the panels.
//!
//! They live apart from the drawing code because they are the parts worth
//! testing: none of them needs a window, and all of them have an edge case
//! that only shows up on someone else's data.

/// Shorten a path from the middle.
///
/// The two ends of a path are the informative parts — which volume it is on
/// and which directory it is — so dropping the middle keeps more meaning than
/// an ellipsis at either end. The mock calls this a middle-elided path (M-7).
///
/// Counts characters rather than bytes: a path full of Japanese would
/// otherwise be cut to a third of the intended width, or panic on a boundary.
pub fn elide_middle(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max || max == 0 {
        return text.to_string();
    }
    let keep = max - 1;
    let head = keep.div_ceil(2);
    let tail = keep - head;
    let mut out: String = text.chars().take(head).collect();
    out.push('…');
    out.extend(text.chars().skip(count - tail));
    out
}

/// One line, at most `limit` characters.
pub fn one_line(text: &str, limit: usize) -> String {
    let cleaned: String = text
        .chars()
        .map(|c| {
            if c == '\n' || c == '\r' || c == '\t' {
                ' '
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.chars().count() <= limit {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(limit.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// A file size a person can read at a glance.
pub fn bytes(value: i64) -> String {
    if value < 0 {
        return "-".to_string();
    }
    let value = value as f64;
    if value < 1024.0 {
        return format!("{value:.0} B");
    }
    if value < 1024.0 * 1024.0 {
        return format!("{:.1} KB", value / 1024.0);
    }
    if value < 1024.0 * 1024.0 * 1024.0 {
        return format!("{:.1} MB", value / (1024.0 * 1024.0));
    }
    format!("{:.2} GB", value / (1024.0 * 1024.0 * 1024.0))
}

/// Thousands separators, written out rather than pulled from a crate: the rule
/// is six lines and the interface is English only (FR-9.3).
pub fn thousands(value: i64) -> String {
    let negative = value < 0;
    let digits = value.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    if negative {
        return format!("-{out}");
    }
    out
}

/// A timestamp in milliseconds since the epoch, as local wall-clock text.
///
/// `chrono` is already in the workspace for the command line, but the window
/// does not need it: a date is derived here from the epoch arithmetic directly
/// so that the window carries no date library of its own.
pub fn timestamp(milliseconds: Option<i64>) -> String {
    let Some(ms) = milliseconds else {
        return "—".to_string();
    };
    let (date, time) = civil_from_millis(ms);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        date.0, date.1, date.2, time.0, time.1, time.2
    )
}

/// Just the clock part, for the left margin of a transcript entry.
pub fn clock(milliseconds: Option<i64>) -> String {
    let Some(ms) = milliseconds else {
        return String::new();
    };
    let (_, time) = civil_from_millis(ms);
    format!("{:02}:{:02}:{:02}", time.0, time.1, time.2)
}

/// Civil date and time (UTC) from milliseconds since the epoch.
///
/// Howard Hinnant's `civil_from_days`, which is the algorithm every date
/// library uses underneath. UTC rather than local time: the tools write their
/// timestamps in UTC, and a window that quietly shifts them by the machine's
/// current offset would mislabel every session recorded on the other side of a
/// daylight-saving change.
fn civil_from_millis(ms: i64) -> ((i64, u32, u32), (u32, u32, u32)) {
    let seconds = ms.div_euclid(1000);
    let days = seconds.div_euclid(86_400);
    let rest = seconds.rem_euclid(86_400);

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if month <= 2 { year + 1 } else { year };

    (
        (year, month, day),
        (
            (rest / 3600) as u32,
            ((rest % 3600) / 60) as u32,
            (rest % 60) as u32,
        ),
    )
}
