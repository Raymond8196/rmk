#![no_std]
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

//! Hand-written FFI bindings for the Gazell C shim layer.
//!
//! These match the declarations in `c/gazell_shim.h`.
//! We avoid bindgen because it requires libclang, which may not
//! work in all environments (e.g. file-encryption software).

pub type gz_error_t = i32;
pub const GZ_OK: gz_error_t = 0;
pub const GZ_ERR_SEND_FAILED: gz_error_t = -1;
pub const GZ_ERR_RECEIVE_FAILED: gz_error_t = -2;
pub const GZ_ERR_FRAME_TOO_LARGE: gz_error_t = -3;
pub const GZ_ERR_NOT_INITIALIZED: gz_error_t = -4;
pub const GZ_ERR_BUSY: gz_error_t = -5;
pub const GZ_ERR_INVALID_CONFIG: gz_error_t = -6;
pub const GZ_ERR_HARDWARE: gz_error_t = -7;

pub type gz_mode_t = i32;
pub const GZ_MODE_DEVICE: gz_mode_t = 0;
pub const GZ_MODE_HOST: gz_mode_t = 1;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct gz_config_t {
    pub channel: u8,
    pub data_rate: u8,
    pub tx_power: i8,
    pub max_retries: u8,
    pub ack_timeout_us: u16,
    pub base_address: [u8; 4],
    pub address_prefix: u8,
}

impl Default for gz_config_t {
    fn default() -> Self {
        Self {
            channel: 4,                               // Nordic Gazell default channel
            data_rate: 2,                              // 2Mbps (Nordic default)
            tx_power: 4,                               // +4 dBm
            max_retries: 5,
            ack_timeout_us: 600,
            base_address: [0xE7, 0xE7, 0xE7, 0xE7],   // Nordic Gazell default base address
            address_prefix: 0xC0,                      // Nordic Gazell default prefix for pipe 0
        }
    }
}

// On ARM targets: real FFI declarations (linked from C shim + Nordic SDK)
#[cfg(target_arch = "arm")]
extern "C" {
    pub fn gz_init(config: *const gz_config_t) -> gz_error_t;
    pub fn gz_set_mode(mode: gz_mode_t) -> gz_error_t;
    pub fn gz_send(data: *const u8, len: u8, pipe: u8) -> gz_error_t;
    pub fn gz_recv(out_buf: *mut u8, out_len: *mut u8, out_pipe: *mut u8, max_len: u8) -> gz_error_t;
    pub fn gz_is_ready(pipe: u8) -> bool;
    pub fn gz_flush() -> gz_error_t;
    pub fn gz_deinit();
    pub fn gz_set_ack_payload(pipe: u8, data: *const u8, len: u8) -> gz_error_t;
    pub fn gz_get_ack_payload(out_buf: *mut u8, out_len: *mut u8, max_len: u8) -> gz_error_t;
    pub fn gz_init_default(mode: gz_mode_t) -> gz_error_t;
}

// On non-ARM targets: stubs so host builds (cargo test, cargo doc, IDEs) link successfully.
// These are never called at runtime on non-ARM — the feature-gated wrappers in rmk
// ensure FFI calls only happen on actual hardware.

/// # Safety
///
/// This is a stub implementation for non-ARM targets. On real hardware (ARM),
/// this calls the C shim `gz_init()` which requires valid nRF5 hardware context.
/// Never called at runtime on non-ARM systems.
#[cfg(not(target_arch = "arm"))]
pub unsafe fn gz_init(_config: *const gz_config_t) -> gz_error_t {
    GZ_ERR_HARDWARE
}

/// # Safety
///
/// This is a stub implementation for non-ARM targets. On real hardware (ARM),
/// this calls the C shim `gz_set_mode()` which requires Gazell to be initialized.
/// Never called at runtime on non-ARM systems.
#[cfg(not(target_arch = "arm"))]
pub unsafe fn gz_set_mode(_mode: gz_mode_t) -> gz_error_t {
    GZ_ERR_HARDWARE
}

/// # Safety
///
/// This is a stub implementation for non-ARM targets. On real hardware (ARM),
/// this calls the C shim `gz_send()` which requires `data` to point to valid memory
/// of at least `len` bytes. Never called at runtime on non-ARM systems.
#[cfg(not(target_arch = "arm"))]
pub unsafe fn gz_send(_data: *const u8, _len: u8, _pipe: u8) -> gz_error_t {
    GZ_ERR_HARDWARE
}

/// # Safety
///
/// This is a stub implementation for non-ARM targets. On real hardware (ARM),
/// this calls the C shim `gz_recv()` which requires `out_buf`, `out_len`, and
/// `out_pipe` to point to valid, writable memory. Never called at runtime on
/// non-ARM systems.
#[cfg(not(target_arch = "arm"))]
pub unsafe fn gz_recv(_out_buf: *mut u8, _out_len: *mut u8, _out_pipe: *mut u8, _max_len: u8) -> gz_error_t {
    GZ_ERR_HARDWARE
}

/// # Safety
///
/// This is a stub implementation for non-ARM targets. Returns false and has no side effects.
/// Never called at runtime on non-ARM systems.
#[cfg(not(target_arch = "arm"))]
pub unsafe fn gz_is_ready(_pipe: u8) -> bool {
    false
}

/// # Safety
///
/// This is a stub implementation for non-ARM targets. On real hardware (ARM),
/// this calls the C shim `gz_flush()` which requires Gazell to be initialized.
/// Never called at runtime on non-ARM systems.
#[cfg(not(target_arch = "arm"))]
pub unsafe fn gz_flush() -> gz_error_t {
    GZ_ERR_HARDWARE
}

/// # Safety
///
/// This is a stub implementation for non-ARM targets. No-op on non-ARM.
/// Never called at runtime on non-ARM systems.
#[cfg(not(target_arch = "arm"))]
pub unsafe fn gz_deinit() {}

/// # Safety
///
/// This is a stub implementation for non-ARM targets. On real hardware (ARM),
/// this calls the C shim `gz_set_ack_payload()` which queues an ACK payload
/// for the specified pipe (host mode only). `data` must point to valid memory
/// of at least `len` bytes. Never called at runtime on non-ARM systems.
#[cfg(not(target_arch = "arm"))]
pub unsafe fn gz_set_ack_payload(_pipe: u8, _data: *const u8, _len: u8) -> gz_error_t {
    GZ_ERR_HARDWARE
}

/// # Safety
///
/// This is a stub implementation for non-ARM targets. On real hardware (ARM),
/// this calls the C shim `gz_get_ack_payload()` which retrieves the ACK payload
/// from the last successful transmission (device mode only). `out_buf` and `out_len`
/// must point to valid, writable memory. Never called at runtime on non-ARM systems.
#[cfg(not(target_arch = "arm"))]
pub unsafe fn gz_get_ack_payload(_out_buf: *mut u8, _out_len: *mut u8, _max_len: u8) -> gz_error_t {
    GZ_ERR_HARDWARE
}

/// # Safety
///
/// This is a stub implementation for non-ARM targets. On real hardware (ARM),
/// this calls the C shim `gz_init_default()` which initializes Gazell with
/// Nordic defaults and the specified mode. Never called at runtime on non-ARM systems.
#[cfg(not(target_arch = "arm"))]
pub unsafe fn gz_init_default(_mode: gz_mode_t) -> gz_error_t {
    GZ_ERR_HARDWARE
}
