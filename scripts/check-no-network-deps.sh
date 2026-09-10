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
# NFR-3.1 / NFR-4b.8: Mochi runs fully offline and ships no telemetry. The
# cheapest durable guard is to refuse HTTP client and telemetry crates in the
# dependency graph outright, so that "we do not send anything" is a property of
# the build rather than a promise.
#
# Note the deliberate exception: doc/tool-integration.md ss3.3 proposes talking to
# OpenCode's *local* HTTP API. If that lands, add the chosen client here as an
# allowed entry together with a test proving it only ever binds to loopback.

set -euo pipefail
cd "$(dirname "$0")/.."

FORBIDDEN=(
  reqwest hyper ureq curl isahc surf attohttpc awc
  tokio-tungstenite tungstenite
  sentry opentelemetry tracing-opentelemetry posthog-rs segment
)

# Cargo.lock is the resolved set: exactly what gets built and shipped. A
# manifest's optional or dev-only dependency declarations are not, which is why
# this reads the lock file rather than `cargo metadata`.
if [ ! -f Cargo.lock ]; then
  echo "error: Cargo.lock is missing; run 'cargo build' and commit it." >&2
  exit 1
fi

resolved="$(grep -E '^name = "' Cargo.lock | sed -E 's/^name = "(.*)"$/\1/')"

found=()
for crate in "${FORBIDDEN[@]}"; do
  if printf '%s\n' "$resolved" | grep -qx "$crate"; then
    found+=("$crate")
  fi
done

if [ ${#found[@]} -ne 0 ]; then
  echo "error: network/telemetry crate in the dependency graph (NFR-3.1, NFR-4b.8):" >&2
  printf '  %s\n' "${found[@]}" >&2
  echo >&2
  echo "Run 'cargo tree -i <crate>' to find who pulled it in." >&2
  exit 1
fi

echo "no-network check: OK ($(printf '%s\n' "$resolved" | wc -l | tr -d ' ') resolved crates, ${#FORBIDDEN[@]} names screened)"
