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

//! Secret masking (NFR-3.3, R-3).
//!
//! Session logs are written by tools that print whatever a command produced,
//! `.env` files and `Authorization` headers included. Anything Mochi shows or
//! exports goes through here first.

/// One masked span of the input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Stable name of the rule that matched, e.g. `github_token`.
    pub kind: &'static str,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Masked {
    pub text: String,
    pub findings: Vec<Finding>,
}

impl Masked {
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }
}

/// Compiled rule set. Building one costs real time, so share it.
#[derive(Debug)]
pub struct Masker {
    rules: Vec<Rule>,
}

#[derive(Debug)]
struct Rule {
    kind: &'static str,
    re: regex::Regex,
    /// Which capture group holds the part to hide. 0 means the whole match.
    group: usize,
}

impl Default for Masker {
    fn default() -> Self {
        Self::new()
    }
}

impl Masker {
    pub fn new() -> Masker {
        todo!()
    }

    /// Replace every secret with a placeholder that names the rule, so the
    /// reader can tell what was removed without seeing it.
    pub fn mask(&self, input: &str) -> Masked {
        todo!()
    }

    pub fn has_secret(&self, input: &str) -> bool {
        todo!()
    }
}
