# Security policy

## Supported versions

Only the latest commit on `main` (currently the 0.1.x line) receives security fixes.
There are no older maintained branches.

## Reporting a vulnerability

**There is currently no private reporting channel for this repository**: GitHub
private vulnerability reporting is not enabled, and the project does not publish an
email address. The process is:

1. If the problem can be described without giving away how to exploit it (for
   example a crash on malformed input, or a dependency advisory), open an issue at
   <https://github.com/federiconbaez/crate-audit-checklist/issues> with the affected commit or version and steps to reproduce.
2. If describing it would expose users before a fix exists, open an issue titled
   **"Security contact request"** that contains **no technical details**. The
   maintainer will answer in that issue to agree on a private way to receive the
   details before you share them.

Please do not publish exploit details in issues, pull requests or elsewhere before a
fix is available.

## What to expect

- The maintainer aims to acknowledge every report within **14 days**. This is a
  target, not a track record: no vulnerability report has been received so far.
- Confirmed vulnerabilities are fixed on `main`, released with a new version, and
  disclosed with credit to the reporter (unless they prefer to stay anonymous).
- Release notes in [CHANGELOG.md](CHANGELOG.md) list every fixed vulnerability that
  has a CVE, GHSA or RUSTSEC identifier.

## Scope

crate-audit-checklist runs `cargo` subcommands (`test`, `clippy`, `fmt`, `audit`,
optionally `miri`) on the crate you point it at. **Running it on a crate executes that
crate's build scripts, proc-macros and tests**, exactly like `cargo test` would: only
audit crates you would be willing to build. That behavior is by design and is not a
vulnerability.

Relevant issues include: the tool modifying the audited crate (it is meant to use a
temporary `CARGO_TARGET_DIR` and leave the crate's files untouched), reporting a pass
for a check that did not run, writing outside the requested `--out` path, or crashing
on crafted input files.
