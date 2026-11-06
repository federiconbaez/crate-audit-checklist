# crate-audit-checklist

CLI que corre una checklist reproducible de revisión sobre un crate de Rust y emite un
reporte JSON con puntaje: tests, clippy, rustfmt, `cargo audit`, higiene de `unsafe`,
metadatos de publicación y, opcionalmente, Miri.

*English below.*

## Español

### Uso

```sh
crate-audit-checklist [RUTA] [--json] [--out reporte.json] [--checks LISTA] [--miri] [--fail-under N]
```

```
# extracto de la salida sobre fixtures/seeded
crate fixture-seeded 0.2.0
FAIL  metadata  w=10  7 metadata gap(s)
WARN  fmt       w=5   1 file(s) need rustfmt
WARN  clippy    w=15  3 warning(s)
      src/lib.rs:13: clippy::needless_return: unneeded `return` statement
FAIL  test      w=25  1 passed, 1 failed
      failed: tests::seeded_failure
FAIL  unsafe    w=15  3 unsafe site(s), 3 unjustified
      src/lib.rs:17: unsafe block without a `// SAFETY:` comment
score 14/100 (fail-under 0) -> FAILED
```

Códigos de salida: `0` aprobado, `1` reprobado (puntaje bajo `--fail-under` o falla de un
check crítico), `2` uso incorrecto, `3` no es un crate. El reporte sigue
[`schema/report.schema.json`](schema/report.schema.json).

### Checks

| id | peso | crítico | qué hace |
|---|---:|:---:|---|
| `test` | 25 | sí | `cargo test --no-fail-fast`; suma resultados y lista los tests fallidos |
| `audit` | 20 | sí | `cargo audit --json` sobre `Cargo.lock`; vulnerabilidades = fail, avisos = warn |
| `clippy` | 15 | | `cargo clippy --all-targets --message-format=json`; errores y warnings con archivo:línea, sin duplicados |
| `unsafe` | 15 | | escaneo léxico propio: cada bloque/`impl` `unsafe` necesita `// SAFETY:` y cada `unsafe fn` una sección `# Safety` |
| `metadata` | 10 | | `license`, `description`, `repository`, `rust-version`, LICENSE, README, CI |
| `miri` | 10 | | `cargo +nightly miri test` (sólo con `--miri`) |
| `fmt` | 5 | | `cargo fmt --check -l` |

Puntaje = media ponderada de los checks no salteados (warn vale la mitad). Si falta una
herramienta (`cargo-audit`, Miri, clippy) el check queda `skip`, no `fail`. Todo `cargo` corre
con un `CARGO_TARGET_DIR` temporal: el `target/` del crate auditado no se toca.

El escáner de `unsafe` ignora comentarios (incluidos anidados), strings, raw strings y
literales de carácter, así que `"unsafe {"` en un string no cuenta.

### Tests (17)

- **9 unitarios:** puntaje y regla de check crítico; parseo de resultados de libtest, de
  diagnósticos JSON de cargo (con deduplicación) y de `cargo audit --json` (incluido `yanked`
  sin advisory); el escáner de `unsafe` contra comentarios, strings, raw strings, escapes,
  bloques multilínea, `forbid(unsafe_code)` y clasificación justificado/no justificado.
- **8 de integración** con el binario real sobre crates de prueba en `fixtures/`, copiados a
  un directorio temporal:
  - `clean`: 100/100, todos los checks en pass, código 0;
  - `seeded`: hallazgos sembrados detectados uno por uno (7 huecos de metadatos, archivo sin
    formato, `needless_return` y `missing_safety_doc`, test fallido, `unsafe` en las líneas
    6, 8 y 17) y puntaje exacto 14;
  - `vuln-lock`: un `Cargo.lock` con `smallvec 0.6.9` debe dar `RUSTSEC-2019-0009`;
  - `ub`: una lectura fuera del slice que **pasa** `cargo test` nativo pero Miri reporta como
    *Undefined Behavior*; Miri también pasa sobre `clean`;
  - el JSON valida contra el schema (con un validador mínimo que además se prueba con un
    reporte roto), `--out` y códigos de salida 2/3.

En CI `cargo-audit` y Miri están instalados y `CAC_REQUIRE_AUDIT=1`/`CAC_REQUIRE_MIRI=1`
convierten un skip en falla. Además el CI corre la herramienta sobre este mismo repo con
`--fail-under 90` y exige que el fixture sembrado falle.

### Medido

Sobre este repo (release, VM de 8 vCPU, caché de cargo caliente): 94/100 antes de tener
README y 100/100 con él (clon limpio), ~9,4 s de punta a punta, dominado por `cargo test` y `clippy` del crate auditado.

### Límites

- El escáner de `unsafe` es léxico, no un parser de Rust: no ve `unsafe` producido por
  macros y una sección `# Safety` sólo se busca en los comentarios de doc inmediatamente
  arriba.
- No audita workspaces de varios crates como unidad; se corre por crate.
- Los pesos son una opinión explícita (tabla de arriba), no un estándar.

## English

CLI that runs a reproducible review checklist on a Rust crate and emits a scored JSON report:
`cargo test` (critical, 25), `cargo audit` (critical, 20), clippy with file:line findings (15),
a lexical `unsafe` hygiene scan requiring `// SAFETY:` comments and `# Safety` docs (15),
publishing metadata (10), optional Miri (10) and rustfmt (5). Score is the weighted mean of
non-skipped checks (warn counts half); a failing critical check fails the run. Missing tools
make a check `skip`, never `fail`. Exit codes: 0 passed, 1 failed, 2 usage, 3 not a crate.
Report format: [`schema/report.schema.json`](schema/report.schema.json).

**Tests (17):** 9 unit (scoring, libtest/cargo-JSON/cargo-audit parsing, the `unsafe` lexer
against comments, strings, raw strings, escapes and multi-line blocks) and 8 integration tests
running the real binary on fixture crates: `clean` scores 100; `seeded` yields every planted
finding and an exact score of 14; `vuln-lock` must report RUSTSEC-2019-0009; `ub` passes native
`cargo test` but Miri reports Undefined Behavior; the JSON validates against the schema; exit
codes. CI installs cargo-audit and Miri and turns skips into failures, dogfoods the tool on
this repo with `--fail-under 90`, and requires the seeded fixture to fail.

**Measured:** on this repo, release build, warm cargo cache, 8-vCPU VM: 94/100 before the
README existed and 100/100 with it (fresh clone), ~9.4 s end to end, dominated by the audited crate's own test and clippy runs.

**Limits:** the `unsafe` scan is lexical (no macro expansion); one crate at a time, not whole
workspaces; the weights are an explicit opinion.

## Contributing, bugs and security / Contribuir, errores y seguridad

- Bugs and feature requests / errores y pedidos: <https://github.com/federiconbaez/crate-audit-checklist/issues>
- How to contribute / cómo contribuir: [CONTRIBUTING.md](CONTRIBUTING.md)
- Reporting vulnerabilities, no private channel yet / reportar vulnerabilidades, todavía sin canal privado: [SECURITY.md](SECURITY.md)
- Changes per version / cambios por versión: [CHANGELOG.md](CHANGELOG.md)

## License

Apache-2.0.
