# Contributing

Thanks for your interest. Issues and pull requests are welcome in **English or
Spanish**.

## Getting the software

```sh
git clone https://github.com/federiconbaez/crate-audit-checklist.git
```

See [README.md](README.md) for usage.

## Reporting bugs and requesting features

- Bugs and enhancement requests: open an issue at <https://github.com/federiconbaez/crate-audit-checklist/issues>.
  Include the commit or version, your OS and toolchain version, the exact command,
  and the output you got versus what you expected.
- Security problems: do **not** open a public issue; follow [SECURITY.md](SECURITY.md).

## Contribution process

1. Fork the repository and create a branch from `main`.
2. Make your change, with tests (see below).
3. Run the local checks listed below; all of them must pass.
4. Open a pull request against `main` describing what changed and why.
5. The maintainer reviews it. CI must be green before merging. Merges are done as
   fast-forward when possible so history stays linear.

## Requirements for acceptable contributions

Run these before opening a pull request (the same commands run in CI):

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
# cargo-audit and nightly Miri must be installed; these variables turn a skip into a failure
CAC_REQUIRE_AUDIT=1 CAC_REQUIRE_MIRI=1 cargo test
# dogfood: the tool must score at least 90 on its own repository
cargo run -q --release -- . --fail-under 90
```

A new check or a change to a check's logic needs a fixture under `fixtures/` (or an
extension of an existing one) and an integration test in `tests/fixtures.rs` that
asserts the exact finding. Changes to weights or to the report format must update
the README tables and `schema/report.schema.json`.

General rules:

- **New functionality must come with automated tests** in the existing test suite,
  and bug fixes should add a regression test. A skipped test counts as a failure.
- **Documentation must stay honest:** the README separates what is tested from what
  is not. Do not add numbers or claims that were not measured; if you add a
  measurement, document how to reproduce it.
- Do not commit secrets, private keys, real IP addresses, host paths or personal data.
- By contributing you agree that your contribution is licensed under the
  [Apache License 2.0](LICENSE), the same license as the project.

## Commit message format

Commits follow this format (type and scope as in Conventional Commits):

```
type(scope): short title

Body explaining the change.

Cambios:
- ...

Problemas detectados:
- ...

Ref: CX9-<digits>

License: Apache-2.0
```

`type` is one of `feat`, `fix`, `docs`, `test`, `ci`, `chore`, `refactor`, `perf`.
External contributors may write a plain descriptive message; the maintainer can
adjust it when merging.

## Versioning and releases

- Versions follow [Semantic Versioning](https://semver.org/). The version lives in the
  package manifest and every release is tagged in git as `vX.Y.Z`.
- Every release gets a human-written entry in [CHANGELOG.md](CHANGELOG.md) (not a raw
  `git log`) and a GitHub release with the same notes. The entry lists every fixed
  vulnerability that has a public identifier (CVE, GHSA, RUSTSEC).
- Release steps: update the version and CHANGELOG in a pull request, merge it with
  CI green, tag the merge commit `vX.Y.Z`, push the tag, and create the GitHub
  release from that tag.
