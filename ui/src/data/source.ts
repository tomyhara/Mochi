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

/**
 * Where the interface gets its data.
 *
 * Two implementations exist. The fixture one reads a file produced by
 * `mochi export`, which is what the browser tests and `npm run dev` use. The
 * desktop one will call the Rust core directly. Keeping them behind this
 * interface is what lets the interface be built and tested before the shell
 * around it exists — and the shell is still an open question (R-10).
 */
export interface DataSource {
  readonly kind: 'fixture' | 'desktop';

  /** Read everything. `revealSecrets` asks the source not to mask. */
  load(revealSecrets: boolean): Promise<IndexDocument>;

  /**
   * Whether this source can hand back unmasked content on request.
   *
   * The fixture cannot: it was masked when it was written, which is the
   * point — an export is safe to pass around (FR-8.5). Unmasking has to
   * happen at the source, so the view never holds a secret it is hiding.
   */
  readonly canReveal: boolean;

  /**
   * Whether this source can start a session, rather than only describe how.
   *
   * Nothing can yet. Starting an interactive CLI needs somewhere for it to
   * be interactive, and that is the integrated terminal (FR-7.8), which is
   * milestone 3. Until then the window says so rather than offering a button
   * that does nothing.
   */
  readonly canLaunch: boolean;
}
