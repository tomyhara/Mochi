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
| Node | 22 | only for the design-mock browser tests, never for the product |

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

## The window

```sh
cargo run -p mochi-cli                 # opens the window
cargo run -p mochi-cli -- doctor       # the same binary, as the command line tool
```

`mochi` is one executable: no subcommand opens the window, a subcommand prints.
That is the whole distribution story — there is nothing to install beside it,
no web view and no runtime (NFR-4b.7). Nothing extra is needed to build it on
any platform; `eframe` draws the widgets and reaches the GPU through `wgpu`
(Direct3D 12 on Windows, Metal on macOS, Vulkan or GL on Linux).

`crates/mochi-gui` is deliberately thin, and is arranged so that most of it can
be tested without a screen:

| File | What it is |
| --- | --- |
| `worker.rs` | the only thing that touches the index or the filesystem, on its own thread |
| `view.rs` | what the window draws, as plain data — grouping, filtering, resume state |
| `app.rs` | the panels: layout 1b, and nothing that decides anything |
| `theme.rs` | the palette, carried over from the web interface unchanged |
| `fonts.rs` | borrowing a CJK face from the operating system (NFR-6.4) |

Anything that decides something belongs in `mochi-core`.

The global flags work on the window too, which is how you look at something
other than your own sessions:

```sh
cargo run -p mochi-cli -- ui --db /tmp/scratch.sqlite3 \
  --home crates/mochi-core/tests/golden/claude_code/home
```

`MOCHI_FONT=/path/to/font.ttc` overrides the face the window borrows for
Japanese, Chinese and Korean text — useful for checking the fallback without
uninstalling anything.

## Testing the window

The interface is tested in Rust, with no screen involved:

```sh
cargo test -p mochi-gui
```

- `tests/window.rs` runs the real window headless — egui lays out and paints
  into a buffer, and only the last step needs a screen — against a real index,
  and reads the text that was painted. This is where the promise that a masked
  index never *draws* a credential is checked.
- `tests/worker.rs` drives the index thread over its channels.
- `tests/view.rs`, `tests/format.rs` cover the decisions the panels make.
- `tests/theme.rs` measures every colour pair against WCAG AA (NFR-6.2). The
  browser used to measure this on a rendered page; a native window has no such
  page, so the check moved to where the colours are decided.

If you add a panel, add the text it draws to `tests/window.rs`. A window that
is never run is a window that is never tested.

## Browser tests

One suite, Chromium, and it has nothing to do with the interface: it opens the
design mock in `doc/`. The mock is a generated bundle and FR-9.1 makes `1b` the
reference the implementation is built against, so regenerating it and losing an
agreed correction fails the build instead of passing quietly.

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

## Trying the indexer

The other half of the same executable: an indexer you can drive from a
terminal.

```sh
cargo run -p mochi-cli -- doctor              # which CLIs and stores were found
cargo run -p mochi-cli -- scan                # build/refresh the index
cargo run -p mochi-cli -- repos               # repositories, grouped
cargo run -p mochi-cli -- sessions --limit 20
cargo run -p mochi-cli -- search ECONNRESET
cargo run -p mochi-cli -- resume <session-id> # prints the command, never runs it
cargo run -p mochi-cli -- licenses            # attribution, carried inside the binary
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
