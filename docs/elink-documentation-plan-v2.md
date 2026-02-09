# Elink Documentation Reorganization Plan v2
## For Independent Protocol Development

**Vision**: Elink is an independent, firmware-agnostic communication protocol. RMK is the first integration platform for testing and validation.

---

## Goals

1. **Elink-protocol as standalone project** - Complete, independent, ready for any firmware
2. **RMK as reference integration** - Show how to integrate Elink into a keyboard firmware
3. **Clear separation** - Protocol development vs firmware integration
4. **Contribution path** - Easy for others to adopt Elink in their firmware

---

## Phase 1: Establish elink-protocol as Complete Independent Project

### A. Repository Structure

```
elink-protocol/                          (github.com/Raymond8196/elink-protocol)
├── README.md                            ⭐ Main project overview
├── README-zh.md                         ⭐ Chinese overview
├── CLAUDE.md                            ⭐ Development standards for Elink
├── LICENSE                              (Choose: MIT/Apache-2.0)
├── CONTRIBUTING.md                      (How to contribute)
│
├── docs/                                📚 Complete protocol documentation
│   ├── README.md                        (Documentation index)
│   ├── README-zh.md                     (Chinese doc index)
│   │
│   ├── protocol-specification-en.md    ⭐ Core protocol spec
│   ├── protocol-specification-zh.md    ⭐ Chinese spec
│   │
│   ├── architecture.md                  (Protocol design rationale)
│   ├── architecture-zh.md               (Chinese version)
│   │
│   ├── faq.md                           (General protocol FAQ)
│   ├── faq-zh.md                        (Chinese FAQ)
│   │
│   ├── PROTOCOL_DOCS_GUIDE.md           (How to maintain specs)
│   │
│   └── integrations/                    📖 Integration guides
│       ├── README.md                    (Integration overview)
│       ├── generic-guide.md             (Generic integration guide)
│       ├── rmk.md                       (RMK-specific integration)
│       ├── qmk.md                       (Future: QMK integration)
│       └── zmk.md                       (Future: ZMK integration)
│
├── elink-core/                          💎 Core protocol implementation
│   ├── src/
│   ├── Cargo.toml
│   └── README.md
│
├── elink-rmk-adapter/                   🔌 RMK adapter (example)
│   ├── src/
│   ├── examples/
│   ├── Cargo.toml
│   └── README.md
│
├── elink-embed/                         ⚙️ Embedded utilities
└── elink-bin/                           🛠️ Tools and utilities
```

### B. Documents to Move FROM rmk/docs/elink/

| Document | Destination | Purpose |
|----------|-------------|---------|
| protocol-specification-en.md | elink-protocol/docs/ | Core spec |
| protocol-specification-zh.md | elink-protocol/docs/ | Chinese spec |
| faq.md | elink-protocol/docs/ | Protocol FAQ |
| faq-zh.md | elink-protocol/docs/ | Chinese FAQ |
| PROTOCOL_DOCS_GUIDE.md | elink-protocol/docs/ | Maintenance guide |
| README.md | elink-protocol/docs/ | Doc index |
| README-zh.md | elink-protocol/docs/ | Chinese index |

### C. New Documents to Create in elink-protocol

1. **README.md** (Root)
```markdown
# Elink Protocol

A high-reliability communication protocol for keyboard peripherals.

## Features
- CRC verification (CRC-4/CRC-16)
- 4-level priority system
- Extended device addressing (16-bit)
- No_std compatible

## Integrations
- ✅ RMK (production-ready)
- 🔄 QMK (community port)
- 🔄 ZMK (community port)

## Documentation
- [Protocol Specification](docs/protocol-specification-en.md)
- [中文文档](docs/protocol-specification-zh.md)
- [Integration Guide](docs/integrations/)

## Quick Start
See [docs/integrations/generic-guide.md](docs/integrations/generic-guide.md)
```

2. **CLAUDE.md** (Elink development standards)
```markdown
# Elink Protocol Development Guide

Standards for developing the Elink protocol itself (not firmware integration).

## Language Policy
- All code, docs, commits in English
- Chinese versions for core specs

## Protocol Standards
- No breaking changes without major version bump
- CRC algorithms must match spec exactly
- All frame structures validated with tests

## Contributing
See CONTRIBUTING.md
```

3. **CONTRIBUTING.md**
```markdown
# Contributing to Elink Protocol

## How to Contribute
1. Protocol improvements
2. Performance optimizations
3. Additional language bindings (C, Python, etc.)
4. Integration examples for other firmware

## Process
1. Open issue for discussion
2. Submit PR with tests
3. Update protocol spec if needed
```

4. **docs/architecture.md**
```markdown
# Elink Protocol Architecture

Design principles and rationale behind Elink protocol.

## Design Goals
1. Reliability over speed
2. Embedded-first (no_std, fixed buffers)
3. Firmware-agnostic
4. Multi-device support
```

---

## Phase 2: RMK as Reference Integration

### A. Repository Structure

```
rmk/                                     (Your RMK fork)
├── CLAUDE.md                            (RMK development standards)
├── docs/
│   └── integrations/                    📂 Third-party integrations
│       └── elink/
│           ├── README.md                (Elink in RMK overview)
│           ├── integration-guide.md     (How Elink is integrated)
│           ├── usage-guide.md           (How to use in your keyboard)
│           ├── rmk-faq.md               (RMK-specific FAQ)
│           ├── roadmap.md               (Elink-in-RMK roadmap)
│           └── performance.md           (Benchmarks on RMK)
│
├── rmk/src/split/elink/                 (Integration code)
└── elink-protocol/                      (Git submodule)
```

### B. Documents to Keep in RMK (Reorganize)

| Document | New Location | Purpose |
|----------|--------------|---------|
| roadmap.md | docs/integrations/elink/roadmap.md | RMK-specific roadmap |
| (new) integration-guide.md | docs/integrations/elink/ | How Elink is integrated |
| (new) usage-guide.md | docs/integrations/elink/ | How to use |
| (new) rmk-faq.md | docs/integrations/elink/ | RMK-specific FAQ |

### C. New RMK Integration Docs to Create

1. **docs/integrations/elink/README.md**
```markdown
# Elink Integration in RMK

RMK uses Elink protocol for split keyboard communication.

## What is Elink?
See [Elink Protocol Repository](https://github.com/Raymond8196/elink-protocol)

## Using Elink in Your RMK Keyboard
See [Usage Guide](usage-guide.md)

## Integration Details
See [Integration Guide](integration-guide.md)
```

2. **docs/integrations/elink/integration-guide.md**
```markdown
# How Elink is Integrated into RMK

Technical details of the integration for RMK contributors.

## Architecture
- SplitReader/SplitWriter trait implementation
- Feature flags
- Adapter layer

## Code Locations
- rmk/src/split/elink/mod.rs
- Depends on elink-protocol submodule
```

---

## Phase 3: Cross-Repository References

### In elink-protocol/README.md:
```markdown
## Integrations

### Production-Ready
- **[RMK](https://github.com/HaoboGu/rmk)** - Rust keyboard firmware
  - Integration guide: [docs/integrations/rmk.md](docs/integrations/rmk.md)
  - Example: elink-rmk-adapter/

### Community Ports
- QMK (planned)
- ZMK (planned)

Want to integrate Elink? See [Generic Integration Guide](docs/integrations/generic-guide.md)
```

### In elink-protocol/docs/integrations/rmk.md:
```markdown
# Integrating Elink with RMK

## Quick Start
Add to your Cargo.toml:
```toml
[dependencies]
rmk = { version = "0.8", features = ["split", "elink"] }
```

## Detailed Guide
For complete RMK integration documentation, see:
- [RMK Elink Documentation](https://github.com/YOUR_USERNAME/rmk/docs/integrations/elink/)

## Reference Implementation
The RMK integration serves as the reference implementation for Elink.
See: rmk/src/split/elink/
```

### In rmk/docs/integrations/elink/README.md:
```markdown
# Elink Integration in RMK

## About Elink Protocol
Elink is an independent, firmware-agnostic communication protocol.

**Protocol Repository**: https://github.com/Raymond8196/elink-protocol

**Protocol Documentation**:
- [Protocol Specification](https://github.com/Raymond8196/elink-protocol/docs/protocol-specification-en.md)
- [FAQ](https://github.com/Raymond8196/elink-protocol/docs/faq.md)

## RMK-Specific Documentation
- [Usage Guide](usage-guide.md) - How to use Elink in your RMK keyboard
- [Integration Guide](integration-guide.md) - How Elink is integrated (for contributors)
- [FAQ](rmk-faq.md) - RMK-specific questions
```

---

## Implementation Checklist

### Part 1: elink-protocol Repository (Independent Project)
- [ ] Create docs/ directory
- [ ] Move protocol specifications (EN + ZH)
- [ ] Move FAQ (EN + ZH)
- [ ] Move PROTOCOL_DOCS_GUIDE
- [ ] Move README (EN + ZH)
- [ ] Create root README.md (project overview)
- [ ] Create CLAUDE.md (Elink dev standards)
- [ ] Create CONTRIBUTING.md
- [ ] Create docs/architecture.md (design rationale)
- [ ] Create docs/integrations/generic-guide.md
- [ ] Create docs/integrations/rmk.md (RMK-specific integration)
- [ ] Update all internal links
- [ ] Commit to elink-protocol
- [ ] Push to GitHub

### Part 2: RMK Repository (Integration Example)
- [ ] Rename docs/elink/ to docs/integrations/elink/
- [ ] Move roadmap.md to docs/integrations/elink/
- [ ] Create docs/integrations/elink/README.md
- [ ] Create docs/integrations/elink/integration-guide.md
- [ ] Create docs/integrations/elink/usage-guide.md
- [ ] Extract RMK-specific FAQ to docs/integrations/elink/rmk-faq.md
- [ ] Update all references to point to elink-protocol repo
- [ ] Update .gitmodules if needed
- [ ] Commit to RMK
- [ ] Update submodule reference

### Part 3: Validation
- [ ] Verify all links work
- [ ] Verify elink-protocol can be cloned and used independently
- [ ] Verify RMK integration docs are complete
- [ ] Test building RMK with elink feature
- [ ] Update CLAUDE.md in both repos

---

## Benefits of This Approach

### For Elink Project
✅ **Independent identity** - Standalone project, not "part of RMK"
✅ **Reusability** - Any firmware can adopt it
✅ **Clear ownership** - You control protocol evolution
✅ **Portfolio piece** - Showcases your protocol design skills
✅ **Community growth** - Easier for others to contribute

### For RMK Project
✅ **Clean integration** - Reference implementation
✅ **Contribution value** - Shows how to integrate external protocols
✅ **Maintainability** - Clear separation of concerns
✅ **Documentation** - Complete guide for RMK users

### For Future Adopters (QMK/ZMK/etc.)
✅ **Complete documentation** - Everything in elink-protocol repo
✅ **Reference implementation** - Learn from RMK integration
✅ **Generic guide** - Step-by-step integration process
✅ **Support** - Direct contribution to elink-protocol

---

## Timeline

**Estimated time: 3-4 hours**

- Part 1 (elink-protocol): 1.5 hours
  - Move files: 30 min
  - Create new docs: 1 hour

- Part 2 (RMK): 1 hour
  - Reorganize: 30 min
  - Create integration docs: 30 min

- Part 3 (Validation): 30 min
  - Test links: 15 min
  - Verify builds: 15 min

---

## Next Steps

**Option A: Execute Now** ⚡
- I can start the reorganization immediately
- Follow checklist step by step
- You review and approve each phase

**Option B: Prepare First** 📋
- Check elink-protocol repo status
- Ensure you can push to both repos
- Plan a good time for 3-4 hour session

**Option C: Incremental** 🔄
- Phase 1 today (elink-protocol setup)
- Phase 2 tomorrow (RMK integration docs)
- Phase 3 when ready (validation)

Which approach do you prefer?
