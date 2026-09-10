#!/usr/bin/env bash
# Copyright 2026 The Mochi Authors
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.
#
# Regenerate ui/src/fixtures/index.json.
#
# The interface renders whatever `mochi export` produces, so its fixture is
# built by running the real scanner over the golden session files rather than
# being written by hand. If the exported shape changes, this fails loudly
# instead of leaving the interface rendering a contract nobody implements.
#
# Machine-specific paths are rewritten to the same placeholder home the golden
# files use, so the committed fixture is identical wherever it is regenerated.

set -euo pipefail
cd "$(dirname "$0")/.."

OUT="ui/src/fixtures/index.json"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

HOME_DIR="$WORK/home"
mkdir -p "$HOME_DIR"
for tool in claude_code codex opencode; do
  cp -R "crates/mochi-core/tests/golden/$tool/home/." "$HOME_DIR/"
done

# One repository that really exists, so the interface has something grouped to
# show: the sidebar is the whole point of FR-3.
REPO="$HOME_DIR/code/mochi"
mkdir -p "$REPO"
git -C "$REPO" init -q --initial-branch=main
git -C "$REPO" config user.email "fixtures@example.invalid"
git -C "$REPO" config user.name "Mochi Fixtures"
git -C "$REPO" config commit.gpgsign false
echo "fixture" > "$REPO/README.md"
git -C "$REPO" add README.md
# Fixed dates so the root commit hash — and therefore the whole fixture — comes
# out identical on every machine that regenerates it.
GIT_AUTHOR_DATE="2026-09-01T00:00:00+0000" \
GIT_COMMITTER_DATE="2026-09-01T00:00:00+0000" \
  git -C "$REPO" commit -qm "initial"
git -C "$REPO" remote add origin "git@github.com:example/mochi.git"

PROJECT="$HOME_DIR/.claude/projects/-code-mochi"
mkdir -p "$PROJECT"
REPO="$REPO" python3 - "$PROJECT/ui-demo-0001.jsonl" <<'PY'
import json, os, sys

cwd = os.environ["REPO"]
lines = [
    {"type": "user", "uuid": "e1", "parentUuid": None, "sessionId": "ui-demo-0001",
     "timestamp": "2026-09-08T09:12:00.000Z", "cwd": cwd, "gitBranch": "main", "version": "1.0.60",
     "message": {"role": "user", "content":
                 "Two clones of the same repo show up as separate rows in the sidebar. Decide the "
                 "identity rule and write it down."}},
    {"type": "assistant", "uuid": "e2", "parentUuid": "e1", "sessionId": "ui-demo-0001",
     "timestamp": "2026-09-08T09:12:06.000Z", "cwd": cwd, "gitBranch": "main", "version": "1.0.60",
     "message": {"role": "assistant", "model": "claude-opus-5", "content": [
         {"type": "thinking", "thinking":
          "Each signal fails differently. The root commit survives moves and renames but is missing "
          "for a repository with no commits. The origin URL survives clones but not forks. The path "
          "is always available and always wrong after a move."},
         {"type": "text", "text":
          "Order them by how they fail: root commit first, then the normalised origin URL, then the "
          "absolute path."},
         {"type": "tool_use", "id": "t1", "name": "Read",
          "input": {"file_path": "src/repo/identity.rs"}}],
         "usage": {"input_tokens": 1840, "output_tokens": 143}}},
    {"type": "user", "uuid": "e3", "parentUuid": "e2", "sessionId": "ui-demo-0001",
     "timestamp": "2026-09-08T09:12:07.000Z", "cwd": cwd, "version": "1.0.60",
     "message": {"role": "user", "content": [
         {"type": "tool_result", "tool_use_id": "t1",
          "content": "pub fn identity(repo: &GitRepo) -> RepoKey {\n    RepoKey::Path(repo.root.clone())\n}\n"}]}},
    {"type": "assistant", "uuid": "e4", "parentUuid": "e3", "sessionId": "ui-demo-0001",
     "timestamp": "2026-09-08T09:13:41.000Z", "cwd": cwd, "gitBranch": "main", "version": "1.0.60",
     "message": {"role": "assistant", "model": "claude-opus-5", "content": [
         {"type": "text", "text": "Both call sites now go through identity(). Running the scanner tests."}],
         "usage": {"input_tokens": 2310, "output_tokens": 88}}},
    {"type": "summary", "summary": "Repo identity: root commit vs origin URL", "leafUuid": "e4"},
]
with open(sys.argv[1], "w", encoding="utf-8") as handle:
    for line in lines:
        handle.write(json.dumps(line) + "\n")
PY

cargo build --quiet -p mochi-cli
BIN="target/debug/mochi"

"$BIN" --db "$WORK/index.sqlite3" --home "$HOME_DIR" scan >/dev/null
"$BIN" --db "$WORK/index.sqlite3" --home "$HOME_DIR" export --pretty --out "$WORK/raw.json"

# Rewrite the temporary directory back to the placeholder home, so the fixture
# does not carry whichever machine happened to build it.
WORK="$WORK" python3 - "$WORK/raw.json" "$OUT" <<'PY'
import json, os, sys

work = os.environ["WORK"]
raw = open(sys.argv[1], encoding="utf-8").read()
raw = raw.replace(work + "/home", "/Users/you").replace(work, "/Users/you")
document = json.loads(raw)

# The scan ran against a directory that existed, and the placeholder path does
# not; keep the flag describing the session rather than this machine.
for session in document["sessions"]:
    if session["cwd"] and session["cwd"].startswith("/Users/you/code/mochi"):
        session["cwdExists"] = True

with open(sys.argv[2], "w", encoding="utf-8") as handle:
    json.dump(document, handle, indent=2, ensure_ascii=False)
    handle.write("\n")
PY

echo "wrote $OUT"
python3 - "$OUT" <<'PY'
import json, sys
d = json.load(open(sys.argv[1], encoding="utf-8"))
print(f"  masked={d['masked']} repositories={len(d['repositories'])} "
      f"sessions={len(d['sessions'])} "
      f"messages={sum(len(s['messages']) for s in d['sessions'])}")
PY
