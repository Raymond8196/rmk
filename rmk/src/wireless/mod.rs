//! Wireless communication module for RMK
//!
//! This module provides configuration for wireless keyboard communication,
//! currently supporting Nordic Gazell 2.4GHz protocol via the split keyboard driver.

pub mod config;

#[cfg(feature = "wireless_gazell")]
pub mod radio_dispatch;

#[cfg(all(feature = "wireless_gazell", feature = "_ble"))]
pub mod connection_manager;

// Re-export commonly used types
pub use config::GazellConfig;
