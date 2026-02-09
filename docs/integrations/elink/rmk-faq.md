# RMK-Specific Elink FAQ

Frequently asked questions about using Elink protocol in RMK keyboards.

## General Questions

### Q: Can I use Elink with my RMK keyboard?

**A**: Yes, if your RMK version is 0.8 or later and you have a split keyboard setup.

**Requirements**:
- RMK 0.8+
- Split keyboard with UART or BLE connection
- `elink` feature enabled in Cargo.toml

### Q: How do I enable Elink?

**A**: Add the `elink` feature to your dependencies:

```toml
[dependencies]
rmk = { version = "0.8", features = ["split", "elink"] }
```

See [Usage Guide](usage-guide.md) for complete setup instructions.

### Q: Is Elink compatible with my MCU?

**A**: Elink works on any MCU supported by RMK that has UART or BLE capability:

| MCU Family | Status | Notes |
|------------|--------|-------|
| STM32H7 | ✅ Tested | Production-ready |
| nRF52840 | ✅ Tested | BLE splits working |
| RP2040 | ✅ Compatible | Not extensively tested |
| ESP32 | ✅ Compatible | Should work |
| STM32F4 | ✅ Compatible | Should work |

### Q: Do I need to modify hardware for Elink?

**A**: No hardware modification needed if you already have a working UART or BLE split connection.

Elink is a software protocol that runs over your existing transport layer.

## Configuration Questions

### Q: What baud rate should I use?

**A**: Recommended: **115200** (default, good balance)

Options:
- **57600**: More reliable over long/noisy cables
- **115200**: Default, works for most setups
- **230400**: Lower latency, requires good connection
- **921600**: Maximum speed, test thoroughly

### Q: What device IDs should I use?

**A**: Standard assignment:

```rust
// Central half (connected to computer)
device_id: 0x0001

// Peripheral half (split keyboard side)
device_id: 0x0002
```

For multi-device setups, use sequential IDs: 0x0003, 0x0004, etc.

### Q: How do I set device ID differently for each half?

**A**: Three options:

**Option 1**: Compile-time feature flags
```rust
#[cfg(feature = "central")]
const DEVICE_ID: u16 = 0x0001;

#[cfg(not(feature = "central"))]
const DEVICE_ID: u16 = 0x0002;
```

**Option 2**: Runtime detection (recommended)
```rust
// Detect based on GPIO pin state
let is_central = gpio_pin.is_high();
let device_id = if is_central { 0x0001 } else { 0x0002 };
```

**Option 3**: Separate firmware builds
- Build once with `device_id: 0x0001`, flash to central
- Build again with `device_id: 0x0002`, flash to peripheral

### Q: What buffer size should I use?

**A**: Default: **512 bytes** (recommended for most keyboards)

Adjust based on use case:
- **256 bytes**: Minimal, simple 2-device split
- **512 bytes**: Default, good for typical keyboards
- **1024 bytes**: High traffic, multiple peripherals

## Troubleshooting

### Q: Keys pressed on peripheral half don't register

**Checklist**:
1. ✅ Both halves powered on?
2. ✅ UART pins connected correctly (TX→RX, RX→TX)?
3. ✅ Common ground between halves?
4. ✅ Device IDs different (0x0001 vs 0x0002)?
5. ✅ Same baud rate on both halves?
6. ✅ `elink` feature enabled in Cargo.toml?

**Debug steps**:
```rust
// Add logging to verify communication
use defmt::info;

// On peripheral
info!("Sending key event: {:?}", event);

// On central
info!("Received from peripheral: {:?}", message);
```

### Q: I see frequent CRC errors in logs

**Causes**:
- Electromagnetic interference
- Long or poor-quality cables
- Incorrect UART configuration
- Baud rate too high for cable length

**Solutions**:
1. **Check UART config**: Verify both halves use same baud rate
2. **Improve cable**: Use shorter, shielded cable
3. **Reduce baud rate**: Try 115200 or 57600
4. **Add hardware filtering**: 100nF capacitors on TX/RX lines
5. **Check power**: Ensure stable power supply

### Q: Typing feels laggy

**First, measure**:
```rust
let start = embassy_time::Instant::now();
driver.write(&message).await?;
info!("Latency: {}µs", start.elapsed().as_micros());
```

**Expected latency**: < 50µs (imperceptible to humans)

**If higher**:
- Increase baud rate to 230400
- Check for CPU bottlenecks
- Reduce buffer sizes if very large
- Profile with embedded tools

### Q: Peripheral half stops responding after some time

**Possible causes**:
- Buffer overflow (messages arriving faster than processed)
- Memory corruption
- Power supply instability
- UART hardware issue

**Solutions**:
1. **Increase buffer size**: Try 1024 bytes
2. **Monitor memory**: Check for leaks or corruption
3. **Verify power**: Measure voltage stability
4. **Add watchdog**: Reset on communication timeout

### Q: Build fails with "elink-core not found"

**A**: Elink protocol is a git submodule. Initialize it:

```bash
git submodule update --init --recursive
```

If already initialized, check:
```bash
ls elink-protocol/
# Should see: elink-core, elink-rmk-adapter, etc.
```

## Performance Questions

### Q: What's the latency of Elink compared to serial?

**A**: Measured on STM32H7:

| Metric | Elink | Serial |
|--------|-------|--------|
| Encoding | 10µs | - |
| CRC | 5µs | - |
| UART TX | 10µs | 10µs |
| **Total overhead** | +22µs | baseline |

**Human perception**: ~10ms (10,000µs)
**Elink overhead**: 22µs = **0.22%** of perception threshold

### Q: How much RAM does Elink use?

**A**: Memory footprint:

| Component | Size |
|-----------|------|
| ElinkRmkAdapter | 576 bytes |
| Stack (peak) | 128 bytes |
| **Total** | ~760 bytes |

For comparison, typical keyboard matrix scanning uses 200-500 bytes.

### Q: Does Elink impact battery life on wireless keyboards?

**A**: Minimal impact.

**CPU usage**: < 0.5% at typical typing rates
**Power**: Negligible compared to BLE radio and matrix scanning

**Recommendation**: Use Elink on wireless keyboards - the reliability benefits outweigh minimal power cost.

### Q: Can Elink handle high typing speeds?

**A**: Yes. Tested up to 1000+ messages/second (far exceeds human typing).

**Typical typing**: 100-200 keystrokes/minute = ~3 messages/second
**Peak capacity**: 1000+ messages/second
**Margin**: 300x headroom

## Compatibility Questions

### Q: Can I use Elink with existing serial split code?

**A**: No, Elink requires its own driver implementation.

Migration path:
1. Add `elink` feature
2. Replace `SerialSplitDriver` with `ElinkSplitDriver`
3. Update configuration (device IDs)
4. Recompile and flash

See [Usage Guide](usage-guide.md) for migration instructions.

### Q: Is Elink compatible with QMK/ZMK?

**A**: Not yet.

**Current status**:
- ✅ RMK: Production-ready
- 🔄 QMK: Community port planned
- 🔄 ZMK: Community port planned

Elink is designed to be firmware-agnostic. See [Elink Generic Integration Guide](https://github.com/Raymond8196/elink-protocol/blob/main/docs/integrations/generic-guide.md) for porting to other firmware.

### Q: Can I mix Elink and serial on different halves?

**A**: No, both halves must use the same protocol.

**Why**: Elink and serial use different frame structures and cannot interoperate.

## Advanced Questions

### Q: Can I use Elink for multiple peripherals (3+ devices)?

**A**: Theoretically yes, but not fully tested.

**Current implementation**: Optimized for 2-device (central + peripheral)

**To add more devices**:
1. Assign unique device IDs (0x0003, 0x0004, etc.)
2. Modify routing logic in RMK
3. Test thoroughly

See [Roadmap](roadmap.md) for multi-peripheral support plans.

### Q: How do I customize priority levels?

**A**: Priority is automatically assigned based on message type:

```rust
// In elink-rmk-adapter/src/message_mapper.rs
SplitMessage::Key(_) => Priority::High      // User input
SplitMessage::Led(_) => Priority::Normal    // Status
SplitMessage::Battery(_) => Priority::Low   // Non-urgent
```

**To customize**: Modify `message_mapper.rs` and rebuild.

### Q: Can I encrypt Elink frames?

**A**: Not built-in, but can be added.

**For BLE**: BLE already provides link-layer encryption

**For UART**: Add encryption layer between serialization and framing:
```rust
let payload = encrypt(message_bytes)?;
let frame = create_elink_frame(payload)?;
```

See [Elink Roadmap](https://github.com/Raymond8196/elink-protocol/blob/main/docs/roadmap.md) for encryption feature plans.

### Q: How do I debug Elink communication?

**A**: Enable logging:

```rust
use defmt::{info, error};

// Log sent messages
info!("TX: {:?}", message);

// Log received messages
info!("RX: {:?}", message);

// Log errors
error!("Elink error: {:?}", error);
```

**Hardware debugging**: Use logic analyzer on UART TX/RX pins to see raw bytes.

## Build and Deployment

### Q: Do I need different firmware for central and peripheral?

**A**: No, use same firmware with different device ID configuration.

**Recommended approach**: Runtime detection
```rust
let device_id = detect_device_role();  // Based on GPIO or other signal
```

### Q: How do I flash firmware to both halves?

**A**: Flash same firmware, ensure device IDs are set correctly.

```bash
# Flash central half
probe-rs run --chip <MCU> target/release/firmware

# Flash peripheral half (if using compile-time config)
cargo build --release --features peripheral
probe-rs run --chip <MCU> target/release/firmware
```

### Q: Can I update only one half?

**A**: Yes, as long as protocol version matches.

**Safe to update one half**:
- Bug fixes in non-protocol code
- LED effects changes
- Keymap changes

**Update both halves**:
- Protocol version changes
- Message format changes
- CRC algorithm changes

## Getting Help

### Q: Where can I get help with Elink in RMK?

**Resources**:
1. **This FAQ** - Common RMK-specific questions
2. **[Usage Guide](usage-guide.md)** - Setup and configuration
3. **[Integration Guide](integration-guide.md)** - Technical details
4. **[Elink FAQ](https://github.com/Raymond8196/elink-protocol/blob/main/docs/faq.md)** - Protocol questions
5. **RMK Community** - GitHub Discussions

### Q: How do I report bugs?

**For protocol bugs** (CRC errors, frame parsing):
- Open issue in [elink-protocol repository](https://github.com/Raymond8196/elink-protocol/issues)

**For RMK integration bugs** (build errors, RMK-specific issues):
- Open issue in [RMK repository](https://github.com/HaoboGu/rmk/issues)

**Include**:
- RMK version
- MCU type
- Minimal reproducible example
- Error logs

### Q: How can I contribute?

**Ways to contribute**:
1. Test on different hardware platforms
2. Report bugs with detailed information
3. Improve documentation
4. Add examples for new MCUs
5. Performance optimizations

See [CONTRIBUTING.md](https://github.com/Raymond8196/elink-protocol/blob/main/CONTRIBUTING.md)

---

**Still have questions?** Open a GitHub Discussion in the RMK repository!

*Last updated: 2026-02-09*
