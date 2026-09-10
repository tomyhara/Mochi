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

import type { DataSource } from '../data/source';
import type { Repository, Session } from '../types';
import { TOOL_NAMES } from '../types';

interface Props {
  session: Session | null;
  repository: Repository | null;
  source: DataSource;
  masked: boolean;
  busy: boolean;
  onSetMasking: (reveal: boolean) => void;
}

function bytes(value: number): string {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / (1024 * 1024)).toFixed(1)} MB`;
}

function Field({ label, value }: { label: string; value: string }) {
  return (
    <div className="field">
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}

/** The right rail: what this session is, and what can be done with it. */
export function MetadataRail({ session, repository, source, masked, busy, onSetMasking }: Props) {
  if (!session) {
    return (
      <aside className="rail" aria-label="Session details">
        <h2>Session</h2>
        <p style={{ color: 'var(--text-muted)' }}>Nothing selected.</p>
      </aside>
    );
  }

  return (
    <aside className="rail" aria-label="Session details">
      {session.parseStatus === 'archived' && (
        <p className="notice">
          The original transcript has been deleted by the tool that wrote it. What you are reading is
          Mochi&rsquo;s copy, which is now the only one.
        </p>
      )}
      {session.parseStatus === 'partial' && session.parseError && (
        <p className="notice">Read with problems: {session.parseError}</p>
      )}
      {session.parseStatus === 'failed' && (
        <p className="notice">This session could not be read: {session.parseError}</p>
      )}
      {!session.cwdExists && session.cwd && (
        <p className="notice">
          Working directory missing. You can still read this session, but it cannot be resumed.
        </p>
      )}

      <h2>Session</h2>
      <dl>
        <Field label="Tool" value={TOOL_NAMES[session.tool]} />
        <Field label="Session id" value={session.nativeId} />
        {session.model && <Field label="Model" value={session.model} />}
        {session.gitBranch && <Field label="Branch" value={session.gitBranch} />}
        <Field label="Messages" value={String(session.messageCount)} />
        <Field
          label="Tokens"
          value={`${session.tokensIn.toLocaleString()} in · ${session.tokensOut.toLocaleString()} out`}
        />
      </dl>

      <h2>Location</h2>
      <dl>
        <Field label="Repository" value={repository ? repository.displayName : 'None'} />
        <Field label="Working directory" value={session.cwd ?? 'Not recorded'} />
        <Field label="Source file" value={session.sourcePath} />
        <Field label="Size" value={bytes(session.sourceSize)} />
      </dl>

      <h2>Secrets</h2>
      <p style={{ color: 'var(--text-muted)', fontSize: 12.5, margin: '0 0 10px' }}>
        {masked
          ? 'Credentials are masked in what you see.'
          : 'Credentials are shown in full.'}
      </p>
      <button
        type="button"
        className="action"
        disabled={!source.canReveal || busy}
        onClick={() => onSetMasking(masked)}
      >
        {busy ? 'Reading…' : masked ? 'Reveal secrets' : 'Mask secrets'}
      </button>
      {!source.canReveal && (
        <p style={{ color: 'var(--text-muted)', fontSize: 12, marginTop: 8 }}>
          This data was masked when it was exported, so there is nothing here to reveal. Unmasking
          happens where the files are read.
        </p>
      )}

      <h2>Resume</h2>
      {session.resume.available && session.resume.command ? (
        // Shown rather than offered as a second button: copying already lives
        // in the header (FR-9.1b), and seeing the command is what tells you it
        // will start in the right directory (FR-7.1).
        <dl>
          <div className="field">
            <dt>Command</dt>
            <dd>{session.resume.command}</dd>
          </div>
        </dl>
      ) : (
        <p style={{ color: 'var(--text-muted)', fontSize: 12.5, margin: 0 }}>
          Cannot resume: {session.resume.reason ?? 'unknown reason'}.
        </p>
      )}
    </aside>
  );
}
