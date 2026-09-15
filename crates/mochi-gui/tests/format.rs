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

//! The text helpers, which are where the window's edge cases live.

use mochi_gui::format::{
    bytes, clock, date, elide_middle, hour_minute, now_ms, one_line, relative, thousands, timestamp,
};

#[test]
fn a_short_path_is_left_alone() {
    assert_eq!(elide_middle("/home/you/code", 44), "/home/you/code");
}

#[test]
fn a_long_path_loses_its_middle_and_keeps_both_ends() {
    let path = "/Users/you/code/some/very/deeply/nested/place/repo";
    let short = elide_middle(path, 20);
    assert_eq!(short.chars().count(), 20);
    assert!(short.starts_with("/Users/you"));
    assert!(short.ends_with("repo"));
    assert!(short.contains('…'));
}

/// The reason this counts characters rather than bytes: a Japanese path was
/// otherwise cut to a third of the width asked for, or panicked mid-character.
#[test]
fn eliding_counts_characters_not_bytes() {
    let path = "/Users/you/コード/プロジェクト/リポジトリ/セッション";
    let short = elide_middle(path, 12);
    assert_eq!(short.chars().count(), 12);
}

#[test]
fn one_line_flattens_and_trims() {
    assert_eq!(one_line("  a\nb\tc  ", 40), "a b c");
    assert_eq!(one_line("あいうえおかきくけこ", 5), "あいうえ…");
}

#[test]
fn sizes_are_readable() {
    assert_eq!(bytes(512), "512 B");
    assert_eq!(bytes(2048), "2.0 KB");
    assert_eq!(bytes(5 * 1024 * 1024), "5.0 MB");
    assert_eq!(bytes(3 * 1024 * 1024 * 1024), "3.00 GB");
    // A size the index never has, rather than a panic.
    assert_eq!(bytes(-1), "-");
}

#[test]
fn numbers_get_separators() {
    assert_eq!(thousands(0), "0");
    assert_eq!(thousands(999), "999");
    assert_eq!(thousands(1_000), "1,000");
    assert_eq!(thousands(1_234_567), "1,234,567");
    assert_eq!(thousands(-4_000), "-4,000");
}

#[test]
fn timestamps_are_utc_wall_clock() {
    // 2026-09-11T01:34:56Z
    assert_eq!(timestamp(Some(1_789_090_496_000)), "2026-09-11 01:34:56");
    assert_eq!(clock(Some(1_789_090_496_000)), "01:34:56");
}

#[test]
fn the_epoch_and_before_it_do_not_panic() {
    assert_eq!(timestamp(Some(0)), "1970-01-01 00:00:00");
    assert_eq!(timestamp(Some(-1)), "1969-12-31 23:59:59");
}

/// A session with no recorded time is normal — half the formats do not write
/// one — so it has to read as absent rather than as 1970.
#[test]
fn a_missing_time_says_so() {
    assert_eq!(timestamp(None), "—");
    assert_eq!(clock(None), "");
}

/// The repository rows say how long ago rather than when, because on that row
/// the point is which one you touched last.
#[test]
fn how_long_ago_is_said_in_the_largest_unit_that_is_still_true() {
    // 2026-09-11T01:34:56Z, and a clock to measure it from.
    let now = 1_789_090_496_000;
    let ago = |seconds: i64| relative(Some(now - seconds * 1000), now);

    assert_eq!(ago(0), "just now");
    assert_eq!(ago(59), "just now");
    assert_eq!(ago(60), "1m ago");
    assert_eq!(ago(3_599), "59m ago");
    assert_eq!(ago(3_600), "1h ago");
    assert_eq!(ago(86_400), "1d ago");
    assert_eq!(ago(29 * 86_400), "29d ago");
    // Past a month, how long ago stops meaning anything and the date is what
    // a reader can actually place.
    assert_eq!(ago(60 * 86_400), "2026-07-13");
}

/// A clock that has run ahead, or a session written a second into the future,
/// must not come out as a negative age.
#[test]
fn a_time_in_the_future_is_not_counted_backwards() {
    let now = 1_789_090_496_000;
    assert_eq!(relative(Some(now + 86_400_000), now), "just now");
}

#[test]
fn the_date_and_the_clock_can_be_had_separately() {
    // 2026-09-11T01:34:56Z
    assert_eq!(date(Some(1_789_090_496_000)), "2026-09-11");
    assert_eq!(hour_minute(Some(1_789_090_496_000)), "01:34");
    assert_eq!(date(None), "—");
    assert_eq!(hour_minute(None), "—");
    assert_eq!(relative(None, 0), "—");
}

/// The one impure helper: it has to be a plausible moment in this century,
/// because every dated heading in the session pane is measured from it.
#[test]
fn the_clock_reads_the_present() {
    // 2020-01-01, comfortably before this code was written.
    assert!(now_ms() > 1_577_836_800_000);
}
