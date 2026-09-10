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

//! Mochi's core: find agent CLI sessions, read them, group them by repository,
//! index them and search them.
//!
//! The crate is deliberately free of UI and of any network code. Everything a
//! front end needs is reachable from [`index::Index`] and [`scan::Scanner`].
//!
//! Requirement identifiers such as `FR-3.3` and `NFR-6.4` in these docs refer
//! to `doc/requirements.md`.

pub mod adapter;
pub mod command;
pub mod error;
pub mod index;
pub mod mask;
pub mod model;
pub mod paths;
pub mod repo;
pub mod scan;

pub use error::{Error, Result};
pub use model::{Message, ParseStatus, ParsedSession, Role, SessionRef, ToolId};
