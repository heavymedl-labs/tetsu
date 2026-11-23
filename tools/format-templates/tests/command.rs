//! The command as the checks run it, from a repository's root: `--check` fails on an unformatted template and says
//! which, plain formats it, and a formatted tree passes.

#[cfg(test)]
mod command {
    use std::path::PathBuf;
    use std::process::Command;

    /// A folder in the system's temporary folder, named as the workspace's other scratch files are, so a sweep finds
    /// any left behind, and removed when dropped.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn run(root: &Scratch, args: &[&str]) -> (bool, String) {
        let output = Command::new(env!("CARGO_BIN_EXE_tetsu-format-templates"))
            .args(args)
            .current_dir(&root.0)
            .output()
            .unwrap();
        (
            output.status.success(),
            String::from_utf8(output.stderr).unwrap(),
        )
    }

    #[test]
    fn check_fails_until_the_templates_are_formatted() {
        let root = Scratch(
            std::env::temp_dir().join(format!("tetsu-{}-format-command", std::process::id())),
        );
        let folder = root.0.join("web/templates");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::create_dir_all(root.0.join("crates")).unwrap();
        std::fs::write(folder.join("page.html"), "<ul>\n<li>x</li>\n</ul>\n").unwrap();

        let (passed, said) = run(&root, &["--check"]);
        assert!(!passed);
        assert!(
            said.starts_with("error: ./web/templates/page.html is not formatted."),
            "{said}"
        );
        assert_eq!(run(&root, &[]), (true, String::new()));
        assert_eq!(run(&root, &["--check"]), (true, String::new()));
    }
}
