## What this changes

<!-- One or two sentences. What behaviour is different after this PR? -->

## Requirements covered

<!-- e.g. FR-3.3, NFR-6.4. Use "n/a" for chores. -->

## How it was tested

- [ ] `cargo test --workspace` passes locally
- [ ] `cargo fmt --all -- --check` and `cargo clippy -- -D warnings` are clean
- [ ] new behaviour has a test that fails without the change

## Checklist

- [ ] No change writes to the original session files (FR-2.5, NFR-3.5)
- [ ] No new outbound network call or telemetry (NFR-3.1, NFR-4b.8)
- [ ] External processes are still launched from an argument array, never a
      shell string (NFR-3.6)
- [ ] Any new golden file is free of credentials and personal paths (NFR-4b.4)
- [ ] New source files carry the Apache-2.0 header (NFR-4b.1b)
