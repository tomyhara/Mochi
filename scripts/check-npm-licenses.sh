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
# NFR-4b.2 names two dependency graphs: Rust, checked by cargo-deny, and npm,
# checked here. Permissive licences only; copyleft is rejected, because
# Apache-2.0 is incompatible with GPLv2 and can only be absorbed into GPLv3 one
# way.
#
# This reads the installed tree rather than shelling out to a licence-checker
# package, so the check itself adds no dependency to audit.

set -euo pipefail
cd "$(dirname "$0")/.."

if [ ! -d node_modules ]; then
  echo "error: node_modules is missing; run 'npm ci' first." >&2
  exit 1
fi

node - <<'NODE'
const fs = require('fs');
const path = require('path');

const ALLOWED = new Set([
  'Apache-2.0', 'MIT', 'BSD-2-Clause', 'BSD-3-Clause', 'ISC', 'Zlib',
  'Unicode-3.0', 'Unlicense', 'CC0-1.0', 'MIT-0', '0BSD', 'BlueOak-1.0.0',
  'Python-2.0',
]);

// Licences accepted only for packages that never reach a user's machine:
// build tooling and its data. CC-BY-4.0 carries an attribution obligation on
// distribution and no share-alike, so it is not the copyleft NFR-4b.2 rejects,
// but it has no business in anything Mochi ships either.
const ALLOWED_BUILD_ONLY = new Set(['CC-BY-4.0', 'CC-BY-3.0']);

/** Packages marked dev-only in the lockfile — build tooling, not product. */
function devOnlyPaths() {
  const paths = new Set();
  if (!fs.existsSync('package-lock.json')) return paths;
  const lock = JSON.parse(fs.readFileSync('package-lock.json', 'utf8'));
  for (const [location, info] of Object.entries(lock.packages || {})) {
    if (info && info.dev) paths.add(location);
  }
  return paths;
}

/** Walk node_modules, including scoped packages and nested trees. */
function* packages(dir) {
  if (!fs.existsSync(dir)) return;
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (!entry.isDirectory() && !entry.isSymbolicLink()) continue;
    if (entry.name === '.bin' || entry.name === '.cache') continue;
    const full = path.join(dir, entry.name);
    if (entry.name.startsWith('@')) {
      yield* packages(full);
      continue;
    }
    const manifest = path.join(full, 'package.json');
    if (fs.existsSync(manifest)) yield manifest;
    yield* packages(path.join(full, 'node_modules'));
  }
}

/** "(MIT OR Apache-2.0)" passes if any term is allowed; "A AND B" needs both. */
function acceptable(expression) {
  if (!expression) return false;
  const cleaned = expression.replace(/[()]/g, ' ').trim();
  if (/\sOR\s/i.test(cleaned)) {
    return cleaned.split(/\s+OR\s+/i).some(acceptable);
  }
  if (/\sAND\s/i.test(cleaned)) {
    return cleaned.split(/\s+AND\s+/i).every(acceptable);
  }
  return ALLOWED.has(cleaned.replace(/\s+WITH\s+.*$/i, '').trim());
}

const problems = [];
const buildOnly = devOnlyPaths();
let checked = 0;
let buildOnlyAccepted = 0;

for (const manifest of packages('node_modules')) {
  const pkg = JSON.parse(fs.readFileSync(manifest, 'utf8'));
  if (!pkg.name) continue;
  checked++;
  const licence =
    typeof pkg.license === 'string'
      ? pkg.license
      : pkg.license?.type || (Array.isArray(pkg.licenses) ? pkg.licenses.map((l) => l.type).join(' OR ') : '');

  if (acceptable(licence)) continue;

  const location = path.dirname(manifest).replace(/\\/g, '/');
  if (buildOnly.has(location) && ALLOWED_BUILD_ONLY.has(licence.trim())) {
    buildOnlyAccepted++;
    continue;
  }

  problems.push(`${pkg.name}@${pkg.version}: ${licence || '(no licence field)'}`);
}

if (problems.length) {
  console.error('error: npm dependency licence not permitted (NFR-4b.2):');
  for (const problem of problems) console.error('  ' + problem);
  console.error('');
  console.error('Permitted: ' + [...ALLOWED].join(', '));
  console.error('Permitted for dev-only packages: ' + [...ALLOWED_BUILD_ONLY].join(', '));
  process.exit(1);
}

console.log(
  `npm licence check: OK (${checked} packages` +
    (buildOnlyAccepted ? `, ${buildOnlyAccepted} accepted as build-only` : '') +
    ')',
);
NODE
