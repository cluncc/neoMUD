//! Library crate root — exposes internal modules for integration testing.
//! Not part of the binary surface; only used by tests/.

pub mod color;
pub mod commands;
pub mod config;
pub mod entity;
pub mod events;
pub mod scripting;
pub mod session;
pub mod state;
pub mod world;

// combat, server, ssh, time are internal only (not needed by integration tests)
pub mod combat;
pub mod time;
