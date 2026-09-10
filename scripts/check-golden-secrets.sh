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
# NFR-4b.4 / R-12: golden files are derived from real agent sessions and live in
# a public repository. A real session file routinely contains API keys, tokens
# and the contents of .env files, so committing one unedited would publish the
# author's credentials.
#
# This script fails the build when a fixture looks like it still carries a real
# secret or a real person's home directory. It is a backstop; the reviewer
# reading the fixture is the actual control (see CONTRIBUTING.md).

set -euo pipefail
cd "$(dirname "$0")/.."

GOLDEN_DIR="crates/mochi-core/tests/golden"

if [ ! -d "$GOLDEN_DIR" ]; then
  echo "golden secret scan: no fixtures yet ($GOLDEN_DIR missing)"
  exit 0
fi

# A match is treated as an intentional placeholder when it says so in the token
# itself. Fixtures must use these words so that both the scanner and a human
# reviewer can tell a sample from the real thing at a glance.
PLACEHOLDER='EXAMPLE|PLACEHOLDER|REDACTED|DUMMY|NOTAREAL|FAKE|XXXX|0000'

# name -> extended regular expression
PATTERNS=(
  "OpenAI API key|sk-[A-Za-z0-9_-]{20,}"
  "Anthropic API key|sk-ant-[A-Za-z0-9_-]{20,}"
  "GitHub token|gh[pousr]_[A-Za-z0-9]{20,}"
  "GitHub fine-grained PAT|github_pat_[A-Za-z0-9_]{20,}"
  "AWS access key id|(A3T[A-Z0-9]|AKIA|ASIA|ABIA|ACCA)[A-Z0-9]{16}"
  "AWS secret access key|aws_secret_access_key[[:space:]]*[=:][[:space:]]*[A-Za-z0-9/+=]{40}"
  "Google API key|AIza[A-Za-z0-9_-]{35}"
  "Slack token|xox[baprs]-[A-Za-z0-9-]{10,}"
  "Private key block|-----BEGIN[A-Z ]*PRIVATE KEY-----"
  "JSON Web Token|eyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}\."
  "Bearer token|[Bb]earer[[:space:]]+[A-Za-z0-9._-]{24,}"
  "Basic auth in URL|https?://[A-Za-z0-9._%-]+:[^@/[:space:]]+@"
)

fail=0

report() {
  if [ "$fail" -eq 0 ]; then
    echo "error: possible real secret in golden files (NFR-4b.4):" >&2
    echo >&2
  fi
  fail=1
  echo "  [$1] $2" >&2
}

for entry in "${PATTERNS[@]}"; do
  name="${entry%%|*}"
  regex="${entry#*|}"
  while IFS= read -r hit; do
    [ -z "$hit" ] && continue
    # hit is "path:line:text"; drop obvious placeholders.
    if printf '%s' "$hit" | grep -Eqi "$PLACEHOLDER"; then
      continue
    fi
    report "$name" "$hit"
  done < <(grep -rEn --binary-files=without-match -e "$regex" "$GOLDEN_DIR" || true)
done

# Personal home directories. Fixtures use the sanctioned placeholder users so
# that path-handling tests stay realistic without naming a real person.
while IFS= read -r hit; do
  [ -z "$hit" ] && continue
  if printf '%s' "$hit" | grep -Eq '(/home/|Users[\\/]+)(you|user|example)([^A-Za-z0-9._-]|$)'; then
    continue
  fi
  report "Personal home directory" "$hit"
done < <(grep -rEn --binary-files=without-match -e '(/home/|Users[\\/]+)[A-Za-z0-9._-]+' "$GOLDEN_DIR" || true)

if [ "$fail" -ne 0 ]; then
  echo >&2
  echo "Replace the value with an obvious placeholder (containing EXAMPLE," >&2
  echo "PLACEHOLDER, DUMMY or FAKE) and re-run. If the credential was ever" >&2
  echo "committed or is real, rotate it." >&2
  exit 1
fi

echo "golden secret scan: OK ($(find "$GOLDEN_DIR" -type f | wc -l | tr -d ' ') files, ${#PATTERNS[@]} patterns)"
