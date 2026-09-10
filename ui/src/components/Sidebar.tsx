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

import { useMemo, useState } from 'react';

import { elideMiddle } from '../format';
import type { Repository, Session } from '../types';
import { TOOL_SHORT } from '../types';

/** How many sessions a repository shows before "Show N more" (FR-9.1). */
const COLLAPSED = 5;

interface Props {
  repositories: Repository[];
  sessions: Session[];
  selectedId: number | null;
  filter: string;
  onSelect: (id: number) => void;
}

interface Group {
  key: string;
  name: string;
  path: string | null;
  sessions: Session[];
}

/**
 * Repositories, with their sessions inline.
 *
 * Sessions whose working directory is gone or was never in a repository are
 * grouped separately rather than dropped: FR-3.6 is explicit that they stay
 * visible, and they are usually the ones a user is looking for.
 */
export function Sidebar({ repositories, sessions, selectedId, filter, onSelect }: Props) {
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});

  const groups = useMemo<Group[]>(() => {
    const needle = filter.trim().toLowerCase();
    const matches = (session: Session) =>
      !needle ||
      (session.title ?? '').toLowerCase().includes(needle) ||
      session.nativeId.toLowerCase().includes(needle) ||
      session.tool.includes(needle);

    const visible = sessions.filter(matches);
    const byRepo = new Map<number, Session[]>();
    const loose: Session[] = [];

    for (const session of visible) {
      if (session.repoId === null) {
        loose.push(session);
        continue;
      }
      const list = byRepo.get(session.repoId) ?? [];
      list.push(session);
      byRepo.set(session.repoId, list);
    }

    const result: Group[] = [];
    for (const repo of repositories) {
      const owned = byRepo.get(repo.id) ?? [];
      if (owned.length === 0 && needle) continue;
      result.push({
        key: `repo-${repo.id}`,
        name: repo.displayName,
        path: repo.rootPath,
        sessions: owned,
      });
    }
    if (loose.length > 0) {
      result.push({ key: 'unassigned', name: 'No repository', path: null, sessions: loose });
    }
    return result;
  }, [repositories, sessions, filter]);

  if (groups.length === 0) {
    return (
      <nav className="sidebar" aria-label="Repositories">
        <h2>Repositories</h2>
        <p className="sidebar-empty">Nothing matches “{filter}”.</p>
      </nav>
    );
  }

  return (
    <nav className="sidebar" aria-label="Repositories">
      <h2>Repositories</h2>
      {groups.map((group) => {
        const isOpen = expanded[group.key] ?? false;
        const shown = isOpen ? group.sessions : group.sessions.slice(0, COLLAPSED);
        const hidden = group.sessions.length - shown.length;

        return (
          <div className="repo" key={group.key}>
            <div className="repo-header">
              <span className="repo-name">{group.name}</span>
              <span className="repo-count">{group.sessions.length}</span>
            </div>
            {group.path && (
              <div className="repo-path" title={group.path}>
                {elideMiddle(group.path, 38)}
              </div>
            )}

            {shown.map((session) => (
              <button
                type="button"
                className="session-row"
                key={session.id}
                aria-current={session.id === selectedId}
                onClick={() => onSelect(session.id)}
              >
                <span className="tool-tag">{TOOL_SHORT[session.tool]}</span>
                <span className="title">{session.title ?? session.nativeId}</span>
                {session.parseStatus !== 'ok' && (
                  <span className="badge warn">{session.parseStatus}</span>
                )}
              </button>
            ))}

            {hidden > 0 && (
              <button
                type="button"
                className="show-more"
                onClick={() => setExpanded((state) => ({ ...state, [group.key]: true }))}
              >
                Show {hidden} more
              </button>
            )}
          </div>
        );
      })}
    </nav>
  );
}
