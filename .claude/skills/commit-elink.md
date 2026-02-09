# Skill: commit-elink

High-frequency operation skill (Boris Tip #7): Standardized commit workflow for Elink changes.

## Usage
```
/commit-elink <message>
```

## What This Skill Does

1. **Verify code quality** (fmt, clippy, tests)
2. **Stage appropriate files** based on change type
3. **Create well-formatted commit** following CLAUDE.md standards
4. **Handle submodule commits** separately if needed

## Implementation

### Step 1: Pre-commit checks
```bash
cargo fmt --all
cargo clippy --all-targets --all-features
cargo test --features split,elink
```

### Step 2: Determine change scope
- If changes in `elink-protocol/`: submodule commit needed
- If changes in `rmk/src/split/elink/`: main repo commit
- If both: two separate commits required

### Step 3: Stage and commit (submodule first if needed)
```bash
# If submodule changed
cd elink-protocol
git add .
git commit -m "feat(core): <specific change>"
cd ..

# Main repo
git add rmk/src/split/elink/
git commit -m "feat(elink): <user-provided message>

<detailed description>

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

### Step 4: Verify commit
```bash
git log -1 --stat
git show HEAD
```

## Commit Message Format

Follow conventional commits with Elink scope:

```
feat(elink): add priority-based message scheduling

Implement 4-level priority system for real-time message handling.
High-priority key events are now processed before low-priority
config sync messages.

Performance impact: < 1µs additional latency
Memory overhead: +16 bytes per adapter instance

Closes #456

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
```

## Pre-commit Checklist

- [ ] Code formatted with `cargo fmt`
- [ ] No clippy warnings
- [ ] All tests pass
- [ ] Commit message in English
- [ ] Commit message follows conventional format
- [ ] No debug println! statements
- [ ] Documentation updated if API changed

## Troubleshooting

- If fmt fails: Fix formatting issues manually
- If tests fail: Don't commit, fix issues first
- If submodule out of sync: Use `git submodule update --remote`
