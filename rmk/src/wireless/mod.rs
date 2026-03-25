//! Wireless communication module for RMK
//!
//! This module provides configuration for wireless keyboard communication,
//! currently supporting Nordic Gazell 2.4GHz protocol via the split keyboard driver.

pub mod config;

// Re-export commonly used types
pub use config::GazellConfig;
