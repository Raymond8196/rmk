# Phase 2：通过 Gazell 传输 SplitMessage + 双向通信

> **分支**: `feat/gazell-2g4-verify`
> **状态**: 计划 v10 — 新增 codegen 层（步骤 9）、rmk-config（步骤 10）、dongle USB HID（步骤 11）、Charybdis 集成（步骤 12-13）
> **前置依赖**: Phase 1（最小测试包 TX/RX）— 代码完成，ARM 交叉编译通过

---

## 1. 目标

用真实的 `SplitMessage` 序列化替换 Phase 1 的测试包，
通过 Gazell ACK payload 实现双向通信（central <-> peripheral），
并创建 `GazellSplitDriver` 类型来接入 RMK 现有的 split 架构。

**最终状态**: 目前驱动 BLE 和串口 split 键盘的 `SplitPeripheral::run()` / `PeripheralManager::run()` 循环，
也将可以通过 Gazell 运行。

---

## 2. 协议兼容性声明

- Gazell split 与 BLE/串口 split 共享 **同一个 `SplitMessage` 枚举和 postcard 编码**。不会为 Gazell 添加新的枚举变体。
- Gazell 心跳包是 **driver 层内部机制**（2 字节 `[0xFE, 0xFE]` 标记）。它不会作为 `SplitMessage` 值出现，BLE/串口代码不受影响。单测验证无任何 `SplitMessage` 变体序列化为该标记（见 §5 步骤 8）。
- **键盘和 dongle 必须使用相同版本的 RMK 固件。** 不支持跨版本混用。
- BLE（`_ble`）和 Gazell（`wireless_gazell`）feature **互斥**（使用同一个 nRF52 射频硬件）。通过 `compile_error!` 在编译期强制执行。

### SplitMessage 方向与语义分类

所有 `SplitMessage` 变体的通信方向和语义类型：

| 变体 | 方向 | 语义 | 可覆盖？ | 备注 |
|------|------|------|----------|------|
| `Key(KeyboardEvent)` | peripheral → central | **事件** | 否 — 每次按下/释放都有意义 | 异步发布，有背压 |
| `Touchpad(TouchpadEvent)` | peripheral → central | **事件** | 否 — 每个增量都有意义 | 非阻塞，满则丢弃 |
| `Pointing(PointingEvent)` | peripheral → central | **事件** | 否 — 每个采样都有意义 | 非阻塞，满则丢弃 |
| `BatteryState(BatteryStateEvent)` | peripheral → central | 状态 | 是 | 最新电量值即可 |
| `ConnectionState(bool)` | central → peripheral | 状态 | 是 | 每 3000ms 同步一次 |
| `KeyboardIndicator(u8)` | central → peripheral | 状态 | 是 | 最新指示灯位即可 |
| `Layer(u8)` | central → peripheral | 状态 | 是 | 最新层号即可 |
| `LedState(bool)` | central → peripheral | 状态 | 是 | 最新 LED 状态即可 |
| `ClearPeer` | central → peripheral | **事件** | **否** — 必须执行一次 | 清除配对命令 |
| `Address([u8; 6])` | （未使用） | — | — | 当前代码库无调用 |

**对 Gazell 状态合并策略的影响**：
- 状态型消息（ConnectionState、KeyboardIndicator、Layer、LedState）：可安全合并到 `pending_state`——只需最新值。
- **`ClearPeer` 是事件型**：不能合并到 `pending_state`。`GazellCentralDriver::write()` 特殊处理——先尝试立即 `gz_set_ack_payload` 并重试；失败后存入 `pending_event`，在下次收到包（含心跳）时延迟投递。见 §4b。
- **`pending_event` 覆盖语义**：`pending_event` 是单槽位。如果第二个事件型消息在第一个尚未投递时到来，会覆盖前者。这是安全的，因为 `ClearPeer`（当前唯一的事件型 central→peripheral 消息）是**幂等的**——清除配对执行两次和执行一次效果相同。如果未来添加非幂等的事件型消息，`pending_event` 需要改为队列。

**`SplitMessage::POSTCARD_MAX_SIZE`**：已通过 `SplitMessage` 枚举上的 `#[derive(MaxSize)]` 存在。在 `split/mod.rs:20` 使用：`pub const SPLIT_MESSAGE_MAX_SIZE: usize = SplitMessage::POSTCARD_MAX_SIZE + 4`。

---

## 3. 架构概览

```
键盘 (nRF52840, peripheral / device 模式)
  ┌──────────────────────────────────────────────┐
  │  SplitPeripheral::run()                      │
  │    ├─ read()  ← GazellPeripheralDriver       │
  │    │     检查 ack_buffer 中缓存的 ACK 数据    │
  │    │     如果为空且空闲 > heartbeat_interval  │
  │    │     → 发心跳 →                           │
  │    │     再次检查 ACK 数据                     │
  │    └─ write() → GazellPeripheralDriver        │
  │          postcard::to_slice(SplitMessage)      │
  │          → gz_send(data, len, pipe)            │
  │          → gz_get_ack_payload → 缓存结果       │
  └──────────────────────────────────────────────┘
              │  Gazell 2.4GHz (来自 config 的 pipe)
              ▼
  ┌──────────────────────────────────────────────┐
  │  Dongle (nRF52840, central / host 模式)      │
  │  PeripheralManager::run()                    │
  │    ├─ read()  ← GazellCentralDriver          │
  │    │     轮询 gz_recv()，过滤心跳             │
  │    │     刷新 pending_event/pending_state     │
  │    │     → 返回 (Key, Pointing, Touchpad)     │
  │    └─ write() → GazellCentralDriver           │
  │          状态型：合并到 pending_state          │
  │          事件型：立即 gz_set_ack，BUSY 时      │
  │            存入 pending_event 延迟投递         │
  └──────────────────────────────────────────────┘
              │  USB HID
              ▼
             PC
```

### 关键设计决策

| 决策 | 选择 | 理由 |
|------|------|------|
| 序列化方式 | `postcard::to_slice` / `postcard::from_bytes`（无 COBS） | Gazell 是包帧协议，自带长度信息，不需要流式分帧。BLE split 也用同样的非 COBS 方式。 |
| 最大 payload | 32 bytes（Gazell 硬件限制）[^1] | `SplitMessage` postcard 最大序列化约 13 bytes，有 19 bytes 余量。通过编译期测试强制保证。 |
| Central→Peripheral 通道 | ACK payload（`gz_set_ack_payload`） | Gazell 内建的 host→device 数据机制。非阻塞，搭载在下一个 ACK 上。 |
| Central write() 模型 | 状态合并 + 事件延迟投递 | 状态型消息合并到 `pending_state`。事件型（`ClearPeer`）先尝试立即发送，失败则存入 `pending_event`。两者均在收到任何包（含心跳）时刷新。见 §2 方向表。 |
| ACK payload FIFO 深度 | 3 个包 [^2] | `NRF_GZLL_CONST_FIFO_LENGTH = 3`。状态合并保证最多 1 个排队 ACK payload，远在限制内。 |
| 心跳机制 | Gazell 内部 2 字节标记（`[0xFE, 0xFE]`） | **不修改** `SplitMessage` 枚举。Driver 层自行过滤。无跨协议兼容性影响。单测验证无 `SplitMessage` 变体序列化为此标记。 |
| Peripheral read()/write() 协调 | `last_send_time` + ack_buffer | 单线程 Embassy executor — read() 和 write() 之间无真正并发。write() 设置 last_send_time 并检查 ACK payload。read() 仅在空闲 > `heartbeat_interval_ms` 时发心跳。无需 mutex。 |
| Pipe 编号 | `self.config.pipe`（默认 0） | 当前单键盘对单 dongle。多键盘可用不同 pipe。`recv()` 验证 `out_pipe` 是否匹配预期 pipe。 |
| Feature 互斥 | `compile_error!` 守卫 | 防止误同时启用 `_ble` 和 `wireless_gazell`。 |

---

## 4. 当前本地改动中发现的问题

### 问题 1：Rust FFI 签名与 C shim 不匹配

**文件**: `rmk-gazell-sys/src/lib.rs`

C shim 已更新为：
```c
gz_error_t gz_send(const uint8_t* data, uint8_t len, uint8_t pipe);
gz_error_t gz_recv(uint8_t* out_buf, uint8_t* out_len, uint8_t* out_pipe, uint8_t max_len);
bool       gz_is_ready(uint8_t pipe);
```

Rust 仍声明旧签名（无 `pipe` 参数）。

**影响**: ARM 上链接错误，ABI 不匹配。

### 问题 2：Rust 的 `gz_config_t` 缺少 `pipe` 字段

C 结构体有 `uint8_t pipe` 字段，但 Rust 的 `repr(C)` 结构体没有。
内存布局不匹配 = 传给 C 时产生未定义行为。

### 问题 3：缺少新 C 函数的 Rust 绑定

`gz_set_ack_payload()` 和 `gz_get_ack_payload()` 在 C shim 中已实现，
但没有 Rust 的 `extern` 声明和 stub 函数。

### 问题 4：C 回调中 `ack_payload_length` 类型错误

**文件**: `rmk-gazell-sys/c/gazell_shim.c`，第 47-61 行

`nrf_gzll_fetch_packet_from_rx_fifo()` 期望 `uint32_t*` [^4]，但 `gz_state.ack_payload_length` 是 `uint8_t`。
SDK 向 1 字节字段写入 4 字节 = 栈溢出。

注意：host 端回调没有此问题，因为 `gz_state.rx_length` 本身就是 `uint32_t`。

### 问题 5：`GazellTransport` 方法未适配新 FFI 签名

所有 FFI 调用点仍使用旧签名，缺少 pipe 参数。

---

## 5. 实施步骤

### 步骤 1：修复 C shim `ack_payload_length` 类型问题

**文件**: `rmk-gazell-sys/c/gazell_shim.c`

**修改**: 在 `nrf_gzll_device_tx_success` 回调中使用临时 `uint32_t`：

```c
void nrf_gzll_device_tx_success(uint32_t pipe, nrf_gzll_device_tx_info_t tx_info) {
    if (tx_info.payload_received_in_ack) {
        uint32_t temp_len = MAX_PAYLOAD_LENGTH;
        if (nrf_gzll_fetch_packet_from_rx_fifo(pipe,
                gz_state.ack_payload_buffer,
                &temp_len)) {
            gz_state.ack_payload_length = (uint8_t)temp_len;
            gz_state.ack_payload_ready = true;
        }
    }
    gz_state.tx_success = true;
}
```

**验证命令**:
```bash
cargo build --manifest-path rmk-gazell-sys/Cargo.toml \
  --target thumbv7em-none-eabihf --features nrf52840
```

---

### 步骤 2：修复 Rust FFI 绑定

**文件**: `rmk-gazell-sys/src/lib.rs`

#### 2a. 给 `gz_config_t` 添加 `pipe` 字段

```rust
#[repr(C)]
pub struct gz_config_t {
    // ... 原有字段 ...
    pub pipe: u8,               // 新增
}
impl Default for gz_config_t {
    fn default() -> Self {
        Self { /* ... */ pipe: 0 }
    }
}
```

#### 2b. 更新 ARM extern 声明块

```rust
#[cfg(target_arch = "arm")]
extern "C" {
    pub fn gz_init(config: *const gz_config_t) -> gz_error_t;
    pub fn gz_set_mode(mode: gz_mode_t) -> gz_error_t;
    pub fn gz_send(data: *const u8, len: u8, pipe: u8) -> gz_error_t;                          // 更新
    pub fn gz_recv(out_buf: *mut u8, out_len: *mut u8, out_pipe: *mut u8, max_len: u8) -> gz_error_t; // 更新
    pub fn gz_is_ready(pipe: u8) -> bool;                                                       // 更新
    pub fn gz_set_ack_payload(pipe: u8, data: *const u8, len: u8) -> gz_error_t;               // 新增
    pub fn gz_get_ack_payload(out_buf: *mut u8, out_len: *mut u8, max_len: u8) -> gz_error_t;  // 新增
    pub fn gz_flush() -> gz_error_t;
    pub fn gz_deinit();
}
```

#### 2c. 更新非 ARM stub 函数（签名必须一一对应）

```rust
#[cfg(not(target_arch = "arm"))]
pub unsafe fn gz_send(_data: *const u8, _len: u8, _pipe: u8) -> gz_error_t { GZ_ERR_HARDWARE }
#[cfg(not(target_arch = "arm"))]
pub unsafe fn gz_recv(_out_buf: *mut u8, _out_len: *mut u8, _out_pipe: *mut u8, _max_len: u8) -> gz_error_t { GZ_ERR_HARDWARE }
#[cfg(not(target_arch = "arm"))]
pub unsafe fn gz_is_ready(_pipe: u8) -> bool { false }
#[cfg(not(target_arch = "arm"))]
pub unsafe fn gz_set_ack_payload(_pipe: u8, _data: *const u8, _len: u8) -> gz_error_t { GZ_ERR_HARDWARE }
#[cfg(not(target_arch = "arm"))]
pub unsafe fn gz_get_ack_payload(_out_buf: *mut u8, _out_len: *mut u8, _max_len: u8) -> gz_error_t { GZ_ERR_HARDWARE }
```

**验证命令**:
```bash
cargo check --manifest-path rmk-gazell-sys/Cargo.toml
cargo build --manifest-path rmk-gazell-sys/Cargo.toml \
  --target thumbv7em-none-eabihf --features nrf52840
```

---

### 步骤 3：更新 GazellTransport FFI 调用点

**文件**: `rmk/src/wireless/gazell.rs`、`rmk/src/wireless/config.rs`

**修改对照表**:

| 方法 | 旧调用 | 新调用 |
|------|--------|--------|
| `init()` | `gz_config_t { channel, ... }` | 添加 `pipe: self.config.pipe` |
| `send_frame()` | `gz_send(ptr, len)` | `gz_send(ptr, len, self.config.pipe)` |
| `recv_frame()` | `gz_recv(buf, &mut len, max)` | `gz_recv(buf, &mut len, &mut pipe, max)` |
| `is_ready()` | `gz_is_ready()` | `gz_is_ready(self.config.pipe)` |

同时给 `GazellConfig`（`rmk/src/wireless/config.rs`）添加字段：

**`pipe: u8`**:
- 默认值：`0`
- 验证：`self.pipe <= 7`
- 所有预设构造器：`pipe: 0`

**`heartbeat_interval_ms: u16`**:
- 默认值：`50`（低延迟，Layer/LED 指示灯响应快）
- 验证：`self.heartbeat_interval_ms >= 10 && self.heartbeat_interval_ms <= 5000`
- 预设值：
  - `low_latency()`：`50`（20 次/秒）
  - `long_range()`：`200`（5 次/秒）
  - `low_power()`：`500`（2 次/秒，Layer 同步延迟最多 500ms）
- 注：每种心跳率的空闲功耗取决于 radio TX 时间和睡眠电流。实际值待 Phase 3 硬件测量确定。

**验证命令**:
```bash
cargo check --manifest-path rmk/Cargo.toml --features wireless_gazell
cargo test --manifest-path rmk/Cargo.toml --lib -- wireless
```

---

### 步骤 4：创建 GazellSplitDriver

**文件**: `rmk/src/split/gazell.rs`（新建）

Phase 2 的核心。两个 driver 结构体，都实现 `SplitReader + SplitWriter`。

#### 4a. `GazellPeripheralDriver`（键盘端，device 模式）

**内部状态**:
```rust
pub(crate) struct GazellPeripheralDriver {
    pipe: u8,
    heartbeat_interval_ms: u16,         // 来自 GazellConfig，空闲心跳阈值
    ack_buffer: Option<SplitMessage>,   // 上次发送后缓存的 ACK payload
    last_send_time: Instant,            // 记录上次 gz_send 时间
}
```

**SplitWriter::write()**（peripheral -> central）:
```
1. postcard::to_slice(&message, &mut buf)
2. gz_send(buf.as_ptr(), len, self.pipe)
   - GZ_ERR_BUSY：等 1ms 重试，最多 3 次，然后返回 SplitDriverError::SerialError
   - GZ_ERR_SEND_FAILED：返回 SplitDriverError::SerialError
     （上层 SplitPeripheral::run() 记录日志并继续——输入事件丢失）
3. self.last_send_time = Instant::now()
4. 检查 gz_get_ack_payload()
   - len > 0：postcard::from_bytes → 存入 self.ack_buffer
   - len == 0：无 ACK payload
5. 返回 Ok(bytes_written)
```

> **已知限制**：如果 `gz_send()` 在 3 次重试后仍失败（如 dongle 不在范围内、持续干扰），输入事件将丢失。这与 BLE split 的行为一致——driver 层不做额外重试。在活跃打字时，链路恢复后下一个事件即可成功。见 §8 已知限制。

**SplitReader::read()**（central -> peripheral，通过 ACK payload）:
```
loop {
    1. 如果 self.ack_buffer.take() 有值 → return Ok(msg)
    2. 如果 self.last_send_time.elapsed() > self.heartbeat_interval_ms：
       a. 发送心跳：gz_send(&[0xFE, 0xFE], 2, self.pipe)
          - 出错时：忽略（非关键），仍然更新 last_send_time
       b. self.last_send_time = Instant::now()
       c. 检查 gz_get_ack_payload()
          - len > 0：反序列化 → return Ok(msg)
    3. Timer::after_millis(5).await   // 让出给 executor
}
```

**心跳设计**:
- 心跳是 2 字节包 `[0xFE, 0xFE]` — Gazell 内部标记，**不是** `SplitMessage` 变体
- `SplitMessage` 枚举完全不变 — 无跨协议兼容性影响
- Central 端 `GazellCentralDriver::read()` 过滤掉 `len == 2 && buf[0] == 0xFE && buf[1] == 0xFE` 的包
- 为何用 2 字节而非 1 字节：降低与未来 `SplitMessage` 编码冲突的风险。单测（步骤 8）显式验证无任何 `SplitMessage` 变体序列化为 `[0xFE, 0xFE]`。
- 心跳间隔：通过 `GazellConfig::heartbeat_interval_ms` 配置（默认 50ms，低延迟）。实际使用中，键盘事件频繁触发 `write()`，活跃打字时心跳很少。对功耗敏感的应用可调大到 200-500ms，代价是 central→peripheral 状态下发延迟增加。

**ACK payload 清理语义**: `gz_get_ack_payload()`（C shim 第 370 行）读取后设置 `ack_payload_ready = false`。不会重复消费。

**并发安全性**: Embassy 是单线程协作式 executor。`SplitPeripheral::run()` 中的 `select(driver.read(), event_source)` 同时 poll 两个 future 但一次只运行一个。`gz_send()` 是同步阻塞的（最多 ~10ms [^3]），executor 无法在 `gz_send()` 期间交错执行 `write()` 调用。不需要 mutex。

#### 4b. `GazellCentralDriver`（dongle 端，host 模式）

**内部状态**:
```rust
pub(crate) struct GazellCentralDriver {
    pipe: u8,
    pending_state: Option<SplitMessage>,   // 待发送的最新状态型消息
    pending_event: Option<SplitMessage>,   // 待投递的事件型消息（如 ClearPeer）
}
```

**SplitReader::read()**（peripheral -> central）:
```
loop {
    1. let ret = gz_recv(&mut buf, &mut len, &mut rx_pipe, max)
    2. 如果 ret != GZ_OK：
       a. log warning（记录 ret）
       b. Timer::after_millis(1).await   // 让出
       c. continue   // 错误时 len 未定义，不可使用
    3. 如果 len > 0：
       a. 刷新待发消息（对每个收到的包执行，不论 pipe）：
          i.  如果 self.pending_event 有值：
              - let ack_len = postcard::to_slice(pending_event, &mut ack_buf).len()
              - let ack_ret = gz_set_ack_payload(self.pipe, ack_buf.as_ptr(), ack_len)
              - GZ_OK 时：self.pending_event = None
              - 任何错误时：保留待下次迭代（trace 级别日志）
          ii. 否则如果 self.pending_state 有值：
              - let ack_len = postcard::to_slice(pending_state, &mut ack_buf).len()
              - let ack_ret = gz_set_ack_payload(self.pipe, ack_buf.as_ptr(), ack_len)
              - GZ_OK 时：self.pending_state = None
              - 任何错误时：保留待下次迭代（trace 级别日志）
       b. 如果 rx_pipe != self.pipe → log warning，continue
       c. 如果 len == 2 && buf[0] == 0xFE && buf[1] == 0xFE → 心跳包，continue
       d. postcard::from_bytes(&buf[..len]) → return Ok(msg)
          - 反序列化错误：log warning，继续轮询
    4. 如果 len == 0：
       a. Timer::after_millis(1).await   // 让出
}
```

> **关键修复 (v3)**：`pending_state` 仅在 `gz_set_ack_payload` 返回 `GZ_OK` 后才消费。任何错误（BUSY、HARDWARE 等）均保留待重试。
>
> **关键修复 (v4)**：刷新逻辑在**每个收到的包（含心跳）上执行**（步骤 3a），在心跳过滤（步骤 3c）之前。当外设空闲仅发送心跳时，中央仍可投递待发状态/事件。`pending_event` 优先于 `pending_state`，保证事件型消息优先投递。
>
> **关键修复 (v8)**：`gz_recv` 返回码现在显式检查（步骤 2）。错误时 `len` 未定义，不可使用——记录日志、让出、重试。刷新 pending 使用"任何错误都保留"语义（不仅限于 `GZ_ERR_BUSY`），与 host stub 返回 `GZ_ERR_HARDWARE` 的测试保持一致。

**SplitWriter::write()**（central -> peripheral，状态合并 + 事件延迟投递）:
```
1. 如果消息是事件型（ClearPeer）：
   a. let ack_len = postcard::to_slice(&message, &mut ack_buf).len()
   b. gz_set_ack_payload(self.pipe, ack_buf.as_ptr(), ack_len)
   c. GZ_ERR_BUSY 时：等 1ms 重试，最多 3 次
   d. 最终失败时：存入 self.pending_event（下次 read() 时重试）
   e. 返回 Ok(ack_len)
2. 否则（状态型：ConnectionState、KeyboardIndicator、Layer、LedState）：
   a. self.pending_state = Some(*message)   // 覆盖 — 只保留最新状态
   b. 返回 Ok(SPLIT_MESSAGE_MAX_SIZE)
```

> **关键修复 (v4)**：`ClearPeer` 不再在 FIFO 满时返回错误。改为存入 `pending_event`，在下次收到包（含心跳）时重试。保证事件型消息不会永久丢失。

**为什么状态型消息用状态合并**: Nordic Gazell ACK payload FIFO 深度为 3（`NRF_GZLL_CONST_FIFO_LENGTH = 3`，定义于 `nrf_gzll_constants.h:121`）。如果 central 在 peripheral 发包（触发 ACK）之前多次调用 `gz_set_ack_payload`，FIFO 会满并返回 `GZ_ERR_BUSY`。状态型消息（ConnectionState、Layer、KeyboardIndicator、LedState）只需最新值。通过合并到 `pending_state`，在收到包时才刷新，保证：
- 任何时刻最多 1 个待发 ACK payload
- payload 始终是最新状态
- 不会 FIFO 溢出
- GZ_ERR_BUSY 时不丢失状态（pending_state 保留待重试）
- **空闲期投递**：心跳也触发刷新，因此即使无按键，状态也能到达外设

**为什么事件型消息用 `pending_event`**: `ClearPeer` 是一次性命令，必须执行。先尝试立即 `gz_set_ack_payload` 并重试。如果 FIFO 仍满，存入 `pending_event`，在下次收到包（含心跳）时刷新。`pending_event` 在刷新顺序中优先于 `pending_state`。

**刷新优先级**: `pending_event` > `pending_state`。每个包只刷新一个（每个 ACK 只能搭载一个 payload）。这意味着一个待发事件可能延迟一个包周期（约 `heartbeat_interval_ms`）的状态更新。由于状态是幂等的，这是可接受的。

**注意**: `Address` 当前在代码库中未使用。如果将来启用，需确定其方向和语义并添加到 §2 方向表中。

#### 4c. 错误处理策略

| FFI 调用 | 错误 | 策略 |
|----------|------|------|
| `gz_send()` | `GZ_ERR_BUSY` | 等 1ms 重试，最多 3 次。穷尽后返回 `SplitDriverError::SerialError`。 |
| `gz_send()` | `GZ_ERR_SEND_FAILED` | 返回 `SplitDriverError::SerialError`。上层 `SplitPeripheral::run()` 记录日志并继续。 |
| `gz_send()`（心跳中） | 任何错误 | trace 级别日志，忽略。心跳是尽力发送。 |
| `gz_recv()` | `GZ_OK`，`len == 0` | 正常 — 无数据。让出后重试。 |
| `gz_recv()` | 非 `GZ_OK` 返回 | log warning。`len` 未定义——不可使用。让出后重试。 |
| `gz_set_ack_payload()`（`read()` 刷新中） | 任何错误（BUSY、HARDWARE 等） | 保持 `pending_event`/`pending_state` 不变。trace 级别日志。下次收到包（含心跳）时重试。 |
| `gz_set_ack_payload()`（`write()` 中，`ClearPeer`） | `GZ_ERR_BUSY` | 等 1ms 重试，最多 3 次。穷尽后存入 `self.pending_event` 延迟投递。 |
| `gz_get_ack_payload()` | `len == 0` | 正常 — 未收到 ACK payload。 |
| `postcard::from_bytes()` | 错误 | log warning，跳过该包，继续轮询。 |
| `postcard::to_slice()` | 错误 | 返回 `SplitDriverError::SerializeError`。 |

**验证命令**:
```bash
cargo check --manifest-path rmk/Cargo.toml --features "split,wireless_gazell"
```

---

### 步骤 5：接入 split 模块

**文件**:
- `rmk/src/split/mod.rs`
- `rmk/src/split/peripheral.rs`
- `rmk/src/split/central.rs`

#### 5a. 添加模块声明和 feature 守卫

在 `rmk/src/split/mod.rs` 中：
```rust
#[cfg(feature = "wireless_gazell")]
pub mod gazell;
```

在 `rmk/src/split/mod.rs` 或 `rmk/src/lib.rs` 中添加编译期互斥：
```rust
#[cfg(all(feature = "_ble", feature = "wireless_gazell"))]
compile_error!(
    "Features `_ble` and `wireless_gazell` are mutually exclusive. \
     BLE and Gazell share the same radio hardware on nRF52."
);
```

#### 5b. 更新 peripheral 分发逻辑

三路 cfg 分发：

```rust
pub async fn run_rmk_split_peripheral<...>(
    #[cfg(feature = "_ble")] /* BLE 参数 */,
    #[cfg(feature = "wireless_gazell")] config: GazellConfig,
    #[cfg(not(any(feature = "_ble", feature = "wireless_gazell")))] serial: S,
) {
    #[cfg(feature = "wireless_gazell")]
    {
        crate::split::gazell::run_gazell_split_peripheral(config).await;
    }

    #[cfg(feature = "_ble")]
    {
        crate::split::ble::peripheral::initialize_nrf_ble_split_peripheral_and_run(...).await;
    }

    #[cfg(not(any(feature = "_ble", feature = "wireless_gazell")))]
    {
        let mut peripheral = SplitPeripheral::new(SerialSplitDriver::new(serial));
        loop { peripheral.run().await; }
    }
}
```

#### 5c. 更新 central 分发逻辑

同样的三路分发模式：

```rust
pub async fn run_peripheral_manager<...>(
    id: usize,
    #[cfg(feature = "_ble")] /* BLE 参数 */,
    #[cfg(feature = "wireless_gazell")] config: GazellConfig,
    #[cfg(not(any(feature = "_ble", feature = "wireless_gazell")))] receiver: S,
) {
    #[cfg(feature = "wireless_gazell")]
    { crate::split::gazell::run_gazell_peripheral_manager::<ROW, COL, ROW_OFFSET, COL_OFFSET>(id, config).await; }

    #[cfg(feature = "_ble")]
    { /* 现有 BLE 代码 */ }

    #[cfg(not(any(feature = "_ble", feature = "wireless_gazell")))]
    { /* 现有串口代码 */ }
}
```

#### 5d. `split/gazell.rs` 中的辅助函数

遵循 BLE 的模式，创建：

```rust
/// 初始化 Gazell 并运行 split peripheral 循环（永不返回）
pub async fn run_gazell_split_peripheral(config: GazellConfig) {
    // 1. 通过 GazellTransport 初始化 Gazell
    // 2. 设置 device 模式
    // 3. 创建 GazellPeripheralDriver { pipe: config.pipe, heartbeat_interval_ms: config.heartbeat_interval_ms, ack_buffer: None, last_send_time: Instant::now() }
    // 4. 创建 SplitPeripheral::new(driver)
    // 5. loop { peripheral.run().await; }
}

/// 运行 central 端的单个 Gazell peripheral 管理器
pub async fn run_gazell_peripheral_manager<
    const ROW: usize, const COL: usize,
    const ROW_OFFSET: usize, const COL_OFFSET: usize,
>(id: usize, config: GazellConfig) {
    // 1. 通过 GazellTransport 初始化 Gazell
    // 2. 设置 host 模式
    // 3. 创建 GazellCentralDriver { pipe: config.pipe, pending_state: None }
    // 4. 创建 PeripheralManager::new(driver, id)
    // 5. peripheral_manager.run().await
}
```

**验证命令**:
```bash
# 三种 feature 组合必须编译通过
cargo check --manifest-path rmk/Cargo.toml --features "split,wireless_gazell"
cargo check --manifest-path rmk/Cargo.toml --features "split"

# 验证 compile_error! 守卫能拒绝 BLE + Gazell 组合
# 这条应该失败——如果成功说明守卫有问题
cargo check --manifest-path rmk/Cargo.toml --features "split,wireless_gazell,_ble" 2>&1 \
  | grep -q "mutually exclusive" && echo "Guard works" || echo "ERROR: guard missing"
```

---

### 步骤 6：更新 Cargo.toml feature gate

**文件**: `rmk/Cargo.toml`

```toml
## Enable Gazell support for nRF52840
wireless_gazell_nrf52840 = ["wireless_gazell", "rmk-gazell-sys/nrf52840", "split"]
```

添加 `"split"` 确保 Gazell 构建时自动启用 `split`（及其传递依赖 `controller`），
因为 Gazell 键盘一定是 split 架构（键盘 + dongle）。

**验证命令**:
```bash
cargo check --manifest-path rmk/Cargo.toml --features wireless_gazell_nrf52840
```

---

### 步骤 7：更新示例

**文件**:
- `examples/use_rust/nrf52840_2g4/src/main.rs`
- `examples/use_rust/nrf52840_dongle/src/main.rs`

示例当前直接使用 `GazellTransport`。步骤 3 完成后不需要代码改动。

验证 Cargo.toml 中的 feature 正确即可。

**验证命令**:
```bash
cd examples/use_rust/nrf52840_2g4 && cargo build --release && cd -
cd examples/use_rust/nrf52840_dongle && cargo build --release && cd -

ls -la examples/use_rust/nrf52840_2g4/target/thumbv7em-none-eabihf/release/rmk-nrf52840-2g4
ls -la examples/use_rust/nrf52840_dongle/target/thumbv7em-none-eabihf/release/rmk-nrf52840-dongle
```

---

### 步骤 8：完整验证套件

```bash
# ---- Host 检查 ----

# 1. FFI crate host 检查
cargo check --manifest-path rmk-gazell-sys/Cargo.toml

# 2. RMK Gazell split feature
cargo check --manifest-path rmk/Cargo.toml --features "split,wireless_gazell"

# 3. RMK 串口 split（回归测试）
cargo check --manifest-path rmk/Cargo.toml --features "split"

# 4. 单元测试
cargo test --manifest-path rmk/Cargo.toml --lib -- wireless
cargo test --manifest-path rmk/Cargo.toml --lib -- split

# ---- ARM 交叉编译 ----

# 5. FFI crate
cargo build --manifest-path rmk-gazell-sys/Cargo.toml \
  --target thumbv7em-none-eabihf --features nrf52840

# 6. 示例（必须 cd 进目录）
cd examples/use_rust/nrf52840_2g4 && cargo build --release && cd -
cd examples/use_rust/nrf52840_dongle && cargo build --release && cd -

# ---- 代码质量 ----

# 7. 格式检查
cargo fmt --all -- --check

# 8. Clippy
cargo clippy --manifest-path rmk/Cargo.toml \
  --features "split,wireless_gazell" -- -D warnings
cargo clippy --manifest-path rmk-gazell-sys/Cargo.toml -- -D warnings
```

**编译期大小断言**（添加到 `rmk/src/split/gazell.rs` 模块作用域，**不在** `#[cfg(test)]` 内）:

```rust
// 每次构建都检查（包括固件 release），而非仅在测试时。
const _: () = assert!(
    SplitMessage::POSTCARD_MAX_SIZE <= 32,
    "SplitMessage max size exceeds Gazell 32-byte payload limit"
);
```

**单元测试**（添加到 `rmk/src/split/gazell.rs`）:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use postcard::to_slice;
    use crate::event::{
        Axis, AxisEvent, AxisValType, KeyPos,
        KeyboardEvent, KeyboardEventPos,
        PointingEvent, TouchpadEvent,
    };

    /// 验证无任何 SplitMessage 变体序列化为心跳标记 [0xFE, 0xFE]。
    ///
    /// 原理：Central 过滤 `len==2 && [0xFE, 0xFE]` 为心跳。
    /// 如果真实 SplitMessage 产生此编码，会被静默丢弃。
    ///
    /// 覆盖策略：测试每个变体。有 payload 的变体使用 0xFE 作为值（冲突最坏情况）。
    /// bool 变体测试 true/false。struct payload 使用显式构造器（不依赖 Default trait）。
    #[test]
    fn heartbeat_marker_does_not_collide_with_any_split_message() {
        let heartbeat: [u8; 2] = [0xFE, 0xFE];
        let mut buf = [0u8; 32];

        let zero_axis = AxisEvent { typ: AxisValType::Abs, axis: Axis::X, value: 0 };
        let fe_axis = AxisEvent { typ: AxisValType::Abs, axis: Axis::X, value: 0xFE };

        let variants: &[SplitMessage] = &[
            SplitMessage::Key(KeyboardEvent { pressed: true,
                pos: KeyboardEventPos::Key(KeyPos { row: 0xFE, col: 0xFE }) }),
            SplitMessage::Key(KeyboardEvent { pressed: false,
                pos: KeyboardEventPos::Key(KeyPos { row: 0, col: 0 }) }),
            SplitMessage::Touchpad(TouchpadEvent { finger: 0, axis: [zero_axis, zero_axis] }),
            SplitMessage::Touchpad(TouchpadEvent { finger: 0xFE, axis: [fe_axis, fe_axis] }),
            SplitMessage::Pointing(PointingEvent([zero_axis, zero_axis, zero_axis])),
            SplitMessage::Pointing(PointingEvent([fe_axis, fe_axis, fe_axis])),
            SplitMessage::LedState(true),
            SplitMessage::LedState(false),
            SplitMessage::ConnectionState(true),
            SplitMessage::ConnectionState(false),
            SplitMessage::Address([0xFE; 6]),
            SplitMessage::ClearPeer,
            SplitMessage::KeyboardIndicator(0xFE),
            SplitMessage::Layer(0xFE),
            // BatteryState 在 #[cfg(feature = "_ble")] 后——启用时另行测试
        ];
        for msg in variants {
            let serialized = to_slice(msg, &mut buf).unwrap();
            assert_ne!(
                serialized, &heartbeat,
                "SplitMessage variant {:?} serializes to heartbeat marker!", msg
            );
        }
    }

    /// 验证 GazellCentralDriver 在刷新失败时保留 pending_state。
    ///
    /// 非 ARM host 上，gz_set_ack_payload stub 返回 GZ_ERR_HARDWARE。
    /// "任何错误都保留 pending"策略意味着这与真实硬件上
    /// GZ_ERR_BUSY 走相同的保留路径。
    #[test]
    fn pending_state_retained_on_flush_failure() {
        let mut driver = GazellCentralDriver {
            pipe: 0,
            pending_state: Some(SplitMessage::Layer(5)),
            pending_event: None,
        };
        driver.try_flush_pending();
        assert!(
            matches!(driver.pending_state, Some(SplitMessage::Layer(5))),
            "pending_state 必须在刷新失败后保留，且值不变"
        );
    }

    /// 验证 pending_event 优先于 pending_state 刷新，且失败时同样保留。
    #[test]
    fn pending_event_priority_and_retention() {
        let mut driver = GazellCentralDriver {
            pipe: 0,
            pending_state: Some(SplitMessage::Layer(3)),
            pending_event: Some(SplitMessage::ClearPeer),
        };
        driver.try_flush_pending();
        assert!(driver.pending_event.is_some(), "pending_event 必须在刷新失败后保留");
        assert!(driver.pending_state.is_some(),
            "pending_event 刷新失败时 pending_state 不应被触碰");
    }
}
```

> **说明**：
> - `const` 大小断言在**模块作用域**（不在 `#[cfg(test)]` 内），因此在每次构建时强制执行，包括固件 release。
> - 心跳冲突测试使用显式 struct 构造器——不依赖 `Default` trait。`AxisEvent`、`TouchpadEvent`、`PointingEvent` 都手动构造。
> - 刷新测试依赖从 `read()` 循环中提取的 `try_flush_pending()` 辅助方法以提高可测试性。
> - 非 ARM host 上，FFI stub 返回 `GZ_ERR_HARDWARE`，自然覆盖保留路径。
> - 心跳冲突测试使用代表值（0x00、0xFE）而非穷举枚举。对 varint 编码的整型字段来说，这覆盖了最可能碰撞的候选值。如果未来需要更严格的覆盖，可添加 `proptest` 或 fuzz（低优先级——2 字节 `[0xFE, 0xFE]` 标记本身是安全的，因为 postcard 使用 varint 编码 enum 判别值 [^5]，而 `SplitMessage` 远少于 254 个变体，不可能产生 0xFE 的判别值）。

---

## 6. SplitMessage 大小分析

关键约束：Gazell 最大 payload = 32 bytes [^1]。

| SplitMessage 变体 | 内部类型字段 | Postcard 大小 (bytes) |
|---|---|---|
| `Key(KeyboardEvent)` | pressed: bool (1) + pos: KeyboardEventPos (1 tag + 2 字段 = 3) | 1 + 4 = **5** |
| `Touchpad(TouchpadEvent)` | finger: u8 (1) + axis: [AxisEvent; 2] (2 x 4 = 8) | 1 + 9 = **10** |
| `Pointing(PointingEvent)` | [AxisEvent; 3] (3 x 4 = 12) | 1 + 12 = **13** |
| `LedState(bool)` | bool (1) | 1 + 1 = **2** |
| `ConnectionState(bool)` | bool (1) | 1 + 1 = **2** |
| `Address([u8; 6])` | 6 bytes | 1 + 6 = **7** |
| `ClearPeer` | unit | **1** |
| `KeyboardIndicator(u8)` | u8 (1) | 1 + 1 = **2** |
| `Layer(u8)` | u8 (1) | 1 + 1 = **2** |
| `BatteryState(BatteryStateEvent)` | enum (1 tag + 最大 1 byte) | 1 + 2 = **3** |

**最大值**: `Pointing(PointingEvent)` = ~13 bytes。
**余量**: 32 - 13 = **19 bytes**。

**注意**: `SplitMessage` 枚举不会为 Gazell 修改。心跳包（`[0xFE, 0xFE]`）是原始字节，不是 `SplitMessage` 值。以上分析覆盖了所有实际的 `SplitMessage` 变体。

**安全网**: `const` 断言强制 `SplitMessage::POSTCARD_MAX_SIZE <= 32`（见步骤 8）。如果未来变体超过 32 bytes，编译将失败。

---

## 7. 变更文件汇总

### 步骤 1-8：运行时层（SplitMessage 协议 + 驱动）

| 步骤 | 文件 | 操作 | 预估行数 |
|------|------|------|----------|
| 1 | `rmk-gazell-sys/c/gazell_shim.c` | 修复 ack_payload_length uint32_t 临时变量 | ~5 |
| 2 | `rmk-gazell-sys/src/lib.rs` | 添加 pipe 到 config，更新/新增 FFI 签名 | ~30 |
| 3 | `rmk/src/wireless/config.rs` | 给 GazellConfig 添加 `pipe: u8` + `heartbeat_interval_ms: u16` + 验证 | ~12 |
| 3 | `rmk/src/wireless/gazell.rs` | 更新 FFI 调用点使用 `self.config.pipe` | ~10 |
| 4 | `rmk/src/split/gazell.rs` | **新建**: GazellPeripheralDriver + GazellCentralDriver | ~250 |
| 5 | `rmk/src/split/mod.rs` | 添加 `pub mod gazell` + `compile_error!` 守卫 | ~6 |
| 5 | `rmk/src/split/peripheral.rs` | 添加 Gazell 分发分支 | ~15 |
| 5 | `rmk/src/split/central.rs` | 添加 Gazell 分发分支 | ~15 |
| 6 | `rmk/Cargo.toml` | 更新 wireless_gazell_nrf52840 feature（添加 `"split"`） | ~1 |
| 7 | 示例（两个） | 不需要代码改动 | 0 |
| 8 | `rmk/src/split/gazell.rs` | 添加 `const` 大小断言 + 心跳冲突测试 + pending_state 测试 | ~40 |

### 步骤 9-13：Codegen 层 + keyboard.toml 集成 + dongle USB HID

| 步骤 | 文件 | 操作 | 预估行数 |
|------|------|------|----------|
| 9 | `rmk-macro/src/codegen/entry.rs` | 在 split 连接分发中添加 `"gazell"` 分支 | ~40 |
| 9 | `rmk-macro/src/codegen/split/central.rs` | 在 `expand_split_communication_config` 中添加 `"gazell"` 分支 | ~30 |
| 9 | `rmk-macro/src/codegen/split/peripheral.rs` | 在 peripheral 分发中添加 `"gazell"` 分支 | ~30 |
| 10 | `rmk-config/src/lib.rs` | 在 `SplitConfig` / `SplitBoardConfig` 中添加 Gazell 配置字段 | ~25 |
| 11 | `examples/use_rust/nrf52840_dongle/src/main.rs` | USB CDC 测试替换为 `GazellCentralDriver` + USB HID 转发 | ~200 |
| 12 | Charybdis `keyboard.toml`（新建或改编） | `connection = "gazell"` 及 Gazell 特定配置 | ~15 |
| 13 | 硬件验证 | 左手按键 → Gazell → dongle → USB HID → PC 打字 | 0 |

**新增代码总量**: ~660 行
**修改代码总量**: ~100 行

### 架构决策记录

| 决策 | 选择 | 理由 |
|------|------|------|
| 目标键盘 | Charybdis（NoirGuo/rmk-keyboard，nRF52840 split） | 用户实际硬件 |
| 初始范围 | 仅左手 → Gazell → dongle → USB HID → PC | 最简可验证路径；右手暂缓 |
| 最终范围 | 双手 → Gazell → dongle → USB HID → PC（multi-pipe） | 完整 split 键盘走 2.4GHz |
| Dongle 构建方式 | `use_rust` 示例（手写 main.rs） | Dongle 不是键盘；codegen 不适用 |
| 轨迹球（pmw3610） | 推迟到多 peripheral 阶段 | 仅在右手（central）上，左手优先方案跳过 |
| keyboard.toml 集成 | 在 codegen 中添加 `connection = "gazell"` | 允许现有 Charybdis TOML 改一个字段即可使用 Gazell |

---

## 8. 风险评估

### 低风险

- **步骤 1-3**（修复 FFI 不匹配）：机械性修改，编译器可验证。
- **步骤 6**（Cargo.toml）：单行修改。
- **步骤 7**（示例）：不需要代码改动。
- **步骤 10**（rmk-config）：添加带 `#[serde(default)]` 的可选字段，向后兼容。
- **步骤 12**（keyboard.toml）：配置文件，无代码逻辑。

### 中等风险

- **步骤 5**（接入 split 模块）：函数签名上的 `#[cfg]` 属性操作最棘手。需确保三种 feature 组合都能编译。`compile_error!` 守卫降低了误配置风险。
- **步骤 9**（codegen 层）：必须严格遵循现有 `"ble"` 和 `"serial"` 分支的模式。需要从 keyboard.toml 字段正确生成 `GazellConfig` 初始化。风险：codegen 错误难以调试（proc-macro 输出）。缓解：用 `cargo expand` 检查生成的代码。
- **步骤 11**（dongle USB HID）：需要将 `SplitMessage::Key` 转换为 USB HID report。RMK 已有 `KeyboardReportChannel` 和 `UsbHidWriter` 基础设施可复用，但 dongle 是独立的 `use_rust` 示例（无 codegen），HID report 组装需手动完成。

### 高风险

- **步骤 4**（GazellSplitDriver）：最复杂的步骤。关键风险及缓解：

| 风险 | 缓解措施 |
|------|----------|
| read() 发心跳时 write() 要发数据 | 单线程 executor 保证互斥。last_send_time 协调避免不必要的心跳。 |
| Central ACK payload FIFO 溢出 | 状态合并策略：pending_state 只保存最新值，在 read() 成功后刷新。`NRF_GZLL_CONST_FIFO_LENGTH = 3`，确认于 `nrf_gzll_constants.h:121`。 |
| `pending_state` 在刷新错误时丢失 | **v3 修复**：`pending_state` 仅在 `gz_set_ack_payload` 返回 `GZ_OK` 后才消费。任何错误（BUSY、HARDWARE 等）均保留待重试。 |
| `pending_state` 空闲期永远不刷新 | **v4 修复**：刷新在每个收到的包（含心跳）上执行（步骤 3a），在心跳过滤（步骤 3c）之前。 |
| `gz_recv` 错误时 `len` 未定义 | **v8 修复**：返回码显式检查（步骤 2）。非 `GZ_OK` 时不使用 `len`——记录日志、让出、重试。 |
| 损坏的包导致反序列化失败 | log 后跳过，继续轮询。永不 panic。 |
| 心跳被误认为数据 | 2 字节 `[0xFE, 0xFE]` 标记显式过滤。单测验证无 `SplitMessage` 变体序列化为此标记。 |
| `ClearPeer` 事件永久丢失 | **v4 修复**：`ClearPeer` 先尝试立即发送并重试。失败后存入 `pending_event`，在下次收到包时刷新。`pending_event` 优先于 `pending_state`。 |
| `gz_send()` 重试穷尽导致丢失输入事件 | 设计如此：与 BLE split 行为一致。已记入已知限制。 |

### 已知限制（Phase 2 非阻塞项）

1. **仅支持单 peripheral**：`self.config.pipe` 可配置，但多键盘未测试。推迟。
2. **无重连逻辑**：Gazell 没有连接状态。键盘持续发送。比 BLE 更简单。
3. **异步上下文中的阻塞 FFI**：`gz_send()` 最长阻塞 ~10ms [^3]。当前可接受；异步封装推迟到 Phase 3。
4. **C shim 中的 ACK payload 竞态**：`nrf_gzll_device_tx_success` 回调（中断上下文）写 `ack_payload_ready = true`，而 `gz_get_ack_payload`（主线程上下文）读取并清除。在当前"发送-然后-检查"单次流程下，此竞态不会发生。Phase 3 引入异步封装时应添加适当的 atomic/volatile 语义。
5. **持续链路故障时丢失输入事件**：如果 `gz_send()` 在 3 次重试后仍失败（dongle 不在范围内、持续干扰），`SplitPeripheral::run()` 的输入事件将丢失。这与 BLE split 行为一致。活跃打字时，链路恢复后下一个事件即可成功。Phase 3 可考虑在 driver 层添加短重试队列以改善延迟敏感场景。

---

## 9. 依赖关系图

```
步骤 1（C shim 修复）
    │
    ▼
步骤 2（Rust FFI 绑定）
    │
    ├──────────────────────┐
    ▼                      ▼
步骤 3（GazellTransport） 步骤 4（GazellSplitDriver）
    │                      │
    ▼                      ▼
步骤 7（示例）           步骤 5（接入 split + compile_error!）
                           │
                           ▼
                          步骤 6（Cargo.toml）
                           │
                           ▼
                          步骤 8（完整验证 + 大小测试）
                           │
              ┌────────────┼────────────┐
              ▼            ▼            ▼
步骤 10       步骤 9       步骤 11
(rmk-config)  (codegen)    (dongle USB HID)
              │                         │
              ▼                         │
步骤 12 (keyboard.toml)                │
              │                         │
              └────────┬────────────────┘
                       ▼
              步骤 13（硬件验证）
```

步骤 3 和 4 可以在步骤 2 完成后并行进行。
步骤 5 和 7 可以在步骤 3/4 完成后并行进行。
步骤 9、10、11 可以在步骤 8 完成后并行进行。
步骤 12 依赖步骤 9 和 10。
步骤 13 依赖步骤 11 和 12。

---

## 10. 步骤 9-13：Codegen、Config、Dongle 及集成

> **v10 新增**：这些步骤将原始 Phase 2 计划扩展到覆盖从 `keyboard.toml` 到 Charybdis 键盘硬件验证的完整流水线。
>
> 详细实施说明请参阅英文版计划文档的对应章节（§10 Steps 9-13）。
> 中文版此处保留架构决策和步骤概要，具体代码模板以英文版为准。

### 步骤 9：codegen 层添加 `"gazell"` 分支

在 `rmk-macro/src/codegen/` 的三个文件中，参照现有 `"ble"` 和 `"serial"` 分支，添加 `"gazell"` 分支：
- `entry.rs`：central 入口点，生成 `run_peripheral_manager` 调用 + `GazellConfig` 参数
- `split/central.rs`：`expand_split_communication_config` 添加 `"gazell"` 匹配臂
- `split/peripheral.rs`：peripheral 分发，生成 `run_rmk_split_peripheral` 调用

### 步骤 10：rmk-config 添加 Gazell 配置字段

在 `SplitBoardConfig` 中添加可选的 `gazell: Option<GazellSplitConfig>` 字段（pipe、channel、heartbeat_interval_ms），均带 `#[serde(default)]`。

### 步骤 11：Dongle USB HID 转发

将 dongle 示例的 USB CDC 测试固件替换为：`GazellCentralDriver` 接收 `SplitMessage` → 转换为 USB HID report → 发送到 PC。初版使用硬编码的 Charybdis 第 0 层 keymap 查找表。

### 步骤 12：Charybdis keyboard.toml 适配

基于 NoirGuo/rmk-keyboard 的 TOML，将 `connection = "ble"` 改为 `connection = "gazell"`，去掉 `ble_addr`，去掉 `[ble]` 段。

### 步骤 13：硬件验证

左手矩阵按键 → Gazell → dongle → USB HID → PC 能打字即通过。

---

## 11. 后续工作（Phase 2 之后）

以下内容**不属于本次计划**，仅作为后续阶段的备忘和提示。

### Phase 3：异步 FFI + 功耗优化

| 项目 | 描述 | 优先级 |
|------|------|--------|
| **异步 gz_send() 封装** | 当前 `gz_send()` 阻塞最多 ~10ms，会饿死 Embassy executor。封装到专用 task 或在每次调用后用 `embassy_futures::yield_now()`。 | 高 |
| **空闲 radio 休眠** | 连续 `heartbeat_interval_ms * N` 次心跳无 ACK 回应后，关闭 radio 省电。按键事件时重新唤醒。 | 中 |
| **ACK payload 原子标志** | C shim 中 `ack_payload_ready` 在中断（callback）中写、在主上下文中读。引入异步封装后需添加 `volatile` / 原子语义。 | 中 |
| **输入事件重试队列** | 短内部缓冲（2-3 个事件），在活跃打字时的瞬时链路故障中幸存。目前 `gz_send()` 3 次重试失败后事件丢失。 | 低 |

### Phase 4：BLE / 2.4G 运行时切换

| 项目 | 描述 | 优先级 |
|------|------|--------|
| **移除 compile_error! 守卫** | 允许同一个二进制中同时包含 `_ble` 和 `wireless_gazell`。radio 同一时刻只能运行一种协议，但软件可以切换。 | 高 |
| **Radio 模式管理器** | 运行时抽象：停止当前协议 → 重新配置 radio → 启动新协议。需处理：FIFO 排空、pending_state 迁移、连接状态重置。 | 高 |
| **切换时状态同步** | 从 BLE 切到 2.4G（或反过来）时：pending 的按键事件、Layer 状态、ConnectionState 如何处理？定义清晰的语义（如：flush 所有 pending，切换后重新同步状态）。 | 高 |
| **用户态切换机制** | 按键组合、物理拨码开关、或 TOML 配置的快捷键来触发协议切换。 | 中 |
| **USB HID 重枚举** | 切换到/从 Gazell dongle 时，PC 端 USB HID 可能需要重连。定义 dongle 是保持连接还是重新枚举。 | 低 |

### Phase 5：多外设支持

| 项目 | 描述 | 优先级 |
|------|------|--------|
| **多 pipe 路由** | 不同键盘半侧使用不同 Gazell pipe（0-7）。Central 管理每个 pipe 的 `pending_state`。 | 中 |
| **Pipe 感知的 PeripheralManager** | 每个 `PeripheralManager` 实例绑定到特定 pipe。需要更新 `run_peripheral_manager` 接受 pipe ID。 | 中 |
| **配对 / pipe 分配** | 键盘半侧与 dongle 首次连接时协商 pipe 分配的协议。 | 低 |

---

## 参考文献

[^1]: `NRF_GZLL_CONST_MAX_PAYLOAD_LENGTH = 32` — Nordic nRF5 SDK v17.1.0, `components/proprietary_rf/gzll/nrf_gzll_constants.h:123`。C shim 中对应 `MAX_PAYLOAD_LENGTH`（`rmk-gazell-sys/c/gazell_shim.c:9`）。
[^2]: `NRF_GZLL_CONST_FIFO_LENGTH = 3` — Nordic nRF5 SDK v17.1.0, `components/proprietary_rf/gzll/nrf_gzll_constants.h:121`。
[^3]: `gz_send()` ~10ms 阻塞 — `rmk-gazell-sys/c/gazell_shim.c:254-271`，忙等循环 `timeout = 100000`（约 10ms，每次迭代约 10 个时钟周期）。理论依据：`NRF_GZLL_DEFAULT_TIMESLOT_PERIOD = 600μs`（SDK `nrf_gzll_constants.h:170`）× 默认重试次数。
[^4]: `nrf_gzll_fetch_packet_from_rx_fifo(uint32_t pipe, uint8_t* p_payload, uint32_t* p_length)` — Nordic nRF5 SDK v17.1.0, `components/proprietary_rf/gzll/nrf_gzll.h:374`。
[^5]: postcard 线格式：enum 变体使用 varint 判别值编码，后跟 payload — https://postcard.jamesmunns.com/wire-format#enums
