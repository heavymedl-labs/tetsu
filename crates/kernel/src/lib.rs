//! The contract every Tetsu module implements (ADR-0003).
//!
//! This is a contract, not a framework: modules plug in at compile time, and
//! the `tetsu` binary registers them.

/// One numbered SQL file. Versions only ever grow, and an applied file never changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Migration {
    pub version: i32,
    pub sql: &'static str,
}

/// Where a module keeps its data: the one Postgres schema it owns, and its migrations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Storage {
    pub schema: &'static str,
    pub migrations: &'static [Migration],
}

/// A module that plugs into the kernel.
pub trait Module {
    /// The module's name, unique among the modules the binary registers.
    fn name(&self) -> &'static str;

    /// The module's storage, if it stores data.
    fn storage(&self) -> Option<Storage> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Plain;

    impl Module for Plain {
        fn name(&self) -> &'static str {
            "plain"
        }
    }

    #[test]
    fn a_module_stores_nothing_unless_it_says_so() {
        assert_eq!(Plain.name(), "plain");
        assert_eq!(Plain.storage(), None);
    }
}
