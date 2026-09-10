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

import { test, expect, type Page } from '@playwright/test';

/**
 * The interface itself, layout 1b (FR-9.1), driven against the fixture that
 * `scripts/build-ui-fixture.sh` produces by running the real scanner over the
 * golden session files. So these tests exercise the actual export contract,
 * not a hand-written stand-in.
 */

const UI = '/';

async function open(page: Page) {
  await page.goto(UI);
  await page.locator('.topbar').waitFor();
}

test.describe('interface', () => {
  test.beforeEach(async ({ page }) => {
    await open(page);
  });

  test('opens on the most recent session, in its repository (FR-4.4)', async ({ page }) => {
    const breadcrumb = page.locator('.breadcrumb');
    await expect(breadcrumb).toContainText('mochi');
    await expect(breadcrumb).toContainText('Claude Code');
    await expect(breadcrumb).toContainText('Repo identity');
  });

  test('the three panes of layout 1b are all present (FR-9.1)', async ({ page }) => {
    await expect(page.locator('nav.sidebar')).toBeVisible();
    await expect(page.locator('main.centre .transcript')).toBeVisible();
    await expect(page.locator('aside.rail')).toBeVisible();
  });

  test('search, copy command and resume are always on screen (FR-9.1b)', async ({ page }) => {
    await expect(page.getByLabel('Search sessions')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Copy command' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Resume', exact: true })).toBeVisible();
  });

  test('the middle pane has the four views the requirements name (FR-9.1a)', async ({ page }) => {
    const tabs = page.getByRole('tab');
    await expect(tabs).toHaveText(['Transcript', 'Tool calls', 'Diffs', 'Raw JSONL']);
  });

  test('sessions group under their repository, with the rest kept visible (FR-3.6)', async ({
    page,
  }) => {
    const names = page.locator('.repo-name');
    await expect(names).toHaveText(['mochi', 'No repository']);
    // A session whose working directory is gone is still listed — it is
    // usually the one someone is looking for.
    await expect(page.locator('.session-row')).not.toHaveCount(0);
  });

  test('a long repository list collapses behind "Show N more" (FR-9.1)', async ({ page }) => {
    const more = page.getByRole('button', { name: /Show \d+ more/ });
    await expect(more).toBeVisible();
    const before = await page.locator('.session-row').count();
    await more.click();
    expect(await page.locator('.session-row').count()).toBeGreaterThan(before);
    await expect(more).toHaveCount(0);
  });

  test('the transcript distinguishes each kind of entry (FR-5.1)', async ({ page }) => {
    const roles = await page.locator('.entry').evaluateAll((nodes) =>
      nodes.map((node) => node.getAttribute('data-role')),
    );
    expect(roles).toEqual(['user', 'thinking', 'assistant', 'tool_call', 'tool_result', 'assistant']);
  });

  test('tool arguments and their output are shown, not just named (FR-5.3)', async ({ page }) => {
    await expect(page.locator('.entry.tool_call')).toContainText('identity.rs');
    await expect(page.locator('.entry.tool_result')).toContainText('RepoKey');
    await expect(page.locator('.entry.tool_call .tool-name')).toHaveText('Read');
  });

  test('switching to another session changes the transcript', async ({ page }) => {
    await page.getByLabel('Search sessions').fill('docker');
    await page.locator('.session-row', { hasText: 'Trim the docker image' }).click();
    await expect(page.locator('.breadcrumb')).toContainText('OpenCode');
    await expect(page.locator('.transcript')).toContainText('1.2GB');
  });

  test('the tool calls view keeps only the tool entries', async ({ page }) => {
    await page.getByRole('tab', { name: 'Tool calls' }).click();
    const roles = await page.locator('.entry').evaluateAll((nodes) =>
      nodes.map((node) => node.getAttribute('data-role')),
    );
    expect(roles.length).toBeGreaterThan(0);
    expect(new Set(roles)).toEqual(new Set(['tool_call', 'tool_result']));
  });

  test('an unbuilt view says so instead of showing an empty box (FR-9.6)', async ({ page }) => {
    await page.getByRole('tab', { name: 'Diffs' }).click();
    await expect(page.locator('.empty')).toContainText('not extracted yet');
    await expect(page.locator('.empty')).toContainText('Tool calls');
  });

  test('the rail carries the session identity and where it came from (FR-5.9)', async ({ page }) => {
    const rail = page.locator('aside.rail');
    await expect(rail).toContainText('ui-demo-0001');
    await expect(rail).toContainText('claude-opus-5');
    await expect(rail).toContainText('/Users/you/code/mochi');
    await expect(rail).toContainText('.jsonl');
  });

  test('a session that cannot be resumed says why, and the button is off (FR-7.6)', async ({
    page,
  }) => {
    await page.getByLabel('Search sessions').fill('test suite');
    await page.locator('.session-row', { hasText: 'Run the test suite' }).click();
    await expect(page.getByRole('button', { name: 'Resume', exact: true })).toBeDisabled();
    await expect(page.locator('aside.rail')).toContainText('Working directory missing');
    await expect(page.locator('aside.rail')).toContainText('working directory no longer exists');
  });

  test('a resumable session offers the command the core would run (FR-7.2, FR-7.4)', async ({
    page,
  }) => {
    await expect(page.locator('aside.rail')).toContainText('claude --resume ui-demo-0001');
    await expect(page.locator('aside.rail')).toContainText('cd /Users/you/code/mochi');
    await page.getByRole('button', { name: 'Copy command' }).click();
    await expect(page.getByRole('button', { name: 'Copied' })).toBeVisible();
  });

  test('resume stays off until something can actually run a session (FR-7.8)', async ({ page }) => {
    // An interactive CLI needs somewhere to be interactive, and that is the
    // integrated terminal, which is milestone 3. A button that looks ready and
    // does nothing is worse than one that says why it is not.
    const resume = page.getByRole('button', { name: 'Resume', exact: true });
    await expect(resume).toBeVisible();
    await expect(resume).toBeDisabled();
    await expect(resume).toHaveAttribute('title', /integrated terminal/);
  });

  test('secrets are masked, and the view says it cannot unmask them (NFR-3.3)', async ({ page }) => {
    await page.getByLabel('Search sessions').fill('deploy');
    await page.locator('.session-row', { hasText: 'cat .env' }).click();
    const transcript = page.locator('.transcript');
    await expect(transcript).toContainText('redacted:openai_api_key');
    await expect(transcript).not.toContainText('sk-EXAMPLE');
    // The fixture was masked where the files were read, so there is nothing
    // in the browser to reveal. Saying so is better than a button that lies.
    await expect(page.getByRole('button', { name: 'Reveal secrets' })).toBeDisabled();
  });

  test('search narrows the sidebar (FR-4.5)', async ({ page }) => {
    await page.getByLabel('Search sessions').fill('docker');
    await expect(page.locator('.session-row')).toHaveCount(1);
    await expect(page.locator('.session-row')).toContainText('Trim the docker image');
  });

  test('search reports having found nothing rather than showing a blank pane', async ({ page }) => {
    await page.getByLabel('Search sessions').fill('nothing matches this');
    await expect(page.locator('.sidebar-empty')).toBeVisible();
  });

  test('a shortcut reaches search, and escape clears it (FR-9.5)', async ({ page }) => {
    await page.keyboard.press('Control+k');
    await expect(page.getByLabel('Search sessions')).toBeFocused();

    await page.keyboard.type('docker');
    await page.keyboard.press('Escape');
    await expect(page.getByLabel('Search sessions')).toHaveValue('');
  });

  test('the first tab stop skips past the sidebar (FR-9.4)', async ({ page }) => {
    // A long repository list should not stand between the keyboard and the
    // thing the window is for.
    await page.keyboard.press('Tab');
    await expect(page.getByRole('link', { name: 'Skip to transcript' })).toBeFocused();
  });

  test('a session read with problems shows the reason (FR-2.7)', async ({ page }) => {
    await page.getByLabel('Search sessions').fill('orphan');
    await page.locator('.session-row', { hasText: 'orphan turn' }).click();
    await expect(page.locator('aside.rail .notice')).toContainText('Read with problems');
  });

  test('the theme follows the operating system and can be overridden (FR-9.2)', async ({ page }) => {
    await expect(page.locator('html')).not.toHaveAttribute('data-theme', /.*/);
    await page.getByRole('button', { name: 'Colour theme' }).click();
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
    await page.getByRole('button', { name: 'Colour theme' }).click();
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  });

  test('nothing in the interface is in a language it does not support (FR-9.3)', async ({ page }) => {
    // The chrome is English only (NS-6). Session content is not, and is
    // excluded here: NFR-6.4 requires it to render.
    const chrome = await page
      .locator('.topbar, .sidebar, .tabs, .rail')
      .evaluateAll((nodes) => nodes.map((node) => (node as HTMLElement).innerText).join('\n'));
    expect(chrome).not.toMatch(/[぀-ヿ㐀-䶿一-鿿]/);
  });
});
