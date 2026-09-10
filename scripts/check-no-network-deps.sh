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

# Resolve the graph per target rather than reading Cargo.lock.
#
# Cargo.lock lists every platform's dependencies at once, which over-reports:
# Tauri depends on reqwest only for Android and iOS, and Mochi ships to neither.
# Asking cargo what each shipped target actually builds is both accurate and
# still catches the regression this guards against.
#
# Build and dev dependencies are excluded (-e normal): they do not ship.
TARGETS=(
  x86_64-pc-windows-msvc
  aarch64-apple-darwin
  x86_64-apple-darwin
  x86_64-unknown-linux-gnu
)

found=()
for target in "${TARGETS[@]}"; do
  graph="$(cargo tree --workspace --locked -e normal --target "$target" \
            --prefix none --format '{p}' 2>/dev/null | awk '{print $1}' | sort -u)"
  if [ -z "$graph" ]; then
    echo "error: could not resolve the dependency graph for $target" >&2
    exit 1
  fi
  for crate in "${FORBIDDEN[@]}"; do
    if printf '%s\n' "$graph" | grep -qx "$crate"; then
      found+=("$crate ($target)")
    fi
  done
done

if [ ${#found[@]} -ne 0 ]; then
  echo "error: network/telemetry crate in the dependency graph (NFR-3.1, NFR-4b.8):" >&2
  printf '  %s\n' "${found[@]}" >&2
  echo >&2
  echo "Run 'cargo tree -i <crate>' to find who pulled it in." >&2
  exit 1
fi

echo "no-network check: OK (${#TARGETS[@]} shipped targets, ${#FORBIDDEN[@]} crate names screened)"
