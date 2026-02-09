# Elink Protocol Development Roadmap

Status and future plans for the Elink protocol integration in RMK.

**[中文版 (Chinese Version)](roadmap-zh.md)**

## Project Status

### ✅ Phase 1: Core Protocol (Complete)

**Completed:**
- [x] Define frame structure (StandardFrame, CompatibleFrame)
- [x] Implement CRC4/CRC16 calculation and verification
- [x] Design priority system (4 levels)
- [x] Extended device addressing (16-bit)
- [x] Frame parsing and serialization
- [x] Error types and handling

**Files:**
- `elink-protocol/elink-core/src/protocol_frame.rs`
- `elink-protocol/elink-core/src/protocol_crc.rs`
- `elink-protocol/elink-core/src/priority.rs`

### ✅ Phase 2: RMK Integration (Complete)

**Completed:**
- [x] Create `elink-rmk-adapter` crate
- [x] Implement `ElinkAdapter` (encoding/decoding)
- [x] Map `SplitMessage` to Elink frames
- [x] Implement `SplitReader`/`SplitWriter` traits
- [x] Feature-gate integration (`elink` feature)
- [x] No_std compatibility verification

**Files:**
- `elink-protocol/elink-rmk-adapter/src/adapter.rs`
- `elink-protocol/elink-rmk-adapter/src/message_mapper.rs`
- `rmk/src/split/elink/mod.rs`

### ✅ Phase 3: Optimization (Complete)

**Completed:**
- [x] Reduce buffer sizes (1024→512 bytes receive buffer)
- [x] Optimize loop logic (check adapter buffer first)
- [x] Reduce temporary buffer (256→128 bytes)
- [x] Memory footprint reduction (47.6% savings)
- [x] CPU usage optimization (50% fewer unnecessary calls)

**Documentation:**
- `ELINK_EMBEDDED_OPTIMIZATION.md`
- `ELINK_OPTIMIZATION_SUMMARY.md`

### ✅ Phase 4: Testing & Documentation (Complete)

**Completed:**
- [x] PC test harness (`pc_test.rs`)
- [x] Benchmark suite (`benchmark.rs`)
- [x] Performance analysis documents
- [x] Integration guide
- [x] Usage documentation
- [x] Feasibility assessment

**Artifacts:**
- Benchmark results: 22.59µs total latency
- Memory usage: 760 bytes
- CPU overhead: < 0.5%

## Current Status: Ready for Hardware Testing

The Elink protocol is **feature-complete** for normal keyboard use cases. The next phase focuses on validation and refinement based on real-world testing.

---

## 🚧 Phase 5: Hardware Validation (In Progress)

**Objectives:**
- Validate on actual STM32H7 hardware
- Test on nRF52840 BLE split keyboard
- Long-term stability testing (24+ hours)
- Real-world usage feedback

**Tasks:**

### Hardware Testing
- [ ] Test on STM32H7-based keyboard
- [ ] Test on nRF52840 BLE split
- [ ] Test on RP2040-based split
- [ ] Verify with logic analyzer

### Performance Validation
- [ ] Measure actual latency on hardware
- [ ] Profile CPU usage on device
- [ ] Monitor memory usage at runtime
- [ ] Test under various load scenarios

### Stability Testing
- [ ] 24-hour continuous typing test
- [ ] Stress test (rapid key presses)
- [ ] BLE reconnection scenarios
- [ ] Power cycle recovery

**Timeline:** 2-4 weeks

---

## 🔮 Phase 6: Advanced Features (Planned)

### Priority: High

#### Automatic Retry Mechanism
- **Goal**: Automatically retry on CRC failure
- **Implementation**: Add retry counter to adapter
- **Benefit**: Improved reliability in noisy BLE environments

```rust
pub struct ElinkAdapter {
    max_retries: u8,
    // ...
}
```

**Estimated effort:** 1 week

#### Dynamic Priority Adjustment
- **Goal**: Adjust message priority based on queue depth
- **Implementation**: Monitor pending message count, boost priority if queue grows
- **Benefit**: Better responsiveness under load

**Estimated effort:** 2 weeks

### Priority: Medium

#### Multi-Peripheral Optimization
- **Goal**: Optimize for keyboards with 3+ peripherals
- **Implementation**: Shared buffer pool, priority-based scheduling
- **Benefit**: Reduced memory overhead for complex setups

**Estimated effort:** 2-3 weeks

#### Compression Support
- **Goal**: Optional frame payload compression
- **Implementation**: Integrate lightweight compression (e.g., heatshrink)
- **Benefit**: Reduced bandwidth for complex messages

**Estimated effort:** 3-4 weeks

### Priority: Low

#### Encrypted Frames
- **Goal**: Optional encryption for sensitive data
- **Implementation**: Add encryption layer between serialization and framing
- **Benefit**: Enhanced security (though BLE already provides encryption)

**Estimated effort:** 2-3 weeks

#### Telemetry and Diagnostics
- **Goal**: Built-in frame statistics and error reporting
- **Implementation**: Counters for CRC errors, retries, buffer overflows
- **Benefit**: Easier debugging and monitoring

**Estimated effort:** 1 week

---

## 🎯 Phase 7: Upstream Integration (Future)

**Goal**: Contribute Elink to upstream RMK repository

### Prerequisites
- [ ] Hardware testing complete with positive results
- [ ] At least 3 months of stable usage in production keyboards
- [ ] Community feedback incorporated
- [ ] Complete documentation and examples
- [ ] Approval from RMK maintainers

### Tasks
- [ ] Prepare comprehensive PR
- [ ] Write migration guide for existing users
- [ ] Create comparison documentation vs existing protocols
- [ ] Add CI/CD tests for Elink
- [ ] Provide example keyboards using Elink

**Timeline:** 6+ months from now

---

## Long-term Vision

### Goals for 1.0 Release

- [ ] Proven stable on 5+ different MCU platforms
- [ ] Used in 10+ production keyboards
- [ ] Formal protocol specification document
- [ ] Reference implementations for QMK/ZMK (community-driven)
- [ ] Tooling for protocol analysis (e.g., Wireshark dissector)

### Goals for 2.0

- [ ] Multi-hop support (daisy-chain peripherals)
- [ ] Automatic topology discovery
- [ ] Hot-swap peripheral support
- [ ] Standardized device classes (keyboard, mouse, touchpad, screen)

---

## Contributing to Roadmap

We welcome community input on roadmap priorities. To suggest features:

1. Open a GitHub discussion in RMK repository
2. Describe use case and benefits
3. Provide technical details if possible
4. Indicate willingness to contribute implementation

Priority is given to:
- Features with clear use cases
- Features with community support
- Features that maintain backward compatibility
- Features that don't significantly increase complexity

---

## Version History

- 2026-02-09: Initial roadmap created
  - Documented completed phases 1-4
  - Defined phase 5 (hardware validation)
  - Planned phases 6-7 (advanced features, upstream integration)

*Last updated: 2026-02-09*
