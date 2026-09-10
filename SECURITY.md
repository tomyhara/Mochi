# Security Policy

Mochi reads agent CLI session logs, which routinely contain API keys, tokens and
the contents of `.env` files. Security issues in Mochi can therefore expose the
most sensitive material on a developer's machine. We take reports seriously.

## Reporting a vulnerability

**Do not open a public issue for a security problem.**

Report privately through GitHub Security Advisories:
<https://github.com/tomyhara/Mochi/security/advisories/new>

Please include:

- affected version (Mochi version, OS, and CLI tool versions if relevant)
- a description of the impact
- reproduction steps, ideally with a minimal sample session file
  **from which you have removed every real credential**

## What to expect

| Stage | Target |
| --- | --- |
| Acknowledgement of the report | 3 business days |
| Initial assessment (severity, affected versions) | 10 business days |
| Fix or documented mitigation for High/Critical issues | 30 days |

We will credit reporters in the release notes unless asked not to.

## Scope

In scope:

- leakage of session content, credentials or file paths off the machine
  (Mochi is designed to make no outbound network calls at all — any outbound
  traffic is a bug, see NFR-3.1)
- command injection through session data, paths or session IDs (NFR-3.6)
- script execution from rendered transcript content (NFR-3.7)
- modification or deletion of the original session files outside the explicit
  delete flow (NFR-3.5, FR-8.3)
- secrets written to Mochi's own logs or index in a less protected location
  (NFR-3.4, NFR-5.5)
- failures of the secret masking filter that cause a credential to be shown or
  exported when masking is enabled (NFR-3.3)

Out of scope:

- vulnerabilities in Codex CLI, Claude Code or OpenCode themselves — report
  those to the respective projects
- the fact that a CLI launched from Mochi's integrated terminal can run
  arbitrary commands. That is the agent's normal behaviour and is explicitly
  not a sandbox (FR-7.8i)
- secrets that are present in the user's own session files and shown in the
  UI with masking deliberately turned off

## Handling of test data

Golden files in this repository are derived from real sessions. They must never
contain real credentials or personal data. CI runs a secret scan over
`crates/mochi-core/tests/golden` and fails the build on a hit (NFR-4b.4). If you
find a credential that slipped through, treat it as a security report.
