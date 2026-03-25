# rmk-gazell-sys

Low-level FFI bindings to Nordic Gazell protocol for nRF52 series MCUs.

This is a `-sys` crate providing unsafe bindings to the Nordic nRF5 SDK's Gazell protocol implementation. Most users should use the split keyboard codegen (`connection = "gazell"` in `keyboard.toml`) rather than this crate directly.

## Overview

rmk-gazell-sys provides a minimal C shim layer that wraps the Nordic Gazell SDK and exposes a simple C API with hand-written Rust bindings (no bindgen needed).

### Architecture

```
┌─────────────────────────────────────┐
│  rmk::split::gazell                 │  ← Split keyboard driver
│  (GazellCentralHub, PipeDriver,     │
│   GazellPeripheralDriver)           │
└──────────────┬──────────────────────┘
               │
┌──────────────▼──────────────────────┐
│  rmk-gazell-sys (this crate)        │  ← Unsafe FFI bindings
│  - Hand-written Rust bindings       │
│  - C shim (gazell_shim.c)           │
└──────────────┬──────────────────────┘
               │
┌──────────────▼──────────────────────┐
│  Nordic nRF5 SDK v17.1.0            │  ← Gazell protocol stack
│  (External dependency)              │
└─────────────────────────────────────┘
```

## Prerequisites

### 1. Hardware

This crate supports the following Nordic MCUs:

- nRF52840 (recommended for keyboards)
- nRF52833
- nRF52832

### 2. Nordic nRF5 SDK

Download and install the Nordic nRF5 SDK v17.1.0:

- Official: https://www.nordicsemi.com/Products/Development-software/nRF5-SDK
- Direct (v17.1.0): https://nsscprodmedia.blob.core.windows.net/prod/software-and-other-downloads/sdks/nrf5/binaries/nrf5_sdk_17.1.0_ddde560.zip

```bash
export NRF5_SDK_PATH=~/nRF5_SDK_17.1.0
```

### 3. Build Tools

- **Rust nightly toolchain**: `rustup default nightly`
- **ARM target**: `rustup target add thumbv7em-none-eabihf`
- **GCC ARM toolchain**: `sudo apt install gcc-arm-none-eabi` (Linux) / `brew install arm-none-eabi-gcc` (macOS)

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
rmk-gazell-sys = { path = "../rmk-gazell-sys", features = ["nrf52840"] }
```

### Example (unsafe FFI)

```rust
use rmk_gazell_sys as sys;

unsafe {
    // Initialize with Nordic defaults in device mode
    let result = sys::gz_init_default(sys::GZ_MODE_DEVICE);
    assert_eq!(result, sys::GZ_OK);

    // Send a packet on pipe 0
    let data = [0xAA, 0xBB, 0xCC];
    let result = sys::gz_send(data.as_ptr(), data.len() as u8, 0);

    // Cleanup
    sys::gz_deinit();
}
```

## API Reference

### Error Codes

| Code | Constant | Description |
|------|----------|-------------|
| 0 | `GZ_OK` | Success |
| -1 | `GZ_ERR_SEND_FAILED` | Transmission failed |
| -2 | `GZ_ERR_RECEIVE_FAILED` | Reception failed |
| -3 | `GZ_ERR_FRAME_TOO_LARGE` | Frame exceeds 32 bytes |
| -4 | `GZ_ERR_NOT_INITIALIZED` | Gazell not initialized |
| -5 | `GZ_ERR_BUSY` | TX FIFO full |
| -6 | `GZ_ERR_INVALID_CONFIG` | Invalid configuration |
| -7 | `GZ_ERR_HARDWARE` | Hardware error |

### Functions

| Function | Description |
|----------|-------------|
| `gz_init(config)` | Initialize with custom configuration |
| `gz_init_default(mode)` | Initialize with Nordic defaults |
| `gz_set_mode(mode)` | Set DEVICE (0) or HOST (1) mode |
| `gz_send(data, len, pipe)` | Send frame (blocking with timeout) |
| `gz_recv(buf, len, pipe, max)` | Receive frame (non-blocking) |
| `gz_set_ack_payload(pipe, data, len)` | Set ACK payload for a pipe (host mode) |
| `gz_get_ack_payload(buf, len, max)` | Get piggybacked ACK payload (device mode) |
| `gz_is_ready(pipe)` | Check if ready to transmit |
| `gz_flush()` | Flush TX/RX FIFOs |
| `gz_deinit()` | Deinitialize Gazell |

See `c/gazell_shim.h` for detailed API documentation.

## Features

- `nrf52840`: Enable support for nRF52840
- `nrf52833`: Enable support for nRF52833
- `nrf52832`: Enable support for nRF52832

Enable exactly one chip feature when building.

## Troubleshooting

**"NRF5_SDK_PATH not set"**: Set `export NRF5_SDK_PATH=/path/to/nRF5_SDK_17.1.0`

**"No chip feature enabled"**: Add `--features nrf52840` to your build command.

**Linker errors about missing Gazell library**: Verify the SDK contains:
```
$NRF5_SDK_PATH/components/proprietary_rf/gzll/gcc/gzll_nrf52840_gcc.a
```

## License

Rust bindings and C shim: MIT / Apache-2.0 dual license.

Nordic nRF5 SDK: Nordic 5-Clause License. See SDK documentation for details.
