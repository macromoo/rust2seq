#![feature(rustc_private)]

// Wires up the rustc-private linkage that the driver crate needs at runtime.
fn main() {
    rustc_plugin::build_main();
}
