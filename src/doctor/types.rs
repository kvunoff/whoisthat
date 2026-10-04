use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckItem {
    pub name: String,
    pub status: CheckStatus,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix_hint: Option<String>,
}

impl CheckItem {
    pub fn pass(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Pass,
            message: message.into(),
            fix_hint: None,
        }
    }

    pub fn warn(
        name: impl Into<String>,
        message: impl Into<String>,
        fix_hint: Option<String>,
    ) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Warn,
            message: message.into(),
            fix_hint,
        }
    }

    pub fn fail(
        name: impl Into<String>,
        message: impl Into<String>,
        fix_hint: Option<String>,
    ) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Fail,
            message: message.into(),
            fix_hint,
        }
    }

    pub fn info(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Info,
            message: message.into(),
            fix_hint: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckCategory {
    pub title: String,
    pub items: Vec<CheckItem>,
}

impl CheckCategory {
    pub fn new(title: impl Into<String>, items: Vec<CheckItem>) -> Self {
        Self {
            title: title.into(),
            items,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorSummary {
    pub passed: usize,
    pub warnings: usize,
    pub failed: usize,
    pub info: usize,
    pub is_healthy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    pub version: String,
    pub categories: Vec<CheckCategory>,
    pub summary: DoctorSummary,
}

impl DoctorReport {
    pub fn new(categories: Vec<CheckCategory>) -> Self {
        let mut passed = 0;
        let mut warnings = 0;
        let mut failed = 0;
        let mut info = 0;

        for cat in &categories {
            for item in &cat.items {
                match item.status {
                    CheckStatus::Pass => passed += 1,
                    CheckStatus::Warn => warnings += 1,
                    CheckStatus::Fail => failed += 1,
                    CheckStatus::Info => info += 1,
                }
            }
        }

        let is_healthy = failed == 0;
        let summary = DoctorSummary {
            passed,
            warnings,
            failed,
            info,
            is_healthy,
        };

        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            categories,
            summary,
        }
    }

    pub fn print_pretty(&self) {
        let color = std::env::var_os("NO_COLOR").is_none();

        let bold = if color { "\x1b[1m" } else { "" };
        let reset = if color { "\x1b[0m" } else { "" };
        let green = if color { "\x1b[32m" } else { "" };
        let yellow = if color { "\x1b[33m" } else { "" };
        let red = if color { "\x1b[31m" } else { "" };
        let cyan = if color { "\x1b[36m" } else { "" };
        let dim = if color { "\x1b[2m" } else { "" };

        println!(
            "{bold}WhoisThat Doctor v{} — System Diagnostics{reset}\n",
            self.version
        );

        for cat in &self.categories {
            println!("{bold}[+] {}{reset}", cat.title);
            for item in &cat.items {
                let (sym, col) = match item.status {
                    CheckStatus::Pass => ("✓", green),
                    CheckStatus::Warn => ("⚠", yellow),
                    CheckStatus::Fail => ("✗", red),
                    CheckStatus::Info => ("ℹ", cyan),
                };

                println!(
                    "  {col}{sym}{reset} {bold}{}:{reset} {}",
                    item.name, item.message
                );

                if let Some(ref fix) = item.fix_hint {
                    println!("    {yellow}→ Fix:{reset} {dim}{}{reset}", fix);
                }
            }
            println!();
        }

        // Summary line
        let status_color = if self.summary.failed > 0 {
            red
        } else if self.summary.warnings > 0 {
            yellow
        } else {
            green
        };

        let verdict = if self.summary.failed > 0 {
            "Some checks failed. See fix suggestions above."
        } else if self.summary.warnings > 0 {
            "System is usable with warnings."
        } else {
            "All systems operational!"
        };

        println!(
            "{bold}Result:{reset} {} passed, {} warnings, {} failed, {} info. {status_color}{bold}{}{reset}",
            self.summary.passed,
            self.summary.warnings,
            self.summary.failed,
            self.summary.info,
            verdict
        );
    }
}
