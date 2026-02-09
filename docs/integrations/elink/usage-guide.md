# Using Elink in Your RMK Keyboard

Step-by-step guide for using the Elink protocol in your RMK-based split keyboard.

## Prerequisites

- RMK version 0.8 or later
- Split keyboard with UART or BLE connection
- Basic understanding of RMK keyboard configuration

## Step 1: Enable Elink Feature

Add Elink feature to your keyboard's `Cargo.toml`:

```toml
[dependencies]
rmk = { version = "0.8", features = ["split", "elink"] }
```

**Note**: The `elink` feature automatically includes the protocol implementation.

## Step 2: Configure Hardware

### For UART Split

Configure UART pins in your keyboard's hardware setup:

```rust
use embassy_stm32::usart::{Config as UartConfig, Uart};

// UART configuration
let mut uart_config = UartConfig::default();
uart_config.baudrate = 115200;  // Adjust as needed

// Initialize UART
let uart = Uart::new(
    p.USART1,
    p.PA10,  // RX pin
    p.PA9,   // TX pin
    Irqs,
    p.DMA1_CH4,
    p.DMA1_CH5,
    uart_config,
)?;
```

**Recommended baud rates**:
- **115200**: Good balance (default)
- **230400**: Higher throughput
- **921600**: Maximum speed (check MCU support)

### For BLE Split

BLE uses the same Elink protocol over wireless:

```rust
use embassy_nrf::uarte::{Config as UartConfig, Uarte};

let uart_config = UartConfig::default();
uart_config.baudrate = Baudrate::BAUD115200;

let uart = Uarte::new(
    p.UARTE0,
    Irqs,
    p.P0_08,  // TX
    p.P0_06,  // RX
    uart_config,
);
```

## Step 3: Initialize Elink Driver

### Basic Setup

```rust
use rmk::split::elink::{ElinkSplitDriver, ElinkConfig};

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // ... hardware initialization ...

    // Configure Elink
    let elink_config = ElinkConfig {
        device_id: 0x0001,      // This device (central=0x0001, peripheral=0x0002)
        target_device: 0x0002,   // Peer device
        receive_buffer_size: 512, // Optional: adjust if needed
    };

    // Create Elink driver
    let (uart_rx, uart_tx) = uart.split();
    let elink_driver = ElinkSplitDriver::new(
        uart_rx,
        uart_tx,
        elink_config,
    );

    // Use with RMK
    rmk::start_with_split(
        keyboard_matrix,
        elink_driver,
        spawner,
    ).await;
}
```

### Device ID Assignment

| Device | ID | Description |
|--------|-----|-------------|
| Central (main half) | 0x0001 | Connected to USB/BLE host |
| Peripheral (split half) | 0x0002 | Sends events to central |

**For future multi-device setups**:
- Numpad: 0x0003
- Macropad: 0x0004
- etc.

## Step 4: Flash Firmware

### Central Half

```bash
# Build for central device
cd your_keyboard_project

# Flash central half
probe-rs run --chip STM32H750VBTx target/thumbv7em-none-eabihf/release/firmware

# Or for nRF52840
probe-rs run --chip nRF52840_xxAA target/thumbv7em-none-eabihf/release/firmware
```

### Peripheral Half

**Important**: Use the same firmware with different device ID.

Option 1: Compile-time configuration
```rust
#[cfg(feature = "central")]
const DEVICE_ID: u16 = 0x0001;

#[cfg(feature = "peripheral")]
const DEVICE_ID: u16 = 0x0002;
```

Option 2: Runtime detection (recommended)
```rust
// Detect based on GPIO or other hardware signal
let device_id = if is_central_half() { 0x0001 } else { 0x0002 };
```

## Configuration Options

### Buffer Sizes

Adjust based on your use case:

```rust
let elink_config = ElinkConfig {
    device_id: 0x0001,
    target_device: 0x0002,
    receive_buffer_size: 1024,  // Increase for high-traffic scenarios
};
```

**Guidelines**:
- **256 bytes**: Minimal, simple 2-device split
- **512 bytes**: Default, recommended for most keyboards
- **1024 bytes**: High-traffic, multiple peripherals

### UART Baud Rate

Higher baud rates = lower latency:

```rust
uart_config.baudrate = 230400;  // 2x default
```

**Trade-offs**:
- **Higher rate**: Lower latency, may be less reliable over long cables
- **Lower rate**: More reliable, slightly higher latency

**Recommended**: Start with 115200, increase only if needed.

### Priority Levels (Advanced)

Elink automatically assigns priorities:

```rust
// In elink-rmk-adapter (automatic)
SplitMessage::Key(_) => Priority::High      // Immediate processing
SplitMessage::Led(_) => Priority::Normal    // Status updates
SplitMessage::Battery(_) => Priority::Low   // Non-urgent
```

No manual configuration needed for typical use cases.

## Verification

### Check Communication

Add debug logging to verify messages are being sent/received:

```rust
use defmt::info;

// In your main loop or split driver
info!("Sent message: {:?}", message);
info!("Received message: {:?}", message);
```

### Test Typing

1. **Press keys on central half** → Should register immediately
2. **Press keys on peripheral half** → Should register via Elink
3. **Check LEDs** → Should sync between halves

### Monitor Errors

```rust
match driver.read().await {
    Ok(msg) => { /* process */ },
    Err(e) => {
        defmt::error!("Split read error: {:?}", e);
    }
}
```

**Common errors**:
- `TransportError`: UART issue, check wiring
- `ProtocolError`: CRC failure, check interference
- `Timeout`: No data, check peer device

## Troubleshooting

### Keys Not Registering from Peripheral

**Check**:
1. ✅ Both halves powered on
2. ✅ UART TX/RX connected (TX → RX, RX → TX)
3. ✅ Common ground between halves
4. ✅ Correct device IDs
5. ✅ Same baud rate on both halves

**Debug**:
```rust
// On peripheral: verify sending
info!("Sending key event: {:?}", event);

// On central: verify receiving
info!("Received from peripheral: {:?}", message);
```

### Intermittent Key Loss

**Possible causes**:
- Electrical noise on UART line
- Cable too long (>1 meter)
- Loose connections
- Insufficient power

**Solutions**:
- Add pull-up resistors (4.7kΩ) on TX/RX lines
- Use shielded cable
- Reduce baud rate
- Check power supply stability

### High Latency

**Measure latency**:
```rust
let start = embassy_time::Instant::now();
driver.write(&message).await?;
let duration = start.elapsed();
info!("Latency: {}µs", duration.as_micros());
```

**Typical latencies**:
- UART write: 10-20µs
- Processing: 20-30µs
- Total: < 50µs (imperceptible)

**If higher than expected**:
- Increase UART baud rate
- Check buffer sizes
- Profile code for bottlenecks

### CRC Errors

**Symptom**: `ProtocolError::InvalidCrc` in logs

**Causes**:
- Electromagnetic interference
- Poor cable quality
- Incorrect UART configuration
- Hardware issue

**Solutions**:
1. **Verify UART settings match** on both halves
2. **Check cable**: Replace if damaged
3. **Add filtering**: 100nF capacitors on UART lines
4. **Reduce baud rate**: Try 57600 or 115200

## Performance Tips

### Minimize Latency

1. **Use high baud rate** (230400 or higher)
2. **Reduce buffer sizes** (if memory-constrained)
3. **Use wired connection** (lower latency than BLE)
4. **Optimize priority** (High for key events)

### Maximize Reliability

1. **Use quality cables** (shielded, short)
2. **Add hardware filtering** (pull-ups, capacitors)
3. **Enable error logging** (monitor CRC errors)
4. **Test thoroughly** (24+ hour stability test)

### Balance Both

**Recommended configuration**:
```rust
// UART
uart_config.baudrate = 115200;  // Reliable, fast enough

// Elink
receive_buffer_size: 512;  // Good buffer without waste

// Priorities: Automatic (default)
```

## Examples

### Complete Example: nRF52840 BLE Split

Located in `rmk/examples/use_rust/nrf52840_ble_split_elink/`:

```rust
#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_nrf::init(Default::default());

    // UART for split communication
    let mut uart_config = uarte::Config::default();
    uart_config.baudrate = uarte::Baudrate::BAUD115200;

    let uart = Uarte::new(p.UARTE0, Irqs, p.P0_08, p.P0_06, uart_config);
    let (uart_rx, uart_tx) = uart.split();

    // Elink driver
    let device_id = if is_central() { 0x0001 } else { 0x0002 };
    let target_id = if is_central() { 0x0002 } else { 0x0001 };

    let elink_config = ElinkConfig {
        device_id,
        target_device: target_id,
        receive_buffer_size: 512,
    };

    let elink_driver = ElinkSplitDriver::new(uart_rx, uart_tx, elink_config);

    // RMK keyboard
    let keyboard = KeyboardBuilder::new()
        .split_driver(elink_driver)
        .build();

    keyboard.run(spawner).await;
}
```

### Build and Flash

```bash
# Build example
cd examples/use_rust/nrf52840_ble_split_elink
cargo build --release

# Flash central half
probe-rs run --chip nRF52840_xxAA

# Flash peripheral half (change device_id first)
probe-rs run --chip nRF52840_xxAA
```

## When to Use Elink vs Serial

### Use Elink if:
- ✅ Wireless split (BLE)
- ✅ Multiple peripherals (3+)
- ✅ Need reliability guarantees
- ✅ Want error detection
- ✅ Future-proofing

### Use Serial if:
- ✅ Simple wired 2-device split
- ✅ Absolute minimal latency needed
- ✅ Very memory-constrained MCU
- ✅ Don't need error detection

### Performance Comparison

| Metric | Elink | Serial |
|--------|-------|--------|
| Latency | ~50µs | ~20µs |
| Reliability | 99.99%+ | ~95% |
| Error detection | CRC-16 | Parity |
| Multi-device | Yes | No |
| RAM usage | 760 bytes | 128 bytes |

**Recommendation**: Use Elink for wireless or multi-device setups. For simple wired splits where minimal latency is critical, serial may suffice.

## Next Steps

- Read [Integration Guide](integration-guide.md) for technical details
- Check [RMK FAQ](rmk-faq.md) for common questions
- See [Elink Protocol Specification](https://github.com/Raymond8196/elink-protocol/blob/main/docs/protocol-specification-en.md) for protocol details
- Join RMK community for support

---

*Last updated: 2026-02-09*
