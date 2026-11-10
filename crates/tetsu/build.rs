//! Embeds the front-end build Vite wrote to `assets/build` (ADR-0034), as a table `src/assets.rs` serves from.
//! The work is in `src/manifest.rs`, which the crate's tests cover.

#[path = "src/manifest.rs"]
mod manifest;

use std::path::Path;

fn main() -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/build");
    println!("cargo::rerun-if-changed={}", root.display());
    let json = std::fs::read_to_string(root.join(".vite/manifest.json"))
        .map_err(|error| format!("no front-end build: run npm run build ({error})"))?;
    let table = manifest::table(&root.display().to_string(), &manifest::files(&json)?)?;
    let out = std::env::var("OUT_DIR").map_err(|error| error.to_string())?;
    std::fs::write(Path::new(&out).join("assets.rs"), table).map_err(|error| error.to_string())
}
