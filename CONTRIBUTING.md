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

## The desktop application

```sh
npm ci
npx tauri dev   --config crates/mochi-desktop/tauri.conf.json   # window + hot reload
npx tauri build --config crates/mochi-desktop/tauri.conf.json   # installers
```

On Linux you need the webview headers first:

```sh
sudo apt-get install -y libwebkit2gtk-4.1-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev
```

`crates/mochi-desktop` is deliberately thin: it opens the index, hands the
window the same document `mochi export` produces, and gets out of the way.
Anything that decides something belongs in `mochi-core`.

Note that `cargo build -p mochi-desktop` produces a binary that looks for the
dev server — Tauri picks `devUrl` over the bundled interface unless the build
goes through its own CLI. Use `npx tauri build` when you want the real thing.

The application version comes from the workspace `Cargo.toml`;
`tauri.conf.json` deliberately has no `version` field so the installer and
`mochi --version` cannot disagree. CI enforces that.

## The interface

```sh
npm ci
npm run dev          # http://localhost:5173, fixture data, no shell needed
npm run typecheck
```

`ui/` is React and TypeScript, no framework beyond that. It reads through
`DataSource` (`ui/src/data/`): the fixture implementation in development and in
tests, and a desktop implementation that will call the Rust core once there is a
shell to call it from. Which shell — Tauri or Electron — is R-10, decided by the
milestone-0 terminal spike; writing the interface against that one interface is
what keeps the answer from mattering here.

The fixture is not hand-written. `./scripts/build-ui-fixture.sh` runs the real
scanner over the golden session files and exports the result, so the interface
is always rendering a shape the core actually produces. CI regenerates it and
fails if the committed copy has drifted. If you change what `mochi export`
emits, regenerate and commit in the same change.

## Browser tests

Three suites, all Chromium:

- `e2e/ui.spec.ts` drives the interface itself.
- `e2e/ui-contrast.spec.ts` measures every rendered string against WCAG AA in
  both themes (NFR-6.2).
- `e2e/ui-mocks.spec.ts` opens the design mock in `doc/`. It is a generated
  bundle and FR-9.1 makes `1b` the reference the implementation is built
  against, so regenerating it and losing an agreed correction fails the build
  instead of passing quietly.

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

## Cutting a release

Releases are built by CI on real Windows and macOS runners — there is no way to
produce a `.exe` or a `.dmg` from a Linux machine, and cross-compiling the
bundled SQLite is not worth the risk of shipping something nobody ran.

1. Set the version in the workspace `Cargo.toml` and commit the `Cargo.lock`
   that comes with it.
2. Tag it: `git tag v<version> && git push origin v<version>`.
3. `.github/workflows/release.yml` runs from the tag, refuses to continue if
   the tag and `Cargo.toml` disagree, runs the tests in release mode, builds a
   Windows binary and a universal macOS binary, and attaches them to a **draft**
   release with a `SHA256SUMS.txt`.
4. Read the draft, then publish it yourself.

Nothing is signed or notarised yet (R-7), so the notes tell people how to get
past SmartScreen and Gatekeeper and how to check the binary against the
checksums. Keep that section honest as long as it is true.

## Pull requests

- one logical change per PR; keep the diff reviewable
- include tests — a bug fix without a regression test will be asked for one
- reference the requirement IDs your change implements (`FR-3.3`, `NFR-6.4`, …)
- CI must be green on Windows, macOS and Linux (NFR-5.4)

## Code of conduct

Be decent to each other. Harassment or personal attacks are not welcome here.
