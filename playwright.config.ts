// Copyright 2026 The Mochi Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import { defineConfig, devices } from '@playwright/test';

/**
 * Browser tests, for one thing only: the design mock in `doc/`, which FR-9.1
 * names as the reference the window is built against.
 *
 * The mock is a generated bundle, so regenerating it can silently drop a
 * correction that was agreed in review; these tests pin the decisions from
 * doc/ui-spec.md and doc/CHANGELOG-ui.md.
 *
 * The interface itself is no longer a web page — it is drawn by `mochi-gui`
 * and tested in Rust, `crates/mochi-gui/tests/`. Nothing here serves it.
 */
export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: process.env.CI ? [['github'], ['list']] : [['list']],
  // The mock tests open a local file, so there is nothing to serve.
  use: {
    trace: 'retain-on-failure',
    launchOptions: {
      // Environments that ship their own Chromium (CI images, this repo's
      // dev containers) set this instead of downloading a second copy.
      executablePath: process.env.MOCHI_CHROMIUM_PATH || undefined,
    },
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'], viewport: { width: 1600, height: 1200 } },
    },
  ],
});
