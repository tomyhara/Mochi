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

/**
 * Shorten a path from the middle.
 *
 * The two ends of a path are the informative parts — which volume it is on and
 * which directory it is — so dropping the middle keeps more meaning than an
 * ellipsis at either end. The mock calls this a middle-elided path (M-7).
 *
 * Done here rather than with `direction: rtl`, which reads the string
 * backwards and moves a leading separator to the end.
 */
export function elideMiddle(text: string, max = 44): string {
  if (text.length <= max) return text;
  const keep = max - 1;
  const head = Math.ceil(keep / 2);
  const tail = Math.floor(keep / 2);
  return `${text.slice(0, head)}…${text.slice(text.length - tail)}`;
}
