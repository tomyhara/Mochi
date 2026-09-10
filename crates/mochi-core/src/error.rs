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

//! Error type shared by the crate.
//!
//! Failures are per-session wherever possible: one unreadable file must never
//! stop the rest of the index (FR-2.7, NFR-2.2).

use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("could not read session {path}: {reason}")]
    Session { path: PathBuf, reason: String },

    #[error("index error: {0}")]
    Index(#[from] rusqlite::Error),

    #[error("{0}")]
    Invalid(String),
}

impl Error {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Error {
        Error::Io {
            path: path.into(),
            source,
        }
    }

    pub fn session(path: impl Into<PathBuf>, reason: impl Into<String>) -> Error {
        Error::Session {
            path: path.into(),
            reason: reason.into(),
        }
    }

    pub fn invalid(reason: impl Into<String>) -> Error {
        Error::Invalid(reason.into())
    }
}
