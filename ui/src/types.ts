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

/** The shape `mochi export` produces. See crates/mochi-cli/src/main.rs. */

export type ToolId = 'claude_code' | 'codex' | 'opencode';

export type Role = 'user' | 'assistant' | 'tool_call' | 'tool_result' | 'system' | 'thinking';

export type ParseStatus = 'ok' | 'partial' | 'failed' | 'archived';

export interface Repository {
  id: number;
  displayName: string;
  rootPath: string;
  remoteUrl: string | null;
  rootCommit: string | null;
  isWorktree: boolean;
  hidden: boolean;
}

export interface Message {
  seq: number;
  role: Role;
  content: string;
  toolName: string | null;
  timestamp: number | null;
  raw: string | null;
}

/** Whether a session can be restarted, and why not when it cannot. */
export interface Resume {
  available: boolean;
  command: string | null;
  reason: string | null;
}

export interface Session {
  id: number;
  tool: ToolId;
  nativeId: string;
  repoId: number | null;
  title: string | null;
  model: string | null;
  cwd: string | null;
  cwdExists: boolean;
  gitBranch: string | null;
  startedAt: number | null;
  updatedAt: number | null;
  messageCount: number;
  tokensIn: number;
  tokensOut: number;
  status: string;
  parseStatus: ParseStatus;
  parseError: string | null;
  sourcePath: string;
  sourceSize: number;
  resume: Resume;
  messages: Message[];
}

export interface IndexDocument {
  schema: number;
  /** Whether secrets were masked when this was produced (NFR-3.3). */
  masked: boolean;
  repositories: Repository[];
  sessions: Session[];
}

export const TOOL_NAMES: Record<ToolId, string> = {
  claude_code: 'Claude Code',
  codex: 'Codex CLI',
  opencode: 'OpenCode',
};

export const TOOL_SHORT: Record<ToolId, string> = {
  claude_code: 'CC',
  codex: 'CDX',
  opencode: 'OC',
};
