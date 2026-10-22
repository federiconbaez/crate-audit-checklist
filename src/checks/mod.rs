//! The individual checks. Each one returns a `CheckResult`; checks that need a
//! tool which is not installed report `skip` instead of failing.

pub mod unsafe_scan;

use crate::report::{CheckResult, Finding, Status};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

pub struct Ctx {
    pub root: PathBuf,
    pub manifest: toml::Table,
    /// Separate target dir so the audited crate's own `target/` is untouched.
    pub target_dir: PathBuf,
}

pub const ALL: &[&str] = &[
    "metadata", "fmt", "clippy", "test", "unsafe", "audit", "miri",
];
pub const DEFAULT: &[&str] = &["metadata", "fmt", "clippy", "test", "unsafe", "audit"];

pub fn run(id: &str, ctx: &Ctx) -> CheckResult {
    let t = Instant::now();
    let (weight, critical) = match id {
        "test" => (25, true),
        "audit" => (20, true),
        "clippy" => (15, false),
        "unsafe" => (15, false),
        "metadata" => (10, false),
        "miri" => (10, false),
        _ => (5, false), // fmt
    };
    let (status, detail, findings) = match id {
        "metadata" => metadata(ctx),
        "fmt" => fmt(ctx),
        "clippy" => clippy(ctx),
        "test" => test(ctx),
        "unsafe" => unsafe_check(ctx),
        "audit" => audit(ctx),
        "miri" => miri(ctx),
        other => (Status::Skip, format!("unknown check {other}"), vec![]),
    };
    let id = ALL.iter().find(|x| **x == id).copied().unwrap_or("unknown");
    CheckResult {
        id,
        status,
        weight,
        critical,
        detail,
        findings,
        duration_ms: t.elapsed().as_millis() as u64,
    }
}

type Outcome = (Status, String, Vec<Finding>);

fn cargo(ctx: &Ctx, args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(args)
        .current_dir(&ctx.root)
        .env("CARGO_TARGET_DIR", &ctx.target_dir)
        .env("CARGO_TERM_COLOR", "never")
        .output()
}

fn tool_missing(out: &std::io::Result<std::process::Output>, needle: &str) -> bool {
    match out {
        Err(_) => true,
        Ok(o) => {
            let e = String::from_utf8_lossy(&o.stderr);
            e.contains("no such command") || e.contains(needle)
        }
    }
}

fn metadata(ctx: &Ctx) -> Outcome {
    let pkg = ctx.manifest.get("package").and_then(|p| p.as_table());
    let mut f = Vec::new();
    let has = |k: &str| pkg.and_then(|p| p.get(k)).is_some();
    if !has("license") && !has("license-file") {
        f.push(Finding::new("Cargo.toml: no `license` or `license-file`"));
    }
    for k in ["description", "repository", "rust-version"] {
        if !has(k) {
            f.push(Finding::new(format!("Cargo.toml: no `{k}`")));
        }
    }
    let exists_any = |names: &[&str]| names.iter().any(|n| ctx.root.join(n).exists());
    if !exists_any(&[
        "LICENSE",
        "LICENSE-APACHE",
        "LICENSE-MIT",
        "LICENSE.md",
        "LICENSE.txt",
        "COPYING",
    ]) {
        f.push(Finding::new("no LICENSE file"));
    }
    if !exists_any(&["README.md", "README"]) {
        f.push(Finding::new("no README"));
    }
    if !ctx.root.join(".github/workflows").is_dir() {
        f.push(Finding::new("no CI workflow under .github/workflows"));
    }
    let status = match f.len() {
        0 => Status::Pass,
        1 | 2 => Status::Warn,
        _ => Status::Fail,
    };
    (status, format!("{} metadata gap(s)", f.len()), f)
}

fn fmt(ctx: &Ctx) -> Outcome {
    let out = cargo(ctx, &["fmt", "--all", "--", "--check", "-l"]);
    if tool_missing(&out, "rustfmt")
        && out
            .as_ref()
            .map_or(true, |o| !o.status.success() && o.stdout.is_empty())
    {
        return (Status::Skip, "rustfmt not available".into(), vec![]);
    }
    let out = out.unwrap();
    let root = ctx.root.canonicalize().unwrap_or(ctx.root.clone());
    let files: Vec<Finding> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            Finding::new(format!(
                "not rustfmt-formatted: {}",
                rel(&root, Path::new(l.trim()))
            ))
        })
        .collect();
    if out.status.success() {
        (Status::Pass, "formatted".into(), vec![])
    } else {
        (
            Status::Warn,
            format!("{} file(s) need rustfmt", files.len()),
            files,
        )
    }
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).display().to_string()
}

/// Parses `cargo --message-format=json` output into (errors, warnings).
pub fn parse_diagnostics(stdout: &str, root: &Path) -> (Vec<Finding>, Vec<Finding>) {
    let (mut errs, mut warns) = (Vec::new(), Vec::new());
    for line in stdout.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v["reason"] != "compiler-message" {
            continue;
        }
        let m = &v["message"];
        let level = m["level"].as_str().unwrap_or("");
        let code = m["code"]["code"].as_str().unwrap_or("");
        let text = m["message"].as_str().unwrap_or("");
        if text.starts_with("aborting due to")
            || text.contains("warning emitted")
            || text.contains("warnings emitted")
        {
            continue;
        }
        let msg = if code.is_empty() {
            text.to_string()
        } else {
            format!("{code}: {text}")
        };
        let span = m["spans"]
            .as_array()
            .and_then(|s| s.iter().find(|x| x["is_primary"] == true));
        let finding = match span {
            Some(s) => Finding::at(
                msg,
                rel(root, Path::new(s["file_name"].as_str().unwrap_or("?"))),
                s["line_start"].as_u64().unwrap_or(0) as u32,
            ),
            None => Finding::new(msg),
        };
        // --all-targets compiles the library twice (lib and lib test), so the
        // same diagnostic can appear more than once.
        let bucket = match level {
            "error" => &mut errs,
            "warning" => &mut warns,
            _ => continue,
        };
        if !bucket.contains(&finding) {
            bucket.push(finding);
        }
    }
    (errs, warns)
}

fn clippy(ctx: &Ctx) -> Outcome {
    let out = cargo(
        ctx,
        &["clippy", "--all-targets", "--message-format=json", "-q"],
    );
    if tool_missing(&out, "clippy-driver") {
        return (Status::Skip, "clippy not available".into(), vec![]);
    }
    let out = out.unwrap();
    let (errs, warns) = parse_diagnostics(&String::from_utf8_lossy(&out.stdout), &ctx.root);
    if !errs.is_empty() || !out.status.success() {
        let n = errs.len();
        let mut f = errs;
        f.extend(warns);
        return (Status::Fail, format!("{n} error(s)"), f);
    }
    if warns.is_empty() {
        (Status::Pass, "no warnings".into(), vec![])
    } else {
        (Status::Warn, format!("{} warning(s)", warns.len()), warns)
    }
}

/// Sums `test result:` lines from libtest output: (passed, failed, ignored).
pub fn parse_test_totals(stdout: &str) -> (u64, u64, u64) {
    let mut t = (0, 0, 0);
    for line in stdout.lines() {
        let Some(rest) = line.trim().strip_prefix("test result: ") else {
            continue;
        };
        for part in rest.split(';') {
            let mut it = part.split_whitespace().rev();
            let (Some(word), Some(num)) = (it.next(), it.next()) else {
                continue;
            };
            let n: u64 = num.parse().unwrap_or(0);
            match word {
                "passed" => t.0 += n,
                "failed" => t.1 += n,
                "ignored" => t.2 += n,
                _ => {}
            }
        }
    }
    t
}

fn test(ctx: &Ctx) -> Outcome {
    let out = match cargo(ctx, &["test", "--no-fail-fast"]) {
        Ok(o) => o,
        Err(e) => return (Status::Fail, format!("could not run cargo: {e}"), vec![]),
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    let (p, f, i) = parse_test_totals(&stdout);
    let mut findings: Vec<Finding> = stdout
        .lines()
        .filter_map(|l| {
            l.strip_prefix("test ")
                .and_then(|r| r.strip_suffix(" ... FAILED"))
        })
        .map(|name| Finding::new(format!("failed: {name}")))
        .collect();
    if out.status.success() {
        let status = if p == 0 { Status::Warn } else { Status::Pass };
        let detail = if p == 0 {
            "no tests".to_string()
        } else {
            format!("{p} passed, {i} ignored")
        };
        (status, detail, findings)
    } else {
        if findings.is_empty() {
            let err = String::from_utf8_lossy(&out.stderr);
            let first = err
                .lines()
                .find(|l| l.starts_with("error"))
                .unwrap_or("build or test run failed");
            findings.push(Finding::new(first.to_string()));
        }
        (Status::Fail, format!("{p} passed, {f} failed"), findings)
    }
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        let name = e.file_name();
        if p.is_dir() {
            if name != "target" && !name.to_string_lossy().starts_with('.') {
                rust_files(&p, out);
            }
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

fn unsafe_check(ctx: &Ctx) -> Outcome {
    let mut files = Vec::new();
    for d in ["src", "examples", "benches"] {
        rust_files(&ctx.root.join(d), &mut files);
    }
    let mut total = 0;
    let mut forbids = false;
    let mut f = Vec::new();
    for p in &files {
        let Ok(src) = std::fs::read_to_string(p) else {
            continue;
        };
        let s = unsafe_scan::scan(&src);
        forbids |= s.forbids_unsafe && p.ends_with("src/lib.rs")
            || s.forbids_unsafe && p.ends_with("src/main.rs");
        total += s.sites.len();
        for site in s.sites.iter().filter(|x| !x.justified) {
            let what = match site.kind {
                unsafe_scan::Kind::Block => "unsafe block without a `// SAFETY:` comment",
                unsafe_scan::Kind::Impl => "unsafe impl without a `// SAFETY:` comment",
                unsafe_scan::Kind::Fn => "unsafe fn without a `# Safety` doc section",
                unsafe_scan::Kind::Trait => "unsafe trait without a `# Safety` doc section",
            };
            f.push(Finding::at(what, rel(&ctx.root, p), site.line));
        }
    }
    let detail = format!(
        "{total} unsafe site(s), {} unjustified{}",
        f.len(),
        if forbids { ", forbid(unsafe_code)" } else { "" }
    );
    let status = if f.is_empty() {
        Status::Pass
    } else if f.len() <= 2 {
        Status::Warn
    } else {
        Status::Fail
    };
    (status, detail, f)
}

/// Extracts findings from `cargo audit --json`: (vulnerabilities, warnings).
pub fn parse_audit(json: &str) -> Option<(Vec<Finding>, Vec<Finding>)> {
    let v: Value = serde_json::from_str(json).ok()?;
    let fmt = |e: &Value| {
        let a = &e["advisory"];
        let pkg = e["package"]["name"].as_str().unwrap_or("?");
        let ver = e["package"]["version"].as_str().unwrap_or("?");
        match (a["id"].as_str(), a["title"].as_str()) {
            (Some(id), Some(title)) => Finding::new(format!("{id} {pkg} {ver}: {title}")),
            _ => Finding::new(format!("{pkg} {ver}")), // e.g. yanked: no advisory
        }
    };
    let vulns: Vec<Finding> = v["vulnerabilities"]["list"]
        .as_array()?
        .iter()
        .map(fmt)
        .collect();
    let mut warns = Vec::new();
    if let Some(obj) = v["warnings"].as_object() {
        for (kind, list) in obj {
            for e in list.as_array().into_iter().flatten() {
                let mut f = fmt(e);
                f.message = format!("[{kind}] {}", f.message);
                warns.push(f);
            }
        }
    }
    Some((vulns, warns))
}

fn audit(ctx: &Ctx) -> Outcome {
    let lock = ctx.root.join("Cargo.lock");
    if !lock.exists() {
        return (Status::Skip, "no Cargo.lock to audit".into(), vec![]);
    }
    let out = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["audit", "--json", "--file"])
        .arg(&lock)
        .current_dir(&ctx.root)
        .output();
    if tool_missing(&out, "no such subcommand") {
        return (Status::Skip, "cargo-audit not installed".into(), vec![]);
    }
    let out = out.unwrap();
    match parse_audit(&String::from_utf8_lossy(&out.stdout)) {
        None => (
            Status::Skip,
            "cargo audit produced no report (offline advisory DB?)".into(),
            vec![],
        ),
        Some((v, w)) if !v.is_empty() => {
            let n = v.len();
            let mut f = v;
            f.extend(w);
            (Status::Fail, format!("{n} vulnerable dependenc(ies)"), f)
        }
        Some((_, w)) if !w.is_empty() => {
            (Status::Warn, format!("{} advisory warning(s)", w.len()), w)
        }
        Some(_) => (Status::Pass, "no known advisories".into(), vec![]),
    }
}

fn miri(ctx: &Ctx) -> Outcome {
    let out = Command::new("cargo")
        .args(["+nightly", "miri", "test"])
        .current_dir(&ctx.root)
        .env("CARGO_TARGET_DIR", ctx.target_dir.join("miri"))
        .output();
    if tool_missing(&out, "miri") && out.as_ref().map_or(true, |o| !o.status.success()) {
        let e = out
            .as_ref()
            .map(|o| String::from_utf8_lossy(&o.stderr).into_owned())
            .unwrap_or_default();
        if e.contains("Undefined Behavior") {
            return (
                Status::Fail,
                "Miri reported undefined behavior".into(),
                vec![Finding::new(first_ub(&e))],
            );
        }
        return (
            Status::Skip,
            "cargo +nightly miri not available".into(),
            vec![],
        );
    }
    let out = out.unwrap();
    let e = String::from_utf8_lossy(&out.stderr);
    if out.status.success() {
        (Status::Pass, "miri test passed".into(), vec![])
    } else if e.contains("Undefined Behavior") {
        (
            Status::Fail,
            "Miri reported undefined behavior".into(),
            vec![Finding::new(first_ub(&e))],
        )
    } else {
        (Status::Fail, "miri test failed".into(), vec![])
    }
}

fn first_ub(stderr: &str) -> String {
    stderr
        .lines()
        .find(|l| l.contains("Undefined Behavior"))
        .unwrap_or("")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_totals_sum_all_binaries() {
        let s = "test result: ok. 3 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.01s\n\
                 test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n";
        assert_eq!(parse_test_totals(s), (5, 1, 1));
        assert_eq!(parse_test_totals("nothing"), (0, 0, 0));
    }

    #[test]
    fn diagnostics_split_by_level_with_spans() {
        let line = r#"{"reason":"compiler-message","message":{"level":"warning","message":"unused variable: `x`","code":{"code":"unused_variables"},"spans":[{"file_name":"src/lib.rs","line_start":3,"is_primary":true}]}}"#;
        let summary = r#"{"reason":"compiler-message","message":{"level":"warning","message":"1 warning emitted","code":null,"spans":[]}}"#;
        let err = r#"{"reason":"compiler-message","message":{"level":"error","message":"mismatched types","code":{"code":"E0308"},"spans":[{"file_name":"src/main.rs","line_start":9,"is_primary":true}]}}"#;
        let (e, w) = parse_diagnostics(
            &format!("{line}\n{summary}\n{err}\n{line}\nnot json\n"),
            Path::new("/x"),
        );
        assert_eq!(
            w,
            vec![Finding::at(
                "unused_variables: unused variable: `x`",
                "src/lib.rs",
                3
            )]
        );
        assert_eq!(
            e,
            vec![Finding::at("E0308: mismatched types", "src/main.rs", 9)]
        );
    }

    #[test]
    fn audit_report_parsing() {
        let json = r#"{"vulnerabilities":{"found":true,"count":1,"list":[{"advisory":{"id":"RUSTSEC-2019-0009","title":"Double-free and use-after-free in SmallVec::grow()"},"package":{"name":"smallvec","version":"0.6.9"}}]},
            "warnings":{"unmaintained":[{"advisory":{"id":"RUSTSEC-2020-0001","title":"x is unmaintained"},"package":{"name":"x","version":"1.0.0"}}],
                        "yanked":[{"advisory":null,"package":{"name":"y","version":"0.1.0"}}]}}"#;
        let (v, w) = parse_audit(json).unwrap();
        assert_eq!(
            v[0].message,
            "RUSTSEC-2019-0009 smallvec 0.6.9: Double-free and use-after-free in SmallVec::grow()"
        );
        assert_eq!(
            w[0].message,
            "[unmaintained] RUSTSEC-2020-0001 x 1.0.0: x is unmaintained"
        );
        assert!(parse_audit("garbage").is_none());
        let clean = r#"{"vulnerabilities":{"found":false,"count":0,"list":[]},"warnings":{}}"#;
        assert_eq!(
            parse_audit(clean).map(|(v, w)| (v.len(), w.len())),
            Some((0, 0))
        );
    }
}
