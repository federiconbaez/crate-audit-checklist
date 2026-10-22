//! Report model, scoring and rendering.

use serde::Serialize;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pass,
    Warn,
    Fail,
    Skip,
}

impl Status {
    fn factor(self) -> Option<f64> {
        match self {
            Status::Pass => Some(1.0),
            Status::Warn => Some(0.5),
            Status::Fail => Some(0.0),
            Status::Skip => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Finding {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

impl Finding {
    pub fn new(message: impl Into<String>) -> Finding {
        Finding {
            message: message.into(),
            file: None,
            line: None,
        }
    }
    pub fn at(message: impl Into<String>, file: impl Into<String>, line: u32) -> Finding {
        Finding {
            message: message.into(),
            file: Some(file.into()),
            line: Some(line),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckResult {
    pub id: &'static str,
    pub status: Status,
    pub weight: u32,
    /// A failing critical check fails the run regardless of the score.
    pub critical: bool,
    pub detail: String,
    pub findings: Vec<Finding>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub tool_version: &'static str,
    pub crate_name: Option<String>,
    pub crate_version: Option<String>,
    pub score: u32,
    pub passed: bool,
    pub fail_under: u32,
    pub checks: Vec<CheckResult>,
}

/// Weighted mean over non-skipped checks, as an integer percentage
/// (warn counts half). With every check skipped the score is 0.
pub fn score(checks: &[CheckResult]) -> u32 {
    let (mut got, mut total) = (0.0, 0.0);
    for c in checks {
        if let Some(f) = c.status.factor() {
            got += f * f64::from(c.weight);
            total += f64::from(c.weight);
        }
    }
    if total == 0.0 {
        0
    } else {
        (got / total * 100.0).round() as u32
    }
}

pub fn passed(checks: &[CheckResult], score: u32, fail_under: u32) -> bool {
    score >= fail_under
        && !checks
            .iter()
            .any(|c| c.critical && c.status == Status::Fail)
}

pub fn render_text(r: &Report) -> String {
    let mut s = String::new();
    let name = r.crate_name.as_deref().unwrap_or("?");
    let ver = r.crate_version.as_deref().unwrap_or("?");
    s.push_str(&format!("crate {name} {ver}\n"));
    for c in &r.checks {
        let tag = match c.status {
            Status::Pass => "PASS",
            Status::Warn => "WARN",
            Status::Fail => "FAIL",
            Status::Skip => "SKIP",
        };
        s.push_str(&format!(
            "{tag}  {:<9} w={:<3} {}\n",
            c.id, c.weight, c.detail
        ));
        for f in c.findings.iter().take(20) {
            match (&f.file, f.line) {
                (Some(file), Some(line)) => {
                    s.push_str(&format!("      {file}:{line}: {}\n", f.message))
                }
                _ => s.push_str(&format!("      {}\n", f.message)),
            }
        }
        if c.findings.len() > 20 {
            s.push_str(&format!("      ... {} more\n", c.findings.len() - 20));
        }
    }
    s.push_str(&format!(
        "score {}/100 (fail-under {}) -> {}\n",
        r.score,
        r.fail_under,
        if r.passed { "PASSED" } else { "FAILED" }
    ));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(status: Status, weight: u32, critical: bool) -> CheckResult {
        CheckResult {
            id: "x",
            status,
            weight,
            critical,
            detail: String::new(),
            findings: vec![],
            duration_ms: 0,
        }
    }

    #[test]
    fn score_is_weighted_and_ignores_skips() {
        assert_eq!(
            score(&[c(Status::Pass, 30, false), c(Status::Fail, 10, false)]),
            75
        );
        assert_eq!(
            score(&[c(Status::Warn, 10, false), c(Status::Skip, 90, false)]),
            50
        );
        assert_eq!(score(&[c(Status::Skip, 10, false)]), 0);
        assert_eq!(score(&[]), 0);
    }

    #[test]
    fn critical_failure_fails_regardless_of_score() {
        let checks = [c(Status::Pass, 99, false), c(Status::Fail, 1, true)];
        assert_eq!(score(&checks), 99);
        assert!(!passed(&checks, 99, 50));
        let ok = [c(Status::Pass, 99, false), c(Status::Fail, 1, false)];
        assert!(passed(&ok, 99, 99));
        assert!(!passed(&ok, 99, 100));
    }
}
