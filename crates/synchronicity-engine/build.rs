//! Build script — rebuilds the Milkdown JS bundle when its source changes.

use std::path::Path;
use std::process::Command;

fn main() {
    let milkdown_dir = "assets/milkdown";
    let source = "assets/milkdown/src/index.js";
    let package = "assets/milkdown/package.json";
    let bundle = "assets/milkdown-bundle.js";

    println!("cargo:rerun-if-changed={source}");
    println!("cargo:rerun-if-changed={package}");

    let needs_build = !Path::new(bundle).exists() || {
        let bundle_mtime = std::fs::metadata(bundle).and_then(|m| m.modified()).ok();
        let source_mtime = std::fs::metadata(source).and_then(|m| m.modified()).ok();
        match (bundle_mtime, source_mtime) {
            (Some(b), Some(s)) => s > b,
            _ => true,
        }
    };

    if !needs_build {
        return;
    }

    // Install deps if node_modules is missing
    if !Path::new(&format!("{milkdown_dir}/node_modules")).exists() {
        let status = Command::new("npm")
            .args(["install", "--prefer-offline"])
            .current_dir(milkdown_dir)
            .status()
            .expect("failed to run `npm install` for milkdown — is node installed?");
        assert!(status.success(), "npm install failed for milkdown");
    }

    let status = Command::new("npm")
        .args(["run", "build"])
        .current_dir(milkdown_dir)
        .status()
        .expect("failed to run `npm run build` for milkdown — is node installed?");
    assert!(status.success(), "milkdown esbuild bundle failed");
}
