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
//! Every fixture below is a placeholder. The rule the tests enforce is simple:
//! after masking, the secret must not appear in the output, and ordinary text
//! must come through untouched.

use mochi_core::mask::Masker;

fn mask(input: &str) -> String {
    Masker::new().mask(input).text
}

fn kinds(input: &str) -> Vec<&'static str> {
    let mut k: Vec<_> = Masker::new()
        .mask(input)
        .findings
        .into_iter()
        .map(|f| f.kind)
        .collect();
    k.sort_unstable();
    k.dedup();
    k
}

#[test]
fn openai_style_keys_are_masked() {
    let text = "OPENAI_API_KEY=sk-EXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAMPLEEX";
    let out = mask(text);
    assert!(!out.contains("sk-EXAMPLEEXAMPLE"), "got {out}");
    assert!(
        out.contains("OPENAI_API_KEY="),
        "the key name is useful context: {out}"
    );
    assert_eq!(kinds(text), vec!["openai_api_key"]);
}

#[test]
fn anthropic_keys_are_masked() {
    let text = "export ANTHROPIC_API_KEY=sk-ant-api03-EXAMPLEEXAMPLEEXAMPLEEXAMPLE";
    assert!(!mask(text).contains("sk-ant-api03-EXAMPLE"));
    assert_eq!(kinds(text), vec!["anthropic_api_key"]);
}

#[test]
fn github_tokens_are_masked() {
    for (text, kind) in [
        ("ghp_EXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAM01", "github_token"),
        ("gho_EXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAM01", "github_token"),
        (
            "github_pat_EXAMPLEEXAMPLE_EXAMPLEEXAMPLEEXAMPLE",
            "github_pat",
        ),
    ] {
        let out = mask(text);
        assert!(!out.contains("EXAMPLEEXAMPLE"), "{text} -> {out}");
        assert!(kinds(text).contains(&kind), "{text} -> {:?}", kinds(text));
    }
}

#[test]
fn aws_credentials_are_masked() {
    let text = "AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE\naws_secret_access_key = wJalrXUtnFEMIK7MDENGbPxRfiCYEXAMPLEEXAMPLE";
    let out = mask(text);
    assert!(!out.contains("AKIAIOSFODNN7EXAMPLE"), "got {out}");
    assert!(!out.contains("wJalrXUtnFEMIK7MDENG"), "got {out}");
}

#[test]
fn bearer_headers_are_masked_but_the_scheme_stays() {
    let text = "curl -H 'Authorization: Bearer EXAMPLEtokenEXAMPLEtokenEXAMPLEtoken' https://api.internal/v1";
    let out = mask(text);
    assert!(
        out.contains("Bearer"),
        "the reader should still see what kind of header it was: {out}"
    );
    assert!(!out.contains("EXAMPLEtokenEXAMPLE"), "got {out}");
    assert!(
        out.contains("https://api.internal/v1"),
        "the url is not a secret: {out}"
    );
}

#[test]
fn slack_tokens_and_jwts_are_masked() {
    let slack = "xoxb-EXAMPLE-NOT-A-REAL-TOKEN-EXAMPLE";
    assert!(!mask(slack).contains("EXAMPLEEXAMPLE"));

    let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJFWEFNUExFIn0.EXAMPLEsignatureEXAMPLE";
    let out = mask(jwt);
    assert!(!out.contains("EXAMPLEsignature"), "got {out}");
}

#[test]
fn private_key_blocks_are_masked_whole() {
    let text = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaEXAMPLEEXAMPLE\nEXAMPLEEXAMPLEbody\n-----END OPENSSH PRIVATE KEY-----";
    let out = mask(text);
    assert!(!out.contains("b3BlbnNza"), "got {out}");
    assert!(
        !out.contains("EXAMPLEbody"),
        "the body must go, not just the header: {out}"
    );
    assert_eq!(kinds(text), vec!["private_key"]);
}

#[test]
fn credentials_inside_urls_are_masked() {
    let text = "DATABASE_URL=postgres://app:hunter2EXAMPLE@db.internal:5432/app";
    let out = mask(text);
    assert!(!out.contains("hunter2EXAMPLE"), "got {out}");
    assert!(
        out.contains("db.internal"),
        "the host is not the secret: {out}"
    );
}

#[test]
fn secret_looking_assignments_are_masked() {
    // A generic rule for the long tail: a name that says "secret" plus a value
    // long enough to be one.
    for text in [
        "MY_SERVICE_TOKEN=Zm9vYmFyEXAMPLEbazquxEXAMPLEabcdef123456",
        "database_password: EXAMPLEEXAMPLEEXAMPLEpassword123",
        "client_secret=\"EXAMPLEEXAMPLEEXAMPLEEXAMPLEsecret\"",
    ] {
        let out = mask(text);
        assert!(
            !out.contains("EXAMPLEEXAMPLE") && !out.contains("Zm9vYmFyEXAMPLE"),
            "{text} -> {out}"
        );
    }
}

#[test]
fn ordinary_text_is_left_completely_alone() {
    // False positives are expensive: a transcript full of blanked-out words is
    // worse than no masking at all.
    for text in [
        "The retry helper returns a Result and logs the attempt count.",
        "commit 9b1f3c2d4e5f60718293a4b5c6d7e8f901234567 broke the nightly build",
        "docker image ls --format '{{.Size}}' app:latest",
        "https://github.com/example/my-repo/pull/42",
        "fn main() { println!(\"hello\"); }",
        "接続がリセットされました。リトライ間隔を 2 秒にします。",
        "path=/Users/you/code/my-repo/src/fetch.ts line=118",
    ] {
        let masked = Masker::new().mask(text);
        assert!(
            masked.is_clean(),
            "false positive on {text:?}: {:?}",
            masked.findings
        );
        assert_eq!(masked.text, text);
    }
}

#[test]
fn masking_is_idempotent() {
    let text = "key=sk-EXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAMPLEEX and more text";
    let once = mask(text);
    assert_eq!(mask(&once), once, "re-masking must not eat the placeholder");
}

#[test]
fn findings_point_at_the_input() {
    let text = "prefix sk-EXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAMPLEEX suffix";
    let result = Masker::new().mask(text);
    assert_eq!(result.findings.len(), 1);
    let f = &result.findings[0];
    assert!(
        text[f.start..f.end].starts_with("sk-"),
        "span was {:?}",
        &text[f.start..f.end]
    );
}

#[test]
fn multibyte_text_around_a_secret_is_preserved() {
    // NFR-6.4: masking must be safe on character boundaries, not byte offsets
    // into a Japanese string.
    let text =
        "設定ファイルの中身: OPENAI_API_KEY=sk-EXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAMPLEEX でした。";
    let out = mask(text);
    assert!(out.starts_with("設定ファイルの中身: "), "got {out}");
    assert!(out.ends_with(" でした。"), "got {out}");
    assert!(!out.contains("sk-EXAMPLE"), "got {out}");
}

#[test]
fn several_secrets_in_one_blob_all_go() {
    let text = "OPENAI_API_KEY=sk-EXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAMPLEEX\nGITHUB_TOKEN=ghp_EXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAM\nAWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE\n";
    let result = Masker::new().mask(text);
    assert!(result.findings.len() >= 3, "found {:?}", result.findings);
    assert!(!result.text.contains("sk-EXAMPLE"));
    assert!(!result.text.contains("ghp_EXAMPLE"));
    assert!(!result.text.contains("AKIAIOSFODNN7EXAMPLE"));
    assert_eq!(
        result.text.lines().count(),
        3,
        "line structure must survive: {}",
        result.text
    );
}

#[test]
fn has_secret_agrees_with_mask() {
    let masker = Masker::new();
    assert!(masker.has_secret("ghp_EXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAM"));
    assert!(!masker.has_secret("just a sentence about tokens and secrets"));
}

#[test]
fn empty_and_huge_inputs_are_safe() {
    assert_eq!(mask(""), "");
    let big = "line of ordinary text\n".repeat(20_000);
    assert_eq!(mask(&big).len(), big.len());
}
