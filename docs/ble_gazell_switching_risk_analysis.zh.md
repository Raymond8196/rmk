# BLE ↔ Gazell 运行时切换风险分析

> 在 nRF52840 上实现 BLE 与 Gazell 2.4G 协议运行时热切换的风险评估。

## 背景

### 硬件约束

nRF52840 只有**一个 RADIO 外设**。BLE 和 Gazell 无法同时运行。

```
┌─────────────────────────────────────┐
│           nRF52840                   │
│  ┌─────────┐                        │
│  │  RADIO  │ ← 只有一个！            │
│  └────┬────┘                        │
│       │                              │
│  ┌────┴────┬────────────┐           │
│  │   BLE   │   Gazell   │           │
│  │ (二选一)              │           │
│  └─────────┴────────────┘           │
└─────────────────────────────────────┘
```

### 当前架构

| 方面 | BLE (trouble-host) | Gazell (Nordic SDK) |
|-----|-------------------|---------------------|
| 协议栈控制 | `Stack::build()` 初始化 | `gz_init()` / `gz_deinit()` |
| 销毁机制 | ❌ 无（静态生命周期设计） | ✅ 有 `gz_deinit()` |
| RADIO 中断 | `nrf_sdc::mpsl::HighPrioInterruptHandler` | C 库 `RADIO_IRQHandler` |
| RAM 占用 | 约 10KB+ | 约 1KB |

---

## 提案：运行时动态切换（方案C）

### 切换流程

```
用户触发切换（如 Fn+某键）
        ↓
    保存状态到 Flash
        ↓
    断开 BLE / 停止 Gazell
        ↓
    重新绑定 RADIO 中断
        ↓
    初始化新协议
        ↓
    恢复连接
```

---

## 风险分析

### 风险 A：BLE 栈缺乏销毁机制（高风险）

**问题**：`trouble-host` 设计为静态生命周期资源：

```rust
// trouble-host 设计模式：
let mut host_resources: HostResources<'static, ...> = HostResources::new();
let stack: Stack<'static, ...> = trouble_host::new(controller, &mut host_resources);

// Stack 和 HostResources 没有 stop()/destroy() 方法
```

**影响**：切换到 Gazell 后，BLE 静态资源无法释放，RAM 无法复用。

**缓解方案**：
1. 将 `trouble-host` 封装为"暂停"状态（禁用广播 + 停止 runner）
2. 上游修改 `trouble-host` 支持销毁
3. 接受 RAM 浪费（约 10KB），同一时间只激活一个协议

**可行变通**：测试 BLE 栈是否可以"静默"而不销毁：

```rust
// 伪代码
async fn pause_ble(stack: &Stack) {
    // 停止广播
    stack.stop_advertising();
    // 停止 runner 任务（需要跟踪并取消该任务）
    runner_cancel_token.cancel();
    // RADIO 中断将处于空闲
}
```

---

### 风险 B：RADIO 中断重新绑定（中风险）

**问题**：`embassy-nrf` 在编译时绑定中断：

```rust
// 当前方式 - 编译时绑定
#[pac::interrupt]
fn RADIO() {
    // 编译时固定，无法运行时更改
}
```

**解决方案**：动态中断分发器：

```rust
// 运行时分发方式
static RADIO_HANDLER: AtomicU8 = AtomicU8::new(0); // 0=BLE, 1=Gazell

#[pac::interrupt]
fn RADIO() {
    match RADIO_HANDLER.load(Ordering::Relaxed) {
        0 => unsafe { 
            // BLE 处理器
            nrf_sdc::mpsl::HighPrioInterruptHandler::on_radio() 
        },
        1 => unsafe { 
            // Gazell 处理器（来自 C 库）
            RADIO_IRQHandler() 
        },
        _ => {}
    }
}

// 切换函数
fn switch_radio_handler(mode: u8) {
    // 1. 禁用当前处理器
    interrupt::RADIO.disable();
    
    // 2. 更新处理器选择器
    RADIO_HANDLER.store(mode, Ordering::SeqCst);
    
    // 3. 清除挂起的中断
    interrupt::RADIO.clear_pend();
    
    // 4. 重新启用
    interrupt::RADIO.enable();
}
```

**影响**：需要修改 RMK 中 `rmk-macro/src/codegen/` 的中断绑定代码。

---

### 风险 C：切换期间的竞态条件（中风险）

**问题**：用户可能在切换窗口期间按键：

```
时间线：
BLE 广播中 → 用户触发切换 → BLE 断开 → Gazell 启动
       ↑                              ↓
       └──── 用户正在按键 ────────────┘
```

**影响**：切换期间按键可能丢失或产生异常报告。

**缓解方案**：

```rust
struct ConnectionSwitcher {
    switching: AtomicBool,
    pending_events: Vec<KeyboardEvent, 16>,
}

impl ConnectionSwitcher {
    async fn switch(&mut self, target: ConnectionType) {
        // 1. 标记切换中 - 阻止新按键处理
        self.switching.store(true, Ordering::SeqCst);
        
        // 2. 刷新待处理报告
        self.flush_pending_reports().await;
        
        // 3. 执行切换
        // ...
        
        // 4. 清除标志
        self.switching.store(false, Ordering::SeqCst);
    }
}

// 键盘处理中：
if switcher.switching.load() {
    // 缓存切换期间的事件
    pending_events.push(event);
    return;
}
```

---

### 风险 D：内存布局冲突（低风险）

**问题**：两个协议栈都需要静态内存：

```rust
// BLE 资源（始终分配）
static mut HOST_RESOURCES: HostResources<...>; // 约 10KB

// Gazell 资源（始终分配）
static gz_state: GzState; // 约 100 字节

// 总计：约 10KB 在一个协议未激活时"浪费"
```

**影响**：在 nRF52840（256KB RAM）上可接受，但减少了其他功能的可用内存。

**缓解方案**：如果不需要运行时切换，可使用 `#[cfg(feature = "...")]` 在编译时排除未使用协议的静态资源。

---

## 风险矩阵

| 风险 | 严重性 | 可解决性 | 工作量 | 优先级 |
|-----|-------|---------|-------|-------|
| BLE 无 deinit | 高 | 中 | 大 | P0 |
| RADIO 重绑定 | 中 | 高 | 中 | P1 |
| 切换竞态 | 中 | 高 | 中 | P2 |
| RAM 浪费 | 低 | N/A | 无 | P3 |

---

## 实施路线图

### 第一阶段：验证 RADIO 切换（概念验证）

- [ ] 实现动态 RADIO 中断分发器
- [ ] 测试 BLE 可独立启动 → 停止 → 重启
- [ ] 测试 Gazell 可独立初始化 → 反初始化 → 重新初始化
- [ ] 验证两者可交替运行无需硬件复位

**验证标准**：可切换 10+ 次无崩溃或 RADIO 故障。

### 第二阶段：BLE 栈封装

- [ ] 将 `trouble-host` 封装为可重启服务
- [ ] 实现"伪销毁"（禁用广播 + 停止 runner）
- [ ] 测试 BLE runner 可被取消并重启
- [ ] 验证停止后的 RAM 状态（如果重新初始化可行）

**关键问题**：`trouble_host::new()` 可否多次调用？

### 第三阶段：统一连接管理器

- [ ] 实现 `ConnectionManager` trait
- [ ] 添加切换键码：`KeyCode::SwitchToBle`、`KeyCode::SwitchToGazell`
- [ ] 持久化连接类型到 Flash
- [ ] 事件系统集成用于 UI 反馈

### 第四阶段：生产加固

- [ ] 处理边界情况（活动连接期间切换）
- [ ] 添加 LED 指示当前模式
- [ ] 切换失败的超时处理
- [ ] 用户文档

---

## 待确认问题

1. **trouble-host 重新初始化**：`trouble_host::new()` 可否使用同一 `HostResources` 多次调用？
   - 需要：源码调查或实验验证

2. **切换时间要求**：切换可以接受多长时间？
   - < 100ms：用户无感知
   - 100ms - 1s：有 UI 反馈可接受
   - > 1s：可能需要其他方案

3. **RAM 充足性**：BLE + Gazell 静态资源是否放得下？
   - nRF52840：256KB RAM
   - BLE：约 10KB
   - Gazell：约 1KB
   - 剩余：应该充足

---

## 备选方案：方案B（双槽启动）

如果方案C 过于复杂，可退回方案B：

```
Flash 布局：
┌────────────────────────────────────┐ 0x0000
│  Bootloader（模式选择器）           │
├────────────────────────────────────┤ 0x1000
│  BLE 固件                          │
│  - 完整 BLE 栈                     │
│  - CONNECTION_TYPE = Ble           │
├────────────────────────────────────┤ 0x40000
│  Gazell 固件                       │
│  - Gazell 协议                     │
│  - CONNECTION_TYPE = Gazell        │
└────────────────────────────────────┘

切换流程：
1. 用户按切换键
2. 保存目标模式到 UICR/Flash
3. 触发 NVIC_SystemReset()
4. Bootloader 读取保存的模式
5. 跳转到对应固件地址
```

**权衡**：需要重启（约 1-2 秒），但无运行时复杂性。

---

## 参考资料

- `rmk/src/ble/mod.rs` - BLE 初始化
- `rmk/src/wireless/gazell.rs` - Gazell 封装
- `rmk-gazell-sys/c/gazell_shim.c` - C shim 层，含 `gz_deinit()`
- `embassy-nrf/src/interrupt.rs` - 中断绑定
- `trouble-host` crate - BLE 栈实现

---

## 更新日志

- 2026-03-13: 初始风险分析文档
