# ELink 协议使用指南

## 概述

ELink 协议是一个可选的增强型通信协议，用于 RMK split 键盘通信。它提供了 CRC 校验、优先级支持、多设备支持等功能。

## 特性

- ✅ **一键配置**: 通过 feature flag 控制，关闭时完全排除编译
- ✅ **节省固件大小**: 未启用时，ELink 代码完全不会被编译
- ✅ **向后兼容**: 可以与现有的 Serial/BLE 驱动共存
- ✅ **性能可接受**: CPU 开销 < 0.5%，延迟增加 < 20 µs/消息

## 启用 ELink

### 方法 1: 在 Cargo.toml 中启用

在你的键盘项目 `Cargo.toml` 中添加 `elink` feature：

```toml
[dependencies]
rmk = { path = "...", features = ["split", "elink"] }
```

### 方法 2: 在配置文件中启用

在 `rmk.toml` 配置文件中，可以通过配置选项启用（如果支持的话）。

## 禁用 ELink

### 方法 1: 移除 feature

从 `Cargo.toml` 中移除 `elink` feature：

```toml
[dependencies]
rmk = { path = "...", features = ["split"] }  # 不包含 elink
```

### 方法 2: 使用 default-features = false

```toml
[dependencies]
rmk = { path = "...", default-features = false, features = ["split"] }
```

## 固件大小对比

### 测试方法

1. **编译不带 ELink 的固件**:
   ```bash
   cargo build --release --features split
   ```

2. **编译带 ELink 的固件**:
   ```bash
   cargo build --release --features split,elink
   ```

3. **对比固件大小**:
   ```bash
   # 查看固件大小
   ls -lh target/thumbv7em-none-eabihf/release/*.elf
   # 或使用 size 命令
   cargo size --release --features split
   cargo size --release --features split,elink
   ```

### 预期结果

- **代码大小增加**: 约 2-5 KB（取决于优化级别）
- **RAM 使用增加**: 约 1-2 KB（缓冲区）

## 使用示例

### 基本使用

ELink 驱动会自动替换默认的 Serial/BLE 驱动。你不需要修改代码，只需要启用 feature。

### 高级配置

如果需要自定义设备 ID：

```rust
use rmk::split::elink::run_elink_peripheral_manager;

// 运行 ELink 外设管理器
// device_class: 0x1 = Keyboard
// device_address: 0 = Central, 1 = Peripheral
// sub_type: 0x0 = Central, 0x1 = Peripheral
run_elink_peripheral_manager::<3, 3, 0, 0>(
    0,  // peripheral id
    serial_port,
    0x1, // device_class: Keyboard
    1,   // device_address
    0x1, // sub_type: Peripheral
).await;
```

## 性能对比

基于基准测试结果：

| 指标 | ELink | Postcard+COBS | 差异 |
|------|-------|---------------|------|
| 编码时间 | 10.08 µs | 1.99 µs | +8.09 µs |
| 解码时间 | 12.43 µs | 0.68 µs | +11.75 µs |
| 总延迟 | 22.59 µs | 2.76 µs | +19.83 µs |
| CPU 占用 | < 0.5% | < 0.1% | +0.4% |
| 成功率 | 100% | 100% | 相同 |

**结论**: 对于键盘应用（10-100 消息/秒），性能开销完全可以接受。

## 适用场景

### 推荐使用 ELink

- ✅ 使用 BLE 通信（CRC 校验很重要）
- ✅ 有多个 addon 设备（轨迹球、触摸板、屏幕等）
- ✅ 需要优先级支持（关键消息优先）
- ✅ 对可靠性要求高

### 可以不使用 ELink

- ⚠️ 简单的 split 键盘（只有左右两部分）
- ⚠️ 使用串口通信（可靠性已经很好）
- ⚠️ 对固件大小非常敏感

## 故障排除

### 编译错误

如果遇到编译错误，检查：

1. **依赖版本**: 确保 `elink-core` 和 `elink-rmk-adapter` 版本兼容
2. **Feature 配置**: 确保 `elink` feature 正确启用
3. **循环依赖**: 如果遇到循环依赖错误，确保没有在 `elink-rmk-adapter` 中引用 `rmk`

### 运行时错误

如果遇到运行时错误：

1. **检查设备 ID**: 确保 Central 和 Peripheral 使用不同的 device_address
2. **检查缓冲区**: 如果遇到 BufferTooSmall 错误，可能需要增加缓冲区大小
3. **检查 CRC**: 如果遇到 CRC 错误，可能是传输问题

## 更多信息

- 性能分析: 见 `ELINK_PROTOCOL_EVALUATION.md`
- 集成计划: 见 `ELINK_INTEGRATION_PLAN.md`
- ELink 协议文档: 见 `elink-protocol/` 目录
