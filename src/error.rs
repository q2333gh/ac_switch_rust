//! Shared crate result alias.

/// Standard result type used across the CLI.
pub type Result<T> = anyhow::Result<T>;
