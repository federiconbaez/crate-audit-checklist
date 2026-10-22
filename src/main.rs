use crate_audit_checklist::{audit, checks, report, Config};
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
usage: crate-audit-checklist [PATH] [--json] [--out FILE] [--checks LIST] [--miri] [--fail-under N]

  PATH          crate root (default: .)
  --json        print the JSON report instead of text
  --out FILE    also write the JSON report to FILE
  --checks LIST comma-separated subset of: metadata,fmt,clippy,test,unsafe,audit,miri
  --miri        add the miri check (needs `cargo +nightly miri`)
  --fail-under  minimum score (0-100) for exit code 0

exit codes: 0 passed, 1 failed (score below threshold or a critical check failed),
            2 usage error, 3 not a crate";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut root = PathBuf::from(".");
    let (mut json, mut out, mut fail_under) = (false, None, 0u32);
    let mut list: Vec<String> = checks::DEFAULT.iter().map(|s| s.to_string()).collect();
    let mut miri = false;
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        let mut val = || {
            i += 1;
            args.get(i).cloned()
        };
        match a {
            "--json" => json = true,
            "--miri" => miri = true,
            "--out" => match val() {
                Some(v) => out = Some(PathBuf::from(v)),
                None => return usage("--out needs a file"),
            },
            "--fail-under" => match val()
                .and_then(|v| v.parse().ok())
                .filter(|n: &u32| *n <= 100)
            {
                Some(n) => fail_under = n,
                None => return usage("--fail-under needs 0-100"),
            },
            "--checks" => match val() {
                Some(v) => {
                    list = v
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    if let Some(bad) = list.iter().find(|c| !checks::ALL.contains(&c.as_str())) {
                        return usage(&format!("unknown check `{bad}`"));
                    }
                }
                None => return usage("--checks needs a list"),
            },
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            s if s.starts_with('-') => return usage(&format!("unknown flag {s}")),
            s => root = PathBuf::from(s),
        }
        i += 1;
    }
    if miri && !list.iter().any(|c| c == "miri") {
        list.push("miri".into());
    }
    let r = match audit(
        &root,
        &Config {
            checks: list,
            fail_under,
        },
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("crate-audit-checklist: {e}");
            return ExitCode::from(3);
        }
    };
    let js = serde_json::to_string_pretty(&r).expect("report serializes");
    if let Some(p) = out {
        if let Err(e) = std::fs::write(&p, format!("{js}\n")) {
            eprintln!("crate-audit-checklist: {}: {e}", p.display());
            return ExitCode::from(3);
        }
    }
    if json {
        println!("{js}");
    } else {
        print!("{}", report::render_text(&r));
    }
    if r.passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn usage(msg: &str) -> ExitCode {
    eprintln!("crate-audit-checklist: {msg}\n\n{USAGE}");
    ExitCode::from(2)
}
