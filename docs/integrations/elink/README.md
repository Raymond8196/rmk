# Elink Integration in RMK

RMK uses the Elink protocol for split keyboard communication, providing reliable data transfer with CRC verification and priority support.

## What is Elink?

Elink is an **independent, firmware-agnostic communication protocol** designed for embedded systems.

**Elink Protocol Repository**: https://github.com/Raymond8196/elink-protocol

**Key Features**:
- CRC-4/CRC-16 error detection
- 4-level priority system
- Extended device addressing (up to 65,536 devices)
- No_std compatible for embedded systems

## Documentation

### Elink Protocol Documentation (Main Repository)

For protocol specifications and design details, see the Elink repository:

- **[Protocol Specification (EN)](https://github.com/Raymond8196/elink-protocol/blob/main/docs/protocol-specification-en.md)** - Complete technical reference
- **[协议规范（中文）](https://github.com/Raymond8196/elink-protocol/blob/main/docs/protocol-specification-zh.md)** - 完整技术规范
- **[Architecture](https://github.com/Raymond8196/elink-protocol/blob/main/docs/architecture.md)** - Design principles and rationale
- **[FAQ](https://github.com/Raymond8196/elink-protocol/blob/main/docs/faq.md)** - Protocol FAQ ([中文版](https://github.com/Raymond8196/elink-protocol/blob/main/docs/faq-zh.md))
- **[Generic Integration Guide](https://github.com/Raymond8196/elink-protocol/blob/main/docs/integrations/generic-guide.md)** - How to integrate into any firmware

### RMK-Specific Documentation (This Directory)

For RMK-specific integration and usage:

- **[Usage Guide](usage-guide.md)** - How to use Elink in your RMK keyboard
- **[Integration Guide](integration-guide.md)** - How Elink is integrated into RMK (for contributors)
- **[RMK FAQ](rmk-faq.md)** - RMK-specific questions and troubleshooting
- **[Roadmap](roadmap.md)** - Development roadmap for Elink in RMK ([中文版](roadmap-zh.md))

## Quick Start

### Enable Elink in Your RMK Keyboard

```toml
# Cargo.toml
[dependencies]
rmk = { version = "0.8", features = ["split", "elink"] }
```

### Basic Setup

```rust
use rmk::split::elink::ElinkSplitDriver;

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // Initialize UART for split communication
    let uart = /* ... setup UART ... */;

    // Create Elink driver
    let elink_driver = ElinkSplitDriver::new(
        uart,
        device_id: 0x0001,  // Central device
    );

    // Use with RMK
    rmk::start_with_split(keyboard, elink_driver, spawner).await;
}
```

See [Usage Guide](usage-guide.md) for complete instructions.

## Why Elink in RMK?

### Advantages Over Serial Split

| Feature | Elink | Serial |
|---------|-------|--------|
| Error Detection | CRC-16 | Parity (weak) |
| Reliability | High | Medium |
| Multi-device | Yes | No |
| Priority Support | Yes | No |
| Overhead | +22µs | Baseline |

### When to Use Elink

**Recommended for**:
- Wireless split keyboards (BLE) - prone to interference
- Multi-device setups (3+ peripherals)
- Projects requiring reliability guarantees
- Future-proofing your keyboard design

**Use serial instead if**:
- Simple 2-device wired split
- Absolute minimal latency required (< 30µs)
- Resource-constrained MCUs

See [Usage Guide](usage-guide.md) for detailed comparison.

## Performance

Measured on STM32H7 @ 480MHz:

- **Total overhead**: ~22µs per message
- **Memory usage**: 760 bytes RAM
- **CPU usage**: < 0.5% at typical keyboard load
- **Reliability**: 99.99%+ with CRC verification

See [Integration Guide](integration-guide.md) for detailed performance analysis.

## Examples

RMK includes Elink examples:

```bash
# nRF52840 BLE split keyboard with Elink
cd examples/use_rust/nrf52840_ble_split_dongle
cargo build --release

# STM32H7 split keyboard with Elink
cd examples/use_rust/stm32h7_split_elink
cargo build --release
```

## Contributing

Contributions welcome! See:

- **Protocol improvements**: [Elink CONTRIBUTING.md](https://github.com/Raymond8196/elink-protocol/blob/main/CONTRIBUTING.md)
- **RMK integration improvements**: [RMK CONTRIBUTING.md](../../CONTRIBUTING.md)

## Support

### Questions about Elink Protocol

- Check [Elink FAQ](https://github.com/Raymond8196/elink-protocol/blob/main/docs/faq.md)
- Open issue in [elink-protocol repository](https://github.com/Raymond8196/elink-protocol/issues)

### Questions about RMK Integration

- Check [RMK FAQ](rmk-faq.md)
- Check [Usage Guide](usage-guide.md)
- Open issue in [RMK repository](https://github.com/HaoboGu/rmk/issues)

## Version Compatibility

| RMK Version | Elink Protocol Version | Status |
|-------------|------------------------|--------|
| 0.8+ | v1.0 | ✅ Stable |
| 0.7 | Not supported | ❌ |

## License

Elink protocol and RMK integration follow their respective project licenses.

---

**RMK + Elink: Reliable split keyboard communication**

*Last updated: 2026-02-09*
