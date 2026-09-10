# Contributing to Mochi

Thanks for helping out. This document covers how to build, test and submit
changes. The product requirements live in [`doc/requirements.md`](doc/requirements.md);
please read the sections relevant to your change before starting.

## Licence and contributions

Mochi is licensed under the **Apache License 2.0**. Under section 5 of that
licence, any contribution you intentionally submit for inclusion is licensed
under the same terms. **We do not ask for a separate CLA** (NFR-4b.1c).

Every source file carries the Apache-2.0 boilerplate header. CI rejects files
without it — `scripts/check-license-headers.sh` tells you which ones.

## Prerequisites

| Tool | Version | Notes |
| --- | --- | --- |
| Rust | see `rust-toolchain.toml` | `rustup` picks this up automatically |
| A C toolchain | any | needed to build the bundled SQLite |
| Git | 2.30+ | used to resolve repository identity at runtime |

No network access is needed at runtime, and none should ever be added
(NFR-3.1, NFR-4b.8).

## Build and test

```sh
cargo build --workspace
cargo test  --workspace
```

Before pushing, run what CI runs:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./scripts/check-license-headers.sh
./scripts/check-golden-secrets.sh
```

`cargo deny check` (licence and advisory policy, NFR-4b.2 / NFR-3.9) needs
`cargo install cargo-deny`; CI installs it for you, so it is optional locally.

## Browser tests for the mockups

The UI mockups in `doc/` are a generated bundle, and FR-9.1 makes `1b` the
reference the implementation is built against. `e2e/ui-mocks.spec.ts` opens the
document in Chromium and asserts the decisions recorded in `doc/ui-spec.md` and
`doc/CHANGELOG-ui.md` — so regenerating the mock and losing one of them fails
the build instead of passing quietly.

```sh
npm ci
npx playwright install chromium   # skip if your environment ships one
npx playwright test
./scripts/check-npm-licenses.sh
```

If Chromium is already on the machine, point the tests at it instead of
downloading a second copy:

```sh
MOCHI_CHROMIUM_PATH=/path/to/chromium npx playwright test
```

The assertions read the *rendered text*, not the DOM. The document's elements
and class names are generated and are not ours to depend on; the words on the
screen are what the specification actually decided. When you change a mock,
expect to change the matching assertion in the same commit — that is the point
of it.

This is also where the application's own end-to-end tests will live once there
is a window to open.

## Trying the indexer

`mochi-cli` is the milestone-1 deliverable: an indexer you can drive from a
terminal.

```sh
cargo run -p mochi-cli -- doctor              # which CLIs and stores were found
cargo run -p mochi-cli -- scan                # build/refresh the index
cargo run -p mochi-cli -- repos               # repositories, grouped
cargo run -p mochi-cli -- sessions --limit 20
cargo run -p mochi-cli -- search ECONNRESET
cargo run -p mochi-cli -- resume <session-id> # prints the command, never runs it
```

Use `--db <path>` to keep experiments out of your real index, and
`--home <path>` to point the adapters at a fixture tree instead of your
home directory.

## Golden files (NFR-5.2, NFR-4b.4)

Parser regression tests run against fixture sessions in
`crates/mochi-core/tests/golden/<tool>/`. When you add support for a new CLI
version, add a sample there.

**Never commit a raw session file.** Before adding a fixture:

1. replace every API key, token and cookie with an obvious placeholder
2. replace real paths and user names (`/Users/you/...`, `/home/user/...`)
3. remove work-specific content — file names, hostnames, ticket IDs
4. run `./scripts/check-golden-secrets.sh`

The scan is a safety net, not a substitute for reading the file yourself.

## Working on adapters

Each CLI lives behind `ToolAdapter` in `crates/mochi-core/src/adapter/`
(NFR-5.1). Keep tool-specific knowledge inside its adapter module — the point
of the split is that a breaking change in one CLI's private format can only
break one file.

Session formats are **undocumented internals that change without notice**
(R-1). Parsers must therefore:

- never panic on unexpected input; return a per-session error instead (FR-2.7)
- keep the raw JSON for anything they do not understand (FR-2.8)
- skip an incomplete trailing line rather than failing the file (FR-2.6)
- open source files read-only and never write to them (FR-2.5, NFR-3.5)

## Pull requests

- one logical change per PR; keep the diff reviewable
- include tests — a bug fix without a regression test will be asked for one
- reference the requirement IDs your change implements (`FR-3.3`, `NFR-6.4`, …)
- CI must be green on Windows, macOS and Linux (NFR-5.4)

## Code of conduct

Be decent to each other. Harassment or personal attacks are not welcome here.
