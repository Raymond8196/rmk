# Elink Documentation Reorganization Plan

## Problem
Elink is an independent repository (git submodule), but all documentation is currently in the RMK repository under `docs/elink/`. This creates confusion about ownership and maintenance responsibility.

## Solution: Separate Documentation by Ownership

### Phase 1: Move Protocol-Core Docs to elink-protocol Repository

**Documents to move FROM rmk/docs/elink/ TO elink-protocol/docs/:**

1. **protocol-specification-en.md** → `elink-protocol/docs/protocol-specification-en.md`
   - Core protocol definition
   - Frame structures, CRC algorithms, priority system
   - Independent of any firmware

2. **protocol-specification-zh.md** → `elink-protocol/docs/protocol-specification-zh.md`
   - Chinese version of protocol specification

3. **faq.md** → `elink-protocol/docs/faq.md`
   - General protocol FAQ
   - Split RMK-specific questions to separate doc

4. **faq-zh.md** → `elink-protocol/docs/faq-zh.md`
   - Chinese version of FAQ

5. **PROTOCOL_DOCS_GUIDE.md** → `elink-protocol/docs/PROTOCOL_DOCS_GUIDE.md`
   - How to maintain protocol specifications

### Phase 2: Create RMK-Specific Integration Docs

**Keep in rmk/docs/elink/** (rename directory to `docs/elink-integration/`):

1. **integration-guide.md** (to be created)
   - How Elink integrates with RMK split module
   - Trait implementations
   - Feature flags

2. **usage-guide.md** (migrate from old ELINK_USAGE.md)
   - How to use Elink in RMK keyboards
   - Configuration examples
   - When to use Elink vs serial

3. **rmk-faq.md** (extract from current faq.md)
   - RMK-specific questions
   - "Can I use Elink with my RMK keyboard?"
   - RMK build and configuration issues

4. **roadmap.md** (keep)
   - Development roadmap for Elink in RMK
   - RMK-specific milestones

5. **performance.md** (consolidate analysis docs)
   - Performance in RMK context
   - Benchmarks on actual hardware

### Phase 3: Create Cross-References

**In elink-protocol/README.md:**
```markdown
## Usage in Firmware

- **RMK**: See [RMK integration guide](https://github.com/user/rmk/docs/elink-integration/)
- **QMK/ZMK**: Community ports (if available)
```

**In rmk/docs/elink-integration/README.md:**
```markdown
## Protocol Documentation

For protocol specification and architecture, see:
- [Elink Protocol Repository](https://github.com/Raymond8196/elink-protocol)
- [Protocol Specification](https://github.com/Raymond8196/elink-protocol/docs/protocol-specification-en.md)
```

## Directory Structure After Reorganization

```
elink-protocol/                          (Independent repo)
├── README.md                            (Project overview)
├── README-zh.md                         (Chinese overview)
├── CLAUDE.md                            (Elink dev standards)
├── docs/
│   ├── protocol-specification-en.md    ← Moved from RMK
│   ├── protocol-specification-zh.md    ← Moved from RMK
│   ├── faq.md                          ← Moved (protocol-only FAQ)
│   ├── faq-zh.md                       ← Moved
│   ├── PROTOCOL_DOCS_GUIDE.md          ← Moved from RMK
│   ├── architecture.md                 (Protocol architecture)
│   └── contributing.md                 (How to contribute to protocol)
├── elink-core/
├── elink-rmk-adapter/
└── elink-embed/

rmk/                                     (RMK repo)
├── CLAUDE.md                            (RMK dev standards)
├── docs/
│   └── elink-integration/              ← Renamed from docs/elink/
│       ├── README.md                    (Integration overview)
│       ├── integration-guide.md         (How to integrate)
│       ├── usage-guide.md               (How to use in RMK)
│       ├── rmk-faq.md                   (RMK-specific FAQ)
│       ├── roadmap.md                   (RMK development roadmap)
│       └── performance.md               (Performance in RMK)
├── rmk/
│   └── src/
│       └── split/
│           └── elink/                   (Integration code)
└── elink-protocol/                      (Git submodule)
```

## Benefits

1. **Clear Ownership**: Protocol docs in protocol repo, integration docs in RMK repo
2. **Independent Evolution**: Protocol can evolve independently
3. **Reusability**: Other firmware can reference elink-protocol docs
4. **Maintenance**: Each repo maintains its own documentation
5. **Reduced Duplication**: Single source of truth for protocol specification

## Implementation Steps

1. [ ] Create `docs/` directory in elink-protocol
2. [ ] Move protocol specification files
3. [ ] Move protocol FAQ (keep RMK-specific questions in RMK)
4. [ ] Create CLAUDE.md in elink-protocol for protocol development standards
5. [ ] Rename `rmk/docs/elink/` to `rmk/docs/elink-integration/`
6. [ ] Update all cross-references and links
7. [ ] Update README files in both repos
8. [ ] Commit changes to elink-protocol first
9. [ ] Update submodule reference in RMK
10. [ ] Commit changes to RMK

## Timeline

- Phase 1: 30 minutes (move files)
- Phase 2: 1 hour (create/update RMK-specific docs)
- Phase 3: 30 minutes (update cross-references)

Total: ~2 hours
