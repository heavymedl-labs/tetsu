//! The Tetsu binary.
//!
//! This is the composition root: the only place that knows about every
//! capability and vertical, and wires adapters to ports. Capability and
//! vertical crates never depend on this crate.

fn main() {
    println!("tetsu {}", env!("CARGO_PKG_VERSION"));
}
