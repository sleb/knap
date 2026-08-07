#![deny(clippy::correctness)]
#![warn(clippy::suspicious, clippy::perf)]

pub mod cli;
pub mod config;
pub mod edit;
pub mod handlers;
pub mod index;
pub mod parser;
pub mod server;
#[cfg(test)]
mod test_helpers;
