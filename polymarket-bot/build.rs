use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/index");

    let git_sha = command_output("git", &["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".into());
    let git_dirty = command_stdout("git", &["status", "--porcelain"])
        .map(|status| {
            if status.trim().is_empty() {
                "false"
            } else {
                "true"
            }
        })
        .unwrap_or_else(|| "unknown".into());
    let build_timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "unknown".into());

    println!("cargo:rustc-env=POLYMARKET_GIT_SHA={git_sha}");
    println!("cargo:rustc-env=POLYMARKET_GIT_DIRTY={git_dirty}");
    println!("cargo:rustc-env=POLYMARKET_BUILD_TIMESTAMP={build_timestamp}");
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let value = command_stdout(program, args)?;
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn command_stdout(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}
