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
# NFR-4b.1b: verify that every source file carries the Apache-2.0 boilerplate.

set -euo pipefail
cd "$(dirname "$0")/.."

missing=()

while IFS= read -r file; do
  # Only the first 20 lines are inspected, which is where a header belongs.
  if ! head -n 20 "$file" | grep -qF 'Licensed under the Apache License, Version 2.0'; then
    missing+=("$file")
  fi
done < <(git ls-files '*.rs' '*.sh' '*.ts' '*.tsx' '*.js' '*.css' | grep -v '^crates/mochi-core/tests/golden/')

if [ ${#missing[@]} -ne 0 ]; then
  echo "error: missing Apache-2.0 licence header (NFR-4b.1b):" >&2
  printf '  %s\n' "${missing[@]}" >&2
  echo >&2
  echo "Copy the header from any existing source file." >&2
  exit 1
fi

echo "license headers: OK ($(git ls-files '*.rs' '*.sh' | wc -l | tr -d ' ') files checked)"
