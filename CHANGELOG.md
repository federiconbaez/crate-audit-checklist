# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- `SECURITY.md`, `CONTRIBUTING.md` and this changelog.

## [0.1.0] - not yet tagged

First version, as declared in `Cargo.toml`.

### Added
- CLI `crate-audit-checklist [PATH] [--json] [--out FILE] [--checks LIST] [--miri] [--fail-under N]`
  that runs a review checklist on a Rust crate and emits a scored report following
  `schema/report.schema.json`.
- Checks: `test` (critical), `audit` (critical, `cargo audit`), `clippy`, a lexical
  `unsafe` hygiene scan (`// SAFETY:` comments and `# Safety` docs), publishing
  `metadata`, `fmt`, and optional `miri`.
- 17 tests (9 unit, 8 integration on fixture crates). CI turns missing cargo-audit or
  Miri into failures, dogfoods the tool on this repository with `--fail-under 90` and
  requires the seeded fixture to fail.

### Security
- No known vulnerabilities fixed in this version.
