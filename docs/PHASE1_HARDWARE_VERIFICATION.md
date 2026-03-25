# Phase 1 硬件验证文档

**文档版本**: v1.0
**目标平台**: nRF52840 (Charybdis 键盘 + E104-BT5040U Dongle)
**预计耗时**: 30-60 分钟
**难度**: 简单 - 中等

---

## 📋 前置条件

### 硬件需求
- [ ] Charybdis nRF52840 键盘（已有）
- [ ] E104-BT5040U nRF52840 Dongle（已有）
- [ ] nrf programmer（已连接 dongle）
- [ ] USB 线（用于键盘刷写）
- [ ] RTT 日志查看工具（或串口工具 + defmt）

### 软件环境
- [ ] Rust + cargo
- [ ] ARM GCC 工具链 (`arm-none-eabi-gcc`)
- [ ] Nordic SDK v17.1.0+ 已安装
- [ ] `NRF5_SDK_PATH` 环境变量已设置

### 验证前检查
```bash
# 检查环境
echo $NRF5_SDK_PATH
arm-none-eabi-gcc --version
rustc --version
cargo --version

# 检查分支
git branch -v
git log --oneline -3
```

---

## 🔧 编译步骤

### Step 1: 编译键盘示例（Device Mode）

```bash
cd /home/qlg/wkspaces/rmk_q/rmk/examples/use_rust/nrf52840_2g4

# 清理旧编译
cargo clean

# 编译 release 版本（最终刷写）
cargo build --release

# 验证输出
ls -lh target/thumbv7em-none-eabihf/release/rmk-nrf52840-2g4
# 预期: ~25KB ELF 文件
```

**预期输出**:
```
Compiling rmk-gazell-sys v0.1.0 (...)
Compiling rmk v0.8.2 (...)
Finished `release` profile [optimized] target(s) in XX.XXs
```

**如果失败**:
- [ ] 检查 `NRF5_SDK_PATH` 环境变量
- [ ] 运行 `cargo build --verbose` 查看详细错误
- [ ] 检查 ARM GCC 是否已安装

### Step 2: 编译 Dongle 示例（Host Mode）

```bash
cd /home/qlg/wkspaces/rmk_q/rmk/examples/use_rust/nrf52840_dongle

# 清理旧编译
cargo clean

# 编译 release 版本
cargo build --release

# 验证输出
ls -lh target/thumbv7em-none-eabihf/release/rmk-nrf52840-dongle
# 预期: ~25KB ELF 文件
```

**验证编译大小**:
```bash
# 两个二进制都应该在 20-30KB 范围
ls -lh target/thumbv7em-none-eabihf/release/rmk-nrf52840-*
```

---

## 🔌 刷写硬件

### Dongle 刷写（使用 nrf programmer）

```bash
# 获取文件路径
DONGLE_BIN="/home/qlg/wkspaces/rmk_q/rmk/examples/use_rust/nrf52840_dongle/target/thumbv7em-none-eabihf/release/rmk-nrf52840-dongle"

# 使用 nrfjprog（示例）
nrfjprog --eraseall
nrfjprog --program $DONGLE_BIN --sectorerase --verify
nrfjprog --reset

# 或使用 pyocd（如果可用）
pyocd erase -t nrf52840
pyocd load $DONGLE_BIN
pyocd reset
```

**刷写验证**:
- [ ] 设备无错误消息
- [ ] 连接仍稳定

### 键盘刷写

```bash
# 获取文件路径
KEYBOARD_BIN="/home/qlg/wkspaces/rmk_q/rmk/examples/use_rust/nrf52840_2g4/target/thumbv7em-none-eabihf/release/rmk-nrf52840-2g4"

# 使用你的刷写工具
# 方式 1: nrfjprog
nrfjprog --eraseall
nrfjprog --program $KEYBOARD_BIN --sectorerase --verify
nrfjprog --reset

# 方式 2: J-Link（如果使用 J-Link）
JLinkExe -Device NRF52840_XXAA -if SWD -speed 4000 \
  -ExitOnError 1 \
  -CommandFile <(echo -e "loadfile $KEYBOARD_BIN\nr\nq")
```

**刷写验证**:
- [ ] 无错误消息
- [ ] 键盘已收到新固件

---

## 🚀 运行验证

### Phase 1: 启动 Dongle（接收端）

**步骤**:
1. 连接 nrf programmer 到 dongle
2. 打开 RTT 日志查看工具
   ```bash
   # 使用 defmt-print（推荐）
   cargo install probe-run
   probe-run --chip nRF52840 /path/to/elf

   # 或使用 nrfjprog RTT
   nrfjprog --rttclient
   ```
3. 观察日志

**预期日志**:
```
[Info] RMK Dongle (Gazell Host) starting...
[Info] Gazell initialized
[Info] Host mode set
[Info] Listening for 2.4G packets...
```

**检查清单**:
- [ ] 日志输出正常
- [ ] 无初始化错误
- [ ] 已进入接收循环

### Phase 2: 启动键盘（发送端）

**步骤**:
1. 移除 dongle 连接（保持电源）
2. 连接键盘到 USB 刷写器（或 RTT 端口）
3. 上电
4. 打开 RTT 查看键盘日志

**预期日志**:
```
[Info] RMK nRF52840 2.4G Keyboard starting...
[Info] Initializing Gazell 2.4G wireless...
[Info] Gazell initialized successfully
[Info] Gazell set to device mode (transmitter)
[Info] Keyboard ready! Starting test transmission...
[Info] Sent test packet #0 successfully
[Info] Sent test packet #1 successfully
[Info] Sent test packet #2 successfully
...
```

**检查清单**:
- [ ] Gazell 初始化成功
- [ ] Device 模式设置成功
- [ ] 数据包发送成功（无 `Send failed` 错误）
- [ ] 计数器递增

### Phase 3: 在 Dongle 上验证接收

**预期日志**（在 Dongle 终端）:
```
[#1] RX 3 bytes: AA BB 00
[#2] RX 3 bytes: AA BB 01
[#3] RX 3 bytes: AA BB 02
[#4] RX 3 bytes: AA BB 03
...
```

**检查清单**:
- [ ] 收到数据包
- [ ] 计数器值与键盘发送的一致
- [ ] 数据包格式正确 (`AA BB counter`)

---

## 📊 验收标准

### 基本通信验证 ✅ （必须通过）

| 检查项 | 预期结果 | 状态 |
|--------|--------|------|
| **Dongle 初始化** | 无错误 | [ ] |
| **键盘初始化** | 无错误 | [ ] |
| **数据包接收** | 能收到数据 | [ ] |
| **计数器连续** | 0,1,2,3... | [ ] |
| **数据格式** | `AA BB counter` | [ ] |
| **通信时间** | < 100ms 延迟 | [ ] |

### 可靠性测试 ✅ （建议通过）

| 检查项 | 标准 | 状态 |
|--------|------|------|
| **成功率** | > 95% | [ ] |
| **运行时间** | > 5 分钟无错误 | [ ] |
| **近距离** | < 1m 完美 | [ ] |
| **中距离** | 5-10m 无丢包 | [ ] |
| **远距离** | 20m+ 可接收 | [ ] |

### 故障恢复 ⭐ （可选）

| 检查项 | 操作 | 预期 |
|--------|------|------|
| **短暂中断** | 遮挡 1-2s | 恢复接收 |
| **电源重启** | 键盘重启 | 自动重连 |
| **距离移动** | 缓慢增加距离 | 延迟增加但保持连接 |

---

## 🐛 故障排查

### 问题 1: Dongle 无日志输出

**原因可能**:
- nrf programmer 连接断开
- RTT 配置错误
- Dongle 固件未刷写

**解决步骤**:
1. 检查 nrf programmer 连接
2. 重新刷写 Dongle
3. 尝试不同的日志工具
4. 检查 Dongle 电源指示灯

### 问题 2: 键盘初始化失败

**日志**:
```
[Error] Gazell init failed: HardwareError
```

**原因可能**:
- SDK 路径不正确
- 编译时未使用 nrf52840 特性
- 硬件故障

**解决步骤**:
```bash
# 检查编译命令
cd examples/use_rust/nrf52840_2g4
cargo build --verbose --release 2>&1 | grep -i error

# 检查链接
cargo build --release 2>&1 | grep -E "(undefined|link)"
```

### 问题 3: 键盘可以发送但 Dongle 无接收

**原因可能**:
- 频道不匹配（不同的 GazellConfig）
- 距离太远
- 无线干扰

**解决步骤**:
1. 确认两端都使用 `low_latency()` 预设
2. 靠近硬件（< 1m）
3. 移到空旷地方（减少干扰）
4. 检查频道设置（两端应都是 channel 4）

### 问题 4: 间歇性丢包

**原因可能**:
- 2.4GHz 干扰（WiFi、蓝牙等）
- 距离过远
- 硬件噪声

**解决步骤**:
1. 切换频道（修改 `GazellConfig::channel`）
2. 靠近硬件
3. 尝试 `long_range()` 预设（更强的重试）

### 问题 5: 计数器不连续（丢包）

**示例**:
```
[#1] RX ... 00
[#2] RX ... 01
[#3] RX ... 03    ← 丢失 02
[#4] RX ... 04
```

**分析**:
- 正常现象（15-20% 丢包率可接受）
- 记录丢包率用于后续优化
- 如果 > 50% 丢包，检查问题 4

---

## 📈 性能测试

### 丢包率测试

```bash
# 运行键盘 5 分钟
# 记录 Dongle 端日志到文件

# 分析丢包
grep "RX" dongle_log.txt | wc -l        # 收到总数
grep "counter" keyboard_log.txt | wc -l # 发送总数

# 计算丢包率
python3 << 'EOF'
received = $(grep "RX" dongle_log.txt | wc -l)
sent = $(grep "counter" keyboard_log.txt | wc -l)
loss = (sent - received) / sent * 100
print(f"发送: {sent}, 接收: {received}, 丢包率: {loss:.1f}%")
EOF
```

### 延迟测试

```bash
# 用两个终端分别显示键盘和 Dongle 日志
# 观察时间戳差异（如果有）
# 通常应 < 100ms

# 键盘日志样例时间: [00:00:05.123]
# Dongle 日志样例时间: [00:00:05.185]
# 延迟 = 185 - 123 = 62ms ✅
```

### 功耗测试（可选）

```bash
# 测量 Dongle 功耗（接收循环）
# 使用万用表或 power meter

# 预期: 50-100mA（active RX）
# 如果 > 200mA，检查初始化问题
```

---

## ✅ 验收流程

### 快速验收 (15 分钟)

1. [ ] 编译两个示例成功
2. [ ] 刷写两个设备成功
3. [ ] Dongle 显示初始化日志
4. [ ] 键盘显示发送日志
5. [ ] Dongle 收到至少 10 个数据包
6. [ ] 数据包计数器递增

**签字**: ________________  **日期**: ________

### 完整验收 (60 分钟)

1. [ ] 快速验收全部通过
2. [ ] 运行稳定 > 5 分钟
3. [ ] 测试距离 > 10m
4. [ ] 丢包率 < 20%
5. [ ] 无通信错误日志
6. [ ] 故障恢复正常

**签字**: ________________  **日期**: ________

---

## 📝 测试记录

### 测试 1: 基础通信

**日期**: __________
**环境**: 距离: ____m, 温度: ____°C

| 时间 | 事件 | 结果 |
|------|------|------|
| 00:00 | Dongle 启动 | [ ] OK |
| 00:05 | 键盘启动 | [ ] OK |
| 00:10 | 开始接收 | [ ] OK |
| 05:00 | 5 分钟运行 | [ ] OK |

**收到的包数**: _____
**发送的包数**: _____
**丢包率**: _____%
**备注**: ________________

### 测试 2: 距离测试

**日期**: __________

| 距离 | 可接收 | 丢包率 | 延迟 | 备注 |
|------|--------|--------|------|------|
| 1m   | [ ] Y/N | ___% | __ms | |
| 5m   | [ ] Y/N | ___% | __ms | |
| 10m  | [ ] Y/N | ___% | __ms | |
| 20m  | [ ] Y/N | ___% | __ms | |

### 测试 3: 故障恢复

**日期**: __________

| 故障场景 | 表现 | 恢复时间 | 备注 |
|---------|------|---------|------|
| 遮挡 1s  | [ ] OK | __ms | |
| 重启键盘 | [ ] OK | __s | |
| 移至远处 | [ ] OK | __s | |

---

## 🎓 学到的知识点

### 如果测试通过，你现在知道：
- ✅ Gazell FFI 正确链接到 Nordic SDK
- ✅ Device 和 Host 模式都能正常工作
- ✅ 2.4GHz 射频链路可靠
- ✅ 键盘和 Dongle 硬件兼容
- ✅ 测试包收发无问题

### 后续优化方向：
- 📝 Phase 2：传输结构化 SplitMessage 数据
- 📝 Phase 2：支持 ACK payload 双向通信
- 📝 Phase 3：异步包装减少阻塞
- 📝 Phase 3：功耗优化和心跳调度
- 📝 Phase 4：支持多个外设（pipe 路由）

---

## 📞 需要帮助？

**常见问题**:
- 编译错误 → 检查 `NRF5_SDK_PATH`
- 刷写失败 → 检查 nrf programmer 连接
- 无日志 → 检查 RTT 工具配置
- 无通信 → 检查频道和距离

**文档**:
- 快速恢复: `docs/QUICK_RESUME.md`
- FFI 设计: `docs/GAZELL_FFI_PLAN.md`
- 完成报告: `docs/PHASE1_COMPLETION_REPORT.md`

---

**状态**: 🔵 待硬件验证

