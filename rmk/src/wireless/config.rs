//! Configuration for wireless protocols
//!
//! This module provides configuration types for various wireless protocols.
//! Each protocol has its own configuration type (e.g., `GazellConfig` for
//! Nordic Gazell on nRF52840).

/// Common trait for wireless protocol configurations
///
/// This trait provides a protocol-agnostic interface for validating
/// wireless configurations. Each wireless protocol should implement
/// this trait for its configuration type.
///
/// # Example
///
/// ```no_run
/// use rmk::wireless::config::{WirelessConfig, GazellConfig};
///
/// let config = GazellConfig::default();
/// assert!(config.validate());
/// ```
pub trait WirelessConfig {
    /// Validate configuration parameters
    ///
    /// Returns `true` if all parameters are within valid ranges.
    fn validate(&self) -> bool;

    /// Get a human-readable description of the configuration
    fn description(&self) -> &'static str {
        "Wireless configuration"
    }
}

/// Nordic Gazell 2.4GHz protocol configuration
///
/// Contains all parameters for the Gazell protocol on nRF52 series
/// MCUs (nRF52832, nRF52833, nRF52840).
///
/// # Example
///
/// ```no_run
/// use rmk::wireless::GazellConfig;
///
/// let config = GazellConfig {
///     channel: 4,
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Copy)]
pub struct GazellConfig {
    /// RF channel (0-100)
    ///
    /// Each channel is 1MHz wide, starting at 2400MHz:
    /// - Channel 0 = 2400 MHz
    /// - Channel 4 = 2404 MHz
    /// - Channel 100 = 2500 MHz
    ///
    /// **Recommendation**: Use channels 4, 25, 42, 63, or 79 to avoid
    /// WiFi interference (WiFi channels 1, 6, 11).
    pub channel: u8,

    /// Data rate
    ///
    /// Higher data rates provide better throughput but reduced range.
    pub data_rate: DataRate,

    /// Transmit power
    ///
    /// Higher power increases range but consumes more battery.
    pub tx_power: TxPower,

    /// Maximum number of automatic retransmissions
    ///
    /// Range: 0-15
    /// - 0 = No retransmission
    /// - 3 = Recommended for reliability
    /// - 15 = Maximum reliability, higher latency
    pub max_retries: u8,

    /// ACK timeout in microseconds
    ///
    /// Time to wait for acknowledgment before retrying.
    /// Range: 250-4000 μs
    ///
    /// **Recommendation**: 250-500 μs for keyboard applications
    pub ack_timeout_us: u16,

    /// Base address (4 bytes)
    ///
    /// Common base address for all pipes. Should be unique
    /// to avoid interference with other Gazell networks.
    pub base_address: [u8; 4],

    /// Address prefix (pipe 0)
    ///
    /// Each pipe has its own prefix byte combined with base address.
    pub address_prefix: u8,

    /// Gazell pipe number (0-7)
    ///
    /// In device mode, this is the pipe used for transmitting.
    /// In host mode, all pipes are listened on; this field is used
    /// for `is_ready()` checks.
    ///
    /// Default: 0
    pub pipe: u8,

    /// Heartbeat interval in milliseconds for split keyboard communication
    ///
    /// The peripheral (device mode) sends a heartbeat packet at this interval
    /// when there are no key events. This serves two purposes:
    /// 1. Keeps the Gazell link alive
    /// 2. Gives the central (host mode) an opportunity to send ACK payloads
    ///    back to the peripheral (e.g., LED state, layer info)
    ///
    /// Set to 0 to disable heartbeat.
    /// Default: 200 ms
    pub heartbeat_interval_ms: u16,

    /// Startup grace period in milliseconds before heartbeats begin
    ///
    /// Delays the first heartbeat after boot to allow input devices (e.g.,
    /// PMW3610 trackball) to complete SPI initialization without being
    /// disrupted by blocking `gz_send()` calls.
    ///
    /// Default: 2000 ms
    pub startup_grace_ms: u16,

    /// Heartbeat interval when idle (no key/pointing activity)
    ///
    /// After `idle_timeout_ms` of inactivity, the peripheral switches to this
    /// slower heartbeat rate to save power. The first keypress after idle may
    /// have up to this much latency before reaching the central.
    ///
    /// Must be less than the central's disconnect timeout (1000ms).
    /// Default: 500 ms
    pub idle_heartbeat_interval_ms: u16,

    /// Inactivity duration before entering idle mode (ms)
    ///
    /// Default: 5000 ms (5 seconds)
    pub idle_timeout_ms: u16,

    /// Inactivity duration before shutting down the radio (seconds)
    ///
    /// After this many seconds without activity, the peripheral calls
    /// `gz_deinit()` to fully disable the radio, saving ~0.5mA.
    /// The radio is re-initialized on the next keypress or pointing event.
    ///
    /// Set to 0 to disable radio shutdown.
    /// Default: 300 (5 minutes)
    pub deep_idle_timeout_secs: u16,
}

impl Default for GazellConfig {
    fn default() -> Self {
        Self {
            channel: 4,                             // 2404 MHz (safe from WiFi)
            data_rate: DataRate::_1Mbps,            // Good balance
            tx_power: TxPower::Pos0dBm,             // 0dBm (1mW)
            max_retries: 3,                         // Reliable but low latency
            ack_timeout_us: 250,                    // Fast ACK
            base_address: [0xE7, 0xE7, 0xE7, 0xE7], // Default Gazell address
            address_prefix: 0xAA,                   // Default prefix
            pipe: 0,                                // Default pipe 0
            heartbeat_interval_ms: 200,             // 200ms heartbeat (power-optimized)
            startup_grace_ms: 2000,                 // 2s grace period for device init
            idle_heartbeat_interval_ms: 500,        // 2Hz when idle
            idle_timeout_ms: 5000,                  // 5s to enter idle
            deep_idle_timeout_secs: 300,            // 5min to radio shutdown
        }
    }
}

impl WirelessConfig for GazellConfig {
    fn validate(&self) -> bool {
        self.channel <= 100
            && self.max_retries <= 15
            && self.ack_timeout_us >= 250
            && self.ack_timeout_us <= 4000
            && self.pipe <= 7
            // Idle heartbeat should be >= active heartbeat (slower saves power)
            && (self.idle_heartbeat_interval_ms == 0
                || self.idle_heartbeat_interval_ms >= self.heartbeat_interval_ms)
            // Deep idle timeout should be > idle timeout (radio shutdown happens after idle)
            && (self.deep_idle_timeout_secs == 0
                || self.idle_timeout_ms == 0
                || (self.deep_idle_timeout_secs as u64) * 1000 > self.idle_timeout_ms as u64)
    }

    fn description(&self) -> &'static str {
        "Nordic Gazell 2.4GHz protocol (nRF52 series)"
    }
}

impl GazellConfig {
    /// Create a low-latency configuration
    ///
    /// Optimized for keyboard/mouse with minimal latency:
    /// - Fast data rate (2Mbps)
    /// - Low retries
    /// - Short ACK timeout
    pub fn low_latency() -> Self {
        Self {
            data_rate: DataRate::_2Mbps,
            max_retries: 2,
            ack_timeout_us: 250,
            ..Default::default()
        }
    }

    /// Create a long-range configuration
    ///
    /// Optimized for maximum range:
    /// - Lower data rate (1Mbps)
    /// - Maximum TX power
    /// - More retries
    pub fn long_range() -> Self {
        Self {
            data_rate: DataRate::_1Mbps,
            tx_power: TxPower::Pos8dBm,
            max_retries: 5,
            ack_timeout_us: 500,
            ..Default::default()
        }
    }

    /// Create a low-power configuration
    ///
    /// Optimized for battery life:
    /// - Lower TX power
    /// - Fewer retries
    pub fn low_power() -> Self {
        Self {
            data_rate: DataRate::_2Mbps,
            tx_power: TxPower::Neg4dBm,
            max_retries: 2,
            ack_timeout_us: 250,
            ..Default::default()
        }
    }
}

/// Gazell data rate
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataRate {
    /// 1 Mbps - Good balance (default)
    _1Mbps = 1,

    /// 2 Mbps - Minimum latency
    _2Mbps = 2,
}

/// Transmit power levels for nRF52840
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxPower {
    /// -40 dBm
    Neg40dBm = 0xD8,

    /// -20 dBm
    Neg20dBm = 0xEC,

    /// -16 dBm
    Neg16dBm = 0xF0,

    /// -12 dBm
    Neg12dBm = 0xF4,

    /// -8 dBm
    Neg8dBm = 0xF8,

    /// -4 dBm
    Neg4dBm = 0xFC,

    /// 0 dBm (1 mW) - Default
    Pos0dBm = 0x00,

    /// +2 dBm
    Pos2dBm = 0x02,

    /// +3 dBm
    Pos3dBm = 0x03,

    /// +4 dBm
    Pos4dBm = 0x04,

    /// +5 dBm
    Pos5dBm = 0x05,

    /// +6 dBm
    Pos6dBm = 0x06,

    /// +7 dBm
    Pos7dBm = 0x07,

    /// +8 dBm (6.3 mW) - Maximum
    Pos8dBm = 0x08,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_valid() {
        let config = GazellConfig::default();
        assert!(config.validate());
    }

    #[test]
    fn test_low_latency_config_valid() {
        let config = GazellConfig::low_latency();
        assert!(config.validate());
        assert_eq!(config.data_rate, DataRate::_2Mbps);
    }

    #[test]
    fn test_long_range_config_valid() {
        let config = GazellConfig::long_range();
        assert!(config.validate());
        assert_eq!(config.data_rate, DataRate::_1Mbps);
    }

    #[test]
    fn test_low_power_config_valid() {
        let config = GazellConfig::low_power();
        assert!(config.validate());
    }

    #[test]
    fn test_invalid_channel() {
        let mut config = GazellConfig::default();
        config.channel = 101; // Out of range
        assert!(!config.validate());
    }

    #[test]
    fn test_invalid_retries() {
        let mut config = GazellConfig::default();
        config.max_retries = 16; // Out of range
        assert!(!config.validate());
    }
}
