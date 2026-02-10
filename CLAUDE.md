# RMK + Elink Protocol Development Guide

> This document defines Claude Code working standards for RMK keyboard firmware and Elink communication protocol project.
> Update this file whenever Claude makes mistakes or when new standards emerge - forming a flywheel effect (Boris Cherny Tip #4).

## Language Policy

**IMPORTANT**:
- All documentation, commit messages, code comments, and PR descriptions **MUST be in English**
- Conversation with the user can be in Chinese or English
- This ensures the project is accessible to the international community

## Project Overview

### RMK (Rust Mechanical Keyboard)
- **Language**: Rust (no_std)
- **Framework**: Embassy async
- **Supported MCUs**: STM32, nRF52, RP2040, ESP32
- **Core Features**: Keyboard firmware, split keyboard, BLE/USB communication

### Elink Protocol
- **Design Principles**: Industrial-grade communication system design
- **Key Features**: CRC verification, priority support, extended device addressing
- **Integration Location**: `rmk/src/split/elink/`
- **Submodule**: `elink-protocol/` (independent Git repository)

## Code Standards

### Rust General Standards

#### 1. Formatting
- **Mandatory**: All code must pass `cargo fmt`
- **Check**: Use `cargo clippy` to eliminate warnings
- **Commands**:
  ```bash
  cargo fmt --all
  cargo clippy --all-targets --all-features
  ```

#### 2. Error Handling
- ✅ Use `Result<T, E>` instead of `unwrap()`
- ✅ Implement `Display` and `Debug` for custom error types
- ❌ Avoid `panic!()` in library code (dangerous in embedded)
- ✅ Prefer `?` operator for error propagation

```rust
// ✅ Correct
pub fn read_frame(&mut self) -> Result<Frame, ElinkError> {
    let data = self.transport.read().await?;
    Frame::parse(data)
}

// ❌ Wrong
pub fn read_frame(&mut self) -> Frame {
    let data = self.transport.read().await.unwrap();
    Frame::parse(data).unwrap()
}
```

#### 3. Async Code Standards
- Use Embassy's `async/await`
- Avoid blocking operations (no OS scheduler in embedded)
- Prefer channel communication over shared state

```rust
// ✅ Use Embassy Channel
use embassy_sync::channel::Channel;

static KEY_EVENTS: Channel<KeyEvent, 32> = Channel::new();

// ❌ Don't use std sync primitives
// use std::sync::Mutex; // Not available in no_std
```

#### 4. Memory Management
- **Forbidden**: Don't use `Box`, `Vec`, `String` in no_std environments
- **Prefer**: Use fixed-size arrays and `heapless` containers
- **Check**: Ensure code compiles with `#![no_std]`

```rust
// ✅ no_std compatible
use heapless::Vec;
let mut buffer: Vec<u8, 64> = Vec::new();

// ❌ std dependency
let mut buffer = std::vec::Vec::new();
```

### Embedded Rust Specific Standards

#### 1. Dependency Management
- All dependencies must support `no_std`
- Use `default-features = false` in `Cargo.toml`

```toml
[dependencies]
serde = { version = "1.0", default-features = false, features = ["derive"] }
postcard = { version = "1.0", default-features = false }
```

#### 2. Feature Gates
- Use optional features for large functionality
- Allow users to trim firmware size

```rust
#[cfg(feature = "elink")]
pub mod elink;

#[cfg(not(feature = "elink"))]
pub mod serial;
```

#### 3. Stack Memory Control
- Avoid large stack allocations (embedded stack typically < 64KB)
- Use `static` or `static mut` (with `CriticalSection`) for large buffers

```rust
// ✅ Static allocation
static mut RX_BUFFER: [u8; 512] = [0; 512];

// ❌ Dangerous large stack allocation
fn process() {
    let buffer = [0u8; 4096]; // May cause stack overflow
}
```

### RMK Project Specific Standards

#### 1. Module Organization
```
rmk/
├── src/
│   ├── split/          # Split keyboard communication
│   │   ├── serial/     # Serial transport
│   │   ├── ble/        # BLE transport
│   │   ├── elink/      # Elink protocol ← New
│   │   └── driver.rs   # Common trait definitions
│   ├── hid.rs          # HID reports
│   ├── usb/            # USB communication
│   └── ble/            # BLE stack
```

#### 2. Trait Design
- All transport layers implement `SplitReader` and `SplitWriter` traits
- Maintain interface consistency

```rust
pub trait SplitReader {
    async fn read(&mut self) -> Result<SplitMessage, SplitDriverError>;
}

pub trait SplitWriter {
    async fn write(&mut self, message: &SplitMessage) -> Result<usize, SplitDriverError>;
}
```

#### 3. Message Serialization
- Use `postcard` for binary serialization
- Define clear maximum message sizes

```rust
const MAX_MESSAGE_SIZE: usize = 64;

#[derive(Serialize, Deserialize)]
pub enum SplitMessage {
    Key(KeyboardEvent),
    // ...
}
```

### Elink Protocol Specific Standards

#### 1. Frame Validation
- **Must**: Validate CRC for all received frames
- **Must**: Calculate and set CRC before sending
- **Error Recovery**: Continue to next frame on CRC failure, don't abort

```rust
// ✅ Correct error recovery
loop {
    match self.adapter.process_incoming_bytes(&bytes) {
        Ok(Some(msg)) => return Ok(msg),
        Err(ElinkError::InvalidCrc) => {
            // Continue loop, try next frame
            continue;
        }
        Err(e) => return Err(e),
    }
}
```

#### 2. Buffer Management
- **Receive buffer**: 512 bytes (optimized)
- **Send buffer**: 64 bytes (max frame)
- **Temporary buffer**: 128 bytes (on stack)

```rust
pub struct ElinkAdapter {
    receive_buffer: [u8; 512],  // Don't increase arbitrarily
    send_buffer: [u8; 64],
    // ...
}
```

#### 3. Performance Considerations
- Elink overhead: +22.59µs/message (acceptable)
- CPU usage: < 0.5% (100-200 msg/s)
- Don't sacrifice readability for micro-optimizations

#### 4. Priority Usage
```rust
// Use high priority for critical events
KeyboardEvent -> Priority::High      // Key events
MouseEvent -> Priority::High         // Mouse movement
BatteryLevel -> Priority::Low        // Battery reports
ConfigSync -> Priority::Low          // Config sync
```

## Git Commit Standards

### Commit Message Format
```
<type>(<scope>): <subject>

<body>

<footer>
```

**ALL commit messages MUST be in English**

### Type
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation update
- `refactor`: Refactoring (no behavior change)
- `test`: Test related
- `chore`: Build/toolchain updates
- `perf`: Performance optimization

### Scope
- `elink`: Elink protocol related
- `rmk`: RMK core
- `split`: Split keyboard
- `ble`: BLE functionality
- `usb`: USB functionality
- `examples`: Example code

### Example
```bash
feat(elink): add CRC16 verification for standard frames

Implement CRC16 calculation and verification in protocol_frame.rs.
This improves data integrity for BLE split keyboard communication.

Closes #123
```

### Submodule Commits
```bash
# elink-protocol submodule changes should be committed separately
cd elink-protocol
git add .
git commit -m "feat(core): optimize buffer management"
git push

cd ..
git add elink-protocol
git commit -m "chore(elink): update submodule reference"
```

### ❌ Prohibited in Commit Messages

**NEVER include Co-Authored-By lines in commit messages**

```bash
# ❌ FORBIDDEN - Do NOT include these lines
Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
Co-Authored-By: Claude <...>
```

This project does not use co-author attribution for AI assistance. Commit messages should only credit human contributors if applicable.

## Common Mistakes and Prohibitions

### ❌ Absolutely Forbidden

1. **Don't use std in no_std code**
   ```rust
   // ❌ Forbidden
   use std::vec::Vec;
   use std::string::String;
   ```

2. **Don't panic in library code**
   ```rust
   // ❌ Forbidden
   pub fn parse_frame(data: &[u8]) -> Frame {
       assert!(data.len() >= 8, "Frame too short"); // panic risk
   }

   // ✅ Correct
   pub fn parse_frame(data: &[u8]) -> Result<Frame, ParseError> {
       if data.len() < 8 {
           return Err(ParseError::TooShort);
       }
       // ...
   }
   ```

3. **Don't break backward compatibility**
   - RMK is a public library, API changes need caution
   - Use deprecation markers instead of direct removal

4. **Don't commit unformatted code**
   - Must run `cargo fmt` before commit
   - CI will check formatting

### ⚠️ Caution Required

1. **Async function overhead**
   - Embassy async overhead is small but non-zero
   - Simple getters don't need async

2. **Feature combination testing**
   ```bash
   # Test different feature combinations
   cargo test --features split,elink
   cargo test --features split
   cargo test --no-default-features
   ```

3. **Documentation completeness**
   - Public APIs must have doc comments
   - Add examples for complex algorithms

4. **Submodule updates**
   - Remember to update main repo reference after modifying elink-protocol
   - Be careful with submodule branch management

## Testing Standards

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc_calculation() {
        let data = [0x01, 0x02, 0x03];
        let crc = calculate_crc16(&data);
        assert_eq!(crc, 0x1234); // Expected value
    }
}
```

### Integration Tests
```rust
// tests/elink_integration.rs
#[test]
fn test_encode_decode_roundtrip() {
    let message = SplitMessage::Key(/* ... */);
    let encoded = adapter.encode_message(&message).unwrap();
    let decoded = adapter.decode_message(encoded).unwrap();
    assert_eq!(message, decoded);
}
```

### Hardware Testing
- Test on actual hardware for at least 24 hours
- Monitor memory usage (stack, heap, static)
- Use logic analyzer to verify bus timing

## Performance Benchmarks

### Elink Protocol Performance Targets
- **Encoding latency**: < 15µs
- **Decoding latency**: < 15µs
- **CPU usage**: < 1% @ 200 msg/s
- **Memory overhead**: < 1KB total

### Testing Method
```bash
cd elink-protocol/elink-rmk-adapter
cargo run --example benchmark --release
```

## Documentation Structure

### Project Documentation Organization
```
docs/
├── architecture.md      # Architecture design
├── elink-protocol.md    # Elink protocol specification
├── porting-guide.md     # MCU porting guide
└── troubleshooting.md   # Troubleshooting
```

### Inline Documentation (English only)
```rust
/// Parse an Elink protocol frame
///
/// # Arguments
/// - `data`: Raw byte stream, must be at least 8 bytes
///
/// # Returns
/// - `Ok(Frame)`: Successfully parsed frame
/// - `Err(ParseError)`: Reason for parsing failure
///
/// # Example
/// ```
/// let data = [0xAA, 0x55, ...];
/// let frame = parse_frame(&data)?;
/// ```
pub fn parse_frame(data: &[u8]) -> Result<Frame, ParseError> {
    // ...
}
```

## Code Review Checklist

### Pre-commit Self-check
- [ ] Code formatted (`cargo fmt`)
- [ ] No Clippy warnings (`cargo clippy`)
- [ ] Unit tests pass (`cargo test`)
- [ ] Documentation updated (if API changed)
- [ ] Commit message follows standards
- [ ] Feature combination tests pass
- [ ] No `println!` or debug code
- [ ] Memory usage reasonable (check buffer sizes)
- [ ] All text in English (docs, comments, commits)

### When Reviewing Others' Code
- Error handling completeness
- Potential panic points
- Memory allocation reasonableness
- Async code correctness
- no_std compatibility

## Toolchain Configuration

### Required Tools
```bash
# Rust toolchain
rustup default stable
rustup component add clippy rustfmt
rustup target add thumbv7em-none-eabihf  # STM32
rustup target add thumbv7em-none-eabi    # nRF52

# Debug tools
cargo install probe-rs --features cli
cargo install cargo-binutils
```

### VSCode Configuration
```json
{
  "rust-analyzer.check.command": "clippy",
  "rust-analyzer.check.allTargets": true,
  "editor.formatOnSave": true,
  "[rust]": {
    "editor.defaultFormatter": "rust-lang.rust-analyzer"
  }
}
```

## Contributing to Upstream RMK

### Before Preparing PR
1. Complete development on feature branch
2. Ensure all tests pass
3. Update CHANGELOG.md
4. Write clear PR description (in English)
5. Communicate design with maintainers

### PR Description Template
```markdown
## Overview
Brief description of this PR's purpose

## Motivation
Why this feature/fix is needed

## Implementation Details
- Key design decisions
- Performance impact analysis
- Memory overhead assessment

## Testing
- [ ] Unit tests
- [ ] Integration tests
- [ ] Hardware testing (STM32H7)

## Breaking Changes
List any incompatible changes

## Related Issues
Closes #123
```

## Continuous Improvement

### Maintaining This Document
- **When to update**: When Claude makes mistakes, new best practices emerge, or team reaches new consensus
- **How to update**: Edit this file directly and commit
- **Review cycle**: Monthly review, remove outdated content

### Feedback Mechanism
- Update this document immediately when Claude doesn't follow standards
- Team members share learned best practices
- Regularly summarize common issues to FAQ section

---

## Protocol Documentation Maintenance

### Critical Rule: Keep Protocol Docs in Sync

**When modifying Elink protocol implementation, you MUST update protocol specification documents.**

This is a **blocking requirement** - protocol changes without documentation updates are incomplete.

### Protocol Documentation Files

1. **English Specification**: `docs/elink/protocol-specification-en.md`
2. **Chinese Specification**: `docs/elink/protocol-specification-zh.md`

Both files must be kept in sync with the implementation in:
- `elink-protocol/elink-core/src/protocol_frame.rs`
- `elink-protocol/elink-core/src/protocol_crc.rs`
- `elink-protocol/elink-core/src/priority.rs`

### What Requires Documentation Update

Update protocol specifications when changing:

| Change Type | Requires Spec Update | Files to Update |
|-------------|---------------------|-----------------|
| Frame structure (add/remove/modify fields) | ✅ Yes | Both EN + ZH specs, section "Frame Structure" |
| CRC algorithm (polynomial, coverage) | ✅ Yes | Both EN + ZH specs, section "CRC Algorithms" |
| Priority levels (add/remove/modify) | ✅ Yes | Both EN + ZH specs, section "Priority System" |
| Device addressing scheme | ✅ Yes | Both EN + ZH specs, section "Device Addressing" |
| Frame types (add/remove categories) | ✅ Yes | Both EN + ZH specs, section "Frame Types" |
| Protocol constants (size limits, masks) | ✅ Yes | Both EN + ZH specs, section "Protocol Constants" |
| Validation rules | ✅ Yes | Both EN + ZH specs, section "Validation Rules" |
| Bug fixes (no protocol change) | ❌ No | Just update code |
| Performance optimizations (no protocol change) | ❌ No | Update performance docs instead |

### Update Workflow

```bash
# 1. Make protocol changes to code
# Edit elink-protocol/elink-core/src/*.rs

# 2. Update English specification
# Edit docs/elink/protocol-specification-en.md
# Update affected sections with new values/structures

# 3. Update Chinese specification
# Edit docs/elink/protocol-specification-zh.md
# Keep content in sync with English version

# 4. Update version history in both specs
# Add entry with date and description of change

# 5. Commit with proper scope
git commit -m "feat(elink): <change description>

Updated protocol implementation and specifications:
- Changed: <what was changed>
- Impact: <compatibility impact>
- Docs: Updated EN + ZH protocol specs

Closes #XXX"
```

### Pre-commit Checklist for Protocol Changes

Before committing Elink protocol changes:

- [ ] Code changes implemented and tested
- [ ] English spec (`protocol-specification-en.md`) updated
- [ ] Chinese spec (`protocol-specification-zh.md`) updated
- [ ] Version history updated in both specs
- [ ] Examples updated if API changed
- [ ] Breaking changes clearly documented
- [ ] Commit message references spec updates

### Example: Adding a New Frame Type

**Code change:**
```rust
// elink-core/src/protocol_frame.rs
pub enum FrameCategory {
    Command = 0,
    Status = 1,
    Cooperation = 2,
    Extended = 3,  // ← NEW
}
```

**Documentation update:**
```markdown
<!-- In both protocol-specification-en.md and -zh.md -->

### Standard Frame Types (4 bits, 0-15)

| Value | Name | Description |
|-------|------|-------------|
| 0 | Command | Command frames |
| 1 | Status | Status reports |
| 2 | Cooperation | Multi-device coordination |
| 3 | Extended | Extended protocol features | ← ADDED

<!-- Update version history -->
### v1.1 (2026-02-XX)
- Added Extended frame category (type 3)
- Maintains backward compatibility with v1.0
```

### Document Synchronization Verification

To verify docs are in sync with code:

```bash
# Run this before committing protocol changes
./.claude/validate.sh 2  # Ensure tests pass

# Manual checks:
# 1. Grep for constants in code
grep -r "FRAME_SIZE\|POLYNOMIAL\|PRIORITY" elink-protocol/elink-core/src/

# 2. Verify same constants in specs
grep -r "FRAME_SIZE\|POLYNOMIAL\|PRIORITY" docs/elink/protocol-specification-*.md

# 3. Check version history updated
tail -20 docs/elink/protocol-specification-en.md
tail -20 docs/elink/protocol-specification-zh.md
```

### Why This Matters

Protocol documentation serves multiple purposes:
1. **User Reference**: Keyboard designers implementing Elink
2. **Contributor Guide**: Developers extending the protocol
3. **Compatibility Record**: Track breaking changes across versions
4. **Debugging Aid**: Understand expected vs actual behavior
5. **Porting Guide**: Implement Elink in other languages/firmware

Outdated protocol docs are worse than no docs - they create confusion and bugs.

---

## Protocol Evaluation and Comparison Standards

### Objective and Unbiased Evaluation

**CRITICAL REQUIREMENT**: When comparing or evaluating communication protocols (Elink, RMK Serial, QMK Serial, etc.), you MUST maintain a third-party, objective perspective.

#### ❌ Forbidden Approaches

**Never show bias toward any protocol:**
- Don't use subjective ratings (⭐⭐⭐⭐⭐ ratings)
- Avoid value-laden terms like "superior", "better", "worse", "perfect", "excellent"
- Don't make absolute judgments about which protocol is "best"
- Avoid phrases like "X is clearly better than Y"

**Bad examples:**
```markdown
❌ "Elink performs excellently in all scenarios"
❌ "RMK Serial has weak error detection"
❌ "QMK Serial is inferior due to lack of checksums"
❌ Rating: Elink ⭐⭐⭐⭐⭐, RMK Serial ⭐⭐⭐, QMK Serial ⭐⭐
```

#### ✅ Required Approaches

**1. Describe Technical Characteristics Objectively**

State facts without judgment:
```markdown
✅ "Elink uses CRC16 for error detection (2-byte overhead)"
✅ "RMK Serial relies on COBS encoding's inherent consistency checks"
✅ "QMK Serial uses SYNC+LENGTH framing without explicit checksums"
```

**2. Present Design Trade-offs**

Every protocol makes deliberate trade-offs. Present them neutrally:

```markdown
| Protocol | Design Choice | Benefit | Cost |
|----------|--------------|---------|------|
| Elink | Explicit CRC16 | Detects bit-level corruption | +2 bytes overhead |
| RMK Serial | COBS encoding | Low overhead (~0.4%) | Implicit error detection only |
| QMK Serial | Minimal framing | Smallest overhead (2 bytes) | No error detection mechanism |
```

**3. Specify Applicable Scenarios**

Make clear each protocol is designed for specific use cases:

```markdown
**Design Context:**
- Elink: Designed for wireless (BLE) split keyboards with multiple peripherals
  - Wireless introduces higher error rates → CRC necessary
  - Multi-device coordination → device addressing + priority system

- RMK Serial: Designed for wired point-to-point split keyboards
  - Wired connection more reliable → COBS sufficient
  - Single peripheral → no device addressing needed

- QMK Serial: Designed for simple, reliable wired connections
  - Very short cable runs → minimal error risk
  - Simplicity prioritized → minimal protocol overhead
```

**4. Present Test Results as Data Points**

Test results reflect performance under specific conditions, not absolute quality:

```markdown
**Test Results (5% packet loss, simulated wireless)**

| Protocol | Messages Received | Success Rate | Notes |
|----------|------------------|--------------|-------|
| Elink | 480/480 | 100% | CRC detected all corruption |
| RMK Serial | 475/475 | 100% | COBS+Postcard caught errors |
| QMK Serial | 468/470 | 99.6% | 2 frames failed validation |

**Interpretation:**
- All protocols performed well in this scenario
- QMK Serial's 99.6% may be acceptable for its target use case (reliable wired)
- Test does not reflect real-world wired conditions (lower error rates)
```

### Embedded Environment Considerations

**CRITICAL**: Keyboard firmware runs on resource-constrained embedded systems. Always evaluate protocols considering:

#### 1. Code Size / Flash Usage

```markdown
| Protocol | Library Size | Impact on Firmware |
|----------|-------------|-------------------|
| Elink | ~2-3 KB | Significant for 32KB devices |
| RMK Serial | ~1 KB | Moderate overhead |
| QMK Serial | ~500 bytes | Minimal footprint |

**Consideration**: For keyboards with 32KB flash (nRF52832),
every KB matters for features like RGB, macros, and Via support.
```

#### 2. RAM Usage

```markdown
| Protocol | Buffer Requirements | Peak Usage |
|----------|-------------------|------------|
| Elink | 512B RX + 64B TX | ~600 bytes |
| RMK Serial | 512B buffer | ~512 bytes |
| QMK Serial | 256B buffer | ~256 bytes |

**Consideration**: Devices with 64KB RAM need careful memory budgeting.
Larger buffers reduce available RAM for keymap, macros, and runtime state.
```

#### 3. CPU Overhead

```markdown
| Protocol | Per-Message Cost | Impact |
|----------|-----------------|--------|
| Elink | ~15µs encode + CRC | Negligible at 100 msg/s |
| RMK Serial | ~10µs COBS | Minimal CPU usage |
| QMK Serial | ~5µs minimal | Almost zero overhead |

**Consideration**: MCUs run at 64MHz-240MHz. Even "expensive"
protocols use <0.5% CPU at typical message rates (100-200 msg/s).
Performance is rarely the bottleneck.
```

#### 4. Power Consumption

```markdown
**Wireless Keyboards (BLE)**:
- Transmission time = dominant power factor
- Smaller frames → less radio-on time → longer battery life
- Elink: 10 bytes overhead, RMK: 2 bytes, QMK: 2 bytes
- But: retransmissions from errors cost more than overhead

**Consideration**: For BLE keyboards, reliability affects battery
life more than frame size. One retransmission costs more energy
than extra CRC bytes in original frame.
```

### Writing Protocol Comparisons

When creating comparison documents:

**Structure:**
1. **Technical Overview**: Describe each protocol's mechanisms
2. **Design Rationale**: Explain why each made its choices
3. **Trade-off Analysis**: Show benefits and costs of each approach
4. **Use Case Fit**: Describe scenarios where each excels
5. **Test Data**: Present results with clear context
6. **Resource Requirements**: Flash, RAM, CPU, power considerations

**Tone:**
- Analytical, not judgmental
- Educational, not prescriptive
- Comparative, not competitive

**Remember:**
- Protocol design reflects different priorities, not better/worse engineering
- Each protocol serves its intended use case well
- Resource constraints (flash/RAM/power) often matter more than performance
- The "right" protocol depends entirely on the specific application requirements

---

## Working with Uncertainty

### When to Ask for Clarification

**Always ask the user when:**

1. **Requirements are ambiguous**
   - Multiple valid interpretations exist
   - Missing critical information about use case
   - Unclear priorities or constraints

2. **Design decisions need input**
   - Trade-offs between different approaches
   - User preferences matter (API design, naming)
   - Breaking changes or compatibility concerns

3. **Before making assumptions**
   - About intended use cases
   - About performance requirements
   - About hardware constraints or target platforms

4. **When uncertain about next steps**
   - Multiple paths forward are possible
   - Unclear which feature to prioritize
   - Need more context about user's goals

**Good questions:**
- "Do you prefer X or Y approach? X has better performance but Y is simpler."
- "What hardware are you targeting for testing?"
- "Should this be a breaking change or maintain backward compatibility?"
- "Which scenario is more important to you: A or B?"

**Bad assumptions:**
- Guessing user requirements without asking
- Implementing features not explicitly requested
- Making breaking changes without discussion
- Assuming priorities without confirmation

---

## Version History
- 2026-02-10: Added protocol evaluation standards
  - **Critical**: Require objective, third-party perspective when comparing protocols
  - Added embedded environment considerations (flash size, RAM, CPU, power)
  - Defined forbidden biased language and required neutral approaches
  - Emphasize resource constraints over pure performance metrics
- 2026-02-09: Initial version, created based on Boris Cherny's 13 tips
  - Added language policy: All documentation/commits in English
  - Added protocol documentation maintenance rules (critical)
