# BLE / Gazell 2.4G 热切换架构分析

> 日期: 2026-03-14
> 背景: Phase 3 多管道 Gazell 分体已完成（Steps 1-7），待硬件验证。
> 目的: 评估当前实现对未来 BLE+Gazell 热切换及配置同步的影响。

## 1. 当前架构

```
编译时互斥 (split/mod.rs:22):

  compile_error!("_ble 和 wireless_gazell 互斥")

  BLE 路径:    entry.rs → ble/central.rs    (nrf-sdc/MPSL 协议栈)
  Gazell 路径: entry.rs → gazell.rs         (Nordic SDK FFI)
  串口路径:    entry.rs → serial/           (UART)

  三选一，编译期确定，不可运行时切换。
```

## 2. 射频硬件冲突

BLE 和 Gazell **不能同时运行** — 它们共用 nRF52 的 RADIO 外设。

| 资源 | BLE (nrf-sdc/MPSL) | Gazell (Nordic SDK) | 冲突 |
|------|-------------------|---------------------|------|
| RADIO IRQ | `mpsl::HighPrioInterruptHandler` | ISR 桥接 → `RADIO_IRQHandler` | **同一中断，不同处理函数** |
| EGU0_SWI0 | `mpsl::LowPrioInterruptHandler` | ISR 桥接 → `SWI0_EGU0_IRQHandler` | **同一中断** |
| TIMER0 | MPSL 管理 | 未使用 | 无冲突 |
| TIMER2 | 未使用 | ISR 桥接 → `TIMER2_IRQHandler` | 无冲突 |

**结论**: 分时复用可行 — 停止一个协议栈，释放射频资源，启动另一个。预计切换延迟: 50-200ms。

## 3. 当前设计的有利方面

以下设计选择对未来热切换**有利**:

| 设计 | 位置 | 好处 |
|------|------|------|
| `SplitReader` / `SplitWriter` trait | `driver.rs:30-37` | `PeripheralManager` 与传输层无关 |
| `PeripheralManager<T>` 泛型 | `driver.rs:46-57` | 任何实现了 Reader+Writer 的 T 都可以用 |
| 共享 `SplitMessage` 枚举 | `mod.rs:35-58` | BLE 和 Gazell 使用相同的消息格式 |
| `PipeDriver` channel 解耦 | `gazell.rs` | Hub ↔ manager 通过 channel 通信；停止 hub 不影响 manager 状态 |
| `DummyMatrix` 零矩阵中央端 | `matrix.rs` | Dongle 架构与传输层无关 |

## 4. 热切换的阻碍因素

| 阻碍 | 位置 | 问题 | 需要的改动 |
|------|------|------|-----------|
| `compile_error!` 守卫 | `split/mod.rs:22-26` | BLE + Gazell 不能共存于同一二进制 | 移除；改用运行时 `AtomicBool` 互斥 |
| 函数签名上的 `#[cfg]` | `central.rs:29-39` | BLE 参数 vs Gazell 参数在编译期选择 | 统一签名或用 enum 包装 |
| if-else 代码生成链 | `entry.rs:103/163` | 只生成一种连接类型 | 生成两种共存；运行时激活 |
| 中断绑定 | `bind_interrupt.rs:87-131` vs `228-261` | 同一 IRQ → 不同 handler，编译期确定 | 间接跳转表或运行时重绑定 |
| Hub 是无限循环 (`!`) | `run_gazell_central_hub` | 无法优雅停止 | 添加 cancellation token / shutdown 信号 |
| `gz_init_default(1)` 在代码生成中 | `split/central.rs` | 启动时无条件初始化 Gazell | 延迟到运行时 / 条件初始化 |

## 5. SplitMessage — 配置同步缺口

当前 variant（共 9 个）:

```
Peripheral → Central:  Key, Touchpad, Pointing, BatteryState
Central → Peripheral:  LedState, ConnectionState, KeyboardIndicator, Layer
BLE 相关（未 gate）:   Address, ClearPeer
```

配置同步需要但缺少的:

| Variant | 方向 | 用途 |
|---------|------|------|
| `KeymapSync(layer, row, col, action)` | Central → Peripheral | 传播 Vial 改键变更 |
| `TransportSwitch(mode)` | Central → Peripheral | 协调 BLE/Gazell 模式切换 |
| `ConfigQuery` / `ConfigResponse` | 双向 | 重连后重新同步配置 |

**非阻碍性问题** — `SplitMessage` 是 enum，添加 variant 是增量的。当前设计不会阻止后续扩展。

## 6. Storage Feature Gate

```rust
// driver.rs:7-8 — 仅在 storage 和 _ble 同时启用时可用
#[cfg(all(feature = "storage", feature = "_ble"))]
use {FLASH_CHANNEL, PeerAddress, FlashOperationMessage};
```

Gazell dongle 当前 `storage.enabled = false`。要支持 dongle 上的 Vial 键映射持久化，需要将 gate 从 `storage + _ble` 放宽为仅 `storage`。

## 7. 可行的切换架构

### 方案 A: 分时射频复用（推荐）

```
                  ┌── Gazell 模式 ──┐
Peripheral ──────→│  RADIO/TIMER2   │──────→ Dongle (USB HID → PC)
                  │  (2.4GHz, 快速) │
                  └─────────────────┘
                       ↕ 切换 (~100ms)
                  ┌─── BLE 模式 ────┐
Peripheral ──────→│  RADIO/MPSL     │──────→ 手机 / 平板
                  │  (BLE, 标准)    │
                  └─────────────────┘
```

- 停止当前协议栈 → 释放射频 → 初始化新协议栈
- 当前 `SplitReader/Writer` + `PeripheralManager` 可直接复用
- 主要工作: 代码生成输出双模初始化，运行时切换逻辑

### 方案 B: 双二进制 + Bootloader 切换

- 两个固件镜像，通过 DFU 切换
- 简单但慢（秒级），用户体验差

### 方案 C: MPSL Timeslot API

- Nordic MPSL 支持 BLE 和私有协议共享射频
- 需要将 Gazell 重写为 MPSL timeslot 客户端
- 工作量极大，不推荐

## 8. 可提前做的准备性改动

降低后续热切换工作量的可选改动:

| 改动 | 优先级 | 工作量 | 状态 |
|------|--------|--------|------|
| 为 hub + peripheral + pipe manager 添加 cancel token | 中 | 小 | ✅ 完成（poison pill via `Option<SplitMessage>`） |
| 放宽 storage gate 为 `#[cfg(feature = "storage")]` | 中 | 小 | ✅ 不需要（已正常工作） |
| 移除 `compile_error!` → 运行时守卫 | 低 | **中** | ⬜ 需要函数签名重构（见下方说明） |
| 统一 `central.rs` / `peripheral.rs` 函数签名 | 低 | 中 | ⬜ 必须和 compile_error! 移除一起做 |
| 在 `entry.rs` 生成双模代码 | 低 | 大 | ⬜ 热切换核心基础设施 |

**关于 `compile_error!` 移除**（2026-03-25）: 不能简单删除守卫。`peripheral.rs:37-67` 和 `central.rs:25-58` 在泛型参数和函数参数上使用 `#[cfg(feature = "_ble")]` 和 `#[cfg(feature = "wireless_gazell")]`。两个 feature 同时开启时，所有参数都存在，两个代码路径会顺序执行而非互斥。需要重构为 enum 分发。

## 9. 总结

**运行时抽象层**（trait、PeripheralManager、SplitMessage、channel）: 对热切换**友好**。传输层是可插拔的。

**主要障碍**: 代码生成层面的编译时硬编码（中断绑定和射频初始化）。需要"双模初始化 + 运行时切换"的代码生成路径，而非当前的"编译时选一个"。

**配置同步**: 无架构性阻碍。添加 SplitMessage variant + 放宽 storage feature gate 即可支持。
