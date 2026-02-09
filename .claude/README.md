# Claude Code Configuration for RMK + Elink

This directory contains Claude Code configuration and tools for standardized, efficient development workflow (Vibe Coding).

**[中文版 (Chinese Version)](README-zh.md)**

## 📁 Directory Structure

```
.claude/
├── README.md                 # This file
├── permissions.json          # Pre-approved safe commands
├── validation.md            # Validation framework documentation
├── validate.sh              # Automated validation script
└── skills/                  # Custom skills (slash commands)
    ├── test-elink.md
    ├── build-examples.md
    ├── check-embedded.md
    └── commit-elink.md
```

## 🚀 Quick Start

### For Claude

When starting work on this project, Claude should:

1. **Read [CLAUDE.md](../CLAUDE.md)** first - Project standards and conventions
2. **Use pre-approved commands** from `permissions.json` freely
3. **Run validation** after code changes: `./.claude/validate.sh <level>`
4. **Use custom skills** for common workflows (see below)

### For Users

When working with Claude on this project:

1. **Reference CLAUDE.md** when Claude makes mistakes
2. **Use validation script** to check your own changes
3. **Invoke skills** with `/skill-name` for automated workflows

## 🛠️ Custom Skills

### /test-elink
Comprehensive testing of Elink protocol
```
/test-elink          # Run all tests
/test-elink --verbose # With detailed output
```

### /build-examples
Build all RMK examples to verify compatibility
```
/build-examples                    # Build all
/build-examples --target stm32h7   # Build specific target
```

### /check-embedded
Verify no_std compatibility and embedded readiness
```
/check-embedded
```

### /commit-elink
Standardized commit workflow (high-frequency operation)
```
/commit-elink "Add feature X"
```

See [skills/](skills/) directory for detailed documentation of each skill.

## ✅ Validation Framework

The validation framework ensures code quality through 5 levels of checks:

### Validation Levels

| Level | Name | Duration | When to Run |
|-------|------|----------|-------------|
| 1 | Syntax & Format | < 10s | After every code change |
| 2 | Functional Correctness | < 2min | Before committing |
| 3 | Embedded Compatibility | < 1min | For protocol changes |
| 4 | Performance Benchmarks | < 5min | For optimization work |
| 5 | Full Build Matrix | < 10min | Before PR submission |

### Usage

```bash
# Run default validation (Level 2)
./.claude/validate.sh

# Run specific level
./.claude/validate.sh 1  # Quick format check
./.claude/validate.sh 4  # Include benchmarks
./.claude/validate.sh 5  # Full matrix
```

See [validation.md](validation.md) for complete documentation.

## 🔐 Permissions

Pre-approved commands (no confirmation needed):

- **Cargo**: `build`, `check`, `test`, `fmt`, `clippy`, `clean`, `doc`
- **Git (read-only)**: `status`, `diff`, `log`, `branch`, `show`, `ls-files`
- **File operations**: `ls`, `cat`, `find`, `grep`

Dangerous commands that ALWAYS require approval:
- `rm -rf`, `git reset --hard`, `git push --force`, `cargo publish`, `dd`

See [permissions.json](permissions.json) for full list.

## 📚 Documentation

### Project Documentation
- **[CLAUDE.md](../CLAUDE.md)** - Master development guide
  - Code standards (Rust, embedded, RMK-specific)
  - Git commit conventions
  - Testing standards
  - Common mistakes and prohibitions

- **[docs/elink/](../docs/elink/)** - Elink protocol documentation
  - Protocol overview and design
  - Integration guide
  - Performance analysis
  - FAQ and troubleshooting

### Key Principles

1. **All documentation in English** - Code comments, commits, docs
2. **Conversation can be Chinese or English** - User preference
3. **Follow conventional commits** - `type(scope): subject`
4. **Validate before committing** - Use validation script
5. **Update CLAUDE.md when patterns emerge** - Flywheel effect

## 🎯 Workflow Best Practices

Based on [Boris Cherny's 13 Tips](https://x.com/bcherny/status/...):

### 1. Multiple Claude Code Instances
Run separate instances for different tasks, enable system notifications

### 2. CLI + GUI Mode
Use `&` and `--teleport` to seamlessly switch between CLI and GUI

### 3. Use Opus 4.5 Thinking Mode
Current model (Sonnet 4) is fast, but use Opus for complex decisions

### 4. Shared CLAUDE.md (✅ Implemented)
Team maintains shared standards, forming a flywheel effect

### 5. Code Review Auto-references CLAUDE.md (✅ Implemented)
AI learns from past mistakes documented in CLAUDE.md

### 6. Start with Plan Mode (Shift+Tab twice)
For complex tasks, plan first before implementation

### 7. High-frequency Operations as Skills (✅ Implemented)
`/commit-elink`, `/test-elink` - Claude calls autonomously

### 8. Automate with Subagents (✅ Implemented)
Complex workflows delegated to specialized agents

### 9. Use PostToolUse Hook (⏳ Planned)
Format code automatically after changes

### 10. Use /permissions (✅ Implemented)
Pre-approve safe commands to avoid interruptions

### 11. Configure MCP (⏳ Planned)
Integrate tools like Slack, BigQuery, Sentry

### 12. Long Tasks: --permission-mode=dontAsk (⏳ As needed)
Let Claude work autonomously with ralph-wiggum plugin

### 13. Give Claude Validation Means (✅ Implemented)
Quality improves 2-3x when Claude can verify its own work

## 🔄 Continuous Improvement

### When to Update Configuration

- **CLAUDE.md**: When Claude makes mistakes or new patterns emerge
- **Skills**: When common workflows become repetitive
- **Validation**: When new failure patterns discovered
- **Permissions**: When new safe commands identified

### Maintenance Schedule

- **Weekly**: Review validation logs
- **Monthly**: Update CLAUDE.md and documentation
- **Per PR**: Ensure all validation passes

## 📊 Project Status

### Completed Setup (2026-02-09)
- ✅ CLAUDE.md with comprehensive standards
- ✅ Validation framework (5 levels)
- ✅ Custom skills (4 skills)
- ✅ Permissions configuration
- ✅ Documentation structure (docs/elink/)
- ✅ FAQ and roadmap

### Next Steps
1. Run validation: `./.claude/validate.sh 2`
2. Test skills: Try `/test-elink` and `/check-embedded`
3. Start development with standardized workflow
4. Update CLAUDE.md as you learn new patterns

## 🤝 Contributing

When contributing to this project:

1. Read [CLAUDE.md](../CLAUDE.md) thoroughly
2. Follow all code standards
3. Run validation before committing
4. Write commit messages in English
5. Update documentation for new features

## 📞 Support

- **Issues**: Report bugs on GitHub
- **Questions**: Check [docs/elink/faq.md](../docs/elink/faq.md)
- **Standards**: Refer to [CLAUDE.md](../CLAUDE.md)

---

**Prepared for efficient Vibe Coding with Claude Code**

*Last updated: 2026-02-09*
