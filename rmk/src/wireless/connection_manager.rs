//! Phase 4.3: Connection Manager architecture (**interface only — not yet implemented**)
//!
//! Manages dynamic switching between Gazell 2.4GHz and BLE connections
//! on nRF52840-based split keyboards.
//!
//! **Status**: Design types and constants only. No runtime logic.
//! The `gazell_ble` config path in codegen rejects this at compile time.
//! This module will be activated when Phase 4.3 implementation begins.
//!
//! # Architecture Overview
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │         ConnectionManager (task)         │
//! │                                         │
//! │  current_mode: RadioMode                │
//! │  ├── Gazell mode:                       │
//! │  │   run_gazell_central_hub             │
//! │  │   run_gazell_pipe_manager × N        │
//! │  └── BLE mode:                          │
//! │      run_ble_peripheral_manager × N     │
//! │      ble_central_task (scan/connect)    │
//! │                                         │
//! │  switch_request ← Channel<SwitchEvent>  │
//! │  (from keycode handler)                 │
//! └─────────────────────────────────────────┘
//! ```
//!
//! # State Machine
//!
//! ```text
//!                  ┌─────────┐
//!        boot ───► │ Gazell  │ ◄── default (dongle mode)
//!                  └────┬────┘
//!                       │ SwitchToBle
//!                       ▼
//!                  ┌─────────┐
//!                  │Switching│ ─── gz_deinit + stop advertising
//!                  │ (BLE)   │ ─── settle delay (200ms)
//!                  └────┬────┘ ─── switch_to_ble() + ble advertise
//!                       │
//!                       ▼
//!                  ┌─────────┐
//!                  │   BLE   │ ─── advertising / connected
//!                  └────┬────┘
//!                       │ SwitchToGazell
//!                       ▼
//!                  ┌─────────┐
//!                  │Switching│ ─── stop advertising + settle
//!                  │(Gazell) │ ─── switch_to_gazell() + gz_init
//!                  └────┬────┘
//!                       │
//!                       ▼
//!                  ┌─────────┐
//!                  │ Gazell  │ ◄── loop
//!                  └─────────┘
//! ```
//!
//! # Switching Latency Budget
//!
//! | Transition          | Steps                              | Est. Latency |
//! |---------------------|------------------------------------|-------------|
//! | Gazell → BLE        | gz_deinit + settle + switch + adv  | ~300ms      |
//! | BLE → Gazell        | adv stop + settle + switch + init  | ~350ms      |
//! | Idle → Gazell       | switch + gz_init                   | ~50ms       |
//! | Idle → BLE          | switch + adv start                 | ~100ms      |
//!
//! # Error Recovery
//!
//! - BLE advertise failure after Gazell deinit → retry with backoff (3x), then
//!   fall back to Gazell
//! - Gazell init failure after BLE stop → retry with backoff (3x), then
//!   fall back to BLE
//! - Persistent failure → stay in last known good mode, log error
//!
//! # Key Bindings (Phase 4.3)
//!
//! The switch trigger will be a keycode action (e.g., `SwitchToGazell`,
//! `SwitchToBle`, `ToggleConnection`) processed by the keyboard task
//! and sent to ConnectionManager via a channel.
//!
//! # Implementation Status
//!
//! This is a design document. Implementation requires:
//! 1. Hardware verification of Phase 4.2 PoC (BLE pause/resume)
//! 2. `connection = "gazell_ble"` config support in keyboard.toml
//! 3. New keycode variants for connection switching
//! 4. Integration with existing BLE central/peripheral drivers

use core::sync::atomic::AtomicU8;

use crate::wireless::radio_dispatch::RadioMode;

/// Events that trigger connection mode changes.
#[derive(Debug, Clone, Copy)]
pub enum SwitchEvent {
    /// Switch to Gazell mode (user pressed switch key).
    ToGazell,
    /// Switch to BLE mode (user pressed switch key).
    ToBle,
    /// Fallback to alternative mode (error recovery).
    Fallback,
}

/// Current state of the ConnectionManager state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ManagerState {
    /// Gazell is active and running.
    GazellActive = 0,
    /// BLE is active (advertising or connected).
    BleActive = 1,
    /// Transitioning from Gazell to BLE.
    SwitchingToBle = 2,
    /// Transitioning from BLE to Gazell.
    SwitchingToGazell = 3,
    /// Error state — stuck in last known good mode.
    Error = 4,
}

/// Static state of the ConnectionManager, accessible from ISR context.
pub static MANAGER_STATE: AtomicU8 = AtomicU8::new(ManagerState::GazellActive as u8);

impl ManagerState {
    /// Convert from raw u8 value.
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0 => Some(Self::GazellActive),
            1 => Some(Self::BleActive),
            2 => Some(Self::SwitchingToBle),
            3 => Some(Self::SwitchingToGazell),
            4 => Some(Self::Error),
            _ => None,
        }
    }

    /// Returns the RadioMode for this manager state.
    pub fn radio_mode(&self) -> RadioMode {
        match self {
            Self::GazellActive | Self::SwitchingToGazell => RadioMode::Gazell,
            Self::BleActive | Self::SwitchingToBle => RadioMode::Ble,
            Self::Error => RadioMode::Idle,
        }
    }
}

/// Maximum number of retry attempts for mode switching.
pub const MAX_SWITCH_RETRIES: u8 = 3;

/// Settle delay after stopping a protocol before switching RADIO (ms).
pub const SETTLE_DELAY_MS: u64 = 200;

/// Backoff delay between switch retries (ms).
pub const RETRY_BACKOFF_MS: u64 = 500;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manager_state_roundtrip() {
        for state in [
            ManagerState::GazellActive,
            ManagerState::BleActive,
            ManagerState::SwitchingToBle,
            ManagerState::SwitchingToGazell,
            ManagerState::Error,
        ] {
            assert_eq!(ManagerState::from_u8(state as u8), Some(state));
        }
        assert_eq!(ManagerState::from_u8(5), None);
    }

    #[test]
    fn manager_state_radio_mode_mapping() {
        assert_eq!(ManagerState::GazellActive.radio_mode(), RadioMode::Gazell);
        assert_eq!(ManagerState::SwitchingToGazell.radio_mode(), RadioMode::Gazell);
        assert_eq!(ManagerState::BleActive.radio_mode(), RadioMode::Ble);
        assert_eq!(ManagerState::SwitchingToBle.radio_mode(), RadioMode::Ble);
        assert_eq!(ManagerState::Error.radio_mode(), RadioMode::Idle);
    }

    #[test]
    fn manager_state_initial_gazell() {
        use core::sync::atomic::Ordering;
        assert_eq!(
            ManagerState::from_u8(MANAGER_STATE.load(Ordering::Relaxed)),
            Some(ManagerState::GazellActive)
        );
    }
}
