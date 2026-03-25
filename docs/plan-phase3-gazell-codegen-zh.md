# Phase 3: 多管道 Gazell + 代码生成 + keyboard.toml 集成

> **分支**: `feat/gazell-2g4-verify`
> **状态**: 规划完成，待实现
> **前置条件**: Phase 2 单管道 Gazell 双向通信（已完成）

## 1. 概述

Phase 2 建立了单管道 Gazell 双向通信，实现了 `GazellCentralDriver` 和 `GazellPeripheralDriver`（位于 `rmk/src/split/gazell.rs`）。Phase 3 将其扩展为生产级分体键盘系统：

- Charybdis 的两半键盘通过 Gazell 2.4GHz 连接到 USB dongle
- Dongle 运行完整的 RMK 键盘处理（keymap/层/宏）
- Dongle 通过 USB HID 输出到主机 PC
- 配置由 `keyboard.toml` 驱动，支持代码生成

### 目标架构

```
左手 (nRF52840, peripheral)
    |  Gazell pipe 0
    v
USB Dongle (nRF52840, central)  ──USB HID──>  PC
    ^
    |  Gazell pipe 1
右手 (nRF52840, peripheral)
```

## 2. Phase 2 经验教训

| 教训 | 在 P3 中的应用 |
|---|---|
| FFI 类型不匹配（uint8_t vs uint32_t）导致栈损坏 | 多管道 hub 复用已验证的 FFI；无需新 C shim 修改 |
| Mock fallback 对主机测试至关重要 | 所有新的静态 channel 和 hub 逻辑必须在 mock 模式下工作 |
| 示例必须从自己的目录构建 | 验证命令始终 `cd` 到示例目录 |
| State vs Event 区分至关重要 | Hub 的 flush 逻辑保留 P2 的 `pending_event > pending_state` 优先级 |
| `compile_error!` 防止 BLE+Gazell 共存 | 维持不变；代码生成使用 `wireless_gazell_nrf52840` 特性 |
| 每步必须可独立验证 | 每步都有明确的验证命令和成功标准 |

## 3. 自查：发现的关键问题

在计划审查中，通过追踪实际源代码发现了 6 个阻塞级问题：

| # | 问题 | 影响 | 修复（步骤） |
|---|---|---|---|
| 1 | `bind_interrupt_default()` 在 `bind_interrupt.rs:100` 调用 `communication.get_ble_config().unwrap()` — Gazell 无 `[ble]` 时 panic | **阻塞**：central 无法编译 | 步骤 5：在访问 ble_config 之前为 Gazell 短路 nRF52 路径 |
| 2 | `expand_bind_interrupt_for_split_peripheral()` 在 `peripheral.rs:77` 同样调用 `get_ble_config().unwrap()` | **阻塞**：peripheral 无法编译 | 步骤 5：在 peripheral ISR 代码生成中添加 Gazell 路径 |
| 3 | `expand_matrix_config()` 在 `matrix.rs:60` 调用 `row_pins.clone().unwrap()` — rows=0 cols=0 时 panic | **阻塞**：零矩阵 panic | 步骤 4：同时守卫 `expand_matrix_config` 和 `expand_matrix_and_keyboard_init` |
| 4 | `rmk_entry_select()` 在 `entry.rs:55` 始终将 `matrix` 加入 devices — 零矩阵无此变量 | **阻塞**：未定义变量 | 步骤 3+4：条件跳过 `matrix` |
| 5 | 原步骤 7 和 8 重复 | 计划清晰度 | 合并为单个步骤 7 |
| 6 | `peripheral.rs` 中 BatteryState 使用 `with_feature("_ble")` 宏，非 `#[cfg]` | 步骤 6 范围 | 步骤 6：同时处理 `select_biased_with_feature!` 宏调用 |

## 4. 步骤详解

### 步骤 0：合并 upstream/main

**状态**：已完成

所有主分支重构（PR #726 `refactor/macro`、PR #717 `feat/event`）已合并到当前分支。

---

### 步骤 1：多管道解复用器（GazellCentralHub）

**问题**：当前 `GazellCentralDriver`（`rmk/src/split/gazell.rs:168-337`）每个实例调用 `gz_recv()`。两个 peripheral 时，两个 driver 实例会竞争同一个硬件 FIFO — 数据包被错误的 driver 窃取。

**方案**：单个 `GazellCentralHub` 异步任务拥有 `gz_recv()`，按管道分发到 embassy Channel。每管道 `PipeDriver` 通过 channel send/recv 实现 `SplitReader + SplitWriter`。

**架构**：
```
                gz_recv()
                   |
             GazellCentralHub  (单任务)
              /            \
    PIPE_RX[0]             PIPE_RX[1]        (embassy Channel, 容量 8)
         |                      |
   PipeDriver(0)   PipeDriver(1)
         |                      |
   PeripheralManager(左手)   PeripheralManager(右手)
```

**涉及文件**：
- `rmk/src/split/gazell.rs`（修改现有）
- `rmk/src/split/central.rs`（更新调用方）

**关键修改**：

1. **静态 channel 数组**（MAX_GAZELL_PIPES = 8，Gazell 硬件最大值）。运行时 `num_pipes` 控制实际使用数量：
   ```rust
   pub(crate) const MAX_GAZELL_PIPES: usize = 8;
   static PIPE_RX: [Channel<RawMutex, SplitMessage, 8>; MAX_GAZELL_PIPES] = [
       Channel::new(), Channel::new(), Channel::new(), Channel::new(),
       Channel::new(), Channel::new(), Channel::new(), Channel::new(),
   ];
   static PIPE_TX: [Channel<RawMutex, SplitMessage, 4>; MAX_GAZELL_PIPES] = [
       Channel::new(), Channel::new(), Channel::new(), Channel::new(),
       Channel::new(), Channel::new(), Channel::new(), Channel::new(),
   ];
   ```

2. **GazellCentralHub** 异步函数：拥有 `gz_recv()` 循环，按 `rx_pipe` 分发到 `PIPE_RX[rx_pipe]`，过滤心跳包，通过 `gz_set_ack_payload(pipe_i, ...)` flush 所有活跃管道的 `PIPE_TX[i]`。Flush 优先级：事件 > 状态（复用 `is_event_type()`）。

3. **PipeDriver** `{ pipe_index: usize }` — 通过 channel send/recv 实现 SplitReader/SplitWriter。**无 Gazell 专有代码** — 完全可复用于其他无线协议（ESB、ESP-NOW）。

4. **run_gazell_central_hub(config, num_pipes)** — 初始化 Gazell host 模式，hub 循环包装在 `select(hub_loop, GAZELL_SHUTDOWN.wait())` 中，为 Phase 4 热切换做准备。

5. **run_gazell_pipe_manager\<ROW, COL, ROW_OFFSET, COL_OFFSET\>(pipe_index, id)** — 创建 PipeDriver + PeripheralManager 并运行。

6. `GazellPeripheralDriver` 保持不变（键盘端仍使用直接 FFI）。

7. 将 `central.rs:43-44` 中的 `run_gazell_peripheral_manager` 迁移到基于 hub 的 pipe manager。

**验证计划**：

| # | 命令 | 测试内容 |
|---|---|---|
| V1 | `cargo check --manifest-path rmk/Cargo.toml --features "split,wireless_gazell"` | 主机编译 Gazell 特性 |
| V2 | `cargo test --manifest-path rmk/Cargo.toml --lib -- gazell` | Mock 模式单元测试 |
| V3 | `cargo check --manifest-path rmk/Cargo.toml --features "split"` | 串口分体回归 |
| V4 | `cargo build --manifest-path rmk-gazell-sys/Cargo.toml --target thumbv7em-none-eabihf --features nrf52840` | ARM FFI 交叉编译 |
| V5 | `cd examples/use_rust/nrf52840_dongle && cargo build --release && cd -` | 现有示例仍可构建 |

**成功标准**：5 条命令全部通过。新类型（`GazellCentralHub`、`PipeDriver`）在 `cargo doc` 中可见。

---

### 步骤 2：rmk-config — Gazell 分体字段

**问题**：`SplitConfig`（`rmk-config/src/lib.rs:775-782`）无 Gazell 专有字段。

**涉及文件**：`rmk-config/src/lib.rs`

**修改**：
1. 添加 `GazellSplitConfig` 结构体，包含 `channel`、`data_rate`、`tx_power`、`heartbeat_interval_ms`（均 `#[serde(default)]`）
2. 在 `SplitConfig` 中添加 `gazell: Option<GazellSplitConfig>`
3. 在 `SplitBoardConfig` 中添加 `gazell_pipe: Option<u8>`
4. 为 `SplitBoardConfig.matrix` 字段（行 804）添加 `#[serde(default)]` — 允许 dongle 省略 `[split.central.matrix]`

**验证计划**：

| # | 命令 | 测试内容 |
|---|---|---|
| V1 | `cargo check --manifest-path rmk-config/Cargo.toml` | Config crate 编译 |
| V2 | `cargo test --manifest-path rmk-config/Cargo.toml` | 测试通过，现有 TOML 解析不受影响 |

**成功标准**：两条命令通过。新结构体在 `cargo doc` 中可见。

---

### 步骤 3：代码生成 — "gazell" 连接类型

**问题**：代码生成在 `entry.rs:164` 遇到未知连接类型时 panic。代码生成调度链（`entry.rs`、`split/central.rs`、`split/peripheral.rs`）无 `"gazell"` 路径。

**涉及文件**：
- `rmk-macro/src/codegen/entry.rs` — 添加 `"gazell"` 分支
- `rmk-macro/src/codegen/split/central.rs` — 在 `expand_split_communication_config()` 中添加 `"gazell"` 分支
- `rmk-macro/src/codegen/split/peripheral.rs` — 在 `expand_split_peripheral_entry()` 中添加 `"gazell"` 分支

**修改**：

**3a. entry.rs** — central 调度（在 `"serial"` 分支之后、panic 之前）：
```rust
} else if split_config.connection == "gazell" {
    let rmk_task = quote! { ::rmk::run_rmk(#keymap #usb_driver_arg #storage rmk_config) };
    let num_peripherals = split_config.peripheral.len();
    tasks.push(rmk_task);
    // Hub 任务
    tasks.push(quote! { ::rmk::split::gazell::run_gazell_central_hub(gazell_config, #num_peripherals) });
    // 每 peripheral 的 pipe manager 任务
    for (idx, p) in split_config.peripheral.iter().enumerate() { ... }
    join_all_tasks(tasks)
}
```

**关键守卫**（解决阻塞 #4）：当 `central.rows == 0 && central.cols == 0` 时，不将 `matrix` 推入 `devs`：
```rust
let devices_task = if is_zero_matrix_central { /* 跳过 matrix */ } else { /* 现有代码 */ };
```

**3b. split/central.rs** — `"gazell"` 分支从 TOML 字段生成 `GazellConfig`。

**3c. split/peripheral.rs** — `"gazell"` 分支用 peripheral 的 pipe 生成 `GazellConfig`，调用 `run_rmk_split_peripheral(gazell_config)`。

**验证计划**：

| # | 命令 | 测试内容 |
|---|---|---|
| V1 | `cargo check --manifest-path rmk-macro/Cargo.toml` | 宏 crate 编译 |

**成功标准**：宏 crate 编译通过。完整集成推迟到步骤 7。

---

### 步骤 4：零矩阵 Central（Dongle 无按键）

**问题**：多个代码生成函数无条件生成矩阵代码，rows=0 cols=0 时（dongle 无键矩阵）导致 panic。

**Panic 点**：
1. `expand_matrix_config()` 在 `matrix.rs:60` — `row_pins.clone().unwrap()` panic
2. `expand_matrix_and_keyboard_init()` 在 `orchestrator.rs:338-363` — 用缺失的引脚生成 `Matrix::new()`
3. `rmk_entry_select()` 在 `entry.rs:55` — 将 `matrix` 加入 devices 任务，但变量不存在

**涉及文件**：
- `rmk-macro/src/codegen/matrix.rs` — 零矩阵守卫
- `rmk-macro/src/codegen/orchestrator.rs` — 零矩阵守卫
- `rmk-macro/src/codegen/entry.rs` — 条件跳过 `matrix`（与步骤 3 同步处理）

**修改**（当 `split.central.rows == 0 && split.central.cols == 0` 时）：
1. `expand_matrix_config()` 返回 `quote! {}`（空）
2. `expand_matrix_and_keyboard_init()` 仅返回 `Keyboard::new(&keymap)`，无矩阵
3. 入口任务列表中无 `matrix`，无矩阵扫描任务
4. Keyboard 任务仍然运行（事件驱动，通过 channel 从 PeripheralManager 接收）

**可行性（已验证）**：
- `Keyboard::new(&keymap)` 在 `keyboard.rs:256` — 仅接受 `&RefCell<KeyMap>`，无矩阵依赖
- `Keyboard::run()` 在 `keyboard.rs:156-175` — 事件驱动，通过 `keyboard_event_subscriber.receive()`
- `timer: [[None; 0]; 0]` 是有效的零大小数组（ZST）

**验证计划**：

| # | 命令 | 测试内容 |
|---|---|---|
| V1 | `cargo check --manifest-path rmk-macro/Cargo.toml` | 宏 crate 含零矩阵守卫编译通过 |
| V2 | 步骤 7 示例构建 | 完整集成测试 |

**成功标准**：零矩阵 central 不生成矩阵代码。Keyboard 任务仍然生成。

---

### 步骤 5：Gazell ISR 桥接代码生成

**问题**：Gazell 需要 ISR 桥接（RADIO、TIMER2、EGU0_SWI0）而非 BLE nrf-sdc。当前代码生成始终为 nRF52 发射 BLE 代码，且在 TOML 无 `[ble]` 时 **panic**（阻塞 #1、#2）。

**关键 Panic 点**：
- `bind_interrupt_default()` 在 `bind_interrupt.rs:100` — 无 `[ble]` 时 `communication.get_ble_config().unwrap()`
- `expand_bind_interrupt_for_split_peripheral()` 在 `peripheral.rs:77` — 同样问题

**涉及文件**：
- `rmk-macro/src/codegen/chip/bind_interrupt.rs` — 在 `bind_interrupt_default()` 中添加 Gazell 路径
- `rmk-macro/src/codegen/split/peripheral.rs` — 在 `expand_bind_interrupt_for_split_peripheral()` 中添加 Gazell 路径

**Central 侧修改**（`bind_interrupt_default`，nRF52 路径）：

在访问 ble_config 之前检查 `split_config.connection == "gazell"`。当为 Gazell 时：

1. 生成 ISR 桥接（而非 `bind_interrupts!`）：
   ```rust
   extern "C" { fn RADIO_IRQHandler(); fn TIMER2_IRQHandler(); fn SWI0_EGU0_IRQHandler(); }
   #[pac::interrupt] fn RADIO() { unsafe { RADIO_IRQHandler() } }
   #[pac::interrupt] fn TIMER2() { unsafe { TIMER2_IRQHandler() } }
   #[pac::interrupt] fn EGU0_SWI0() { unsafe { SWI0_EGU0_IRQHandler() } }
   ```
2. 生成中断优先级设置（而非 `mpsl_task`/`build_sdc`）：
   ```rust
   interrupt::RADIO.set_priority(Priority::P0);
   interrupt::TIMER2.set_priority(Priority::P0);
   interrupt::EGU0_SWI0.set_priority(Priority::P1);
   ```
3. 保留 USB 中断绑定（dongle 使用 USB HID）
4. 无 `nrf_sdc` 依赖 — Gazell 不使用 Softdevice Controller

**Peripheral 侧修改**（`expand_bind_interrupt_for_split_peripheral`，nRF52 路径）：

同样模式 — 先检查连接类型，发射 Gazell ISR 桥接而非 BLE nrf-sdc 代码。

**检测方法**：从 `BoardConfig::Split(split_config)` -> `split_config.connection` 提取。两个函数中已有 `board` 变量。

**验证计划**：

| # | 命令 | 测试内容 |
|---|---|---|
| V1 | `cargo check --manifest-path rmk-macro/Cargo.toml` | 宏 crate 编译 |
| V2 | 步骤 7 示例构建 | Gazell 不生成 `nrf_sdc` 代码 |

**成功标准**：宏 crate 编译。Gazell 不触发 `nrf_sdc` 代码路径。

---

### 步骤 6：BatteryState 特性门修复

**问题**：`SplitMessage::BatteryState` 仅在 `#[cfg(feature = "_ble")]` 门控下。Gazell peripheral 也有电池，需要上报电池状态。

**所有受影响位置**：
- `rmk/src/split/mod.rs:4-5` — `use crate::event::BatteryStateEvent` 导入
- `rmk/src/split/mod.rs:56-57` — `BatteryState(BatteryStateEvent)` 变体
- `rmk/src/split/driver.rs:15-16` — `PeripheralBatteryEvent` 导入
- `rmk/src/split/driver.rs:243-247` — `SplitMessage::BatteryState` 匹配臂
- `rmk/src/split/peripheral.rs:12-16` — 导入（`BatteryStateEvent`、`ChargingStateEvent` 等）
- `rmk/src/split/peripheral.rs:88-93` — 订阅者创建（`charging_state_sub`、`battery_sub`）
- `rmk/src/split/peripheral.rs:99-108` — `select_biased_with_feature!` 宏中的 `with_feature("_ble")`

**修改**：将 `#[cfg(feature = "_ble")]` 替换为 `#[cfg(any(feature = "_ble", feature = "wireless_gazell"))]`。对于 `select_biased_with_feature!` 宏调用，将 `with_feature("_ble")` 替换为适当的双特性门控（检查宏是否支持，否则回退到 `cfg_attr`）。

**验证计划**：

| # | 命令 | 测试内容 |
|---|---|---|
| V1 | `cargo check --manifest-path rmk/Cargo.toml --features "split,wireless_gazell"` | Gazell 下 BatteryState 可用 |
| V2 | `cargo check --manifest-path rmk/Cargo.toml --features "split,_nrf_ble"` | BLE 回归 |
| V3 | `cargo check --manifest-path rmk/Cargo.toml --features "split"` | 串口分体（无无线） |
| V4 | `cargo test --manifest-path rmk/Cargo.toml --lib -- split` | 单元测试 |

**成功标准**：4 条命令全部通过。

---

### 步骤 7：Gazell 分体示例（集成测试）

**问题**：需要一个端到端示例来综合验证前面所有步骤（代码生成、配置、ISR 桥接、零矩阵、多管道）。

**文件**：新建 `examples/use_config/nrf52840_gazell_split/`，参照 BLE 分体示例模式。

**结构**（镜像 `nrf52840_ble_split/`）：
```
nrf52840_gazell_split/
+-- keyboard.toml
+-- Cargo.toml           # rmk 含 wireless_gazell_nrf52840 特性
+-- .cargo/config.toml
+-- memory.x
+-- src/
    +-- central.rs       # #[rmk_central] mod keyboard_central {}
    +-- peripheral.rs    # #[rmk_peripheral(id = 0)] mod keyboard_peripheral {}
```

**keyboard.toml 要点**：
- `chip = "nrf52840"`，`usb_enable = true`（dongle 有 USB）
- `connection = "gazell"`
- `[split.central]` rows=0, cols=0（dongle 无按键）
- 无 `[split.central.matrix]` 段（省略，通过 `#[serde(default)]` 使用 Default）
- 两个 `[[split.peripheral]]`，`gazell_pipe = 0` / `gazell_pipe = 1`
- 无 `[ble]` 段（仅 Gazell）

**Cargo.toml 关键依赖**：
- `rmk` 含 `wireless_gazell_nrf52840` 特性（包含 `split`）
- `rmk-gazell-sys` 含 `nrf52840` 特性
- `embassy-nrf`、`cortex-m`、`defmt`（无 `nrf-sdc`、无 `bt-hci`）

**验证计划**（步骤 1-6 的集成测试）：

| # | 命令 | 测试内容 |
|---|---|---|
| V1 | `cd examples/use_config/nrf52840_gazell_split && cargo build --release --bin central && cd -` | Central ARM 构建（验证步骤 1,3,4,5） |
| V2 | `cd examples/use_config/nrf52840_gazell_split && cargo build --release --bin peripheral && cd -` | Peripheral ARM 构建（验证步骤 3,5,6） |

**成功标准**：两个 ARM 构建成功，生成有效 ELF 二进制。

---

### 步骤 8：硬件验证

**问题**：软件编译通过不能保证 RF 通信端到端工作。

**测试矩阵**：

| 测试 | 操作 | 预期结果 |
|------|------|----------|
| V1 | 按左手 'A' 键 | PC 上出现 'A' |
| V2 | 按右手 'L' 键 | PC 上出现 'L' |
| V3 | 两手同时按键 | 两个按键都注册 |
| V4 | 左手按住层切换键，右手按键 | 触发层切换后的动作 |
| V5 | 从 PC 切换 CapsLock | 两个 peripheral 的 LED 响应 |
| V6 | 拔出 dongle USB | 两个 peripheral 检测到断连 |

**成功标准**：6 项测试全部通过。

---

## 5. 依赖关系图

```
步骤 0 (合并上游) -- 已完成
 |-- 步骤 1 (多管道解复用)        <-- 运行时，独立
 |-- 步骤 2 (rmk-config)          <-- 配置，独立
 +-- 步骤 6 (电池状态门修复)      <-- 运行时，独立
       |
步骤 3 (代码生成) <-- 依赖 1, 2
步骤 4 (零矩阵) <-- 依赖 3
步骤 5 (ISR 代码生成) <-- 依赖 3
       |
步骤 7 (示例) <-- 依赖 3, 4, 5, 6  [集成测试]
       |
步骤 8 (硬件验证) <-- 依赖 7
```

**可并行**：步骤 1、2、6 独立。步骤 4、5 独立（步骤 3 之后）。

## 6. 关键源代码引用

| 声明 | 来源 |
|---|---|
| GazellCentralDriver 逐实例调用 `gz_recv()` | `rmk/src/split/gazell.rs:237-290` |
| SplitReader/SplitWriter trait | `rmk/src/split/driver.rs:30-37` |
| PeripheralManager 泛型于传输层 | `rmk/src/split/driver.rs:46-57` |
| Embassy Channel 静态模式 | `rmk/src/channel.rs:17` |
| BatteryState 仅在 `_ble` 门控下 | `rmk/src/split/mod.rs:4-5,56-57` |
| 代码生成入口按连接字符串分发 | `rmk-macro/src/codegen/entry.rs:103,129,164` |
| Central 代码生成通信配置 | `rmk-macro/src/codegen/split/central.rs:17-38` |
| BLE central 的 ISR 桥接代码生成 (nRF52) | `rmk-macro/src/codegen/chip/bind_interrupt.rs:87-214` |
| `get_ble_config().unwrap()` 无 `[ble]` 时 panic | `bind_interrupt.rs:100`、`peripheral.rs:77` |
| 矩阵引脚代码生成 `.unwrap()` panic | `rmk-macro/src/codegen/matrix.rs:60` |
| `Keyboard::new()` 仅接受 `&keymap` | `rmk/src/keyboard.rs:256` |
| `Keyboard::run()` 事件驱动 | `rmk/src/keyboard.rs:156-175` |
| `run_rmk()` 纯 USB 路径 | `rmk/src/lib.rs:272` |
| `wireless_gazell_nrf52840` 特性包含 `split` | `rmk/Cargo.toml:231` |

## 7. Phase 4 准备 & ESB 可移植性

### 架构分层

```
[芯片专有 -- 移植到 ESB/ESP-NOW 时需重写]
  rmk-gazell-sys              FFI crate
  GazellPeripheralDriver      直接 FFI 调用 (gz_send, gz_get_ack_payload)
  GazellCentralHub (P3)       直接 FFI 调用 (gz_recv, gz_set_ack_payload)

[协议无关 -- 可复用于任何多管道无线]
  PipeDriver (P3)             基于 channel 的 SplitReader+SplitWriter（无 FFI）
  static PIPE_RX/PIPE_TX      embassy Channel 数组

[传输无关 -- 完全可复用]
  SplitReader/SplitWriter     trait (rmk/src/split/driver.rs:30-37)
  PeripheralManager           泛型于 T: SplitReader+SplitWriter
  SplitPeripheral             泛型于 S: SplitWriter+SplitReader
  Keyboard / 事件系统         无传输耦合
```

### 热切换准备

Hub 使用 `select(hub_loop, GAZELL_SHUTDOWN.wait())` — Phase 3 永不触发信号，Phase 4 触发以切换无线模式（BLE <-> Gazell）。

### ESB / 其他芯片可移植性

重写边界清晰：FFI + PeripheralDriver + CentralHub（底层）。PipeDriver 以上全部可原样复用。

## 8. 已解决问题

| 问题 | 答案 | 来源 |
|---|---|---|
| 步骤 0 合并状态 | 已合并 | `git merge-base --is-ancestor main HEAD` |
| MAX_GAZELL_PIPES | 8（硬件最大），运行时 `num_pipes` 参数 | 用户要求 |
| 旧 use_rust 示例 | 迁移到 hub 架构 | 用户要求 |
| 零矩阵可行性 | Keyboard 事件驱动，无矩阵依赖 | `keyboard.rs:156,256` |
| TOML 无 `[split.central.matrix]` | 添加 `#[serde(default)]` | `rmk-config/src/lib.rs:804` |
| Dongle USB（无 BLE） | `run_rmk()` 纯 USB 路径可用 | `rmk/src/lib.rs:272` |
| `get_ble_config().unwrap()` panic | 必须在 BLE 代码之前为 Gazell 短路 | `bind_interrupt.rs:100`、`peripheral.rs:77` |
