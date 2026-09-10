# Mochi

**Mochi** collects the sessions that [Codex CLI](https://github.com/openai/codex),
[Claude Code](https://claude.com/claude-code) and [OpenCode](https://opencode.ai)
leave on your machine, groups them **by repository**, and makes them readable,
searchable and resumable from one place.

Each of those tools keeps a full transcript of every session — the conversation,
the commands it ran, the output it saw. Each keeps it somewhere different, in a
different shape, under a different id. So the history exists, but you cannot use
it: you cannot see what you did in a repository last month, you cannot search
across tools for the error you already solved once, and at least one of them
deletes its transcripts after thirty days without telling you.

Mochi is a desktop application for Windows and macOS that fixes that. It works
**entirely offline** and never writes to the original session files.

> **Status: early.** The core — discovery, parsing, repository grouping,
> indexing, search, masking and command building — works and is covered by
> tests. There is no graphical application yet; today the way in is
> `mochi`, the command line indexer. See [Where this is](#where-this-is).

## What it does

- **Groups by repository.** Sessions are matched to a repository by first
  commit, then remote URL, then path — so a clone that moved, a second checkout
  and a `git worktree` still land under one entry.
- **Reads all three tools.** Each CLI sits behind its own adapter, pinned by
  golden files, because these storage formats are private internals that change
  without notice.
- **Searches everything at once.** One full-text index over every message from
  every tool, including substring search inside Japanese and other text without
  spaces.
- **Keeps history the tools throw away.** When a CLI deletes its own transcript,
  Mochi's copy stays readable and searchable, marked as archived.
- **Hides credentials.** Session logs routinely contain API keys and `.env`
  contents. Anything shown or exported is masked by default.
- **Never modifies your sessions.** Files are opened read-only. A test compares
  the bytes of every fixture before and after a full scan.
- **Never talks to the network.** There is no HTTP client and no telemetry in
  the dependency graph, and CI fails if one appears.

## Try the indexer

Requires a [Rust toolchain](https://rustup.rs) and a C compiler for the bundled
SQLite.

```sh
git clone https://github.com/tomyhara/Mochi
cd Mochi
cargo build --release

./target/release/mochi doctor          # which CLIs and stores were found
./target/release/mochi scan            # build or refresh the index
./target/release/mochi repos           # repositories, with session counts
./target/release/mochi sessions        # recent sessions
./target/release/mochi search ECONNRESET
./target/release/mochi resume 42       # prints the command; never runs it
```

Useful flags: `--db <path>` to keep an experiment away from your real index,
`--home <path>` to point at a fixture tree instead of your home directory, and
`--no-mask` to see credentials unmasked.

The index goes to `%APPDATA%\Mochi\` on Windows and
`~/Library/Application Support/Mochi/` on macOS.

## Try the interface

The window is layout `1b` from the mockups: repositories and their sessions on
the left, the transcript in the middle, what the session is and what can be done
with it on the right.

```sh
npm ci
npm run dev          # http://localhost:5173
```

It renders `ui/src/fixtures/index.json`, which is a real `mochi export` of the
golden session files rather than invented data — regenerate it with
`./scripts/build-ui-fixture.sh`. Point it at your own sessions with:

```sh
cargo run -p mochi-cli -- scan
cargo run -p mochi-cli -- export --pretty --out ui/src/fixtures/index.json
npm run dev
```

That export is masked by default, so it is safe to keep and to pass around.

**There is no desktop shell yet.** Whether Mochi ends up on Tauri or Electron is
decided by the integrated-terminal spike at the end of milestone 0 (R-10), and
the interface is written against a one-call data source so that neither answer
costs a rewrite here.

## Where this is

| Area | State |
| --- | --- |
| Repository resolution, path normalisation | working |
| Adapters: Claude Code, Codex CLI | working, golden-file tested |
| Adapter: OpenCode | working for the JSON file layout; the newer SQLite layout is detected and reported, not read |
| Index, incremental scan, full-text search | working |
| Secret masking | working |
| Resume command construction | working; `mochi resume` prints, the app will launch |
| Interface (layout 1b) | reading, browsing, filtering and the metadata rail work, against exported data |
| Desktop shell, integrated terminal, delete flow, editing | not started — see [doc/ui-spec.md](doc/ui-spec.md) |
| UI mockups | 32 screens, with the agreed corrections pinned by browser tests (`npx playwright test`) |

Known gaps, each deliberate and recorded in the tests:

- OpenCode's SQLite storage and its local HTTP API are not implemented. The
  schema has not been verified against a real install, and guessing at it would
  produce a confidently wrong session list.
- Launching an external Terminal.app, iTerm2, cmd.exe or PowerShell window is
  refused rather than implemented, because each needs a shell command line built
  out of session data. That waits for the integrated terminal work.
- Nothing reports a session as *running* yet; that needs the integrated
  terminal, which owns the process.
- The interface reads; it does not yet write. Tags, notes, pinning, deleting and
  export from the window are not built, and Resume shows the command rather than
  launching it — launching belongs to the shell that does not exist yet.

## Documentation

| Document | Contents |
| --- | --- |
| [doc/requirements.md](doc/requirements.md) | Requirements: scope, functional and non-functional requirements, data model, risks, MVP definition of done |
| [doc/ui-spec.md](doc/ui-spec.md) | UI specification, screen by screen |
| [doc/tool-integration.md](doc/tool-integration.md) | What was found out about each CLI's storage and resume commands |
| [doc/Mochi-UI-mocks.html](doc/Mochi-UI-mocks.html) | 32 UI mockups; open in a browser |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Building, testing, and the rules for golden files |
| [SECURITY.md](SECURITY.md) | Reporting a vulnerability |

The requirement identifiers used throughout the code (`FR-3.3`, `NFR-6.4`, …)
refer to `doc/requirements.md`.

## Licence

[Apache License 2.0](LICENSE). Contributions are under the same licence; there
is no CLA.
