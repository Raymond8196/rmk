# How Elink is Integrated into RMK

Technical details of the Elink protocol integration for RMK contributors and advanced users.

## Integration Architecture

### Overview

```
RMK Application Layer
    ↓ SplitMessage (keyboard events, LED states, etc.)
ElinkSplitDriver (rmk/src/split/elink/mod.rs)
    ↓ Implements SplitReader/SplitWriter traits
ElinkRmkAdapter (elink-protocol/elink-rmk-adapter)
    ↓ Message ↔ Frame conversion
ElinkCore (elink-protocol/elink-core)
    ↓ Protocol logic, CRC, parsing
Transport Layer (UART/BLE)
    ↓ Raw bytes
Hardware (MCU peripherals)
```

### Key Components

#### 1. SplitMessage (RMK Core)

RMK's internal split communication format:

```rust
// rmk/src/split/message.rs
pub enum SplitMessage {
    Key(KeyEvent),
    Led(LedState),
    // ... other message types
}
```

#### 2. ElinkSplitDriver (RMK Integration Layer)

Located in `rmk/src/split/elink/mod.rs`:

```rust
pub struct ElinkSplitDriver {
    uart_rx: UartRx<'static>,
    uart_tx: UartTx<'static>,
    adapter: ElinkRmkAdapter,
}

impl SplitReader for ElinkSplitDriver {
    async fn read(&mut self) -> Result<SplitMessage, SplitDriverError> {
        // Read bytes from UART
        // Process through adapter
        // Return decoded message
    }
}

impl SplitWriter for ElinkSplitDriver {
    async fn write(&mut self, message: &SplitMessage) -> Result<usize, SplitDriverError> {
        // Encode message through adapter
        // Write bytes to UART
        // Return bytes written
    }
}
```

**Responsibilities**:
- Implement RMK's split driver traits
- Manage UART peripheral
- Handle async I/O
- Error conversion (Elink errors → RMK errors)

#### 3. ElinkRmkAdapter (Adapter Layer)

Located in `elink-protocol/elink-rmk-adapter/src/adapter.rs`:

```rust
pub struct ElinkRmkAdapter {
    device_id: u16,
    receive_buffer: [u8; 512],
    send_buffer: [u8; 64],
    buffer_len: usize,
}

impl ElinkRmkAdapter {
    pub fn encode_message(&mut self, message: &SplitMessage, target: u16) -> Result<&[u8], Error>;
    pub fn decode_frame(&mut self, bytes: &[u8]) -> Result<SplitMessage, Error>;
    pub fn process_incoming_bytes(&mut self, bytes: &[u8]) -> Result<Option<SplitMessage>, Error>;
}
```

**Responsibilities**:
- Convert SplitMessage ↔ Elink frames
- Manage buffering
- Call ElinkCore for frame serialization/parsing

#### 4. ElinkCore (Protocol Layer)

Located in `elink-protocol/elink-core/src/`:

```rust
// Frame parsing and serialization
pub fn parse_frame(data: &[u8]) -> Result<ProtocolFrame, ElinkError>;

// CRC calculation
pub fn calculate_crc16(data: &[u8]) -> u16;

// Priority handling
pub enum Priority { Low, Normal, High, Critical }
```

**Responsibilities**:
- Pure protocol logic
- Frame structures (CompatibleFrame, StandardFrame)
- CRC algorithms
- Validation rules

## Integration Points

### Feature Flags

```toml
# rmk/Cargo.toml
[features]
split = []
elink = ["dep:elink-core", "dep:elink-rmk-adapter"]

[dependencies]
elink-core = { path = "elink-protocol/elink-core", optional = true }
elink-rmk-adapter = { path = "elink-protocol/elink-rmk-adapter", optional = true }
```

**Compilation**:
```bash
# With Elink
cargo build --features split,elink

# Without Elink (serial only)
cargo build --features split
```

### Message Mapping

#### Priority Assignment

```rust
// elink-rmk-adapter/src/message_mapper.rs
impl SplitMessage {
    pub fn elink_priority(&self) -> Priority {
        match self {
            SplitMessage::Key(_) => Priority::High,      // User input critical
            SplitMessage::Mouse(_) => Priority::High,    // User input critical
            SplitMessage::Led(_) => Priority::Normal,    // Status update
            SplitMessage::Battery(_) => Priority::Low,   // Not urgent
            SplitMessage::Config(_) => Priority::Low,    // Bulk transfer
        }
    }
}
```

#### Serialization

Uses `postcard` for efficient binary serialization:

```rust
use postcard;

// Serialize
let payload = postcard::to_slice(&message, &mut buffer)?;

// Deserialize
let message = postcard::from_bytes::<SplitMessage>(&payload)?;
```

### Device Addressing

Current implementation:
- **Central device**: 0x0001
- **Peripheral device**: 0x0002

Future: Support multiple peripherals with unique IDs.

## Data Flow

### Sending a Message (Central → Peripheral)

1. **User presses key** → RMK generates `SplitMessage::Key(event)`
2. **Application calls** `driver.write(&message)`
3. **ElinkSplitDriver** → calls `adapter.encode_message(&message, target_device)`
4. **ElinkRmkAdapter**:
   - Serializes message with `postcard`
   - Determines priority
   - Creates `StandardFrame` with payload
5. **ElinkCore**:
   - Serializes frame structure
   - Calculates CRC-16
   - Returns byte array
6. **ElinkSplitDriver** → writes bytes to UART
7. **UART peripheral** → transmits bytes

**Latency breakdown**:
- Serialization: ~3µs
- Frame creation: ~7µs (including CRC)
- UART write: ~10µs
- **Total**: ~20µs

### Receiving a Message (Peripheral → Central)

1. **UART receives bytes** → interrupts RMK
2. **ElinkSplitDriver** → reads bytes into temp buffer
3. **ElinkSplitDriver** → calls `adapter.process_incoming_bytes(bytes)`
4. **ElinkRmkAdapter**:
   - Adds bytes to receive buffer
   - Tries to parse frame
5. **ElinkCore**:
   - Validates frame structure
   - Checks CRC-16
   - Returns `ProtocolFrame`
6. **ElinkRmkAdapter**:
   - Extracts payload
   - Deserializes with `postcard`
   - Returns `SplitMessage`
7. **ElinkSplitDriver** → returns message to RMK
8. **RMK processes** key event

**Latency breakdown**:
- UART read: ~10µs
- Frame parsing: ~7µs (including CRC)
- Deserialization: ~3µs
- **Total**: ~20µs

## Error Handling

### Error Types

```rust
// Elink errors
pub enum ElinkError {
    FrameTooShort,
    InvalidStartOfFrame,
    InvalidCrc,
    PayloadTooLarge,
}

// RMK split driver errors
pub enum SplitDriverError {
    TransportError,
    ProtocolError,
    Timeout,
}
```

### Error Recovery Strategy

#### CRC Errors

```rust
// In ElinkRmkAdapter::process_incoming_bytes
match ProtocolFrame::parse(buffer) {
    Ok(frame) => { /* process frame */ },
    Err(ElinkError::InvalidCrc) => {
        // Skip 1 byte, try next frame
        buffer.advance(1);
        continue;
    }
    Err(e) => return Err(e),
}
```

**Strategy**: Continue processing, don't abort on single error.

#### Buffer Overflow

```rust
if buffer.remaining() < new_bytes.len() {
    // Discard oldest data
    buffer.compact();
}
```

**Strategy**: FIFO buffer management, discard old data under pressure.

### Retry Logic

**Not implemented in protocol** - left to application layer:

```rust
// Application-level retry
for attempt in 0..MAX_RETRIES {
    match driver.write(&message).await {
        Ok(_) => break,
        Err(e) if attempt < MAX_RETRIES - 1 => {
            delay_ms(RETRY_DELAY).await;
        }
        Err(e) => return Err(e),
    }
}
```

## Performance Optimization

### Buffer Sizes

Carefully tuned for embedded systems:

```rust
pub struct ElinkRmkAdapter {
    receive_buffer: [u8; 512],  // Holds ~8 max frames
    send_buffer: [u8; 64],      // 1 max frame
}
```

**Rationale**:
- 512 bytes receive: Balance between buffering and RAM usage
- 64 bytes send: Exact max frame size, no waste

### Loop Optimization

**Before**:
```rust
// Always read UART first
let bytes = uart.read().await?;
adapter.process(bytes)?;
```

**After**:
```rust
// Check adapter buffer first
if let Some(msg) = adapter.try_decode_buffered()? {
    return Ok(msg);
}
// Only read UART if buffer empty
let bytes = uart.read().await?;
```

**Benefit**: Reduce unnecessary UART reads by 50%

### Memory Footprint

| Component | Size | Type |
|-----------|------|------|
| ElinkRmkAdapter | 576 bytes | Static |
| Temp buffers | 128 bytes | Stack |
| Code size | ~8KB | Flash |
| **Total RAM** | 704 bytes | - |

## Testing

### Unit Tests

Located in `elink-rmk-adapter/src/adapter.rs`:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_encode_decode_roundtrip() {
        let message = SplitMessage::Key(/* ... */);
        let encoded = adapter.encode_message(&message, 0x0002).unwrap();
        let decoded = adapter.decode_frame(&encoded).unwrap();
        assert_eq!(message, decoded);
    }
}
```

### Integration Tests

Located in `elink-rmk-adapter/examples/`:

```rust
// pc_test.rs - Full encode/decode cycle
// benchmark.rs - Performance testing
```

### Hardware Testing

On actual hardware (STM32H7, nRF52840):

```bash
cd examples/use_rust/nrf52840_ble_split_elink
cargo build --release
probe-rs run --chip nRF52840
```

## Code Locations

### In RMK Repository

```
rmk/
├── src/
│   └── split/
│       └── elink/
│           └── mod.rs          # ElinkSplitDriver implementation
└── examples/
    └── use_rust/
        └── nrf52840_ble_split_elink/  # Example project
```

### In Elink-Protocol Repository

```
elink-protocol/
├── elink-core/              # Core protocol
│   └── src/
│       ├── protocol_frame.rs
│       ├── protocol_crc.rs
│       └── priority.rs
└── elink-rmk-adapter/       # RMK adapter
    ├── src/
    │   ├── adapter.rs       # Main adapter logic
    │   └── message_mapper.rs
    └── examples/
        ├── pc_test.rs
        └── benchmark.rs
```

## Contributing to Integration

### Adding New Message Types

1. **Define in RMK**: Add to `SplitMessage` enum
2. **Map priority**: Add case in `message_mapper.rs`
3. **Test**: Add roundtrip test
4. **Document**: Update this guide

### Performance Improvements

1. **Measure first**: Use `benchmark.rs`
2. **Optimize carefully**: Don't break correctness
3. **Test on hardware**: PC tests don't show real latency
4. **Document**: Update performance numbers

### Bug Fixes

1. **Reproduce**: Write failing test
2. **Fix**: Minimal change
3. **Verify**: All tests pass
4. **Document**: Update if behavior changes

## Troubleshooting

### Messages Not Received

**Check**:
1. Device IDs match
2. UART configuration (baud rate, pins)
3. Feature flags enabled
4. Buffer sizes sufficient

**Debug**:
```rust
defmt::info!("Encoded frame: {:?}", frame_bytes);
defmt::info!("Received bytes: {:?}", received_bytes);
```

### Frequent CRC Errors

**Causes**:
- Electrical noise on UART line
- Baud rate mismatch
- Cable too long/poor quality

**Solutions**:
- Add pull-up resistors
- Reduce baud rate
- Use shielded cable
- Check ground connection

### High Latency

**Check**:
- Priority levels correct?
- Buffer sizes too large?
- UART baud rate sufficient?

**Profile**:
```rust
let start = embassy_time::Instant::now();
driver.write(&message).await?;
let duration = start.elapsed();
defmt::info!("Write took: {}µs", duration.as_micros());
```

## References

- [Elink Protocol Specification](https://github.com/Raymond8196/elink-protocol/blob/main/docs/protocol-specification-en.md)
- [Elink Architecture](https://github.com/Raymond8196/elink-protocol/blob/main/docs/architecture.md)
- [RMK Documentation](https://haobogu.github.io/rmk/)

---

*Last updated: 2026-02-09*
