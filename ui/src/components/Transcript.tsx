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

import type { Message, Session } from '../types';

export type TabId = 'transcript' | 'tools' | 'diffs' | 'raw';

export const TABS: { id: TabId; label: string }[] = [
  { id: 'transcript', label: 'Transcript' },
  { id: 'tools', label: 'Tool calls' },
  { id: 'diffs', label: 'Diffs' },
  { id: 'raw', label: 'Raw JSONL' },
];

const ROLE_LABELS: Record<Message['role'], string> = {
  user: 'You',
  assistant: 'Assistant',
  thinking: 'Thinking',
  tool_call: 'Tool call',
  tool_result: 'Tool result',
  system: 'System',
};

function time(value: number | null): string {
  if (value === null) return '';
  return new Date(value).toISOString().slice(11, 19);
}

function Entry({ message }: { message: Message }) {
  return (
    <article className={`entry ${message.role}`} data-role={message.role}>
      <div className="entry-meta">
        <span className="entry-role">{ROLE_LABELS[message.role]}</span>
        <span className="entry-time">{time(message.timestamp)}</span>
      </div>
      <div className="entry-body">
        {message.toolName && <span className="tool-name">{message.toolName}</span>}
        {message.content || <em>(no text)</em>}
      </div>
    </article>
  );
}

function Empty({ title, children }: { title: string; children: React.ReactNode }) {
  // FR-9.6: an empty state should say what to do next, not just that there is
  // nothing here.
  return (
    <div className="empty">
      <h3>{title}</h3>
      {children}
    </div>
  );
}

interface Props {
  session: Session | null;
  tab: TabId;
}

export function Transcript({ session, tab }: Props) {
  if (!session) {
    return (
      <div className="pane">
        <Empty title="No session selected">
          <p>Pick a session from the sidebar to read it.</p>
        </Empty>
      </div>
    );
  }

  if (tab === 'transcript') {
    if (session.messages.length === 0) {
      return (
        <div className="pane">
          <Empty title="This session has no messages">
            <p>
              {session.parseStatus === 'failed'
                ? 'Its file could not be read. The reason is in the metadata rail.'
                : 'It was started but nothing was recorded before it ended.'}
            </p>
          </Empty>
        </div>
      );
    }
    return (
      <div className="pane" id="transcript" tabIndex={-1}>
        <div className="transcript">
          {session.messages.map((message) => (
            <Entry key={message.seq} message={message} />
          ))}
        </div>
      </div>
    );
  }

  if (tab === 'tools') {
    const calls = session.messages.filter(
      (message) => message.role === 'tool_call' || message.role === 'tool_result',
    );
    return (
      <div className="pane">
        {calls.length === 0 ? (
          <Empty title="No tool calls">
            <p>This session was a conversation; the agent ran nothing.</p>
          </Empty>
        ) : (
          <div className="transcript">
            {calls.map((message) => (
              <Entry key={message.seq} message={message} />
            ))}
          </div>
        )}
      </div>
    );
  }

  if (tab === 'raw') {
    const raw = session.messages.filter((message) => message.raw !== null);
    return (
      <div className="pane">
        {raw.length === 0 ? (
          <Empty title="Nothing kept in raw form">
            <p>
              Raw JSON is kept for entries Mochi did not fully understand, so that a format change
              never hides content. Every entry here was understood.
            </p>
          </Empty>
        ) : (
          <div className="transcript">
            {raw.map((message) => (
              <article className="entry tool_call" key={message.seq}>
                <div className="entry-meta">
                  <span className="entry-role">#{message.seq}</span>
                </div>
                <div className="entry-body">{message.raw}</div>
              </article>
            ))}
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="pane">
      <Empty title="Diffs are not extracted yet">
        <p>
          File changes are currently shown as the tool output that produced them, under{' '}
          <strong>Tool calls</strong>.
        </p>
        <p>Reading edits back out as a diff is FR-5.6, and is not built.</p>
      </Empty>
    </div>
  );
}
