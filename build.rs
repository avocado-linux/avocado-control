fn main() {
    varlink_generator::cargo_build_tosource("src/varlink/org.avocado.Extensions.varlink", false);
    varlink_generator::cargo_build_tosource("src/varlink/org.avocado.Runtimes.varlink", false);
    varlink_generator::cargo_build_tosource("src/varlink/org.avocado.Hitl.varlink", false);
    varlink_generator::cargo_build_tosource("src/varlink/org.avocado.RootAuthority.varlink", false);

    // Embed git commit hash for version identification
    let git_hash = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=GIT_HASH={git_hash}");
}
