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

//! Shared helpers for the integration tests.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// `crates/mochi-core/tests/golden`.
pub fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

/// The pre-built home directory for one tool's fixtures.
pub fn golden_home(tool: &str) -> PathBuf {
    golden_dir().join(tool).join("home")
}

pub fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// Map of relative path -> content hash for every file under `root`.
///
/// Used to prove that reading a session never writes to it (FR-2.5, NFR-3.5,
/// and item 9 of the MVP definition of done).
pub fn fingerprint(root: &Path) -> BTreeMap<String, (u64, u64)> {
    let mut out = BTreeMap::new();
    collect(root, root, &mut out);
    out
}

fn collect(root: &Path, dir: &Path, out: &mut BTreeMap<String, (u64, u64)>) {
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if entry.file_type().unwrap().is_dir() {
            collect(root, &path, out);
        } else {
            let bytes = fs::read(&path).unwrap();
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.insert(rel, (bytes.len() as u64, fnv1a(&bytes)));
        }
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Whether a usable `git` is on PATH. Tests that need one skip without it
/// rather than failing on a machine that has none.
pub fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Create a git repository with one commit and, optionally, an origin.
pub fn init_repo(dir: &Path, remote: Option<&str>) -> String {
    fs::create_dir_all(dir).unwrap();
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    git(&["init", "--initial-branch=main"]);
    git(&["config", "user.email", "tests@example.invalid"]);
    git(&["config", "user.name", "Mochi Tests"]);
    git(&["config", "commit.gpgsign", "false"]);
    fs::write(dir.join("README.md"), "fixture\n").unwrap();
    git(&["add", "README.md"]);
    git(&["commit", "-m", "initial"]);
    if let Some(url) = remote {
        git(&["remote", "add", "origin", url]);
    }
    git(&["rev-list", "--max-parents=0", "HEAD"])
}
