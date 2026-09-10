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
 * Colour contrast (NFR-6.2, WCAG 2.1 AA).
 *
 * This measures the interface as rendered, in both themes. It could not be
 * written against the design mock — that document interleaves the design
 * tool's own captions with the screens, so a measurement there reports on the
 * tool rather than the product. Here every element belongs to Mochi.
 */

/** Relative luminance, per the WCAG definition. */
function contrastScript() {
  return () => {
    const parse = (colour: string) => (colour.match(/[\d.]+/g) || []).map(Number);
    const luminance = ([r, g, b]: number[]) => {
      const channel = (value: number) => {
        const v = value / 255;
        return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4);
      };
      return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
    };
    const ratio = (a: number[], b: number[]) => {
      const [light, dark] = [luminance(a), luminance(b)].sort((x, y) => y - x);
      return (light + 0.05) / (dark + 0.05);
    };
    /** The colour actually behind an element, walking up past transparency. */
    const backgroundOf = (element: Element): number[] => {
      let node: Element | null = element;
      while (node) {
        const colour = parse(getComputedStyle(node).backgroundColor);
        if (colour.length >= 3 && (colour[3] === undefined || colour[3] > 0.5)) {
          return colour.slice(0, 3);
        }
        node = node.parentElement;
      }
      return [255, 255, 255];
    };

    const results: {
      text: string;
      ratio: number;
      required: number;
      selector: string;
      colour: string;
    }[] = [];

    for (const element of document.querySelectorAll<HTMLElement>('body *')) {
      if (element.children.length !== 0) continue;
      const text = (element.textContent || '').trim();
      if (!text) continue;
      const style = getComputedStyle(element);
      if (style.visibility === 'hidden' || style.display === 'none') continue;
      if (element.offsetParent === null && style.position !== 'fixed') continue;

      const size = parseFloat(style.fontSize);
      const bold = parseInt(style.fontWeight, 10) >= 700;
      // WCAG counts 18.66px bold or 24px as large text.
      const large = size >= 24 || (size >= 18.66 && bold);
      const required = large ? 3 : 4.5;

      results.push({
        text: text.slice(0, 48),
        ratio: Number(ratio(parse(style.color).slice(0, 3), backgroundOf(element)).toFixed(2)),
        required,
        selector: element.className || element.tagName.toLowerCase(),
        colour: style.color,
      });
    }
    return results;
  };
}

async function measure(page: Page) {
  await page.goto('/');
  await page.locator('.transcript').waitFor();
  return page.evaluate(contrastScript());
}

for (const scheme of ['light', 'dark'] as const) {
  test(`every piece of text clears WCAG AA in the ${scheme} theme (NFR-6.2)`, async ({ browser }) => {
    const page = await browser.newPage({ colorScheme: scheme });
    const measured = await measure(page);

    // Cover the whole window, not just the first pane: disabled controls and
    // the muted explanatory text under them are exactly where contrast slips.
    await page.getByRole('tab', { name: 'Tool calls' }).click();
    measured.push(...(await page.evaluate(contrastScript())));
    await page.getByRole('tab', { name: 'Diffs' }).click();
    measured.push(...(await page.evaluate(contrastScript())));
    await page.close();

    expect(measured.length).toBeGreaterThan(40);
    const failing = measured.filter((item) => item.ratio < item.required);
    expect(
      failing,
      `${failing.length} of ${measured.length} elements are below AA:\n` +
        failing
          .map((f) => `  ${f.ratio}:1 (needs ${f.required}) ${f.colour} — ${f.selector} — "${f.text}"`)
          .join('\n'),
    ).toEqual([]);
  });
}
