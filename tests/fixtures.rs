//! Runs the real binary against the fixture crates and checks the JSON report.
//! Each fixture is copied to a temp dir first so nothing is written into the repo.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_crate-audit-checklist");

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if src.is_dir() {
            if e.file_name() != "target" {
                copy_tree(&src, &dst);
            }
        } else {
            std::fs::copy(&src, &dst).unwrap();
        }
    }
}

struct Fixture(PathBuf);
impl Fixture {
    fn new(name: &str) -> Fixture {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dst =
            std::env::temp_dir().join(format!("cac-fixture-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dst);
        copy_tree(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures")
                .join(name),
            &dst,
        );
        Fixture(dst)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run(dir: &Path, extra: &[&str]) -> (i32, Value) {
    let out = Command::new(BIN)
        .arg(dir)
        .arg("--json")
        .args(extra)
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("bad JSON ({e}): {}", String::from_utf8_lossy(&out.stderr)));
    (out.status.code().unwrap(), v)
}

fn check<'a>(r: &'a Value, id: &str) -> &'a Value {
    r["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap_or_else(|| panic!("no check {id}"))
}

fn messages(c: &Value) -> Vec<String> {
    c["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["message"].as_str().unwrap().to_string())
        .collect()
}

fn audit_required() -> bool {
    std::env::var("CAC_REQUIRE_AUDIT").is_ok_and(|v| v == "1")
}

#[test]
fn clean_fixture_scores_100_and_exits_0() {
    let f = Fixture::new("clean");
    let (code, r) = run(
        &f.0,
        &[
            "--checks",
            "metadata,fmt,clippy,test,unsafe",
            "--fail-under",
            "100",
        ],
    );
    assert_eq!(code, 0, "{r:#}");
    assert_eq!(r["score"], 100);
    assert_eq!(r["passed"], true);
    assert_eq!(r["crate_name"], "fixture-clean");
    for c in r["checks"].as_array().unwrap() {
        assert_eq!(c["status"], "pass", "{c:#}");
    }
    assert_eq!(
        check(&r, "unsafe")["detail"],
        "1 unsafe site(s), 0 unjustified"
    );
}

#[test]
fn seeded_fixture_reports_every_seeded_finding() {
    let f = Fixture::new("seeded");
    let (code, r) = run(&f.0, &["--checks", "metadata,fmt,clippy,test,unsafe"]);
    assert_eq!(code, 1, "a failing critical check (test) must fail the run");
    assert_eq!(r["passed"], false);

    let meta = check(&r, "metadata");
    assert_eq!(meta["status"], "fail");
    assert_eq!(meta["findings"].as_array().unwrap().len(), 7);

    let fmt = check(&r, "fmt");
    assert_eq!(fmt["status"], "warn");
    assert_eq!(messages(fmt), vec!["not rustfmt-formatted: src/lib.rs"]);

    let clippy = messages(check(&r, "clippy"));
    assert!(
        clippy
            .iter()
            .any(|m| m.starts_with("clippy::needless_return")),
        "{clippy:?}"
    );
    assert!(
        clippy
            .iter()
            .any(|m| m.starts_with("clippy::missing_safety_doc")),
        "{clippy:?}"
    );
    let mut dedup = clippy.clone();
    dedup.dedup();
    assert_eq!(
        dedup.len(),
        clippy.len(),
        "duplicated diagnostics: {clippy:?}"
    );

    let test = check(&r, "test");
    assert_eq!(test["status"], "fail");
    assert_eq!(test["critical"], true);
    assert_eq!(messages(test), vec!["failed: tests::seeded_failure"]);
    assert_eq!(test["detail"], "1 passed, 1 failed");

    let uns = check(&r, "unsafe");
    assert_eq!(uns["status"], "fail");
    let lines: Vec<u64> = uns["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["line"].as_u64().unwrap())
        .collect();
    assert_eq!(lines, vec![6, 8, 17]);

    // weights: metadata 10 fail, fmt 5 warn, clippy 15 warn, test 25 fail, unsafe 15 fail
    // => (2.5 + 7.5) / 70 = 14.29 -> 14
    assert_eq!(r["score"], 14);
}

#[test]
fn vulnerable_lockfile_fails_audit_or_skips_without_cargo_audit() {
    let f = Fixture::new("vuln-lock");
    let (code, r) = run(&f.0, &["--checks", "audit"]);
    let a = check(&r, "audit");
    if a["status"] == "skip" && !audit_required() {
        eprintln!("SKIP: {}", a["detail"]);
        return;
    }
    assert_eq!(a["status"], "fail", "{a:#}");
    assert!(
        messages(a)
            .iter()
            .any(|m| m.starts_with("RUSTSEC-2019-0009 smallvec 0.6.9")),
        "{a:#}"
    );
    assert_eq!(code, 1);
}

#[test]
fn json_report_matches_the_published_schema() {
    let schema: Value = serde_json::from_str(include_str!("../schema/report.schema.json"))
        .expect("schema is valid JSON");
    let g = Fixture::new("seeded");
    let (_, r) = run(&g.0, &["--checks", "metadata,unsafe,fmt"]);
    let mut errors = Vec::new();
    validate(&schema, &schema, &r, "$", &mut errors);
    assert!(errors.is_empty(), "{errors:#?}");
    // and the validator does catch violations
    let mut broken = r.clone();
    broken["score"] = Value::from("high");
    broken["checks"][0]["status"] = Value::from("maybe");
    broken.as_object_mut().unwrap().remove("passed");
    let mut errors = Vec::new();
    validate(&schema, &schema, &broken, "$", &mut errors);
    assert_eq!(errors.len(), 3, "{errors:#?}");
}

/// Minimal JSON Schema subset: type, required, properties, items, enum,
/// minimum/maximum, $ref to #/$defs/*.
fn validate(root: &Value, s: &Value, v: &Value, path: &str, errs: &mut Vec<String>) {
    if let Some(r) = s["$ref"].as_str() {
        let name = r.trim_start_matches("#/$defs/");
        return validate(root, &root["$defs"][name], v, path, errs);
    }
    if let Some(t) = s.get("type") {
        let types: Vec<&str> = match t {
            Value::String(x) => vec![x.as_str()],
            Value::Array(xs) => xs.iter().filter_map(|x| x.as_str()).collect(),
            _ => vec![],
        };
        let ok = types.iter().any(|t| match *t {
            "object" => v.is_object(),
            "array" => v.is_array(),
            "string" => v.is_string(),
            "integer" => v.is_u64() || v.is_i64(),
            "number" => v.is_number(),
            "boolean" => v.is_boolean(),
            "null" => v.is_null(),
            _ => false,
        });
        if !ok {
            errs.push(format!("{path}: expected {types:?}, got {v}"));
            return;
        }
    }
    if let Some(e) = s["enum"].as_array() {
        if !e.contains(v) {
            errs.push(format!("{path}: {v} not in {e:?}"));
        }
    }
    if let (Some(min), Some(n)) = (s["minimum"].as_f64(), v.as_f64()) {
        if n < min {
            errs.push(format!("{path}: {n} < {min}"));
        }
    }
    if let (Some(max), Some(n)) = (s["maximum"].as_f64(), v.as_f64()) {
        if n > max {
            errs.push(format!("{path}: {n} > {max}"));
        }
    }
    if let Some(req) = s["required"].as_array() {
        for k in req.iter().filter_map(|k| k.as_str()) {
            if v.get(k).is_none() {
                errs.push(format!("{path}: missing `{k}`"));
            }
        }
    }
    if let (Some(props), Some(obj)) = (s["properties"].as_object(), v.as_object()) {
        for (k, sub) in props {
            if let Some(x) = obj.get(k) {
                validate(root, sub, x, &format!("{path}.{k}"), errs);
            }
        }
    }
    if let (Some(items), Some(arr)) = (s.get("items"), v.as_array()) {
        for (i, x) in arr.iter().enumerate() {
            validate(root, items, x, &format!("{path}[{i}]"), errs);
        }
    }
}

#[test]
fn usage_and_not_a_crate_exit_codes() {
    let code = |args: &[&str]| {
        Command::new(BIN)
            .args(args)
            .output()
            .unwrap()
            .status
            .code()
            .unwrap()
    };
    assert_eq!(code(&["--checks", "nope"]), 2);
    assert_eq!(code(&["--fail-under", "101"]), 2);
    assert_eq!(code(&["--bogus"]), 2);
    let empty = std::env::temp_dir().join(format!("cac-empty-{}", std::process::id()));
    std::fs::create_dir_all(&empty).unwrap();
    assert_eq!(code(&[empty.to_str().unwrap()]), 3);
    std::fs::remove_dir_all(empty).unwrap();
}

#[test]
fn out_file_gets_the_same_report() {
    let g = Fixture::new("clean");
    let out = g.0.with_extension("json");
    let o = Command::new(BIN)
        .arg(&g.0)
        .args([
            "--checks",
            "metadata,unsafe",
            "--out",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(o.status.success());
    assert!(String::from_utf8_lossy(&o.stdout).contains("score 100/100"));
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!(v["schema_version"], 1);
    std::fs::remove_file(out).unwrap();
}

fn miri_required() -> bool {
    std::env::var("CAC_REQUIRE_MIRI").is_ok_and(|v| v == "1")
}

#[test]
fn miri_catches_ub_that_native_tests_miss() {
    let f = Fixture::new("ub");
    let (code, r) = run(&f.0, &["--checks", "test,miri"]);
    assert_eq!(
        check(&r, "test")["status"],
        "pass",
        "the UB is invisible natively"
    );
    let m = check(&r, "miri");
    if m["status"] == "skip" && !miri_required() {
        eprintln!("SKIP: {}", m["detail"]);
        return;
    }
    assert_eq!(m["status"], "fail", "{m:#}");
    assert_eq!(m["detail"], "Miri reported undefined behavior");
    assert!(messages(m)[0].contains("Undefined Behavior"), "{m:#}");
    assert_eq!(
        code, 0,
        "miri is not critical: score 25/35 passes with fail-under 0"
    );
}

#[test]
fn miri_passes_on_the_clean_fixture() {
    let f = Fixture::new("clean");
    let (_, r) = run(&f.0, &["--checks", "miri"]);
    let m = check(&r, "miri");
    if m["status"] == "skip" && !miri_required() {
        eprintln!("SKIP: {}", m["detail"]);
        return;
    }
    assert_eq!(m["status"], "pass", "{m:#}");
}
