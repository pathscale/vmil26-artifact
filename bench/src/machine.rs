//! What produced these numbers.
//!
//! The counts in this report are machine-independent and the timings are not.
//! Recording the machine is what lets a reader tell which is which, and stops
//! a timing from a laptop on battery being compared against one from the same
//! laptop on mains six weeks later.
//!
//! The engine commit is here for a different reason. The harness depends on an
//! exact public `ps-blitz` Git revision, so every report repeats that revision
//! rather than relying on the reader to inspect a Cargo lockfile.

use std::process::Command;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Machine {
    pub chip: String,
    pub cores: String,
    pub memory_bytes: Option<u64>,
    pub os: String,
    pub arch: String,
    pub rustc: String,
    pub cargo: String,
    /// The harness's own build profile. A debug-profile timing is not a
    /// finding about a binding, so the profile has to travel with the number.
    pub profile: &'static str,
    pub engine: Engine,
}

/// The `ps-blitz` revision this run measured.
#[derive(Debug, Clone, Serialize)]
pub struct Engine {
    pub path: String,
    pub commit: Option<String>,
    pub branch: Option<String>,
    /// Always false for the pinned Git dependency; retained in the report
    /// schema so old and new reports remain comparable.
    pub dirty: bool,
}

fn run(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

const ENGINE_COMMIT: &str = "7e1f0bb096c19a24599b017c6bef0b18616c2d18";
const ENGINE_SOURCE: &str =
    "https://github.com/pathscale/ps-blitz/tree/7e1f0bb096c19a24599b017c6bef0b18616c2d18";

impl Machine {
    #[must_use]
    pub fn detect() -> Self {
        let engine = Engine {
            commit: Some(ENGINE_COMMIT.to_string()),
            branch: None,
            dirty: false,
            path: ENGINE_SOURCE.to_string(),
        };

        Self {
            chip: run("sysctl", &["-n", "machdep.cpu.brand_string"])
                .unwrap_or_else(|| "unknown".into()),
            cores: run("sysctl", &["-n", "hw.ncpu"]).unwrap_or_else(|| "unknown".into()),
            memory_bytes: run("sysctl", &["-n", "hw.memsize"]).and_then(|m| m.parse().ok()),
            os: run("sw_vers", &["-productVersion"])
                .map(|version| format!("macOS {version}"))
                .unwrap_or_else(|| std::env::consts::OS.to_string()),
            arch: std::env::consts::ARCH.to_string(),
            rustc: run("rustc", &["--version"]).unwrap_or_else(|| "unknown".into()),
            cargo: run("cargo", &["--version"]).unwrap_or_else(|| "unknown".into()),
            profile: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
            engine,
        }
    }
}
