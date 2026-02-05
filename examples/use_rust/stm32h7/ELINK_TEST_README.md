# ELink 测试配置指南

## 分支信息

当前在测试分支: `test/elink-stm32h7-pad`

## 重要说明：通信协议架构

### Pad 和 PC 之间的通信

**Pad ↔ PC 使用 USB HID 协议**（标准键盘协议）

- 这是标准的 USB HID 键盘通信
- 使用 `usbd-hid` 库实现
- 与 ELink 协议无关
- 无论是否启用 ELink，pad 和 PC 之间都是 USB HID

### Split 键盘之间的通信

**Central ↔ Peripheral 使用 ELink 协议**（如果启用）或 Serial/BLE（默认）

- ELink 协议只用于 split 键盘的两半之间通信
- 如果启用 `elink` feature，split 键盘之间使用 ELink 协议
- 如果未启用，使用默认的 Serial 或 BLE 协议

### 通信架构图

```
┌─────────┐                    ┌─────────┐
│   PC    │                    │   PC    │
└────┬────┘                    └────┬────┘
     │ USB HID                      │ USB HID
     │                               │
┌────▼────┐                    ┌────▼────┐
│ Central │                    │Peripheral│
│  (Pad)  │◄─── ELink/Serial ──►│  (Pad)  │
└─────────┘                    └─────────┘
```

**总结**：
- ✅ Pad ↔ PC: **USB HID**（标准键盘协议，不受 ELink 影响）
- ✅ Central ↔ Peripheral: **ELink**（如果启用）或 **Serial/BLE**（默认）

## 启用 ELink

### 1. 修改 Cargo.toml

在 `examples/use_rust/stm32h7/Cargo.toml` 中，修改 rmk 依赖：

```toml
[dependencies]
rmk = { path = "../../../rmk", features = ["split", "elink", "defmt"] }
```

### 2. 配置 ELink 参数

ELink 需要以下参数：
- `device_class`: 4-bit 设备类型（0x1 = Keyboard）
- `device_address`: 8-bit 设备地址（0-255）
- `sub_type`: 4-bit 子类型（0x0 = Central, 0x1 = Peripheral）

**注意**: 目前 ELink 集成还在进行中，`run_elink_peripheral_manager` 是 `pub(crate)` 的，需要等待完整的集成。

## 单 Pad 测试

如果你只有一个 pad，可以使用内置的测试套件：

### 启用测试

在编译时启用 `elink_test` feature：

```bash
cargo build --release --features elink_test
```

测试会在设备启动后自动运行（等待 2 秒后开始），包括：
- 基本编码/解码测试
- 不同大小的消息测试
- 性能测试（100 次循环）
- 缓冲区管理测试
- SplitMessage 编码/解码测试

详细说明请参考 `ELINK_SINGLE_PAD_TEST.md`。

## PC 端观测方法

### 方法 1: 使用 defmt 日志（推荐）

#### 1.1 安装工具

```bash
# 安装 probe-rs
cargo install probe-rs --features cli

# 或使用 defmt-print
cargo install defmt-print
```

#### 1.2 连接设备并查看日志

```bash
# 使用 probe-rs
probe-rs attach --chip stm32h7b0vb --defmt

# 或使用 defmt-print（如果使用 RTT）
defmt-print -e target/thumbv7em-none-eabihf/debug/rmk-stm32h7
```

#### 1.3 过滤 ELink 相关日志

```bash
# 只显示 ELink 相关日志
probe-rs attach --chip stm32h7b0vb --defmt | grep -i elink

# 或使用更详细的过滤
probe-rs attach --chip stm32h7b0vb --defmt | grep -E "(ELink|elink|split|buffer)"
```

### 方法 2: 使用串口监控

#### 2.1 查找串口设备

```bash
# Linux
ls /dev/ttyUSB* /dev/ttyACM*

# macOS
ls /dev/cu.usb*

# Windows
# 在设备管理器中查找 COM 端口
```

#### 2.2 使用 minicom/screen/picocom

```bash
# Linux: 使用 picocom（推荐，支持 defmt）
picocom -b 115200 /dev/ttyUSB0

# 或使用 minicom
minicom -D /dev/ttyUSB0 -b 115200

# macOS: 使用 screen
screen /dev/cu.usbmodem* 115200
```

#### 2.3 使用 Python 脚本监控

创建 `monitor_elink.py`:

```python
#!/usr/bin/env python3
import serial
import time
import re

# 配置串口
SERIAL_PORT = '/dev/ttyUSB0'  # 根据实际情况修改
BAUD_RATE = 115200

# 统计信息
stats = {
    'messages_sent': 0,
    'messages_received': 0,
    'errors': 0,
    'buffer_overflows': 0,
    'crc_errors': 0,
}

def parse_log_line(line):
    """解析日志行，提取 ELink 相关信息"""
    line = line.strip()
    
    # 检测 ELink 相关日志
    if 'elink' in line.lower() or 'ELink' in line:
        print(f"[ELink] {line}")
        
        # 统计错误
        if 'error' in line.lower() or 'Error' in line:
            stats['errors'] += 1
        if 'buffer' in line.lower() and 'small' in line.lower():
            stats['buffer_overflows'] += 1
        if 'CRC' in line or 'crc' in line:
            stats['crc_errors'] += 1
    
    # 检测消息统计
    if 'message' in line.lower():
        if 'sent' in line.lower():
            stats['messages_sent'] += 1
        if 'received' in line.lower() or 'recv' in line.lower():
            stats['messages_received'] += 1

def print_stats():
    """打印统计信息"""
    print("\n" + "="*50)
    print("ELink 统计信息:")
    print(f"  发送消息: {stats['messages_sent']}")
    print(f"  接收消息: {stats['messages_received']}")
    print(f"  错误总数: {stats['errors']}")
    print(f"  缓冲区溢出: {stats['buffer_overflows']}")
    print(f"  CRC 错误: {stats['crc_errors']}")
    if stats['messages_sent'] > 0:
        success_rate = (stats['messages_received'] / stats['messages_sent']) * 100
        print(f"  成功率: {success_rate:.1f}%")
    print("="*50 + "\n")

def main():
    try:
        ser = serial.Serial(SERIAL_PORT, BAUD_RATE, timeout=1)
        print(f"连接到 {SERIAL_PORT}，波特率 {BAUD_RATE}")
        print("按 Ctrl+C 退出\n")
        
        last_stats_time = time.time()
        
        while True:
            if ser.in_waiting:
                line = ser.readline().decode('utf-8', errors='ignore')
                parse_log_line(line)
            
            # 每 10 秒打印一次统计
            if time.time() - last_stats_time > 10:
                print_stats()
                last_stats_time = time.time()
            
            time.sleep(0.01)
            
    except KeyboardInterrupt:
        print("\n\n最终统计:")
        print_stats()
        ser.close()
    except Exception as e:
        print(f"错误: {e}")

if __name__ == '__main__':
    main()
```

运行：

```bash
python3 monitor_elink.py
```

### 方法 3: 使用 Wireshark/串口分析工具

#### 3.1 使用 socat 转发到文件

```bash
# 将串口数据转发到文件
socat - /dev/ttyUSB0,b115200,raw,echo=0 > elink_log.txt

# 在另一个终端实时查看
tail -f elink_log.txt | grep -i elink
```

#### 3.2 使用串口分析工具

- **Linux**: `cutecom`, `gtkterm`
- **macOS**: `Serial` (App Store)
- **Windows**: `PuTTY`, `Tera Term`

### 方法 4: 添加性能统计代码（需要修改固件）

如果需要更详细的统计，可以在固件中添加性能监控代码：

```rust
// 在 ElinkSplitDriver 中添加统计
struct ElinkStats {
    messages_sent: u32,
    messages_received: u32,
    errors: u32,
    buffer_overflows: u32,
    crc_errors: u32,
}

// 定期打印统计信息
async fn print_stats_periodically() {
    loop {
        embassy_time::Timer::after(embassy_time::Duration::from_secs(10)).await;
        info!(
            "ELink Stats: sent={}, recv={}, errors={}, buffer_overflow={}, crc_errors={}",
            stats.messages_sent,
            stats.messages_received,
            stats.errors,
            stats.buffer_overflows,
            stats.crc_errors
        );
    }
}
```

## 测试场景

### 1. 正常使用测试

- 正常打字（10-100 消息/秒）
- 观察是否有丢包
- 观察延迟是否可接受

### 2. 快速连击测试

- 快速连续按键（200+ 消息/秒）
- 观察缓冲区使用情况
- 观察是否有丢包

### 3. 长时间运行测试

- 运行数小时
- 观察内存使用
- 观察是否有内存泄漏

### 4. 缓冲区压力测试

- 发送大量消息
- 观察缓冲区使用率
- 观察是否有溢出

## 预期日志输出

启用 ELink 后，你应该看到类似以下的日志：

```
[INFO] Running ELink peripheral manager 0
[INFO] ELink adapter initialized: device_class=0x1, address=0x01, sub_type=0x1
[DEBUG] ELink frame encoded: size=19 bytes
[DEBUG] ELink frame decoded: size=19 bytes
```

如果出现问题，可能会看到：

```
[ERROR] ELink buffer too small
[ERROR] ELink CRC error
[ERROR] ELink invalid frame
```

## 故障排查

### 问题 1: 没有日志输出

- 检查 defmt 是否正确配置
- 检查串口连接
- 检查波特率设置

### 问题 2: 缓冲区溢出

- 检查缓冲区大小（当前 512 字节）
- 考虑增加到 768 或 1024 字节
- 检查消息发送频率

### 问题 3: CRC 错误

- 检查传输线路质量
- 检查波特率设置
- 检查是否有干扰

## 下一步

1. 在实际硬件上测试
2. 收集性能数据
3. 根据测试结果优化配置
