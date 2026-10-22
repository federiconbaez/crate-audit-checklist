//! Reproducible review checklist for a Rust crate with a scored JSON report.

pub mod checks;
pub mod report;

use report::{Report, SCHEMA_VERSION};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Config {
    pub checks: Vec<String>,
    pub fail_under: u32,
}

#[derive(Debug)]
pub enum Error {
    NotACrate(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NotACrate(m) => write!(f, "{m}"),
        }
    }
}

pub fn audit(root: &Path, cfg: &Config) -> Result<Report, Error> {
    let manifest_path = root.join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest_path)
        .map_err(|e| Error::NotACrate(format!("{}: {e}", manifest_path.display())))?;
    let manifest: toml::Table = text
        .parse()
        .map_err(|e| Error::NotACrate(format!("{}: {e}", manifest_path.display())))?;
    let pkg = manifest.get("package").and_then(|p| p.as_table());
    let get = |k: &str| {
        pkg.and_then(|p| p.get(k))
            .and_then(|v| v.as_str())
            .map(str::to_string)
    };
    let target_dir = std::env::temp_dir().join(format!(
        "crate-audit-checklist-{}-{}",
        get("name").unwrap_or_else(|| "crate".into()),
        std::process::id()
    ));
    let ctx = checks::Ctx {
        root: root.to_path_buf(),
        manifest: manifest.clone(),
        target_dir,
    };
    let results: Vec<_> = cfg.checks.iter().map(|id| checks::run(id, &ctx)).collect();
    let _ = std::fs::remove_dir_all(&ctx.target_dir);
    let score = report::score(&results);
    Ok(Report {
        schema_version: SCHEMA_VERSION,
        tool_version: env!("CARGO_PKG_VERSION"),
        crate_name: get("name"),
        crate_version: get("version"),
        score,
        passed: report::passed(&results, score, cfg.fail_under),
        fail_under: cfg.fail_under,
        checks: results,
    })
}
