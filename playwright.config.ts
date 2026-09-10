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
 * Browser tests for the UI mockups in `doc/`.
 *
 * There is no Mochi application to drive yet (see the milestones in
 * doc/requirements.md). What exists is the mock document that FR-9.1 names as
 * the implementation reference, and it is a generated bundle: regenerating it
 * can silently drop a correction that was agreed in review. These tests pin
 * the decisions recorded in doc/ui-spec.md and doc/CHANGELOG-ui.md so that a
 * regeneration which loses one fails the build instead of passing quietly.
 *
 * The same setup is where the application's own end-to-end tests will go once
 * there is a window to open.
 */
export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: process.env.CI ? [['github'], ['list']] : [['list']],
  use: {
    // The mock is a local file and pulls in nothing from the network, so
    // there is no base URL and no server to start.
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
