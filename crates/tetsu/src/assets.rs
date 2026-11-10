//! The front-end build, embedded by `build.rs` from Vite's manifest, and served at the exact paths it names
//! (ADR-0034).

/// One built file.
pub struct Asset {
    pub path: &'static str,
    pub content_type: &'static str,
    pub bytes: &'static [u8],
}

include!(concat!(env!("OUT_DIR"), "/assets.rs"));

/// The built file at `path`, such as `/assets/tetsu-BQbDqwUW.css`, if there is one.
#[must_use]
pub fn find(path: &str) -> Option<&'static Asset> {
    ASSETS.iter().find(|asset| asset.path == path)
}
