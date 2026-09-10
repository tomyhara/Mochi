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

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { MetadataRail } from './components/MetadataRail';
import { TABS, Transcript, type TabId } from './components/Transcript';
import { Sidebar } from './components/Sidebar';
import { resolveSource } from './data/desktop';
import type { DataSource } from './data/source';
import type { IndexDocument, Session } from './types';
import { TOOL_NAMES } from './types';

type Theme = 'system' | 'light' | 'dark';

export function App() {
  const [source, setSource] = useState<DataSource | null>(null);
  const [document_, setDocument] = useState<IndexDocument | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [tab, setTab] = useState<TabId>('transcript');
  const [filter, setFilter] = useState('');
  const [theme, setTheme] = useState<Theme>('system');
  const [copied, setCopied] = useState(false);
  const searchRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const resolved = await resolveSource();
        const loaded = await resolved.load();
        if (cancelled) return;
        setSource(resolved);
        setDocument(loaded);
        // Open the most recently touched session, which is what the user was
        // doing last (FR-4.4).
        const first = [...loaded.sessions].sort(
          (a, b) => (b.updatedAt ?? 0) - (a.updatedAt ?? 0),
        )[0];
        setSelectedId(first ? first.id : null);
      } catch (cause) {
        if (!cancelled) setError(cause instanceof Error ? cause.message : String(cause));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  // FR-9.2: follow the operating system unless the user says otherwise.
  useEffect(() => {
    const root = window.document.documentElement;
    if (theme === 'system') root.removeAttribute('data-theme');
    else root.setAttribute('data-theme', theme);
  }, [theme]);

  const sessions = useMemo(() => {
    if (!document_) return [];
    return [...document_.sessions].sort((a, b) => (b.updatedAt ?? 0) - (a.updatedAt ?? 0));
  }, [document_]);

  const session: Session | null =
    sessions.find((candidate) => candidate.id === selectedId) ?? null;

  const repository =
    document_ && session && session.repoId !== null
      ? document_.repositories.find((repo) => repo.id === session.repoId) ?? null
      : null;

  const copyCommand = useCallback(() => {
    if (!session?.resume.command) return;
    void navigator.clipboard?.writeText(session.resume.command);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 2000);
  }, [session]);

  // FR-9.4 / FR-9.5: the things people reach for constantly get a key.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
      }
      if (event.key === 'Escape' && window.document.activeElement === searchRef.current) {
        setFilter('');
        searchRef.current?.blur();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  if (error) {
    return (
      <div className="empty" role="alert">
        <h3>Mochi could not load its index</h3>
        <p>{error}</p>
        <p>Run `mochi scan` and try again.</p>
      </div>
    );
  }

  if (!document_ || !source) {
    return (
      <div className="empty">
        <h3>Loading…</h3>
      </div>
    );
  }

  return (
    <div className="app">
      <a className="skip-link" href="#transcript">
        Skip to transcript
      </a>

      <header className="topbar">
        <div className="breadcrumb" aria-label="Location">
          <span className="crumb">
            <strong>{repository ? repository.displayName : 'No repository'}</strong>
          </span>
          <span className="sep">/</span>
          <span className="crumb">{session ? TOOL_NAMES[session.tool] : '—'}</span>
          <span className="sep">/</span>
          <span className="crumb">{session?.title ?? '—'}</span>
        </div>

        <div className="search">
          <input
            ref={searchRef}
            type="search"
            placeholder="Search sessions"
            aria-label="Search sessions"
            value={filter}
            onChange={(event) => setFilter(event.target.value)}
          />
          <kbd>{navigator.platform.startsWith('Mac') ? '⌘K' : 'Ctrl+K'}</kbd>
        </div>

        <button
          type="button"
          className="action"
          onClick={copyCommand}
          disabled={!session?.resume.command}
        >
          {copied ? 'Copied' : 'Copy command'}
        </button>

        <button
          type="button"
          className="action primary"
          disabled={!session?.resume.available}
          title={session?.resume.reason ?? undefined}
        >
          Resume
        </button>

        <button
          type="button"
          className="action"
          aria-label="Colour theme"
          onClick={() => setTheme(theme === 'system' ? 'light' : theme === 'light' ? 'dark' : 'system')}
        >
          {theme === 'system' ? 'Theme: OS' : theme === 'light' ? 'Theme: Light' : 'Theme: Dark'}
        </button>
      </header>

      <Sidebar
        repositories={document_.repositories}
        sessions={sessions}
        selectedId={selectedId}
        filter={filter}
        onSelect={(id) => {
          setSelectedId(id);
          setTab('transcript');
        }}
      />

      <main className="centre">
        <div className="tabs" role="tablist" aria-label="Session views">
          {TABS.map((entry) => (
            <button
              type="button"
              role="tab"
              className="tab"
              key={entry.id}
              aria-selected={tab === entry.id}
              onClick={() => setTab(entry.id)}
            >
              {entry.label}
            </button>
          ))}
        </div>
        <Transcript session={session} tab={tab} />
      </main>

      <MetadataRail
        session={session}
        repository={repository}
        source={source}
        masked={document_.masked}
      />
    </div>
  );
}
