use std::process::Command;

fn main() {
    // First check if GIT_HASH is set as env var (e.g., in Docker build)
    let git_hash = std::env::var("GIT_HASH")
        .ok()
        .filter(|s| !s.is_empty() && s != "unknown")
        .or_else(|| {
            // Fall back to git command
            Command::new("git")
                .args(["rev-parse", "--short", "HEAD"])
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=GIT_HASH={}", git_hash);
    println!("cargo:rerun-if-changed=.git/HEAD");
}
