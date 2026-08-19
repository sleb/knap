#![deny(clippy::correctness)]
#![warn(clippy::suspicious, clippy::perf)]

pub mod cli;
pub(crate) mod config;
pub(crate) mod edit;
pub(crate) mod handlers;
pub(crate) mod index;
pub(crate) mod parser;
pub(crate) mod server;
#[cfg(test)]
mod test_helpers;

// Exposed only so this crate's own `tests/` and `examples/` (which are
// compiled as separate crates linking against `knap` as a dependency) can
// reach test-only internals. Not part of the public API: gated behind an
// opt-in feature and hidden from docs, so a normal `cargo add knap` never
// sees it. Enabled automatically for this crate's own dev builds via the
// self-referencing `[dev-dependencies]` entry in Cargo.toml.
#[cfg(feature = "test-support")]
#[doc(hidden)]
pub use handlers::slug;
#[cfg(feature = "test-support")]
#[doc(hidden)]
pub use server::run as server_run;
