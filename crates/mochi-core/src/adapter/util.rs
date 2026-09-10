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

//! Helpers shared by the adapters.
//!
//! Reading a session must never disturb the CLI that owns it. Files are opened
//! read-only and shared, which on Windows matters: Rust's `File::open` asks
//! for `FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE`, so a running
//! agent can keep appending while Mochi reads (NFR-2.5, FR-2.5).

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::time::UNIX_EPOCH;

use serde_json::Value;

use crate::model::{SessionRef, ToolId};
use crate::{Error, Result};

/// One line of a JSONL file, already classified.
pub enum Line {
    Parsed(Value),
    /// The line is not valid JSON. `complete` says whether it ended with a
    /// newline: a half-written final line is the CLI still appending
    /// (FR-2.6), while a broken line in the middle is corruption.
    Broken {
        complete: bool,
    },
}

/// Read a JSONL file into classified lines.
///
/// The whole file is read at once. Sessions are appended to over days but stay
/// in the megabytes; the streaming path that NFR-1.4 asks for belongs with the
/// viewer, which pages a single large transcript, not with indexing.
pub fn read_jsonl(path: &Path) -> Result<Vec<Line>> {
    let file = File::open(path).map_err(|e| Error::io(path, e))?;
    let mut reader = BufReader::new(file);

    let mut buffer = Vec::new();
    reader
        .read_to_end(&mut buffer)
        .map_err(|e| Error::io(path, e))?;
    let ends_with_newline = buffer.last().is_some_and(|b| *b == b'\n' || *b == b'\r');

    let text = String::from_utf8_lossy(&buffer);
    let raw_lines: Vec<&str> = text.lines().collect();
    let last = raw_lines.len().saturating_sub(1);

    let mut out = Vec::with_capacity(raw_lines.len());
    for (index, line) in raw_lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(trimmed) {
            Ok(value) => out.push(Line::Parsed(value)),
            Err(_) => out.push(Line::Broken {
                complete: index != last || ends_with_newline,
            }),
        }
    }
    Ok(out)
}

/// Read and parse a single JSON file.
pub fn read_json(path: &Path) -> Result<Value> {
    let file = File::open(path).map_err(|e| Error::io(path, e))?;
    let reader = BufReader::new(file);
    serde_json::from_reader(reader).map_err(|e| Error::session(path, e.to_string()))
}

/// Size and modification time, which is what incremental scanning compares
/// (FR-2.3).
pub fn file_stats(path: &Path) -> Result<(u64, i64)> {
    let meta = std::fs::metadata(path).map_err(|e| Error::io(path, e))?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    Ok((meta.len(), mtime))
}

pub fn session_ref(tool: ToolId, path: &Path) -> Result<SessionRef> {
    let (size, mtime) = file_stats(path)?;
    Ok(SessionRef {
        tool,
        source_path: path.to_path_buf(),
        source_size: size,
        source_mtime: mtime,
    })
}

/// Parse a timestamp into milliseconds since the Unix epoch.
///
/// Accepts RFC 3339 text and bare epoch numbers, because the three tools do
/// not agree and individual versions do not either.
pub fn timestamp_ms(value: &Value) -> Option<i64> {
    match value {
        Value::String(text) => parse_timestamp(text),
        Value::Number(number) => number.as_i64().map(normalise_epoch),
        _ => None,
    }
}

pub fn parse_timestamp(text: &str) -> Option<i64> {
    if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(text) {
        return Some(parsed.timestamp_millis());
    }
    text.parse::<i64>().ok().map(normalise_epoch)
}

/// Seconds and milliseconds both turn up. Anything below this threshold is
/// seconds; the cutoff is the year 2001 in milliseconds, and no session
/// predates the tools by decades.
fn normalise_epoch(value: i64) -> i64 {
    if value.abs() < 100_000_000_000 {
        value * 1000
    } else {
        value
    }
}

/// Text of a message body, whether it is a plain string or a list of blocks.
pub fn flatten_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .map(flatten_text)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(map) => {
            for key in ["text", "content", "output", "summary", "thinking"] {
                if let Some(inner) = map.get(key) {
                    let text = flatten_text(inner);
                    if !text.is_empty() {
                        return text;
                    }
                }
            }
            String::new()
        }
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Compact JSON, used to make tool arguments searchable (FR-6.1) without
/// pretending Mochi understands every tool's schema.
pub fn compact_json(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// First non-empty string found at any of `keys`.
pub fn first_string<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .filter_map(|key| value.get(*key))
        .filter_map(Value::as_str)
        .find(|text| !text.trim().is_empty())
}

/// A one-line title made from the first prompt (FR-4.3).
pub fn title_from_prompt(text: &str) -> String {
    const MAX: usize = 120;
    let single_line = text
        .split('\n')
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    if single_line.chars().count() <= MAX {
        return single_line.to_string();
    }
    let truncated: String = single_line.chars().take(MAX).collect();
    format!("{truncated}…")
}
