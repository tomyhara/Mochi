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

import type { IndexDocument } from '../types';
import type { DataSource } from './source';

/** What the desktop shell exposes on the window object. */
interface DesktopBridge {
  invoke(command: string, args?: Record<string, unknown>): Promise<unknown>;
}

declare global {
  interface Window {
    __MOCHI__?: DesktopBridge;
  }
}

/**
 * Reads from the Rust core through the desktop shell.
 *
 * The shell itself is not built yet: whether it is Tauri or Electron is the
 * open question R-10 settles at the end of milestone 0. What both would expose
 * is this one call, so the interface is written against it now and neither
 * choice costs a rewrite here.
 */
export class DesktopSource implements DataSource {
  readonly kind = 'desktop' as const;
  readonly canReveal = true;

  static available(): boolean {
    return typeof window !== 'undefined' && typeof window.__MOCHI__?.invoke === 'function';
  }

  async load(): Promise<IndexDocument> {
    const bridge = window.__MOCHI__;
    if (!bridge) throw new Error('the desktop bridge is not available');
    return (await bridge.invoke('load_index')) as IndexDocument;
  }
}

/** The desktop core when it is there, the fixture when it is not. */
export async function resolveSource(): Promise<DataSource> {
  if (DesktopSource.available()) return new DesktopSource();
  const { FixtureSource } = await import('./fixture');
  return new FixtureSource();
}
