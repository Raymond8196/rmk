# RMK 固件开发指南

> 本文档定义了 RMK 键盘固件项目的 Claude Code 协作规范。
> 当 Claude 犯错或出现新规范时，及时更新本文件，形成飞轮效应（Boris Cherny Tip #4）。

[English Version](./CLAUDE.md) | 中文版本

## 语言规范

**重要**:
- 所有文档、commit 消息、代码注释和 PR 描述**必须使用英文**
- 与用户的对话可以使用中文或英文
- 这确保项目对国际社区保持可访问性

| 场景 | 语言 | 示例 |
|------|------|------|
| 日常对话 | 中文 | "请帮我修复这个 bug" |
| 文档 | 英文 | CLAUDE.md（附中文版本） |
| Commit 消息 | 英文 | "feat(gazell): add split driver" |
| 代码注释 | 英文 | `// Initialize Gazell transport` |
| Plan 文档 | 中英双版 | `docs/plan-xxx-en.md` + `docs/plan-xxx-zh.md` |

## 项目概述

### RMK（Rust Mechanical Keyboard）
- **语言**: Rust (no_std)
- **框架**: Embassy async
- **支持的 MCU**: STM32, nRF52, RP2040, ESP32
- **核心功能**: 键盘固件、分体键盘、BLE/USB 通信、指点设备

### 当前工作: `feat/gazell-2g4` 分支
- **目标**: Nordic Gazell 2.4GHz 无线协议支持（键盘 <-> USB dongle）
- **硬件**: Charybdis 分体键盘 (nRF52840) + E104-BT5040U dongle (nRF52840)
- **架构**:
  ```
  键盘 (nRF52840, device 模式)
      ↓  Gazell 2.4GHz
  Dongle (E104-BT5040U, host 模式)
      ↓  USB HID
  PC
  ```
- **关键文件**:
  - `rmk-gazell-sys/` — FFI crate: C shim + Nordic SDK 绑定
  - `rmk/src/wireless/gazell.rs` — `GazellTransport` 安全封装
  - `rmk/src/wireless/config.rs` — `GazellConfig` 配置
  - `rmk/src/wireless/transport.rs` — `WirelessTransport` trait
  - `rmk/src/split/gazell.rs` — `GazellCentralHub`、`PipeDriver`、分体驱动
  - `examples/use_config/nrf52840_gazell_split/` — Charybdis 代码生成示例（central + 2 peripherals）
  - `examples/use_rust/nrf52840_dongle/` — Host/接收端独立示例（Phase 1/2）
  - `examples/use_rust/nrf52840_2g4/` — Device/发送端独立示例（Phase 1）

### 分支进度

**Phase 1: 最小 TX/RX 验证**
| 功能 | 状态 |
|------|------|
| `rmk-gazell-sys` FFI crate (C shim + build.rs) | ✅ 完成 |
| `GazellTransport` 实现 `WirelessTransport` trait | ✅ 完成 |
| Mock 实现（无硬件测试） | ✅ 完成 |
| Dongle 示例（host 模式，USB CDC 调试） | ✅ 完成 |
| 键盘示例（device 模式，测试数据包） | ✅ 完成 |
| Nordic nRF5 SDK v17.1.0 安装 | ✅ 已安装 |
| 交叉编译验证（ARM 目标） | ✅ 均约 25KB |
| 硬件验证（dongle <-> 键盘） | ✅ 完成（2.4GHz 通信已验证） |

**Phase 2: SplitMessage over Gazell + Charybdis 集成**
| 功能 | 状态 |
|------|------|
| C shim `ack_payload_length` 类型修复（Step 1） | ✅ 完成 |
| Rust FFI 绑定: `gz_set_ack_payload` / `gz_get_ack_payload`（Step 2） | ✅ 完成 |
| `GazellConfig` 添加 `heartbeat_interval_ms`，更新调用点（Step 3） | ✅ 完成 |
| `GazellSplitDriver`（Peripheral + Central）（Step 4） | ✅ 完成 |
| 接入 split 模块 + `compile_error!` 守卫（Step 5） | ✅ 完成 |
| Cargo.toml feature gate 更新（Step 6） | ✅ 完成 |
| 代码生成: `rmk-macro` 添加 `"gazell"` 连接类型（Step 9） | ✅ 完成 |
| `rmk-config` 为 Gazell 覆盖 CommunicationConfig（Step 10） | ✅ 完成 |
| Dongle 示例: 静态键映射 USB HID 转发（Step 11） | ✅ 完成 + 硬件验证 |
| 测试 peripheral: 通过 gz_send 发送 SplitMessage::Key（Step 11b） | ✅ 完成 + 硬件验证 |
| Charybdis keyboard.toml 代码生成 peripheral（Step 12） | ✅ 完成 + 硬件验证 |
| 硬件验证: 左手按键 → dongle → PC（Step 13） | ✅ 完成（row2col=true, build.rs 修复） |

**Phase 3: 多管道 Gazell 分体（代码生成驱动的中央端）**
| 功能 | 状态 |
|------|------|
| `GazellCentralHub` + `PipeDriver`（通过 channel 多管道解复用） | ✅ 完成 |
| `rmk-config`: `GazellSplitConfig`、`gazell_pipe` 字段、matrix 的 `#[serde(default)]` | ✅ 完成 |
| `get_communication_config()` 修复: Gazell central → `Usb(...)`，peripheral → `None` | ✅ 完成 |
| 零矩阵中央端: `DummyMatrix`，rows=0/cols=0 跳过引脚初始化 | ✅ 完成 |
| 代码生成: entry.rs 中的 hub + pipe manager 任务派生 | ✅ 完成 |
| ISR 桥接代码生成: central 和 peripheral（bind_interrupt.rs、peripheral.rs） | ✅ 完成 |
| Central 代码生成: HFCLK + IRQ 优先级 + `gz_init_default(1)` | ✅ 完成 |
| `_wireless` feature gate（`BatteryState`） | ✅ 完成 |
| 示例: `nrf52840_gazell_split` — central + peripheral + peripheral_right（ARM 构建） | ✅ 完成 |
| 硬件验证: 双手按键 → dongle → PC | ⏳ 待验证 |

**架构决策**:
- 左手（peripheral）→ Gazell → dongle → USB HID → PC（Phase 2，已验证）
- 双手 → Gazell → dongle → USB HID → PC（Phase 3，代码生成完成，待硬件验证）
- Dongle/central: 使用 `keyboard.toml` 代码生成（`#[rmk_central]`），零矩阵 `rows=0/cols=0`
- `examples/use_rust/nrf52840_dongle/` 保留为独立参考（Phase 1/2 演示）
- 轨迹球（pmw3610）数据: 最终版本也通过 Gazell 传输

### 环境配置
```bash
# Nordic SDK（ARM 交叉编译必需）
export NRF5_SDK_PATH="$HOME/nRF5_SDK_17.1.0/nRF5_SDK_17.1.0_ddde560"

# ARM 工具链
sudo apt-get install -y gcc-arm-none-eabi

# Rust 目标
rustup target add thumbv7em-none-eabihf

# 构建命令
# 重要: 示例项目必须在其自身目录中构建（不是仓库根目录）
# 因为 cargo 从 CWD 解析 .cargo/config.toml，而不是 --manifest-path。
# 从仓库根目录使用 --manifest-path 构建会遗漏链接脚本 (-Tlink.x)，
# 导致生成空的 ELF 二进制文件。
cargo build --manifest-path rmk-gazell-sys/Cargo.toml --target thumbv7em-none-eabihf --features nrf52840
cd examples/use_rust/nrf52840_dongle && cargo build --release
cd examples/use_rust/nrf52840_2g4 && cargo build --release
```

### 相关分支
- `feat/pointing-mode` — 按层指点模式（Cursor/Scroll/Sniper），暂存的 TOML 配置工作

---

## 开发工作流

### 核心原则

1. **行动前先问** — 当需求模糊、信息不完整或存在多种有效方案时，**总是先向用户确认再行动**。不要假设或猜测。
2. **将大任务分解为可验证的步骤** — 将非平凡任务分解为小的、可独立验证的单元。每个步骤应有明确的完成标准，可在进入下一步之前检查。
3. **引用标准和协议的真实来源** — 引用硬件规格、通信协议、SDK API 或任何标准化内容时，始终提供实际来源（数据手册章节、官方文档 URL、SDK 头文件路径、RFC 编号等）。绝不捏造或猜测技术细节 — 先从一手来源验证。

### 变更规模分类

```
小变更（bug 修复、参数调整、单函数修改）
  → 直接修改 → 说明变更 → 自验证

大变更（重构、新功能、多文件架构调整）
  → Plan 模式 → 讨论方案 → 确认 → 实现 → 自验证
```

### Plan 模式使用指南

**何时使用 Plan 模式:**
- 添加新协议或通信层（如 Gazell split driver）
- 重构现有模块架构
- 涉及 3 个以上文件的变更
- 需要讨论的算法或协议设计
- 存在多种可行方案的变更

**Plan 模式工作流:**
1. 进入 Plan 模式（使用 EnterPlanMode 工具）
2. 探索代码库并提出实现方案
3. 与用户讨论并确认方案
4. 退出 Plan 模式开始实现
5. 实现过程中持续自验证

**Plan 文档规范:**
- 在 `docs/` 目录创建计划文档
- 用户要求时同时维护中英文版本
- 核心逻辑必须包含伪代码（不能只有文字描述）
- 每轮 review 后更新版本号（v1, v2, ...）
- 在文档 changelog 中记录 review 发现和修复

### 代码审查飞轮效应

每轮代码审查后，更新 `CLAUDE.md` 文件:
1. **记录问题模式** — 如果出现新类别的问题，添加到"常见错误"部分
2. **积累项目特定规则** — 更新相关标准章节
3. **记录架构决策** — 更新"当前工作"或添加到相关部分
4. **修补验证盲区** — 如果 review 发现了自验证遗漏的问题，添加对应检查

这形成反馈循环：每次 review 都让后续工作更准确。

### 自验证流程

每次代码修改后，**在宣称变更完成之前**，执行以下验证:

#### 1. 格式化（必须）
```bash
cargo fmt --all -- --check
```

#### 2. Lint（必须）
```bash
cargo clippy --all-targets -- -D warnings
# 对于 feature-gated 代码:
cargo clippy --manifest-path rmk/Cargo.toml --features "split,wireless_gazell" -- -D warnings
```

#### 3. 编译（必须）
```bash
# 宿主机构建（mock 模式）
cargo check --manifest-path rmk/Cargo.toml --features wireless_gazell

# ARM 交叉编译（如果修改了 FFI 代码）
cargo build --manifest-path rmk-gazell-sys/Cargo.toml --target thumbv7em-none-eabihf --features nrf52840

# 示例（必须 cd 到目录中）
cd examples/use_rust/nrf52840_2g4 && cargo build --release && cd -
cd examples/use_rust/nrf52840_dongle && cargo build --release && cd -
```

#### 4. 测试（必须）
```bash
cargo test --manifest-path rmk/Cargo.toml --lib
```

#### 5. Feature 组合检查（涉及 feature gate 时）
```bash
# 验证所有相关 feature 组合能编译
cargo check --manifest-path rmk/Cargo.toml --features "split,wireless_gazell"
cargo check --manifest-path rmk/Cargo.toml --features "split"
```

### 验证失败处理

**如果任何验证步骤失败:**
1. **立即停止** — 不提交代码，不宣称变更完成
2. **分析失败原因** — 阅读完整的错误输出
3. **修复问题** — 解决根本原因，而非表面症状
4. **重新验证** — 再次运行完整验证套件

### 验证报告

完成非平凡变更后，输出摘要:

```
## 验证报告
### 修改内容
- 文件: <路径>
- 变更: <描述>

### 验证结果
- cargo fmt: 通过 / 失败
- cargo clippy: 通过 / 失败（N 个警告）
- cargo check (宿主机): 通过 / 失败
- cargo build (ARM): 通过 / 失败 / 跳过
- cargo test: 通过 / 失败（N 个测试）

### 可以提交: 是 / 否
```

---

## 代码标准

### Rust 通用标准

#### 1. 格式化
- **必须**: 所有代码必须通过 `cargo fmt`
- **检查**: 使用 `cargo clippy` 消除警告
- **命令**:
  ```bash
  cargo fmt --all
  cargo clippy --all-targets --all-features
  ```

#### 2. 错误处理
- 使用 `Result<T, E>` 而非 `unwrap()`
- 为自定义错误类型实现 `Display` 和 `Debug`
- 避免在库代码中使用 `panic!()`（嵌入式环境中很危险）
- 优先使用 `?` 操作符进行错误传播

#### 3. 异步代码标准
- 使用 Embassy 的 `async/await`
- 避免阻塞操作（嵌入式无 OS 调度器）
- 优先使用 channel 通信而非共享状态

#### 4. 内存管理
- **禁止**: 不要在 no_std 环境中使用 `Box`, `Vec`, `String`
- **优先**: 使用固定大小数组和 `heapless` 容器
- **检查**: 确保代码能在 `#![no_std]` 下编译

### 嵌入式 Rust 特定标准

#### 1. 依赖管理
- 所有依赖必须支持 `no_std`
- 在 `Cargo.toml` 中使用 `default-features = false`

#### 2. Feature Gate
- 对大功能块使用可选 feature

```rust
#[cfg(feature = "wireless_gazell")]
mod gazell_impl;
```

#### 3. 栈内存控制
- 避免大的栈分配（嵌入式栈通常 < 64KB）
- 大缓冲区使用 `static`

### FFI / C 互操作标准（Gazell 相关）

#### 1. 安全边界
- 所有 `unsafe` FFI 调用必须封装在安全的 Rust 函数中
- 用 `// SAFETY:` 注释文档化安全不变量
- 传递给 C 代码前验证输入

#### 2. 条件编译
- 真实 FFI 代码使用 `#[cfg(feature = "wireless_gazell")]`
- 为 `#[cfg(not(feature = "wireless_gazell"))]` 提供 mock 回退
- 这允许在宿主机上运行 `cargo test` 和 `cargo check`，无需 Nordic SDK

#### 3. 构建系统
- 如果 `NRF5_SDK_PATH` 未设置，`build.rs` 必须给出清晰的错误
- 在非 ARM 目标上优雅地跳过编译（为了 IDE 支持）

---

## Git Commit 规范

### Commit 消息格式
```
<type>(<scope>): <subject>

<body>
```

**所有 commit 消息必须使用英文**

### Type
- `feat`: 新功能
- `fix`: Bug 修复
- `docs`: 文档更新
- `refactor`: 重构（无行为变化）
- `test`: 测试相关
- `chore`: 构建/工具链更新
- `perf`: 性能优化

### Scope
- `gazell`: Gazell 2.4G 无线协议
- `wireless`: 无线传输层
- `dongle`: USB dongle / 接收器
- `pointing`: 指点设备 / 轨迹球逻辑
- `rmk`: RMK 核心
- `split`: 分体键盘
- `ble`: BLE 功能
- `usb`: USB 功能
- `examples`: 示例代码
- `config`: 配置结构体 / TOML 解析
- `macro`: 代码生成宏

### ❌ Commit 消息中禁止的内容

**绝对不要在 commit 消息中包含 Co-Authored-By 行**

```bash
# ❌ 禁止 - 不要包含这些行
Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
Co-Authored-By: Claude <...>
```

本项目不使用 AI 辅助的共同作者署名。

---

## RMK 架构：三层变更规则

RMK 有严格的三层流水线。**任何涉及配置或代码生成的功能都必须同时更新所有三层。**

```
keyboard.toml
    ↓  解析于
rmk-config/src/lib.rs   (配置结构体: JoystickConfig, Pmw3610Config, ...)
    ↓  消费于
rmk-macro/src/codegen/  (代码生成: adc.rs, pmw3610.rs, pmw33xx.rs, ...)
    ↓  生成
运行时 Rust 代码        (JoystickProcessor, NrfAdc, PointingProcessor, ...)
```

### 实现跨层变更前的检查清单

当添加字段、修改结构体或更新函数签名时:

**步骤 1 — 追踪完整数据流**
- 谁*产生*数据？
- 谁*消费*数据？
- **同时**修复生产者和消费者，而非只改一端。

**步骤 2 — 检查所有三层**
- [ ] `rmk-config/src/lib.rs` — 配置结构体是否暴露了新字段？是否需要 `#[serde(default)]`？
- [ ] `rmk-macro/src/codegen/` — 代码生成是否正确传递了新字段/参数？
- [ ] 运行时 (`rmk/src/`) — 结构体/函数是否已更新？

**步骤 3 — 修改函数签名或结构体前搜索所有调用点**

```bash
grep -r "StructName {" --include="*.rs" --include="*.md"
grep -r "fn_name(" --include="*.rs" --include="*.md"
```

**步骤 4 — 更新示例和文档（"第四层"）**
- [ ] `examples/use_rust/` — 所有引用了变更 API 的示例 crate
- [ ] `docs/docs/main/docs/` — 所有包含变更 API 代码块的 `.md` 文件
- [ ] 文档注释 (`/// # Example`) 中的变更结构体/函数

---

## 常见错误与禁令

### ❌ 绝对禁止

1. **不要在 no_std 代码中使用 std**
2. **不要在库代码中 panic**
3. **不要在未明确讨论的情况下破坏向后兼容性**
4. **不要提交未格式化的代码** — 提交前必须运行 `cargo fmt`
5. **不要在任何 commit 中添加 `Co-Authored-By: Claude ...`**

### ⚠️ 需要注意（Gazell 相关）

1. **异步上下文中的阻塞 FFI 调用** — `gz_send()` 会阻塞直到收到 ACK 或超时（约 6ms 忙等待）。在 Embassy async 中，这会饿死执行器，阻止其他任务（包括 USB）运行。**已确认: 当 `gz_send()` 与 USB 同时以 10Hz 调用时，USB CDC 变得无响应。** 必须将 Gazell 收发移到专用任务或使用非阻塞方式。

2. **Nordic SDK 路径敏感性** — `build.rs` 构造类似 `{NRF5_SDK_PATH}/components/proprietary_rf/gzll/gcc/` 的路径。如果 SDK 有嵌套目录（如解压目录内的 `nRF5_SDK_17.1.0_ddde560/`），路径必须指向内层目录。

3. **Feature 组合测试**
   ```bash
   # 宿主机（无 wireless feature，mock 模式）
   cargo test --manifest-path rmk/Cargo.toml --lib --no-default-features --features "wireless_gazell" -- wireless
   # ARM 交叉编译（真实 FFI）
   cargo build --manifest-path rmk-gazell-sys/Cargo.toml --target thumbv7em-none-eabihf --features nrf52840
   ```

4. **`nrf_gzll_init()` 会重置所有设置为默认值** — 每次调用 `nrf_gzll_init()` 都会清除所有自定义配置（信道表、地址、数据速率等）。`gz_set_mode()` 函数内部调用了 `nrf_gzll_init()`，所以配置必须在之后重新应用。已通过在 `gz_state.saved_config` 中保存配置并在每次重新初始化后调用 `gz_apply_config()` 修复。

5. **自定义 Gazell 配置 vs 默认值 — 需要重新测试** — 截至 2026-03-12，`GazellConfig::low_latency()` 自定义设置曾表现为通信失败，而 Nordic 默认值可以工作。但 P2 的根因最终定位为示例 crate 缺少 `build.rs`，导致 Gazell C 库根本未被链接。自定义配置失败可能是误诊 — 现在构建系统正确后需要重新测试。

6. **`GazellTransport::recv_frame()` 有 `initialized` 守卫** — Rust 封装检查 `self.initialized`，如果传输层不是通过 `gazell.init()` 初始化的则返回 `NotInitialized`。如果你绕过封装（如使用 `gz_init_default()`），也必须绕过 `recv_frame()` 并直接调用 `gz_recv()`。

---

## 硬件验证经验教训

### 1. 假设内存布局前务必先读 INFO_UF2.TXT

**问题**: 假设 nice!nano 有 SoftDevice S140，设置了 `FLASH ORIGIN = 0x26000`。固件被写入错误地址，从未执行。

**根因**: 实际的 nice!nano 的 INFO_UF2.TXT 中显示 `SoftDevice: not found`。应用应从 `0x1000` 开始。

**规则**: 为任何 UF2 引导加载器开发板设置 memory.x 前：
```
1. 进入 UF2 模式（双击复位）
2. 读取驱动器上的 INFO_UF2.TXT
3. 如果 "SoftDevice: not found" → FLASH ORIGIN = 0x1000
4. 如果 "SoftDevice: S140 v6.1.1" → FLASH ORIGIN = 0x26000
```

### 2. 带 MBR 的 nRF52840 上 RAM ORIGIN 必须为 0x20000008

MBR 保留了 RAM 的前 8 字节（`0x20000000-0x20000007`）用于前向中断向量。如果 `memory.x` 使用 `RAM ORIGIN = 0x20000000`，`.data` 段初始化会覆盖 MBR 的保留区域，导致热复位时崩溃。始终使用 `0x20000008`。

### 3. 预编译 C 库需要 ISR 桥接才能配合 cortex-m-rt

Nordic 的 `gzll_nrf52840_gcc.a` 导出 CMSIS 命名的 ISR 处理函数（`RADIO_IRQHandler`、`TIMER2_IRQHandler`、`SWI0_EGU0_IRQHandler`）。这些不会自动放入 cortex-m-rt 向量表。必须添加桥接函数：

```rust
#[pac::interrupt]
fn RADIO() {
    unsafe { RADIO_IRQHandler() }
}
```

**命名差异**: PAC 使用 `EGU0_SWI0`，C 库使用 `SWI0_EGU0_IRQHandler`。同一个中断（IRQ #20），不同的命名约定。

### 4. 没有 USB 时必须显式启动 HFCLK

USB 驱动会自动启动 HFCLK（32MHz 晶振）。没有 USB 时，必须在 Gazell 初始化前手动启动：
```rust
pac::CLOCK.tasks_hfclkstart().write_value(1);
while pac::CLOCK.events_hfclkstarted().read() != 1 {}
```

### 5. 诊断计数器对嵌入式调试至关重要

没有调试探针时，在每一层添加计数器：
- **ISR 桥接层**: `#[interrupt]` 函数中的 `AtomicU32` 计数器（R=RADIO 计数, S=SWI0 计数）
- **C 回调层**: 回调中的 `volatile uint32_t` 计数器（rx_cb_count, rx_fetch_ok/fail）
- **Rust 封装层**: 跟踪返回值和错误码

> **注意**: 原始观察（自定义配置 `cb=0`，默认配置 `cb=52`）最初被归因于配置问题，但 P2 根因实际上是缺少 `build.rs` 导致 Gazell C 库未被链接。`cb=0` → `cb=52` 的变化可能反映的是构建修复，而非配置变更。分层计数器技术本身仍然有价值。

### 6. 不要信任 Rust 封装的返回值 — 检查 FFI 层

`GazellTransport::recv_frame()` 返回 `NotInitialized`，因为 `self.initialized` 为 `false`（Gazell 通过 `gz_init_default()` 初始化，绕过了 Rust 封装的 `init()`）。调试时，独立测试每一层。

> **注意**: 原始观察中 "C 层工作正常（`cb=52, ok=52`）" 可能是在修复 build.rs 之后观察到的，而非之前。通用教训（独立检查每一层）仍然有效。

### 7. Edition 2024 需要 `unsafe extern "C"`

Rust edition 2024 要求在 `extern "C"` 块上添加 `unsafe` 关键字：
```rust
// Edition 2024:
unsafe extern "C" {
    fn RADIO_IRQHandler();
}
// 旧版本:
extern "C" {
    fn RADIO_IRQHandler();
}
```

### 8. `cargo build` 不跟踪 `memory.x` 变更

修改 `memory.x` 不会触发重新编译。必须先 `cargo clean`：
```bash
cargo clean && cargo build --release
```

### 9. nRF52840 默认配置自动启用 BLE — Gazell 必须覆盖

`rmk-config` 将用户的 `keyboard.toml` 与 `default_config/nrf52840.toml` 合并，后者包含 `[ble] enabled = true` 和 `usb_enable = true`。对于 Gazell peripheral（无 BLE，无 USB），`get_communication_config()` 返回 `CommunicationConfig::Ble` 而非 `None`，导致代码生成发出 BLE 协议栈初始化代码（`nrf_sdc`, `build_sdc`, `Irqs` 等）。

**修复**（Phase 2）: 当 `connection = "gazell"` 时提前返回 `CommunicationConfig::None`。
**细化**（Phase 3）: Gazell central（dongle）需要 USB，因此现在 `usb_enable=true` + Gazell 返回 `Usb(usb_info)`，`usb_enable=false` + Gazell 返回 `None`。仅 BLE 被 Gazell 抑制（共享射频）。

### 10. Gazell nRF52 Peripheral 的代码生成需要 HFCLK + IRQ 优先级初始化

与 BLE（通过 MPSL 处理时钟/IRQ）不同，Gazell peripheral 需要显式：
1. **HFCLK 启动** — `CLOCK.tasks_hfclkstart()`（无 USB 意味着不会自动启动）
2. **IRQ 优先级** — RADIO/TIMER2 设为 P0，EGU0_SWI0 设为 P1

这些必须由代码生成器生成（在 `expand_split_peripheral_entry` 中），因为 `rmk` crate 不能依赖 `embassy_nrf`。

---

## 测试标准

### 提交前自检
- [ ] 代码已格式化（`cargo fmt --all`）
- [ ] 无 Clippy 警告（`cargo clippy --all-targets`）
- [ ] 单元测试通过（宿主机 mock 模式）
- [ ] 文档已更新（如果 API 有变更）
- [ ] Commit 消息为英文，遵循约定格式
- [ ] Commit 中无 `Co-Authored-By: Claude`
- [ ] 无残留的 `println!` 或仅调试代码

---

## 处理不确定性

### 何时请求澄清

**以下情况必须询问用户:**

1. **需求不明确**
2. **设计决策需要输入** — 例如，dongle 是否需要支持多个配对键盘？
3. **破坏性变更或兼容性问题**
4. **硬件相关假设** — 例如引脚分配、RF 信道选择

---

## CI 故障排除

### 黄金法则: 永远不要只信任本地环境

当 CI 失败但本地检查通过时，说明环境存在差异。常见问题:
- CI 中未设置 `NRF5_SDK_PATH` — Gazell FFI crate 会失败
- ARM GCC 不可用 — C shim 编译失败
- 本地和 CI 之间的 feature flag 不匹配

### 不要做的事

- 不要在本地重复运行同一命令 10 次，期望它自行修复
- 不要创建空 commit 来"触发 CI 重新运行"
- 不要手动编辑格式 — 让 `cargo fmt` 来做
- 不要假设"我的机器上能用"就意味着 CI 有问题

---

## 版本历史

- 2026-02-24: 初始版本，用于 `feat/pointing-mode` 分支
- 2026-03-02: 适配 `feat/gazell-2g4` 分支
  - 更新当前工作部分为 Gazell 2.4G 无线
  - 添加环境配置部分（SDK 路径）
  - 添加 FFI/C 互操作标准部分
  - 添加 Gazell 相关注意事项（阻塞 FFI、SDK 路径、elink_core 依赖）
  - 添加 `gazell`, `wireless`, `dongle` commit scope
  - 保留通用标准（代码、git、架构规则）
- 2026-03-04: 添加开发工作流部分
  - 变更规模分类（小变更 vs 大变更）
  - Plan 模式使用指南和工作流
  - 代码审查飞轮效应（自动更新 CLAUDE.md）
  - 自验证流程（cargo fmt/clippy/check/test/build）
  - 验证失败处理和验证报告模板
- 2026-03-04: 创建中文版本 CLAUDE.zh.md
- 2026-03-06: 添加开发工作流核心原则
  - 行动前先问（澄清歧义后再行动）
  - 将大任务分解为可验证的步骤
  - 引用标准和协议的真实来源
- 2026-03-11: Phase 1 完成，Phase 2 计划更新至 v10
  - 更新分支进度: Phase 1 硬件验证标记完成
  - 添加 Phase 2 进度表（Steps 1-13）
  - 记录架构决策（左手优先、dongle 为 use_rust、多管道最终目标）
  - 计划文档扩展 Steps 9-13（代码生成、rmk-config、dongle USB HID、Charybdis 集成）
- 2026-03-12: 硬件验证会话 — 重要经验教训
  - 发现 nice!nano 没有 SoftDevice（FLASH ORIGIN = 0x1000 而非 0x26000）
  - 修复 `gz_set_mode()` 配置丢失 bug（nrf_gzll_init 重置所有设置）
  - 确认 ISR 桥接工作正常（RADIO/TIMER2/EGU0_SWI0 均在向量表中）
  - 确认 Gazell 无线链路在 Nordic 默认配置下工作（cb=52, ok=52）
  - 识别自定义配置为通信失败的根源（需要逐项二分排查）
  - 添加 10 条硬件验证经验教训
  - 更新注意事项章节（已确认的问题）
- 2026-03-14: Phase 3 多管道 Gazell 实现（Steps 1-7 软件完成）
  - GazellCentralHub + PipeDriver: 单一 gz_recv() 所有者分发到每管道 channel
  - DummyMatrix: 零矩阵中央端（无按键的 USB dongle）
  - ISR 桥接代码生成: central 和 peripheral 均自动生成
  - CommunicationConfig 修复: Gazell central 获得 Usb(...)，peripheral 获得 None
  - `_wireless` feature gate: BatteryState 由 BLE 和 Gazell 共享
  - Charybdis 三二进制示例（central + left + right）均可 ARM 构建
  - 架构决策修订: dongle 现使用 keyboard.toml 代码生成（#[rmk_central]）
  - 标注经验教训 5、6 和注意事项 5: P2 "自定义配置失败" 可能由缺少 build.rs 导致
