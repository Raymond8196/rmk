# Phase 2: SplitMessage over Gazell + Bidirectional Communication

> **Branch**: `feat/gazell-2g4-verify`
> **Status**: Plan v10 — added codegen layer (Step 9), rmk-config (Step 10), dongle USB HID (Step 11), Charybdis integration (Step 12-13)
> **Depends on**: Phase 1 (minimal test packet TX/RX) — code done, builds for ARM

---

## 1. Objective

Replace Phase 1 test packets with real `SplitMessage` serialization over Gazell 2.4GHz,
add ACK-payload-based bidirectional communication (central <-> peripheral),
and create `GazellSplitDriver` types that plug into RMK's existing split architecture.

**End state**: The same `SplitPeripheral::run()` / `PeripheralManager::run()` loops
that today drive BLE and serial split keyboards will also work over Gazell.

---

## 2. Protocol Compatibility Statement

- Gazell split shares the **same `SplitMessage` enum and postcard encoding** as BLE and serial split. No new variants are added to `SplitMessage` for Gazell.
- Gazell heartbeat packets are a **driver-layer internal mechanism** (2-byte `[0xFE, 0xFE]` marker). They never appear as `SplitMessage` values, and BLE/serial code is unaffected. A unit test verifies no `SplitMessage` variant serializes to this marker (see §5 Step 8).
- **Keyboard and dongle must use the same RMK firmware version.** Cross-version mixing is not supported.
- BLE (`_ble`) and Gazell (`wireless_gazell`) features are **mutually exclusive** (same nRF52 radio hardware). This is enforced at compile time via `compile_error!`.

### SplitMessage Direction and Semantics

All `SplitMessage` variants, their communication direction, and semantic type:

| Variant | Direction | Semantic | Safe to overwrite? | Notes |
|---------|-----------|----------|-------------------|-------|
| `Key(KeyboardEvent)` | peripheral → central | **Event** | No — each press/release matters | Async publish with backpressure |
| `Touchpad(TouchpadEvent)` | peripheral → central | **Event** | No — each delta matters | Non-blocking, drop-on-full |
| `Pointing(PointingEvent)` | peripheral → central | **Event** | No — each sample matters | Non-blocking, drop-on-full |
| `BatteryState(BatteryStateEvent)` | peripheral → central | State | Yes | Latest level sufficient |
| `ConnectionState(bool)` | central → peripheral | State | Yes | Synced every 3000ms anyway |
| `KeyboardIndicator(u8)` | central → peripheral | State | Yes | Latest indicator bits sufficient |
| `Layer(u8)` | central → peripheral | State | Yes | Latest layer number sufficient |
| `LedState(bool)` | central → peripheral | State | Yes | Latest LED state sufficient |
| `ClearPeer` | central → peripheral | **Event** | **No** — must execute once | Pairing clear command |
| `Address([u8; 6])` | (unused) | — | — | No callers in current codebase |

**Implications for Gazell state-merge strategy**:
- State-type messages (ConnectionState, KeyboardIndicator, Layer, LedState): safe to merge in `pending_state` — only latest value matters.
- **`ClearPeer` is event-type**: must NOT be merged into `pending_state`. `GazellCentralDriver::write()` handles this specially — attempts immediate `gz_set_ack_payload` with retry; on failure, stores in `pending_event` for deferred delivery on next packet receipt (including heartbeats). See §4b.
- **`pending_event` overwrite semantics**: `pending_event` is a single slot. If a second event-type message arrives while the first is still pending, it overwrites. This is safe because `ClearPeer` (the only current event-type central→peripheral message) is **idempotent** — clearing peer twice has the same effect as clearing once. If a future non-idempotent event-type message is added, `pending_event` would need to become a queue.

**`SplitMessage::POSTCARD_MAX_SIZE`**: Already exists via `#[derive(MaxSize)]` on the `SplitMessage` enum. Used at `split/mod.rs:20` as `pub const SPLIT_MESSAGE_MAX_SIZE: usize = SplitMessage::POSTCARD_MAX_SIZE + 4`.

---

## 3. Architecture Overview

```
Keyboard (nRF52840, peripheral / device mode)
  ┌──────────────────────────────────────────────┐
  │  SplitPeripheral::run()                      │
  │    ├─ read()  ← GazellPeripheralDriver       │
  │    │     checks ack_buffer for cached         │
  │    │     ACK payload; if empty and idle >     │
  │    │     heartbeat_interval_ms →              │
  │    │     sends heartbeat → checks again       │
  │    └─ write() → GazellPeripheralDriver        │
  │          postcard::to_slice(SplitMessage)      │
  │          → gz_send(data, len, pipe)            │
  │          → gz_get_ack_payload → buffer result  │
  └──────────────────────────────────────────────┘
              │  Gazell 2.4GHz (pipe from config)
              ▼
  ┌──────────────────────────────────────────────┐
  │  Dongle (nRF52840, central / host mode)      │
  │  PeripheralManager::run()                    │
  │    ├─ read()  ← GazellCentralDriver          │
  │    │     polls gz_recv(), filters heartbeat   │
  │    │     flushes pending_event/pending_state  │
  │    │     → returns (Key, Pointing, Touchpad)  │
  │    └─ write() → GazellCentralDriver           │
  │          state-type: merges into pending_state│
  │          event-type: immediate gz_set_ack,    │
  │            fallback to pending_event on BUSY  │
  └──────────────────────────────────────────────┘
              │  USB HID
              ▼
             PC
```

### Key Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Serialization | `postcard::to_slice` / `postcard::from_bytes` (no COBS) | Gazell is packet-framed with known length — no stream framing needed. BLE split uses the same non-COBS approach. |
| Max payload | 32 bytes (Gazell hardware limit) [^1] | `SplitMessage` postcard max size is ~13 bytes. Fits with 19 bytes headroom. Enforced by compile-time test. |
| Central->peripheral channel | ACK payload (`gz_set_ack_payload`) | Gazell's built-in mechanism for host->device data. Non-blocking, piggybacked on the next ACK. |
| Central write() model | State-merge + event-deferred | State-type messages merge into `pending_state`. Event-type (`ClearPeer`) attempts immediate send; on failure falls back to `pending_event`. Both are flushed on any packet receipt (including heartbeats). See §2 direction table. |
| ACK payload FIFO depth | 3 packets [^2] | `NRF_GZLL_CONST_FIFO_LENGTH = 3`. State-merge ensures at most 1 queued ACK payload, well within limit. |
| Heartbeat mechanism | Gazell-internal 2-byte marker (`[0xFE, 0xFE]`) | Does **not** modify `SplitMessage` enum. Driver layer filters it out. No cross-protocol compatibility impact. Unit test verifies no `SplitMessage` variant serializes to this marker. |
| Peripheral read()/write() coordination | `last_send_time` + ack_buffer | Single-threaded Embassy executor — no true concurrency between `read()` and `write()`. `write()` sets `last_send_time` and checks ACK payload. `read()` only sends heartbeat if idle > `heartbeat_interval_ms`. No mutex needed. |
| Pipe | `self.config.pipe` (default 0) | Single keyboard per dongle for now. Multi-keyboard would use different pipes. `recv()` validates `out_pipe` matches expected pipe. |
| Feature mutual exclusion | `compile_error!` guard | Prevents accidentally enabling both `_ble` and `wireless_gazell`. |

---

## 4. Issues Found in Current Local Changes

### Issue 1: Rust FFI signatures don't match updated C shim

**Files**: `rmk-gazell-sys/src/lib.rs`

C shim now has:
```c
gz_error_t gz_send(const uint8_t* data, uint8_t len, uint8_t pipe);
gz_error_t gz_recv(uint8_t* out_buf, uint8_t* out_len, uint8_t* out_pipe, uint8_t max_len);
bool       gz_is_ready(uint8_t pipe);
```

Rust still declares the old signatures (no `pipe` parameter):
```rust
pub fn gz_send(data: *const u8, len: u8) -> gz_error_t;                    // missing pipe
pub fn gz_recv(out_buf: *mut u8, out_len: *mut u8, max_len: u8) -> ...;    // missing out_pipe
pub fn gz_is_ready() -> bool;                                               // missing pipe
```

**Impact**: Linker error on ARM, ABI mismatch.

### Issue 2: `gz_config_t` missing `pipe` field in Rust

**Files**: `rmk-gazell-sys/src/lib.rs`

C struct (gazell_shim.h:24-33):
```c
typedef struct {
    uint8_t channel;
    uint8_t data_rate;
    int8_t  tx_power;
    uint8_t max_retries;
    uint16_t ack_timeout_us;
    uint8_t base_address[4];
    uint8_t address_prefix;
    uint8_t pipe;              // <-- exists in C
} gz_config_t;
```

Rust `repr(C)` struct has no `pipe` field. Memory layout mismatch = UB when passed to C.

### Issue 3: Missing Rust bindings for new C functions

**Files**: `rmk-gazell-sys/src/lib.rs`

`gz_set_ack_payload()` and `gz_get_ack_payload()` exist in C shim but have no Rust `extern` declarations (ARM) and no stub functions (non-ARM).

### Issue 4: `ack_payload_length` type issue in C callback

**Files**: `rmk-gazell-sys/c/gazell_shim.c`, lines 47-61

```c
void nrf_gzll_device_tx_success(uint32_t pipe, nrf_gzll_device_tx_info_t tx_info) {
    if (tx_info.payload_received_in_ack) {
        gz_state.ack_payload_length = MAX_PAYLOAD_LENGTH;  // uint8_t = 32
        if (nrf_gzll_fetch_packet_from_rx_fifo(pipe,
                gz_state.ack_payload_buffer,
                &gz_state.ack_payload_length)) {  // expects uint32_t*
```

`nrf_gzll_fetch_packet_from_rx_fifo()` expects `uint32_t* length` [^4], but `gz_state.ack_payload_length` is `uint8_t`. The SDK writes 4 bytes into a 1-byte field = stack corruption.

Note: The host-side callback (`nrf_gzll_host_rx_data_ready`, line 77-88) does NOT have this problem because `gz_state.rx_length` is already `uint32_t`.

### Issue 5: `GazellTransport` methods not updated for new FFI signatures

**Files**: `rmk/src/wireless/gazell.rs`

All FFI call sites still use old signatures:
- `gz_send(frame.as_ptr(), frame.len() as u8)` — missing `pipe`
- `gz_recv(buffer.as_mut_ptr(), &mut length, buffer.len() as u8)` — missing `out_pipe`
- `gz_is_ready()` — missing `pipe`
- `gz_config_t { ... }` construction — missing `pipe` field

---

## 5. Implementation Steps

### Step 1: Fix C shim `ack_payload_length` type issue

**File**: `rmk-gazell-sys/c/gazell_shim.c`

**Change**: In `nrf_gzll_device_tx_success` callback, use a temporary `uint32_t` for the SDK call:

```c
void nrf_gzll_device_tx_success(uint32_t pipe, nrf_gzll_device_tx_info_t tx_info) {
    if (tx_info.payload_received_in_ack) {
        uint32_t temp_len = MAX_PAYLOAD_LENGTH;    // uint32_t for SDK
        if (nrf_gzll_fetch_packet_from_rx_fifo(pipe,
                gz_state.ack_payload_buffer,
                &temp_len)) {
            gz_state.ack_payload_length = (uint8_t)temp_len;
            gz_state.ack_payload_ready = true;
        }
    }
    gz_state.tx_success = true;
}
```

**Verification**:
```bash
cargo build --manifest-path rmk-gazell-sys/Cargo.toml \
  --target thumbv7em-none-eabihf --features nrf52840
```

---

### Step 2: Fix Rust FFI bindings

**File**: `rmk-gazell-sys/src/lib.rs`

**Changes**:

#### 2a. Add `pipe` to `gz_config_t`

```rust
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
    pub pipe: u8,               // NEW
}

impl Default for gz_config_t {
    fn default() -> Self {
        Self {
            // ... existing fields ...
            pipe: 0,            // NEW
        }
    }
}
```

#### 2b. Update ARM extern block

```rust
#[cfg(target_arch = "arm")]
extern "C" {
    pub fn gz_init(config: *const gz_config_t) -> gz_error_t;
    pub fn gz_set_mode(mode: gz_mode_t) -> gz_error_t;
    pub fn gz_send(data: *const u8, len: u8, pipe: u8) -> gz_error_t;                          // UPDATED
    pub fn gz_recv(out_buf: *mut u8, out_len: *mut u8, out_pipe: *mut u8, max_len: u8) -> gz_error_t; // UPDATED
    pub fn gz_is_ready(pipe: u8) -> bool;                                                       // UPDATED
    pub fn gz_set_ack_payload(pipe: u8, data: *const u8, len: u8) -> gz_error_t;               // NEW
    pub fn gz_get_ack_payload(out_buf: *mut u8, out_len: *mut u8, max_len: u8) -> gz_error_t;  // NEW
    pub fn gz_flush() -> gz_error_t;
    pub fn gz_deinit();
}
```

#### 2c. Update non-ARM stubs (all must match)

```rust
#[cfg(not(target_arch = "arm"))]
pub unsafe fn gz_send(_data: *const u8, _len: u8, _pipe: u8) -> gz_error_t { GZ_ERR_HARDWARE }

#[cfg(not(target_arch = "arm"))]
pub unsafe fn gz_recv(_out_buf: *mut u8, _out_len: *mut u8, _out_pipe: *mut u8, _max_len: u8) -> gz_error_t { GZ_ERR_HARDWARE }

#[cfg(not(target_arch = "arm"))]
pub unsafe fn gz_is_ready(_pipe: u8) -> bool { false }

#[cfg(not(target_arch = "arm"))]
pub unsafe fn gz_set_ack_payload(_pipe: u8, _data: *const u8, _len: u8) -> gz_error_t { GZ_ERR_HARDWARE }

#[cfg(not(target_arch = "arm"))]
pub unsafe fn gz_get_ack_payload(_out_buf: *mut u8, _out_len: *mut u8, _max_len: u8) -> gz_error_t { GZ_ERR_HARDWARE }
```

**Verification**:
```bash
# Host build (non-ARM stubs)
cargo check --manifest-path rmk-gazell-sys/Cargo.toml

# ARM cross-compile (real FFI)
cargo build --manifest-path rmk-gazell-sys/Cargo.toml \
  --target thumbv7em-none-eabihf --features nrf52840
```

---

### Step 3: Update GazellTransport FFI call sites

**Files**: `rmk/src/wireless/gazell.rs`, `rmk/src/wireless/config.rs`

**Changes**:

| Method | Old call | New call |
|--------|----------|----------|
| `init()` | `gz_config_t { channel, ... }` | Add `pipe: self.config.pipe` |
| `send_frame()` | `gz_send(ptr, len)` | `gz_send(ptr, len, self.config.pipe)` |
| `recv_frame()` | `gz_recv(buf, &mut len, max)` | `gz_recv(buf, &mut len, &mut pipe, max)` |
| `is_ready()` | `gz_is_ready()` | `gz_is_ready(self.config.pipe)` |

Also add fields to `GazellConfig` in `rmk/src/wireless/config.rs`:

**`pipe: u8`**:
- Default: `0`
- Validation: `self.pipe <= 7`
- Add to all preset constructors (`low_latency`, `long_range`, `low_power`): `pipe: 0`

**`heartbeat_interval_ms: u16`**:
- Default: `50` (low latency for Layer/LED indicator responsiveness)
- Validation: `self.heartbeat_interval_ms >= 10 && self.heartbeat_interval_ms <= 5000`
- Preset values:
  - `low_latency()`: `50` (20 heartbeats/sec)
  - `long_range()`: `200` (5 heartbeats/sec)
  - `low_power()`: `500` (2 heartbeats/sec, Layer sync latency up to 500ms)
- Note: Idle power overhead per heartbeat rate depends on radio TX time and sleep current. Actual values TBD via hardware measurement in Phase 3.

**Verification**:
```bash
# Host check (mock mode)
cargo check --manifest-path rmk/Cargo.toml --features wireless_gazell

# Unit tests
cargo test --manifest-path rmk/Cargo.toml --lib -- wireless
```

---

### Step 4: Create GazellSplitDriver

**File**: `rmk/src/split/gazell.rs` (NEW)

This is the core of Phase 2. Two driver structs, both implementing `SplitReader + SplitWriter`.

#### 4a. `GazellPeripheralDriver` (keyboard half, device mode)

**Internal state**:
```rust
pub(crate) struct GazellPeripheralDriver {
    pipe: u8,
    heartbeat_interval_ms: u16,         // from GazellConfig, idle threshold before heartbeat
    ack_buffer: Option<SplitMessage>,   // buffered ACK payload from last send
    last_send_time: Instant,            // tracks when last gz_send occurred
}
```

**SplitWriter::write()** (peripheral -> central):
```
1. postcard::to_slice(&message, &mut buf)
2. gz_send(buf.as_ptr(), len, self.pipe)
   - On GZ_ERR_BUSY: wait 1ms, retry up to 3 times, then return SplitDriverError::SerialError
   - On GZ_ERR_SEND_FAILED: return SplitDriverError::SerialError
     (upper layer SplitPeripheral::run() logs and continues — input event is lost)
3. self.last_send_time = Instant::now()
4. Check gz_get_ack_payload() for piggybacked central->peripheral data
   - If len > 0: postcard::from_bytes → store in self.ack_buffer
   - If len == 0: no ACK payload, nothing to buffer
5. Return Ok(bytes_written)
```

> **Known limitation**: If `gz_send()` fails after 3 retries (e.g., dongle not in range, sustained interference), the input event is lost. This is consistent with BLE split behavior where dropped packets are not retried at the driver level. During active typing, the next key event will succeed when the link recovers. See §8 Known Limitations.

**SplitReader::read()** (central -> peripheral, via ACK payload):
```
loop {
    1. If self.ack_buffer.take() is Some(msg) → return Ok(msg)
    2. If self.last_send_time.elapsed() > self.heartbeat_interval_ms:
       a. Send heartbeat: gz_send(&[0xFE, 0xFE], 2, self.pipe)
          - On error: ignore (non-critical), update last_send_time anyway
       b. self.last_send_time = Instant::now()
       c. Check gz_get_ack_payload()
          - If len > 0: deserialize → return Ok(msg)
    3. Timer::after_millis(5).await   // yield to executor
}
```

**Heartbeat design**:
- Heartbeat is a 2-byte packet `[0xFE, 0xFE]` — a Gazell-internal marker, **not** a `SplitMessage` variant
- `SplitMessage` enum is unchanged — no cross-protocol compatibility impact
- Central-side `GazellCentralDriver::read()` filters out packets where `len == 2 && buf[0] == 0xFE && buf[1] == 0xFE`
- Why 2 bytes instead of 1: reduces collision risk with future `SplitMessage` encodings. A unit test (Step 8) explicitly verifies no `SplitMessage` variant serializes to `[0xFE, 0xFE]`.
- Heartbeat interval: configurable via `GazellConfig::heartbeat_interval_ms` (default 50ms for low latency). In practice, keyboard events trigger `write()` frequently enough that heartbeats are rare during active typing. For battery-sensitive applications, increase to 200-500ms to reduce idle power draw at the cost of slower central→peripheral state delivery.

**ACK payload cleanup semantics**: `gz_get_ack_payload()` (C shim line 370) sets `ack_payload_ready = false` after reading. No risk of duplicate consumption.

**Concurrency safety**: Embassy is a single-threaded cooperative executor. `select(driver.read(), event_source)` in `SplitPeripheral::run()` polls both futures but only one runs at a time. Since `gz_send()` is synchronous-blocking (~10ms max [^3]), the executor cannot interleave a `write()` call while `read()` is inside `gz_send()`. No mutex required.

#### 4b. `GazellCentralDriver` (dongle, host mode)

**Internal state**:
```rust
pub(crate) struct GazellCentralDriver {
    pipe: u8,
    pending_state: Option<SplitMessage>,   // latest state-type message to send via ACK
    pending_event: Option<SplitMessage>,   // event-type message awaiting delivery (e.g. ClearPeer)
}
```

**SplitReader::read()** (peripheral -> central):
```
loop {
    1. let ret = gz_recv(&mut buf, &mut len, &mut rx_pipe, max)
    2. If ret != GZ_OK:
       a. Log warning with ret
       b. Timer::after_millis(1).await   // yield
       c. continue   // len is undefined on error, do not use
    3. If len > 0:
       a. Flush pending outbound (runs on EVERY received packet, regardless of pipe):
          i.  If self.pending_event.is_some():
              - let ack_len = postcard::to_slice(pending_event, &mut ack_buf).len()
              - let ack_ret = gz_set_ack_payload(self.pipe, ack_buf.as_ptr(), ack_len)
              - On GZ_OK: self.pending_event = None
              - On any error: keep for next iteration (log at trace level)
          ii. Else if self.pending_state.is_some():
              - let ack_len = postcard::to_slice(pending_state, &mut ack_buf).len()
              - let ack_ret = gz_set_ack_payload(self.pipe, ack_buf.as_ptr(), ack_len)
              - On GZ_OK: self.pending_state = None
              - On any error: keep for next iteration (log at trace level)
       b. If rx_pipe != self.pipe → log warning, continue loop
       c. If len == 2 && buf[0] == 0xFE && buf[1] == 0xFE → heartbeat, continue loop
       d. postcard::from_bytes(&buf[..len]) → return Ok(msg)
          - On deserialize error: log warning, continue loop
    4. If len == 0:
       a. Timer::after_millis(1).await   // yield
}
```

> **Critical fix (v3)**: `pending_state` is consumed only AFTER `gz_set_ack_payload` succeeds. On any error (BUSY, HARDWARE, etc.), the state remains and is retried on the next packet.
>
> **Critical fix (v4)**: Flush logic runs on **every received packet including heartbeats** (step 3a), before the heartbeat filter (step 3c). When the peripheral is idle and only sending heartbeats, the central can still deliver pending state/events. `pending_event` has priority over `pending_state` to ensure event-type messages are delivered first.
>
> **Critical fix (v8)**: `gz_recv` return code is now explicitly checked (step 2). On error, `len` is undefined and must not be used — the loop yields and retries. Flush pending uses "any error retains" semantics (not just `GZ_ERR_BUSY`), consistent with the test that exercises `GZ_ERR_HARDWARE` on host stubs.

**SplitWriter::write()** (central -> peripheral, state-merge + event-deferred):
```
1. If message is event-type (ClearPeer):
   a. let ack_len = postcard::to_slice(&message, &mut ack_buf).len()
   b. gz_set_ack_payload(self.pipe, ack_buf.as_ptr(), ack_len)
   c. On GZ_ERR_BUSY: wait 1ms, retry up to 3 times
   d. On final failure: store in self.pending_event (will retry on next read())
   e. Return Ok(ack_len)
2. Else (state-type: ConnectionState, KeyboardIndicator, Layer, LedState):
   a. self.pending_state = Some(*message)   // overwrite — latest state wins
   b. Return Ok(SPLIT_MESSAGE_MAX_SIZE)
```

> **Critical fix (v4)**: `ClearPeer` no longer returns error on FIFO full. Instead, it falls back to `pending_event`, which is retried on the next received packet (including heartbeats). This guarantees event-type messages are not permanently lost.

**Why state-merge for state-type messages**: The Nordic Gazell ACK payload FIFO has depth 3 (`NRF_GZLL_CONST_FIFO_LENGTH = 3`, defined in `nrf_gzll_constants.h:121`). If the central calls `gz_set_ack_payload` multiple times before the peripheral sends a packet (triggering an ACK), the FIFO fills up and returns `GZ_ERR_BUSY`. State-type messages (ConnectionState, Layer, KeyboardIndicator, LedState) only need the latest value. By merging into `pending_state` and only flushing when a packet is received, we guarantee:
- At most 1 pending ACK payload at a time
- The payload is always the most recent state
- No FIFO overflow
- No state loss on GZ_ERR_BUSY (pending_state is kept for retry)
- **Idle-period delivery**: heartbeats trigger flush, so state reaches the peripheral even when no keys are pressed

**Why `pending_event` for event-type messages**: `ClearPeer` is a one-shot command that must execute. It attempts immediate `gz_set_ack_payload` with retry. If FIFO is still full, it falls back to `pending_event` which is flushed on the next packet receipt (heartbeats included). `pending_event` takes priority over `pending_state` in the flush order.

**Flush priority**: `pending_event` > `pending_state`. Only one is flushed per packet (one ACK payload slot per ACK). This means a pending event may delay a state update by one packet cycle (~`heartbeat_interval_ms` at heartbeat rate). Acceptable since state is idempotent.

**Note**: `Address` is currently unused in the codebase. If it becomes active, its direction and semantics should be classified and added to the §2 table.

#### 4c. Error handling strategy

| FFI call | Error | Strategy |
|----------|-------|----------|
| `gz_send()` | `GZ_ERR_BUSY` | Wait 1ms, retry up to 3 times. Return `SplitDriverError::SerialError` on exhaustion. |
| `gz_send()` | `GZ_ERR_SEND_FAILED` | Return `SplitDriverError::SerialError`. Upper layer (`SplitPeripheral::run()`) logs and continues. |
| `gz_send()` in heartbeat | any error | Log at trace level, ignore. Heartbeat is best-effort. |
| `gz_recv()` | `GZ_OK`, `len == 0` | Normal — no data available. Yield and retry. |
| `gz_recv()` | non-`GZ_OK` return | Log warning. `len` is undefined — do not use. Yield and retry. |
| `gz_set_ack_payload()` in `read()` flush | any error (BUSY, HARDWARE, etc.) | Keep `pending_event`/`pending_state` as-is. Log at trace level. Will retry on next received packet (including heartbeats). |
| `gz_set_ack_payload()` in `write()` for `ClearPeer` | `GZ_ERR_BUSY` | Wait 1ms, retry up to 3 times. On exhaustion: store in `self.pending_event` for deferred delivery. |
| `gz_get_ack_payload()` | `len == 0` | Normal — no ACK payload received. |
| `postcard::from_bytes()` | error | Log warning, skip packet, continue polling. |
| `postcard::to_slice()` | error | Return `SplitDriverError::SerializeError`. |

**Verification**:
```bash
cargo check --manifest-path rmk/Cargo.toml --features "split,wireless_gazell"
```

---

### Step 5: Wire into split module

**Files**:
- `rmk/src/split/mod.rs`
- `rmk/src/split/peripheral.rs`
- `rmk/src/split/central.rs`

#### 5a. Add module declaration and feature guard

In `rmk/src/split/mod.rs`:
```rust
#[cfg(feature = "wireless_gazell")]
pub mod gazell;
```

In `rmk/src/split/mod.rs` or `rmk/src/lib.rs`, add compile-time mutual exclusion:
```rust
#[cfg(all(feature = "_ble", feature = "wireless_gazell"))]
compile_error!(
    "Features `_ble` and `wireless_gazell` are mutually exclusive. \
     BLE and Gazell share the same radio hardware on nRF52."
);
```

#### 5b. Update peripheral dispatch

Current `run_rmk_split_peripheral` has two branches:
- `#[cfg(feature = "_ble")]` → BLE path
- `#[cfg(not(feature = "_ble"))]` → serial path (takes `S: Write + Read`)

Need to add a third branch. The cfg logic becomes:

```rust
pub async fn run_rmk_split_peripheral<...>(
    #[cfg(feature = "_ble")] /* BLE params */,
    #[cfg(feature = "wireless_gazell")] config: GazellConfig,
    #[cfg(not(any(feature = "_ble", feature = "wireless_gazell")))] serial: S,
) {
    #[cfg(feature = "wireless_gazell")]
    {
        crate::split::gazell::run_gazell_split_peripheral(config).await;
    }

    #[cfg(feature = "_ble")]
    {
        crate::split::ble::peripheral::initialize_nrf_ble_split_peripheral_and_run(...).await;
    }

    #[cfg(not(any(feature = "_ble", feature = "wireless_gazell")))]
    {
        let mut peripheral = SplitPeripheral::new(SerialSplitDriver::new(serial));
        loop { peripheral.run().await; }
    }
}
```

#### 5c. Update central dispatch

Current `run_peripheral_manager` has two branches. Add third branch following same pattern:

```rust
pub async fn run_peripheral_manager<...>(
    id: usize,
    #[cfg(feature = "_ble")] /* BLE params */,
    #[cfg(feature = "wireless_gazell")] config: GazellConfig,
    #[cfg(not(any(feature = "_ble", feature = "wireless_gazell")))] receiver: S,
) {
    #[cfg(feature = "wireless_gazell")]
    {
        crate::split::gazell::run_gazell_peripheral_manager::<ROW, COL, ROW_OFFSET, COL_OFFSET>(id, config).await;
    }

    #[cfg(feature = "_ble")]
    { /* existing BLE code */ }

    #[cfg(not(any(feature = "_ble", feature = "wireless_gazell")))]
    { /* existing serial code */ }
}
```

#### 5d. Helper functions in `split/gazell.rs`

Following the BLE pattern (`initialize_nrf_ble_split_peripheral_and_run` / `run_ble_peripheral_manager`), create:

```rust
// In rmk/src/split/gazell.rs

/// Initialize Gazell and run the split peripheral loop (never returns)
pub async fn run_gazell_split_peripheral(config: GazellConfig) {
    // 1. Init Gazell via GazellTransport
    // 2. Set device mode
    // 3. Create GazellPeripheralDriver { pipe: config.pipe, heartbeat_interval_ms: config.heartbeat_interval_ms, ack_buffer: None, last_send_time: Instant::now() }
    // 4. Create SplitPeripheral::new(driver)
    // 5. loop { peripheral.run().await; }
}

/// Run the central-side peripheral manager for one Gazell peripheral
pub async fn run_gazell_peripheral_manager<
    const ROW: usize, const COL: usize,
    const ROW_OFFSET: usize, const COL_OFFSET: usize,
>(id: usize, config: GazellConfig) {
    // 1. Init Gazell via GazellTransport
    // 2. Set host mode
    // 3. Create GazellCentralDriver { pipe: config.pipe, pending_state: None }
    // 4. Create PeripheralManager::new(driver, id)
    // 5. peripheral_manager.run().await
}
```

**Verification**:
```bash
# All three feature combinations must compile
cargo check --manifest-path rmk/Cargo.toml --features "split,wireless_gazell"
cargo check --manifest-path rmk/Cargo.toml --features "split"

# Verify compile_error! guard rejects BLE + Gazell combination
# This should FAIL — if it succeeds, the guard is broken
cargo check --manifest-path rmk/Cargo.toml --features "split,wireless_gazell,_ble" 2>&1 \
  | grep -q "mutually exclusive" && echo "Guard works" || echo "ERROR: guard missing"
```

---

### Step 6: Update Cargo.toml feature gates

**File**: `rmk/Cargo.toml`

```toml
## Enable Gazell support for nRF52840
wireless_gazell_nrf52840 = ["wireless_gazell", "rmk-gazell-sys/nrf52840", "split"]
```

Adding `"split"` ensures `split` (and transitively `controller`) is enabled for Gazell builds, since Gazell keyboards are always split (keyboard + dongle).

**Verification**:
```bash
cargo check --manifest-path rmk/Cargo.toml --features wireless_gazell_nrf52840
```

---

### Step 7: Update examples

**Files**:
- `examples/use_rust/nrf52840_2g4/src/main.rs`
- `examples/use_rust/nrf52840_dongle/src/main.rs`

The examples currently use `GazellTransport` (high-level wrapper) directly. After Step 3, the FFI call sites inside `GazellTransport` are updated, so examples should build without code changes.

Verify that Cargo.toml features in examples are correct:

```toml
# nrf52840_2g4/Cargo.toml
[dependencies.rmk]
features = ["wireless_gazell_nrf52840", "defmt"]

# nrf52840_dongle/Cargo.toml
[dependencies.rmk]
features = ["wireless_gazell_nrf52840", "defmt"]
```

**Verification**:
```bash
cd examples/use_rust/nrf52840_2g4 && cargo build --release && cd -
cd examples/use_rust/nrf52840_dongle && cargo build --release && cd -

# Check binary sizes (should be ~25-30KB)
ls -la examples/use_rust/nrf52840_2g4/target/thumbv7em-none-eabihf/release/rmk-nrf52840-2g4
ls -la examples/use_rust/nrf52840_dongle/target/thumbv7em-none-eabihf/release/rmk-nrf52840-dongle
```

---

### Step 8: Full verification suite

```bash
# ---- Host checks ----

# 1. FFI crate host check (non-ARM stubs)
cargo check --manifest-path rmk-gazell-sys/Cargo.toml

# 2. RMK with Gazell split features
cargo check --manifest-path rmk/Cargo.toml --features "split,wireless_gazell"

# 3. RMK with serial split (no Gazell, no BLE) — regression check
cargo check --manifest-path rmk/Cargo.toml --features "split"

# 4. Unit tests (mock mode on host)
cargo test --manifest-path rmk/Cargo.toml --lib -- wireless
cargo test --manifest-path rmk/Cargo.toml --lib -- split

# ---- ARM cross-compile ----

# 5. FFI crate
cargo build --manifest-path rmk-gazell-sys/Cargo.toml \
  --target thumbv7em-none-eabihf --features nrf52840

# 6. Examples (MUST cd into directories — see CLAUDE.md)
cd examples/use_rust/nrf52840_2g4 && cargo build --release && cd -
cd examples/use_rust/nrf52840_dongle && cargo build --release && cd -

# ---- Code quality ----

# 7. Formatting
cargo fmt --all -- --check

# 8. Clippy
cargo clippy --manifest-path rmk/Cargo.toml \
  --features "split,wireless_gazell" -- -D warnings
cargo clippy --manifest-path rmk-gazell-sys/Cargo.toml -- -D warnings
```

**Compile-time size assertion** (add to module scope in `rmk/src/split/gazell.rs`, **outside** `#[cfg(test)]`):

```rust
// Enforced on every build (including firmware release), not just test runs.
const _: () = assert!(
    SplitMessage::POSTCARD_MAX_SIZE <= 32,
    "SplitMessage max size exceeds Gazell 32-byte payload limit"
);
```

**Unit tests** (add to `rmk/src/split/gazell.rs`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use postcard::to_slice;
    use crate::event::{
        Axis, AxisEvent, AxisValType, KeyPos,
        KeyboardEvent, KeyboardEventPos,
        PointingEvent, TouchpadEvent,
    };

    /// Verify no SplitMessage variant serializes to the heartbeat marker [0xFE, 0xFE].
    ///
    /// Rationale: The central filters `len==2 && [0xFE, 0xFE]` as heartbeat.
    /// If any real SplitMessage produced this exact encoding, it would be silently dropped.
    ///
    /// Coverage strategy: Test every variant. For variants with payload, use 0xFE
    /// as the payload value (worst case for collision). For bool variants, test both
    /// true and false. Struct payloads use explicit constructors (no Default dependency).
    #[test]
    fn heartbeat_marker_does_not_collide_with_any_split_message() {
        let heartbeat: [u8; 2] = [0xFE, 0xFE];
        let mut buf = [0u8; 32];

        let zero_axis = AxisEvent { typ: AxisValType::Abs, axis: Axis::X, value: 0 };
        let fe_axis = AxisEvent { typ: AxisValType::Abs, axis: Axis::X, value: 0xFE };

        let variants: &[SplitMessage] = &[
            SplitMessage::Key(KeyboardEvent { pressed: true,
                pos: KeyboardEventPos::Key(KeyPos { row: 0xFE, col: 0xFE }) }),
            SplitMessage::Key(KeyboardEvent { pressed: false,
                pos: KeyboardEventPos::Key(KeyPos { row: 0, col: 0 }) }),
            SplitMessage::Touchpad(TouchpadEvent { finger: 0, axis: [zero_axis, zero_axis] }),
            SplitMessage::Touchpad(TouchpadEvent { finger: 0xFE, axis: [fe_axis, fe_axis] }),
            SplitMessage::Pointing(PointingEvent([zero_axis, zero_axis, zero_axis])),
            SplitMessage::Pointing(PointingEvent([fe_axis, fe_axis, fe_axis])),
            SplitMessage::LedState(true),
            SplitMessage::LedState(false),
            SplitMessage::ConnectionState(true),
            SplitMessage::ConnectionState(false),
            SplitMessage::Address([0xFE; 6]),
            SplitMessage::ClearPeer,
            SplitMessage::KeyboardIndicator(0xFE),
            SplitMessage::Layer(0xFE),
            // BatteryState is behind #[cfg(feature = "_ble")] — tested separately if enabled
        ];
        for msg in variants {
            let serialized = to_slice(msg, &mut buf).unwrap();
            assert_ne!(
                serialized, &heartbeat,
                "SplitMessage variant {:?} serializes to heartbeat marker!", msg
            );
        }
    }

    /// Verify GazellCentralDriver does not lose pending_state on flush failure.
    ///
    /// On host (non-ARM), gz_set_ack_payload stub returns GZ_ERR_HARDWARE.
    /// The "any error retains pending" policy means this exercises the same
    /// retention path as GZ_ERR_BUSY on real hardware.
    #[test]
    fn pending_state_retained_on_flush_failure() {
        let mut driver = GazellCentralDriver {
            pipe: 0,
            pending_state: Some(SplitMessage::Layer(5)),
            pending_event: None,
        };
        driver.try_flush_pending();
        assert!(
            matches!(driver.pending_state, Some(SplitMessage::Layer(5))),
            "pending_state must survive failed flush with value preserved"
        );
    }

    /// Verify pending_event takes priority over pending_state in flush order,
    /// and is also retained on failure.
    #[test]
    fn pending_event_priority_and_retention() {
        let mut driver = GazellCentralDriver {
            pipe: 0,
            pending_state: Some(SplitMessage::Layer(3)),
            pending_event: Some(SplitMessage::ClearPeer),
        };
        driver.try_flush_pending();
        assert!(driver.pending_event.is_some(), "pending_event must survive failed flush");
        assert!(driver.pending_state.is_some(),
            "pending_state must not be touched when pending_event flush fails");
    }
}
```

> **Notes**:
> - The `const` size assertion is at **module scope** (not inside `#[cfg(test)]`), so it is enforced on every build including firmware release.
> - The heartbeat collision test uses explicit struct constructors — no `Default` trait dependency. `AxisEvent`, `TouchpadEvent`, `PointingEvent` are constructed manually.
> - The flush tests rely on a `try_flush_pending()` helper method extracted from the `read()` loop for testability.
> - On non-ARM host, FFI stubs return `GZ_ERR_HARDWARE`, exercising the retention path naturally.
> - The heartbeat collision test uses representative values (0x00, 0xFE) rather than exhaustive enumeration. For varint-encoded integer fields, this covers the most likely collision candidates. If stricter coverage is desired in the future, `proptest` or fuzzing can be added (low priority — the 2-byte `[0xFE, 0xFE]` marker is inherently safe because postcard encodes enum variants using a varint discriminant as the first byte(s) [^5], and `SplitMessage` has far fewer than 254 variants, so no variant can have discriminant 0xFE).

---

## 6. SplitMessage Size Analysis

Critical constraint: Gazell max payload = 32 bytes [^1].

| SplitMessage variant | Inner type fields | Postcard size (bytes) |
|---|---|---|
| `Key(KeyboardEvent)` | pressed: bool (1) + pos: KeyboardEventPos (1 tag + 2 fields = 3) | 1 + 4 = **5** |
| `Touchpad(TouchpadEvent)` | finger: u8 (1) + axis: [AxisEvent; 2] (2 x 4 = 8) | 1 + 9 = **10** |
| `Pointing(PointingEvent)` | [AxisEvent; 3] (3 x 4 = 12) | 1 + 12 = **13** |
| `LedState(bool)` | bool (1) | 1 + 1 = **2** |
| `ConnectionState(bool)` | bool (1) | 1 + 1 = **2** |
| `Address([u8; 6])` | 6 bytes | 1 + 6 = **7** |
| `ClearPeer` | unit | **1** |
| `KeyboardIndicator(u8)` | u8 (1) | 1 + 1 = **2** |
| `Layer(u8)` | u8 (1) | 1 + 1 = **2** |
| `BatteryState(BatteryStateEvent)` | enum (1 tag + max 1 byte) | 1 + 2 = **3** |

**Maximum**: `Pointing(PointingEvent)` = ~13 bytes.
**Headroom**: 32 - 13 = **19 bytes** spare.

**Note**: The `SplitMessage` enum is NOT modified for Gazell. Heartbeat packets (`[0xFE, 0xFE]`) are raw bytes, not `SplitMessage` values. The size analysis above covers all actual `SplitMessage` variants.

**Safety net**: A `const` assertion enforces `SplitMessage::POSTCARD_MAX_SIZE <= 32` at compile time (see Step 8). If a future variant exceeds 32 bytes, compilation will fail.

---

## 7. Files Changed Summary

### Steps 1-8: Runtime layer (SplitMessage protocol + drivers)

| Step | File | Action | Lines (est.) |
|------|------|--------|-------------|
| 1 | `rmk-gazell-sys/c/gazell_shim.c` | Fix ack_payload_length uint32_t temp var | ~5 |
| 2 | `rmk-gazell-sys/src/lib.rs` | Add pipe to config, update/add FFI signatures | ~30 |
| 3 | `rmk/src/wireless/config.rs` | Add `pipe: u8` + `heartbeat_interval_ms: u16` to GazellConfig + validation | ~12 |
| 3 | `rmk/src/wireless/gazell.rs` | Update FFI call sites to use `self.config.pipe` | ~10 |
| 4 | `rmk/src/split/gazell.rs` | **NEW**: GazellPeripheralDriver + GazellCentralDriver | ~250 |
| 5 | `rmk/src/split/mod.rs` | Add `pub mod gazell` + `compile_error!` guard | ~6 |
| 5 | `rmk/src/split/peripheral.rs` | Add Gazell dispatch branch | ~15 |
| 5 | `rmk/src/split/central.rs` | Add Gazell dispatch branch | ~15 |
| 6 | `rmk/Cargo.toml` | Update wireless_gazell_nrf52840 feature (add `"split"`) | ~1 |
| 7 | Examples (both) | No code changes expected | 0 |
| 8 | `rmk/src/split/gazell.rs` | Add `const` size assertion + heartbeat collision test + pending_state test | ~40 |

### Steps 9-13: Codegen layer + keyboard.toml integration + dongle USB HID

| Step | File | Action | Lines (est.) |
|------|------|--------|-------------|
| 9 | `rmk-macro/src/codegen/entry.rs` | Add `"gazell"` branch in split connection dispatch | ~40 |
| 9 | `rmk-macro/src/codegen/split/central.rs` | Add `"gazell"` branch in `expand_split_communication_config` | ~30 |
| 9 | `rmk-macro/src/codegen/split/peripheral.rs` | Add `"gazell"` branch in peripheral dispatch | ~30 |
| 10 | `rmk-config/src/lib.rs` | Add Gazell config fields to `SplitConfig` / `SplitBoardConfig` (pipe, channel, etc.) | ~25 |
| 11 | `examples/use_rust/nrf52840_dongle/src/main.rs` | Replace USB CDC test with real `GazellCentralDriver` + USB HID forwarding | ~200 |
| 12 | Charybdis `keyboard.toml` (new or adapted) | `connection = "gazell"` with Gazell-specific config fields | ~15 |
| 13 | Hardware verification | Left hand keypress → Gazell → dongle → USB HID → PC typing | 0 |

**Total new code**: ~660 lines
**Total modified code**: ~100 lines

---

## 8. Risk Assessment

### Low Risk

- **Steps 1-3** (fix FFI mismatches): Mechanical changes, compiler-verifiable.
- **Step 6** (Cargo.toml): Single line change.
- **Step 7** (examples): No code changes needed.
- **Step 10** (rmk-config): Add optional fields with `#[serde(default)]`, backward compatible.
- **Step 12** (keyboard.toml): Configuration file, no code logic.

### Medium Risk

- **Step 5** (wire into split module): The `#[cfg]` attribute gymnastics on function signatures is the trickiest part. Need to ensure all three feature combinations compile. Follow the exact pattern already established by BLE/serial. The `compile_error!` guard reduces risk of accidental misconfiguration.
- **Step 9** (codegen layer): Must follow the exact pattern of existing `"ble"` and `"serial"` branches. Need to generate correct `GazellConfig` initialization from keyboard.toml fields. Risk: codegen errors are hard to debug (proc-macro output). Mitigation: use `cargo expand` to inspect generated code.
- **Step 11** (dongle USB HID): Must convert `SplitMessage::Key` → USB HID report. The existing `KeyboardReportChannel` and `UsbHidWriter` infrastructure in RMK can be reused, but the dongle firmware is a standalone `use_rust` example (no codegen), so the HID report assembly must be done manually.

### High Risk

- **Step 4** (GazellSplitDriver): Most complex step. Key risks and mitigations:

| Risk | Mitigation |
|------|------------|
| `read()` sends heartbeat while `write()` should send data | Single-threaded executor ensures mutual exclusion. `last_send_time` coordination avoids unnecessary heartbeats. |
| ACK payload FIFO overflow on central | State-merge strategy: `pending_state` holds only latest value, flushed after `read()`. `NRF_GZLL_CONST_FIFO_LENGTH = 3` confirmed in `nrf_gzll_constants.h:121`. |
| `pending_state` lost on flush error | **v3 fix**: `pending_state` consumed only after `gz_set_ack_payload` returns `GZ_OK`. On any error (BUSY, HARDWARE, etc.), state stays for retry. |
| `pending_state` never flushed during idle | **v4 fix**: Flush runs on every received packet including heartbeats (step 3a), before heartbeat filter (step 3c). |
| `gz_recv` error leaves `len` undefined | **v8 fix**: Return code explicitly checked (step 2). On non-`GZ_OK`, `len` is not used — log, yield, retry. |
| Deserialization failure on corrupted packet | Log and skip, continue polling. Never panic. |
| Heartbeat mistaken for data | 2-byte `[0xFE, 0xFE]` marker explicitly filtered. Unit test verifies no `SplitMessage` variant serializes to this marker. |
| `ClearPeer` event permanently lost | **v4 fix**: `ClearPeer` attempts immediate send with retry. On failure, falls back to `pending_event` which is flushed on next packet receipt. `pending_event` has priority over `pending_state`. |
| `gz_send()` retry exhaustion drops input events | By design: consistent with BLE split. Documented in Known Limitations. |

### Known Limitations (not blockers for Phase 2)

1. **Single peripheral only**: `self.config.pipe` is configurable, but multi-keyboard is untested. Deferred.
2. **No reconnection logic**: Gazell has no connection state. Keyboard keeps sending. Simpler than BLE.
3. **Blocking FFI in async context**: `gz_send()` blocks up to ~10ms [^3]. Acceptable for now; async wrapper deferred to Phase 3.
4. **ACK payload race in C shim**: `nrf_gzll_device_tx_success` callback (interrupt context) writes `ack_payload_ready = true`, while `gz_get_ack_payload` (main context) reads and clears it. In the current single-send-then-check flow, this race cannot occur. Phase 3 should add proper atomic/volatile semantics if async wrappers are introduced.
5. **Input event loss under sustained link failure**: If `gz_send()` fails after 3 retries (dongle out of range, sustained interference), the input event from `SplitPeripheral::run()` is lost. This is consistent with BLE split behavior. During active typing, the next event will succeed when the link recovers. For latency-sensitive applications, a short internal retry queue could be added in Phase 3.

---

## 9. Dependency Graph

```
Step 1 (C shim fix)
    │
    ▼
Step 2 (Rust FFI bindings)
    │
    ├──────────────────────┐
    ▼                      ▼
Step 3 (GazellTransport)  Step 4 (GazellSplitDriver)
    │                      │
    ▼                      ▼
Step 7 (examples)         Step 5 (wire into split + compile_error!)
                           │
                           ▼
                          Step 6 (Cargo.toml)
                           │
                           ▼
                          Step 8 (full verification + size test)
                           │
              ┌────────────┼────────────┐
              ▼            ▼            ▼
Step 10       Step 9       Step 11
(rmk-config)  (codegen)    (dongle USB HID)
              │                         │
              ▼                         │
Step 12 (keyboard.toml)                │
              │                         │
              └────────┬────────────────┘
                       ▼
              Step 13 (hardware verification)
```

Steps 3 and 4 can be done in parallel after Step 2.
Steps 5 and 7 can be done in parallel after Steps 3/4.
Steps 9, 10, and 11 can be done in parallel after Step 8.
Step 12 depends on Steps 9 and 10.
Step 13 depends on Steps 11 and 12.

---

## 10. Steps 9-13: Codegen, Config, Dongle, and Integration

> **Added in v10**: These steps extend the original Phase 2 plan to cover the full
> pipeline from `keyboard.toml` to hardware verification on Charybdis keyboard.

### Architecture Decision Record

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Target keyboard | Charybdis (NoirGuo/rmk-keyboard, nRF52840 split) | User's actual hardware |
| Initial scope | Left hand only → Gazell → dongle → USB HID → PC | Simplest verifiable path; right hand deferred |
| Final scope | Both hands → Gazell → dongle → USB HID → PC (multi-pipe) | Full split keyboard over 2.4GHz |
| Dongle build system | `use_rust` example (manual `main.rs`) | Dongle is not a keyboard; codegen doesn't apply |
| Trackball (pmw3610) | Deferred to multi-peripheral phase | Only on right hand (central), left-hand-first approach skips this |
| keyboard.toml integration | Add `connection = "gazell"` to codegen pipeline | Allows existing Charybdis TOML to work with Gazell by changing one field |

---

### Step 9: Add `"gazell"` branch to codegen layer

**Files**:
- `rmk-macro/src/codegen/entry.rs`
- `rmk-macro/src/codegen/split/central.rs`
- `rmk-macro/src/codegen/split/peripheral.rs`

**9a. entry.rs — central entry point**

Add a third branch after the existing `"serial"` branch:

```rust
} else if split_config.connection == "gazell" {
    let rmk_task = quote! {
        ::rmk::run_rmk(#keymap #usb_driver_arg #storage rmk_config),
    };
    tasks.push(rmk_task);

    // Peripheral manager tasks — one per peripheral
    for (i, _peripheral) in split_config.peripheral.iter().enumerate() {
        let gazell_config = expand_gazell_config(split_config, i);
        let peripheral_task = quote! {
            ::rmk::split::central::run_peripheral_manager::<
                #row, #col, #row_offset, #col_offset
            >(#i, #gazell_config),
        };
        tasks.push(peripheral_task);
    }
    join_all_tasks(tasks)
}
```

**9b. split/central.rs — `expand_split_communication_config`**

Add `"gazell"` match arm in `expand_split_communication_config`:

```rust
"gazell" => {
    // No BLE addrs needed; Gazell uses pipe-based addressing
    // Generate GazellConfig construction from TOML fields
    quote! {}
}
```

**9c. split/peripheral.rs — peripheral dispatch**

Add `"gazell"` branch:

```rust
} else if split_config.connection == "gazell" {
    let gazell_config = expand_gazell_config_peripheral(split_config, peripheral_config);
    let peripheral_run = quote! {
        ::rmk::split::peripheral::run_rmk_split_peripheral(
            #id,
            #gazell_config,
        )
    };
    // ...
}
```

**9d. Helper function `expand_gazell_config`**

New function to generate `GazellConfig` from TOML fields:

```rust
fn expand_gazell_config(split_config: &SplitConfig, peripheral_idx: usize) -> TokenStream2 {
    let pipe = peripheral_idx as u8;  // pipe 0 for first peripheral, 1 for second, etc.
    quote! {
        ::rmk::wireless::config::GazellConfig {
            pipe: #pipe,
            ..::rmk::wireless::config::GazellConfig::low_latency()
        }
    }
}
```

**Verification**:
```bash
# Verify codegen compiles (proc-macro crate)
cargo check --manifest-path rmk-macro/Cargo.toml

# Verify generated code with cargo expand (requires cargo-expand)
cd examples/use_rust/nrf52840_2g4 && cargo expand 2>&1 | head -100
```

---

### Step 10: Add Gazell config fields to `rmk-config`

**File**: `rmk-config/src/lib.rs`

Add optional Gazell-specific fields to `SplitBoardConfig` (or as a separate struct):

```rust
/// Gazell 2.4GHz configuration for split keyboards.
/// Only used when `connection = "gazell"`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct GazellSplitConfig {
    /// RF pipe number (0-7). Default: auto-assigned based on peripheral index.
    #[serde(default)]
    pub pipe: Option<u8>,
    /// RF channel (0-125). Default: from GazellConfig::low_latency().
    #[serde(default)]
    pub channel: Option<u8>,
    /// Heartbeat interval in ms. Default: 50.
    #[serde(default)]
    pub heartbeat_interval_ms: Option<u16>,
}
```

Add to `SplitBoardConfig`:
```rust
pub struct SplitBoardConfig {
    // ... existing fields ...
    #[serde(default)]
    pub gazell: Option<GazellSplitConfig>,
}
```

**keyboard.toml syntax** (example):
```toml
[split]
connection = "gazell"

[split.central]
rows = 5
cols = 6
# gazell config is optional — defaults are fine for most cases
# [split.central.gazell]
# channel = 4

[[split.peripheral]]
rows = 5
cols = 6
row_offset = 0
col_offset = 0
# [split.peripheral.gazell]
# pipe = 0
```

**Verification**:
```bash
cargo check --manifest-path rmk-config/Cargo.toml
cargo test --manifest-path rmk-config/Cargo.toml
```

---

### Step 11: Dongle USB HID forwarding

**File**: `examples/use_rust/nrf52840_dongle/src/main.rs`

Replace the current USB CDC test firmware with a real `GazellCentralDriver` + USB HID forwarding implementation.

**Architecture**:
```
Gazell host (gz_recv)
    │
    ▼
GazellCentralDriver::read() → SplitMessage
    │
    ├─ SplitMessage::Key(event) → update KeyState matrix → generate HID report
    ├─ SplitMessage::Pointing(event) → generate Mouse HID report
    └─ SplitMessage::Touchpad(event) → generate Mouse HID report (future)
    │
    ▼
USB HID Writer → PC
```

**Key design**: The dongle doesn't use RMK's full keyboard pipeline (no `KeyboardTask`, no layers).
Instead, it maintains a simple key state array and generates HID reports directly:

```rust
// Simplified: maintain pressed keys, generate 6KRO HID report
fn process_key_event(event: KeyboardEvent, hid_state: &mut HidState) {
    match event.pos {
        KeyboardEventPos::Key(pos) => {
            // Map (row, col) to keycode using a static lookup table
            // Update pressed/released state
            // Generate USB HID keyboard report
        }
    }
}
```

**Note**: For the initial verification (left hand only, no layers), the dongle can use a
hardcoded keymap lookup table derived from the Charybdis `keyboard.toml` layer 0.
Full layer support would require implementing a minimal keymap processor on the dongle.

**Verification**:
```bash
cd examples/use_rust/nrf52840_dongle && cargo build --release
# Check binary size (should be < 100KB)
```

---

### Step 12: Charybdis keyboard.toml adaptation

Create a Gazell variant of the Charybdis keyboard.toml. Key changes from the BLE version:

```diff
-[ble]
-enabled = true
-battery_adc_pin = "vddh"

 [split]
-connection = "ble"
+connection = "gazell"

 [split.central]
 rows = 5
 cols = 6
 row_offset = 0
 col_offset = 6
-ble_addr = [0x7e, 0xfe, 0x73, 0x9e, 0x66, 0xe3]
 [split.central.matrix]
 # ... unchanged ...

 [[split.peripheral]]
 rows = 5
 cols = 6
 row_offset = 0
 col_offset = 0
-ble_addr = [0x18, 0xe2, 0x21, 0x80, 0xc0, 0xc7]
 [split.peripheral.matrix]
 # ... unchanged ...
```

**Note**: For the initial "left hand only" verification, the `[split.central]` section
describes the **dongle** (which receives from the peripheral). But since the dongle is a
`use_rust` example (not codegen), the central config in TOML is only used to define the
matrix dimensions for the codegen side. The dongle reads these dimensions from its own
hardcoded constants.

**Verification**:
```bash
# Build Charybdis firmware with Gazell
rmkit create --keyboard-toml-path keyboard-gazell.toml --target-dir rmk-gazell
cd rmk-gazell && cargo build --release
```

---

### Step 13: Hardware verification

**Test procedure**:

1. Flash dongle with Step 11 firmware (DFU via `nrfutil dfu usb-serial`)
2. Flash Charybdis left hand with Step 12 firmware (UF2 via double-tap reset)
3. Connect dongle USB to PC
4. Power on Charybdis left hand
5. Open text editor on PC
6. Press keys on left hand → verify characters appear on screen

**Success criteria**:
- All 5×6 matrix keys on left hand produce correct keycodes on PC
- Key press and release both register correctly (no stuck keys)
- Latency is acceptable for typing (subjective, < 50ms)
- Dongle USB CDC debug output shows `RX` messages (if CDC is retained alongside HID)

**Failure debugging**:
- No RX on dongle → check RF channel/address match, check ISR bridging
- RX but no USB output → check SplitMessage deserialization, HID report generation
- Wrong keycodes → check keymap lookup table matches Charybdis TOML layer 0

---

## 12. Future Work (beyond Phase 2)

Items below are **not part of this plan** but are recorded as reminders for subsequent phases.

### Phase 3: Async FFI + Power Optimization

| Item | Description | Priority |
|------|-------------|----------|
| **Async gz_send() wrapper** | Current `gz_send()` blocks up to ~10ms, starving the Embassy executor. Wrap in a dedicated task or use `embassy_futures::yield_now()` after each call. | High |
| **Radio sleep on idle** | After `heartbeat_interval_ms * N` with no ACK response, disable the radio to save power. Re-enable on key event. | Medium |
| **Atomic ACK payload flag** | `ack_payload_ready` in C shim is written by interrupt (callback) and read by main context. Add `volatile` / atomic semantics for correctness under async wrappers. | Medium |
| **Input event retry queue** | Short internal buffer (2-3 events) to survive transient link failures during active typing. Currently events are lost on `gz_send()` failure after 3 retries. | Low |

### Phase 4: BLE / 2.4G Runtime Switching

| Item | Description | Priority |
|------|-------------|----------|
| **Remove compile_error! guard** | Allow both `_ble` and `wireless_gazell` features in the same binary. The radio can only run one protocol at a time, but software can switch. | High |
| **Radio mode manager** | Runtime abstraction to stop current protocol → reconfigure radio → start new protocol. Must handle: FIFO drain, pending_state migration, connection state reset. | High |
| **State synchronization on switch** | When switching from BLE to 2.4G (or vice versa): what happens to pending key events, Layer state, ConnectionState? Define clear semantics (e.g., flush all pending, re-sync state after switch). | High |
| **User-facing switch mechanism** | Key combo, physical switch, or TOML-configured shortcut to trigger protocol switch. | Medium |
| **USB HID re-enumeration** | When switching to/from Gazell dongle, the PC-side USB HID may need to reconnect. Define whether the dongle stays connected or re-enumerates. | Low |

### Phase 5: Multi-Peripheral Support

| Item | Description | Priority |
|------|-------------|----------|
| **Multi-pipe routing** | Use different Gazell pipes (0-7) for different keyboard halves. Central manages per-pipe `pending_state`. | Medium |
| **Pipe-aware PeripheralManager** | Each `PeripheralManager` instance binds to a specific pipe. Requires updating `run_peripheral_manager` to accept pipe ID. | Medium |
| **Pairing / pipe assignment** | Protocol for keyboard halves to negotiate pipe assignment with the dongle on first connection. | Low |

---

## References

[^1]: `NRF_GZLL_CONST_MAX_PAYLOAD_LENGTH = 32` — Nordic nRF5 SDK v17.1.0, `components/proprietary_rf/gzll/nrf_gzll_constants.h:123`. Also mirrored as `MAX_PAYLOAD_LENGTH` in `rmk-gazell-sys/c/gazell_shim.c:9`.
[^2]: `NRF_GZLL_CONST_FIFO_LENGTH = 3` — Nordic nRF5 SDK v17.1.0, `components/proprietary_rf/gzll/nrf_gzll_constants.h:121`.
[^3]: `gz_send()` ~10ms blocking — `rmk-gazell-sys/c/gazell_shim.c:254-271`, busy-wait loop with `timeout = 100000` (~10ms at ~10 cycles/iteration). Theoretical basis: `NRF_GZLL_DEFAULT_TIMESLOT_PERIOD = 600μs` (SDK `nrf_gzll_constants.h:170`) × default retries.
[^4]: `nrf_gzll_fetch_packet_from_rx_fifo(uint32_t pipe, uint8_t* p_payload, uint32_t* p_length)` — Nordic nRF5 SDK v17.1.0, `components/proprietary_rf/gzll/nrf_gzll.h:374`.
[^5]: postcard wire format: enum variants encoded as varint discriminant followed by payload — https://postcard.jamesmunns.com/wire-format#enums
