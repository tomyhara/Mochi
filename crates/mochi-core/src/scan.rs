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

//! Turning what is on disk into index rows (FR-2.3, FR-2.9, FR-3.x).

use std::path::PathBuf;

use crate::adapter::{EnvSource, ToolAdapter};
use crate::index::Index;
use crate::model::ToolId;
use crate::repo::GitProbe;
use crate::Result;

/// What one scan did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScanReport {
    pub discovered: usize,
    /// Sessions read because they were new or had changed.
    pub parsed: usize,
    /// Sessions left alone because size and mtime matched the index (FR-2.3).
    pub unchanged: usize,
    /// Sessions whose source file has disappeared since the last scan.
    pub archived: usize,
    /// Sessions that could not be read at all. Isolated, never fatal.
    pub failed: usize,
    pub repositories: usize,
    pub errors: Vec<ScanError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanError {
    pub tool: ToolId,
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct ScanOptions {
    /// Only these tools, or all of them when empty (FR-1.5).
    pub tools: Vec<ToolId>,
    /// Re-read every session even if it looks unchanged (FR-2.10).
    pub force: bool,
    /// Group worktrees and second clones with their origin (FR-3.4).
    pub bundle_clones: bool,
}

impl Default for ScanOptions {
    fn default() -> Self {
        ScanOptions {
            tools: Vec::new(),
            force: false,
            bundle_clones: true,
        }
    }
}

/// Runs a scan against an index.
pub struct Scanner<'a> {
    adapters: Vec<Box<dyn ToolAdapter>>,
    env: EnvSource,
    probe: &'a dyn GitProbe,
    options: ScanOptions,
}

impl<'a> Scanner<'a> {
    pub fn new(env: EnvSource, probe: &'a dyn GitProbe, options: ScanOptions) -> Scanner<'a> {
        todo!()
    }

    /// Discover, parse what changed, resolve repositories, write rows.
    ///
    /// A failure on one session is recorded in the report and the scan carries
    /// on (FR-2.7, NFR-2.2).
    pub fn run(&self, index: &mut Index) -> Result<ScanReport> {
        todo!()
    }
}
