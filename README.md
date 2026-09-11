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
**entirely offline**, never writes to the original session files, and is a
**single executable** — one file, no installer, nothing to install alongside it.

> **Status: early.** The core — discovery, parsing, repository grouping,
> indexing, search, masking and command building — works and is covered by
> tests. The window reads; it does not yet write. See
> [Where this is](#where-this-is).

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
- **Is one file.** The window and the command line tool are the same
  executable, which draws its own interface rather than borrowing a browser.
  Copy it to a USB stick and it runs.

## Install

Download the file for your platform from
[Releases](https://github.com/tomyhara/Mochi/releases) and run it. There is no
installer and nothing to unpack. Nothing is signed yet
(see [below](#these-builds-are-unsigned)).

| File | For |
| --- | --- |
| `Mochi-<version>-windows-x86_64.exe` | Windows 10/11, x86-64 |
| `Mochi-<version>-macos-universal` | macOS, Apple Silicon and Intel |

Run it with no arguments and the window opens. Run it with a subcommand and it
is [the command line indexer](#the-command-line-indexer) instead — same file.
On macOS it has no `.app` bundle around it, so Finder will not start it by
double-clicking; run it from a terminal, or put it on your `PATH` as `mochi`.

Attribution for everything Mochi is built from travels inside the executable:

```sh
mochi licenses
```

Or build it yourself — one command, and nothing to install first beyond a Rust
toolchain:

```sh
cargo build --release -p mochi-cli
./target/release/mochi
```

## The window

Layout `1b` from the mockups: repositories and their sessions on the left, the
transcript in the middle, what the session is and what can be done with it on
the right. On first run it scans your session stores and shows what it found.

It is drawn by Mochi itself — `egui`, reaching the GPU through Direct3D 12 on
Windows and Metal on macOS. There is no web view, which is why there is only
one file. For Japanese, Chinese and Korean text it borrows a font the system already has (Yu Gothic, Meiryo,
Hiragino, Noto CJK); nothing of that font is copied into the executable.

`Ctrl+K` puts the cursor in the search box; `Enter` searches inside every
message of every session. `Ctrl+R` rescans.

To work on it:

```sh
cargo run -p mochi-cli             # the window, against your real sessions
cargo run -p mochi-cli -- ui --db /tmp/scratch.sqlite3 \
  --home crates/mochi-core/tests/golden/claude_code/home
```

The second form is how to look at the golden fixture sessions instead of your
own: `--db` keeps the experiment out of your real index and `--home` points the
adapters at a fixture tree. `--no-mask` opens with credentials shown.

### These builds are unsigned

Code signing and notarisation are not funded yet (R-7):

- **Windows** shows a SmartScreen warning. Choose *More info* then *Run anyway*,
  or check the download against `SHA256SUMS.txt` on the release first.
- **macOS** refuses to run a quarantined download. Clear the flag and make it
  executable:

  ```sh
  chmod +x Mochi-*-macos-universal
  xattr -d com.apple.quarantine Mochi-*-macos-universal
  ```

## The command line indexer

The same file, given a subcommand, does the same reading without a window — for
scripting, or for anyone who would rather not have one.

```sh
mochi doctor          # which CLIs and stores were found
mochi scan            # build or refresh the index
mochi repos           # repositories, with session counts
mochi sessions        # recent sessions
mochi search ECONNRESET
mochi resume 42       # prints the command; never runs it
mochi export --pretty # the whole index as JSON
mochi ui              # the window, asked for explicitly
```

On Windows this is a windowed executable, so that double-clicking it does not
also open a console box. Run with arguments it borrows the console that started
it, which has one visible oddity: `cmd.exe` and PowerShell do not wait for it,
so your prompt comes back before the output does. Redirecting to a file, or
piping, behaves normally.

Useful flags: `--db <path>` to keep an experiment away from your real index,
`--home <path>` to point at a fixture tree instead of your home directory, and
`--no-mask` to see credentials unmasked.

The index goes to `%APPDATA%\Mochi\` on Windows and
`~/Library/Application Support/Mochi/` on macOS.

## Where this is

| Area | State |
| --- | --- |
| Repository resolution, path normalisation | working |
| Adapters: Claude Code, Codex CLI | working, golden-file tested |
| Adapter: OpenCode | working for the JSON file layout; the newer SQLite layout is detected and reported, not read |
| Index, incremental scan, full-text search | working |
| Secret masking | working |
| Resume command construction | working; `mochi resume` prints, the app will launch |
| The window | opens, scans on first run, reads your real sessions; one executable with the indexer |
| Interface (layout 1b) | browsing, filtering, full-text search, the transcript views and the metadata rail |
| Integrated terminal, delete flow, editing | not started — see [doc/ui-spec.md](doc/ui-spec.md) |
| UI mockups | 32 screens, with the agreed corrections pinned by browser tests (`npx playwright test`) |
| Windows and macOS builds | one unsigned file each, built by CI |

Known gaps, each deliberate and recorded in the tests:

- OpenCode's SQLite storage and its local HTTP API are not implemented. The
  schema has not been verified against a real install, and guessing at it would
  produce a confidently wrong session list.
- Launching an external Terminal.app, iTerm2, cmd.exe or PowerShell window is
  refused rather than implemented, because each needs a shell command line built
  out of session data. That waits for the integrated terminal work.
- Nothing reports a session as *running* yet; that needs the integrated
  terminal, which owns the process.
- The window reads; it does not yet write. Tags, notes, pinning, deleting and
  export from the window are not built.
- **Resume shows you the command rather than running it.** An interactive CLI
  needs somewhere to be interactive, and that is the integrated terminal
  (FR-7.8), which is milestone 3. The button says so rather than doing nothing.
- Very long transcripts are drawn a few hundred entries at a time, and a very
  long single entry is cut with the cut shown. Nothing is hidden silently, but
  there is no virtualised scrollback yet.
- The window is drawn with `egui` rather than a web view, which is what makes
  the single file possible. That replaces the Tauri decision in
  doc/requirements.md §7.1 — see the note there, and R-10. The integrated
  terminal (FR-7.8) now means drawing one, which is a different piece of work
  from the one that was planned; `mochi-core` is unaffected either way.

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
