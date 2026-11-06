//! The contract every Tetsu module implements (ADR-0003).
//!
//! This is a contract, not a framework: modules plug in at compile time, and
//! the `tetsu` binary registers them.

/// A module that plugs into the kernel.
pub trait Module {
    /// The module's name, unique among the modules the binary registers.
    fn name(&self) -> &'static str;
}
