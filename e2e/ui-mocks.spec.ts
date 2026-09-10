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
import * as path from 'path';

/**
 * The mock document, in the order the artboards appear in the rendered page.
 * Used both as a completeness check and to slice one artboard's text out of
 * the whole document.
 */
const ARTBOARDS = [
  '3a', '3b', '3c', '3d', '3e', '3f', '3g', '3h', '3i',
  '2a', '2b', '2c', '2d',
  '1a', '1b', '1c',
] as const;

const MOCKS = 'file://' + path.resolve(__dirname, '..', 'doc', 'Mochi-UI-mocks.html');

/** Wait for the React canvas to finish drawing every artboard. */
async function render(page: Page): Promise<string> {
  await page.goto(MOCKS);
  await page.waitForFunction(
    (last) => (document.body.innerText || '').includes(last),
    'Shared overlays',
  );
  return page.evaluate(() => document.body.innerText);
}

/**
 * The text of one artboard: everything from its id label up to the next one.
 *
 * Slicing rendered text rather than querying the DOM is deliberate. The
 * document is generated, so its element structure and class names are not
 * ours to depend on; the words on the screen are what the specification
 * actually decided.
 */
function artboard(document: string, id: string): string {
  const lines = document.split('\n');
  const start = lines.findIndex((line) => line.trim() === id);
  expect(start, `artboard ${id} is missing from the mock`).toBeGreaterThanOrEqual(0);

  let end = lines.length;
  for (let i = start + 1; i < lines.length; i++) {
    if ((ARTBOARDS as readonly string[]).includes(lines[i].trim())) {
      end = i;
      break;
    }
  }
  return lines.slice(start, end).join('\n');
}

/** Numbers in this document are written with thousands separators. */
function count(text: string, pattern: RegExp): number {
  const match = text.match(pattern);
  expect(match, `no match for ${pattern} in:\n${text.slice(0, 400)}`).not.toBeNull();
  return Number(match![1].replace(/,/g, ''));
}

test.describe('UI mockups', () => {
  let mock: string;

  test.beforeAll(async ({ browser }) => {
    const page = await browser.newPage();
    mock = await render(page);
    await page.close();
  });

  test('the document renders every artboard without reaching the network', async ({ browser }) => {
    // The mock is a bundled page: a bad regeneration can leave it blank or
    // depending on an asset that was not inlined. Both would go unnoticed
    // until someone opened it.
    const page = await browser.newPage();
    const errors: string[] = [];
    const external: string[] = [];

    page.on('pageerror', (error) => errors.push(String(error)));
    await page.route('**', (route) => {
      const url = route.request().url();
      if (url.startsWith('file://') || url.startsWith('data:') || url.startsWith('blob:')) {
        return route.continue();
      }
      external.push(url);
      return route.abort();
    });

    const text = await render(page);
    await page.close();

    expect(errors).toEqual([]);
    expect(external, 'the mock must be self-contained so it opens offline').toEqual([]);
    for (const id of ARTBOARDS) {
      expect(text.split('\n').map((l) => l.trim())).toContain(id);
    }
  });

  test('settings point at the OS application data directory, never at XDG (M-1, FR-10.1)', () => {
    expect(mock).toContain('~/Library/Application Support/Mochi');
    // FR-10.1 rules ~/.config out explicitly: it is neither platform's
    // convention and would leave the index outside what users migrate.
    expect(mock).not.toMatch(/~\/\.config/);
    expect(mock).not.toMatch(/XDG_/);
  });

  test('a missing working directory and a missing file are different states (M-2, FR-2.11, FR-7.6)', () => {
    // The two were once both labelled "source missing", which hid the fact
    // that one session can still be read and the other can still be resumed.
    expect(mock).not.toMatch(/source missing/i);
    expect(mock).toContain('WORKING DIRECTORY MISSING');
    expect(mock).toContain('ARCHIVED');
  });

  test('the first-run progress numbers add up (M-3, FR-2.9)', () => {
    // M-3 settled the indexing display as sequential processing. The numbers
    // are only meaningful if the per-tool figures still sum to the total, and
    // a regenerated mock is exactly where that quietly stops being true.
    const firstRun = artboard(mock, '2c');

    const done = count(firstRun, /Indexing ([\d,]+) \//);
    const total = count(firstRun, /Indexing [\d,]+ \/ ([\d,]+)/);
    const codex = count(firstRun, /reading [\d,]+ \/ ([\d,]+)/);
    const [claude, opencode] = [...firstRun.matchAll(/queued · ([\d,]+)/g)].map((m) =>
      Number(m[1].replace(/,/g, '')),
    );

    expect(codex + claude + opencode, 'per-tool totals must sum to the overall total').toBe(total);
    expect(done).toBeLessThanOrEqual(codex);

    const percent = count(firstRun, /· (\d+)%/);
    expect(percent).toBe(Math.round((done / total) * 100));
  });

  test('the network state is a statement, not a switch (M-4, NFR-3.1)', () => {
    // Offering a toggle would imply there is something to turn on.
    const settings = artboard(mock, '2b') + artboard(mock, '2a');
    expect(settings).toContain('Fully offline');
    expect(mock).not.toMatch(/send anonymous/i);
  });

  test('watching counts stores, not running sessions (M-5, FR-4.8)', () => {
    // One number used to stand for both, so three quiet stores read as three
    // agents at work.
    expect(mock).toMatch(/Watching 3 stores/);
    expect(mock).toMatch(/2 sessions running/);
  });

  test('the adopted layout is 1b, with 1a kept only as the alternative (§12.1)', () => {
    expect(artboard(mock, '1b')).toContain('SELECTED');
    expect(artboard(mock, '1a')).not.toContain('SELECTED');
    expect(artboard(mock, '1b')).toContain('Transcript first');
    expect(artboard(mock, '1a')).toContain('Three panes, fixed');
  });

  test('the middle pane carries every view the requirements name (FR-9.1a, M-6)', () => {
    const layout = artboard(mock, '1b');
    for (const tab of ['Transcript', 'Tool calls', 'Diffs', 'Raw JSONL', 'Terminal']) {
      expect(layout, `the ${tab} tab is missing from the adopted layout`).toContain(tab);
    }
  });

  test('search, copy command and resume are always on screen (FR-9.1b)', () => {
    for (const id of ['1b', '3a']) {
      const board = artboard(mock, id);
      expect(board).toContain('Search');
      expect(board).toContain('Copy command');
      expect(board).toContain('Resume');
    }
  });

  test('the integrated terminal is described as launch-only, not as a sandbox (FR-7.8h, FR-7.8i)', () => {
    // FR-7.8i is explicit that the UI must not imply isolation: the CLI it
    // starts can still run anything, and saying otherwise would be a
    // security claim Mochi cannot keep.
    const terminal = artboard(mock, '3b');
    expect(terminal).toMatch(/launch-only/);
    expect(terminal).not.toMatch(/sandbox|isolated|contained/i);
  });

  test('deleting goes through the Trash, with the unsafe path called out (FR-8.3)', () => {
    const deleteFlow = artboard(mock, '3d');
    expect(deleteFlow).toMatch(/move to the Trash|move .* to Trash/i);
    expect(deleteFlow).toMatch(/never deletes them outright/i);
    // The fallback when there is no Trash must say plainly that it is final.
    expect(deleteFlow).toMatch(/cannot go to the Trash/i);
    expect(deleteFlow).toMatch(/permanent/i);
    expect(deleteFlow).toMatch(/There is no undo/i);
  });

  test('a running session cannot be deleted (FR-8.3b)', () => {
    expect(artboard(mock, '3d')).toMatch(/stop it before deleting/i);
  });

  test('an archived session offers removal from the index, not file deletion (FR-2.11, FR-8.3a)', () => {
    expect(artboard(mock, '3d')).toMatch(/Archived sessions have no file left to delete/i);
    expect(artboard(mock, '3d')).toMatch(/Remove from index/i);
  });

  test('every interface string is English (FR-9.3, NFR-6.3, NS-6)', () => {
    // The UI is English only. Japanese data is a separate matter and is
    // covered by the Rust tests for paths, search and masking (NFR-6.4).
    const cjk = mock
      .split('\n')
      .filter((line) => /[぀-ヿ㐀-䶿一-鿿豈-﫿]/.test(line));
    expect(cjk, 'the mock UI must not contain CJK text').toEqual([]);
  });
});
