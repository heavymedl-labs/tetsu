//! Reads Vite's manifest (ADR-0034). `build.rs` uses it to embed the built files, and the crate's tests cover it
//! here, since a build script's own code is outside coverage and mutation testing.

use std::collections::BTreeMap;
use std::fmt::Write as _;

#[derive(serde::Deserialize)]
struct Entry {
    file: String,
}

/// Each source and the file it built to, such as `src/tetsu.css` and `assets/tetsu-BQbDqwUW.css`, in source
/// order.
///
/// # Errors
/// Fails if the manifest does not parse, or names a file outside `assets/` or one Tetsu cannot serve.
pub fn files(json: &str) -> Result<Vec<(String, String)>, String> {
    let entries: BTreeMap<String, Entry> = serde_json::from_str(json)
        .map_err(|error| format!("Vite's manifest does not parse: {error}"))?;
    entries
        .into_iter()
        .map(|(source, Entry { file })| {
            let plain = file.strip_prefix("assets/").is_some_and(|name| {
                !name.is_empty()
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
            });
            if plain && !file.contains("..") && content_type(&file).is_some() {
                Ok((source, file))
            } else {
                Err(format!(
                    "{source} built to {file}, which Tetsu does not serve"
                ))
            }
        })
        .collect()
}

/// The Rust source of the asset table: each built file's URL, content type, and bytes, read from `root` with
/// `include_bytes!`, and the shared stylesheet's URL as `STYLESHEET`.
///
/// # Errors
/// Fails if the build has no `src/tetsu.css`.
pub fn table(root: &str, files: &[(String, String)]) -> Result<String, String> {
    let stylesheet = files
        .iter()
        .find(|(source, _)| source == "src/tetsu.css")
        .map(|(_, file)| format!("/{file}"))
        .ok_or("the build has no src/tetsu.css")?;
    let entries = files
        .iter()
        .try_fold(String::new(), |mut entries, (_, file)| {
            let content_type = content_type(file).unwrap_or("application/octet-stream");
            writeln!(
                entries,
                "    Asset {{ path: {:?}, content_type: {content_type:?}, bytes: include_bytes!({:?}) }},",
                format!("/{file}"),
                format!("{root}/{file}"),
            )?;
            Ok::<_, std::fmt::Error>(entries)
        })
        .map_err(|error| error.to_string())?;
    Ok(format!(
        "pub const ASSETS: &[Asset] = &[\n{entries}];\npub const STYLESHEET: &str = {stylesheet:?};\n"
    ))
}

/// The content type a built file is served with, by its extension.
#[must_use]
pub fn content_type(file: &str) -> Option<&'static str> {
    match file.rsplit_once('.')?.1 {
        "css" => Some("text/css; charset=utf-8"),
        "js" => Some("text/javascript; charset=utf-8"),
        "woff2" => Some("font/woff2"),
        "svg" => Some("image/svg+xml"),
        "png" => Some("image/png"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_source_maps_to_its_built_file() {
        let json = r#"{"src/tetsu.css": {"file": "assets/tetsu-BQbDqwUW.css", "isEntry": true},
                       "_shared-C1gR_m3i.js": {"file": "assets/shared-C1gR_m3i.js"}}"#;
        assert_eq!(
            files(json).unwrap(),
            [
                (
                    "_shared-C1gR_m3i.js".to_string(),
                    "assets/shared-C1gR_m3i.js".to_string()
                ),
                (
                    "src/tetsu.css".to_string(),
                    "assets/tetsu-BQbDqwUW.css".to_string()
                ),
            ]
        );
    }

    #[test]
    fn a_file_outside_assets_or_of_an_unknown_kind_is_refused() {
        for file in [
            "tetsu.css",
            "assets/../secret.css",
            "assets/a/b.css",
            "assets/",
            "assets/x.wasm",
            "/assets/x.css",
        ] {
            let json = format!(r#"{{"src/x": {{"file": "{file}"}}}}"#);
            assert!(files(&json).unwrap_err().contains(file), "{file}");
        }
    }

    #[test]
    fn a_manifest_that_does_not_parse_says_so() {
        assert!(files("not json").unwrap_err().contains("does not parse"));
        assert!(files(r#"{"src/x": {"name": "x"}}"#).is_err());
    }

    #[test]
    fn the_table_embeds_each_file_and_names_the_stylesheet() {
        let files = [
            (
                "src/tetsu.css".to_string(),
                "assets/tetsu-Ab1.css".to_string(),
            ),
            (
                "src/font.woff2".to_string(),
                "assets/font-Cd2.woff2".to_string(),
            ),
        ];
        let table = table("/build", &files).unwrap();
        assert!(table.contains(r#"Asset { path: "/assets/tetsu-Ab1.css", content_type: "text/css; charset=utf-8", bytes: include_bytes!("/build/assets/tetsu-Ab1.css") },"#), "{table}");
        assert!(
            table.contains(r#"path: "/assets/font-Cd2.woff2", content_type: "font/woff2""#),
            "{table}"
        );
        assert!(
            table.ends_with("];\npub const STYLESHEET: &str = \"/assets/tetsu-Ab1.css\";\n"),
            "{table}"
        );
    }

    #[test]
    fn a_build_without_the_stylesheet_is_refused() {
        let files = [(
            "src/other.css".to_string(),
            "assets/other-Ab1.css".to_string(),
        )];
        assert!(
            table("/build", &files)
                .unwrap_err()
                .contains("src/tetsu.css")
        );
    }

    #[test]
    fn content_types_follow_the_extension() {
        assert_eq!(
            content_type("assets/a.css"),
            Some("text/css; charset=utf-8")
        );
        assert_eq!(
            content_type("assets/a.js"),
            Some("text/javascript; charset=utf-8")
        );
        assert_eq!(content_type("assets/a.woff2"), Some("font/woff2"));
        assert_eq!(content_type("assets/a.svg"), Some("image/svg+xml"));
        assert_eq!(content_type("assets/a.png"), Some("image/png"));
        assert_eq!(content_type("assets/a"), None);
    }
}
