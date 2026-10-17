//! `idx`: Indonesian stock market (IDX / IHSG) analysis for humans and AI agents.
//!
//! The binary in `src/main.rs` is a thin wrapper over [`cli::run`]. Everything
//! else is a library so the MCP server and tests can reuse the same code paths.
//!
//! Module map (see `AGENTS.md` for the long version):
//! - [`cli`]       clap subcommands and handlers
//! - [`providers`] the `Provider` trait and one module per data source
//! - [`service`]   provider call + local analysis, shared by CLI and MCP
//! - [`mcp`]       the same capabilities as MCP tools over stdio
//! - [`model`]     provider-agnostic domain types (the JSON contract)
//! - [`output`]    table and JSON renderers
//! - [`http`]      fetcher that puts the cache in front of the network
//! - [`cache`]     content-addressed file cache
//! - [`config`]    flags > env > file > defaults

pub mod analysis;
pub mod cache;
pub mod cli;
pub mod config;
pub mod error;
pub mod http;
pub mod mcp;
pub mod model;
pub mod output;
pub mod providers;
pub mod service;

pub use error::{IdxError, Result};
