# Security policy

## Supported versions

Only the latest commit on `main` (currently the 0.1.x line) receives security fixes.
There are no older maintained branches.

## Reporting a vulnerability

Please report vulnerabilities **privately** through GitHub private vulnerability
reporting:

- Go to <https://github.com/federiconbaez/crate-audit-checklist/security/advisories/new>
  (repository **Security** tab -> **Report a vulnerability**).
- Include the affected commit or version, steps to reproduce, and the impact you expect.

Do **not** open a public issue, pull request or discussion for a suspected
vulnerability. The report stays visible only to you and the maintainer until an
advisory is published.

## What to expect

- The maintainer aims to acknowledge every report within **14 days**.
- If the report is confirmed, the fix is developed in a private fork of the
  advisory, released, and the advisory is published with credit to the reporter
  (unless they prefer to stay anonymous).
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
