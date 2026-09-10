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
//!
//! The rules are deliberately shaped rather than statistical. An entropy
//! threshold would blank out commit hashes, base64 test data and minified
//! code, and a transcript full of holes is worse than no masking at all: the
//! user turns the feature off and is then unprotected. So each rule matches a
//! credential format that is recognisable on sight, plus one generic rule for
//! assignments whose *name* says the value is a secret.

use regex::Regex;

/// Prefix of the text that replaces a secret. Also how [`Masker::mask`]
/// recognises its own output, so masking twice is a no-op.
const PLACEHOLDER_PREFIX: &str = "[redacted:";

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
    re: Regex,
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
        // Order matters: the first rule to claim a span keeps it, so the most
        // specific ones come first. sk-ant- before sk-, and both before the
        // generic "a field called *_KEY" rule, so a finding names what was
        // actually found.
        let specs: &[(&'static str, &str, usize)] = &[
            (
                "private_key",
                r"(?s)-----BEGIN[A-Z ]*PRIVATE KEY-----.*?-----END[A-Z ]*PRIVATE KEY-----",
                0,
            ),
            ("anthropic_api_key", r"sk-ant-[A-Za-z0-9_-]{16,}", 0),
            ("openai_api_key", r"sk-(?:proj-)?[A-Za-z0-9_-]{16,}", 0),
            ("github_pat", r"github_pat_[A-Za-z0-9_]{20,}", 0),
            ("github_token", r"gh[pousr]_[A-Za-z0-9]{20,}", 0),
            (
                "aws_secret_access_key",
                r#"(?i)aws_secret_access_key\s*[=:]\s*["']?([A-Za-z0-9/+=]{40})"#,
                1,
            ),
            (
                "aws_access_key_id",
                r"\b(?:A3T[A-Z0-9]|AKIA|ASIA|ABIA|ACCA)[A-Z0-9]{16}\b",
                0,
            ),
            ("google_api_key", r"AIza[A-Za-z0-9_-]{35}", 0),
            ("slack_token", r"xox[baprs]-[A-Za-z0-9-]{10,}", 0),
            (
                "jwt",
                r"eyJ[A-Za-z0-9_-]{8,}\.eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}",
                0,
            ),
            // Keep the scheme visible: knowing it was an Authorization header
            // is useful, and the header name is not the secret.
            ("bearer_token", r"(?i)bearer\s+([A-Za-z0-9._~+/=-]{16,})", 1),
            // user:password@host in any URL.
            (
                "url_credentials",
                r"[a-zA-Z][a-zA-Z0-9+.-]*://[^\s:/@]+:([^\s@/]+)@",
                1,
            ),
            // The long tail. Requires the field name to say it holds a secret,
            // which is what keeps ordinary assignments out of it.
            (
                "secret_assignment",
                r#"(?i)[A-Za-z0-9_.-]*(?:api[_-]?key|secret|token|password|passwd|pwd|credential)[A-Za-z0-9_.-]*\s*[:=]\s*["']?([^\s"']{12,})"#,
                1,
            ),
        ];

        let rules = specs
            .iter()
            .map(|(kind, pattern, group)| Rule {
                kind,
                re: Regex::new(pattern).expect("masking rule must compile"),
                group: *group,
            })
            .collect();

        Masker { rules }
    }

    /// Replace every secret with a placeholder that names the rule, so the
    /// reader can tell what was removed without seeing it.
    pub fn mask(&self, input: &str) -> Masked {
        let spans = self.spans(input, false);
        if spans.is_empty() {
            return Masked {
                text: input.to_string(),
                findings: Vec::new(),
            };
        }

        let mut text = String::with_capacity(input.len());
        let mut cursor = 0usize;
        for finding in &spans {
            text.push_str(&input[cursor..finding.start]);
            text.push_str(PLACEHOLDER_PREFIX);
            text.push_str(finding.kind);
            text.push(']');
            cursor = finding.end;
        }
        text.push_str(&input[cursor..]);

        Masked {
            text,
            findings: spans,
        }
    }

    pub fn has_secret(&self, input: &str) -> bool {
        !self.spans(input, true).is_empty()
    }

    /// Non-overlapping spans to hide, in the order they appear in `input`.
    fn spans(&self, input: &str, stop_at_first: bool) -> Vec<Finding> {
        let mut found: Vec<Finding> = Vec::new();

        for rule in &self.rules {
            for caps in rule.re.captures_iter(input) {
                let Some(m) = caps.get(rule.group).or_else(|| caps.get(0)) else {
                    continue;
                };
                // Already-masked text: masking twice must not nest.
                if input[m.start()..m.end()].starts_with(PLACEHOLDER_PREFIX) {
                    continue;
                }
                // A more specific rule already claimed this text.
                if found.iter().any(|f| m.start() < f.end && f.start < m.end()) {
                    continue;
                }
                found.push(Finding {
                    kind: rule.kind,
                    start: m.start(),
                    end: m.end(),
                });
                if stop_at_first {
                    return found;
                }
            }
        }

        found.sort_by_key(|f| f.start);
        found
    }
}
