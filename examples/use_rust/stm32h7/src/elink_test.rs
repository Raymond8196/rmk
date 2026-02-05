//! ELink 单 Pad 测试模块
//!
//! 这个模块提供了在单个 pad 上测试 ELink 协议的功能，不需要 split 键盘的另一半。
//! 可以测试：
//! - ELink 编码/解码功能
//! - 缓冲区管理
//! - 性能测试（编码/解码速度）
//! - 内存使用情况

#![allow(dead_code)]

use defmt::{error, info, warn};
use elink_rmk_adapter::ElinkAdapter;
use embassy_time::{Duration, Instant, Timer};
use heapless;
use postcard;
use rmk::event::KeyboardEvent;
use rmk::split::SplitMessage;

/// ELink 测试统计信息
struct ElinkTestStats {
    encode_count: u32,
    decode_count: u32,
    encode_errors: u32,
    decode_errors: u32,
    crc_errors: u32,
    buffer_overflows: u32,
    total_encode_time_us: u64,
    total_decode_time_us: u64,
}

impl ElinkTestStats {
    fn new() -> Self {
        Self {
            encode_count: 0,
            decode_count: 0,
            encode_errors: 0,
            decode_errors: 0,
            crc_errors: 0,
            buffer_overflows: 0,
            total_encode_time_us: 0,
            total_decode_time_us: 0,
        }
    }

    fn print(&self) {
        info!("=== ELink 测试统计 ===");
        info!("编码次数: {}", self.encode_count);
        info!("解码次数: {}", self.decode_count);
        info!("编码错误: {}", self.encode_errors);
        info!("解码错误: {}", self.decode_errors);
        info!("CRC 错误: {}", self.crc_errors);
        info!("缓冲区溢出: {}", self.buffer_overflows);

        if self.encode_count > 0 {
            let avg_encode_time = self.total_encode_time_us / self.encode_count as u64;
            info!("平均编码时间: {} µs", avg_encode_time);
        }

        if self.decode_count > 0 {
            let avg_decode_time = self.total_decode_time_us / self.decode_count as u64;
            info!("平均解码时间: {} µs", avg_decode_time);
        }

        if self.encode_count > 0 && self.decode_count > 0 {
            let success_rate = ((self.decode_count as f32) / (self.encode_count as f32)) * 100.0;
            info!("成功率: {:.1}%", success_rate);
        }
    }
}

/// 测试 1: 基本编码/解码功能
pub async fn test_basic_encode_decode() {
    info!("=== 测试 1: 基本编码/解码 ===");

    let mut adapter = ElinkAdapter::new(
        0x1,  // device_class: Keyboard
        0x01, // device_address: 1
        0x0,  // sub_type: Central
    );

    let test_message = b"Hello, ELink!";

    // 编码
    match adapter.encode_message(test_message) {
        Ok(frame_bytes) => {
            info!("编码成功: {} 字节", frame_bytes.len());

            // 解码
            // 模拟接收：将编码后的帧数据输入到接收缓冲区
            match adapter.process_incoming_bytes(frame_bytes) {
                Ok(Some(decoded)) => {
                    if decoded == test_message {
                        info!("✅ 解码成功，数据匹配");
                    } else {
                        error!("❌ 解码数据不匹配");
                    }
                }
                Ok(None) => {
                    error!("❌ 解码返回 None");
                }
                Err(e) => {
                    error!("❌ 解码错误: {:?}", e);
                }
            }
        }
        Err(e) => {
            error!("❌ 编码错误: {:?}", e);
        }
    }
}

/// 测试 2: 不同大小的消息
pub async fn test_various_message_sizes() {
    info!("=== 测试 2: 不同大小的消息 ===");

    let mut adapter = ElinkAdapter::new(0x1, 0x01, 0x0);

    let sizes = [1, 8, 16, 32, 48, 56]; // 各种大小，最大 56 字节

    for size in sizes.iter() {
        let test_message = vec![0xAA; *size];

        match adapter.encode_message(&test_message) {
            Ok(frame_bytes) => {
                info!("大小 {} 字节: 编码成功，帧大小 {} 字节", size, frame_bytes.len());

                // 解码
                match adapter.process_incoming_bytes(frame_bytes) {
                    Ok(Some(decoded)) => {
                        if decoded == test_message.as_slice() {
                            info!("  ✅ 解码成功");
                        } else {
                            error!("  ❌ 解码数据不匹配");
                        }
                    }
                    Ok(None) => error!("  ❌ 解码返回 None"),
                    Err(e) => error!("  ❌ 解码错误: {:?}", e),
                }
            }
            Err(e) => {
                error!("大小 {} 字节: 编码错误: {:?}", size, e);
            }
        }

        Timer::after(Duration::from_millis(10)).await;
    }
}

/// 测试 3: 性能测试（编码/解码速度）
pub async fn test_performance() {
    info!("=== 测试 3: 性能测试 ===");

    let mut adapter = ElinkAdapter::new(0x1, 0x01, 0x0);
    let mut stats = ElinkTestStats::new();

    let test_message = b"Performance Test Message";
    const ITERATIONS: u32 = 100;

    info!("执行 {} 次编码/解码循环...", ITERATIONS);

    for i in 0..ITERATIONS {
        // 编码
        let encode_start = Instant::now();
        match adapter.encode_message(test_message) {
            Ok(frame_bytes) => {
                let encode_time = encode_start.elapsed().as_micros() as u64;
                stats.encode_count += 1;
                stats.total_encode_time_us += encode_time;

                // 解码
                let decode_start = Instant::now();
                match adapter.process_incoming_bytes(frame_bytes) {
                    Ok(Some(_)) => {
                        let decode_time = decode_start.elapsed().as_micros() as u64;
                        stats.decode_count += 1;
                        stats.total_decode_time_us += decode_time;
                    }
                    Ok(None) => {
                        stats.decode_errors += 1;
                    }
                    Err(e) => {
                        stats.decode_errors += 1;
                        match e {
                            elink_rmk_adapter::Error::InvalidCrc => stats.crc_errors += 1,
                            elink_rmk_adapter::Error::BufferTooSmall => stats.buffer_overflows += 1,
                            _ => {}
                        }
                    }
                }
            }
            Err(e) => {
                stats.encode_errors += 1;
                match e {
                    elink_rmk_adapter::Error::BufferTooSmall => stats.buffer_overflows += 1,
                    _ => {}
                }
            }
        }

        // 每 10 次打印一次进度
        if (i + 1) % 10 == 0 {
            info!("进度: {}/{}", i + 1, ITERATIONS);
        }

        Timer::after(Duration::from_millis(1)).await;
    }

    stats.print();
}

/// 测试 4: 缓冲区管理
pub async fn test_buffer_management() {
    info!("=== 测试 4: 缓冲区管理 ===");

    let mut adapter = ElinkAdapter::new(0x1, 0x01, 0x0);

    // 检查初始缓冲区状态
    // 注意：receive_buffer_len 是 pub(crate) 的，这里我们通过其他方式测试
    info!("开始缓冲区管理测试");

    // 发送多个消息，观察缓冲区使用
    let messages = [b"Message 1", b"Message 2", b"Message 3", b"Message 4", b"Message 5"];

    for (i, msg) in messages.iter().enumerate() {
        match adapter.encode_message(*msg) {
            Ok(frame_bytes) => {
                info!("消息 {}: 编码成功，帧大小 {} 字节", i + 1, frame_bytes.len());

                // 处理部分字节（模拟流式接收）
                let chunk_size = 8;
                let mut offset = 0;
                let mut processed = false;

                while offset < frame_bytes.len() {
                    let end = (offset + chunk_size).min(frame_bytes.len());
                    let chunk = &frame_bytes[offset..end];

                    match adapter.process_incoming_bytes(chunk) {
                        Ok(Some(decoded)) => {
                            info!("  消息 {}: 解码成功（从偏移 {} 开始）", i + 1, offset);
                            processed = true;
                            break;
                        }
                        Ok(None) => {
                            // 继续接收
                        }
                        Err(e) => {
                            warn!("  消息 {}: 处理错误: {:?}", i + 1, e);
                        }
                    }

                    offset = end;
                    Timer::after(Duration::from_millis(1)).await;
                }

                if !processed {
                    // 尝试处理剩余数据
                    match adapter.process_incoming_bytes(&[]) {
                        Ok(Some(decoded)) => {
                            info!("  消息 {}: 延迟解码成功", i + 1);
                        }
                        _ => {
                            warn!("  消息 {}: 未能完全解码", i + 1);
                        }
                    }
                }

                // 缓冲区状态通过处理结果间接观察
                info!("  消息 {} 处理完成", i + 1);
            }
            Err(e) => {
                error!("消息 {}: 编码错误: {:?}", i + 1, e);
            }
        }

        Timer::after(Duration::from_millis(10)).await;
    }

    // 清理缓冲区
    adapter.clear_receive_buffer();
    info!("缓冲区已清理");
}

/// 测试 5: 模拟 SplitMessage 编码/解码
pub async fn test_split_message() {
    info!("=== 测试 5: SplitMessage 编码/解码 ===");

    let mut adapter = ElinkAdapter::new(0x1, 0x01, 0x0);

    // 创建一个测试用的 SplitMessage
    // 注意：这里需要 postcard 序列化
    let test_key_event = KeyboardEvent::key(0, 0, true);
    let split_message = SplitMessage::Key(test_key_event);

    // 序列化 SplitMessage
    let mut serialized = heapless::Vec::<u8, 64>::new();
    match postcard::to_slice(&split_message, &mut serialized) {
        Ok(_) => {
            info!("SplitMessage 序列化成功: {} 字节", serialized.len());

            // 使用 ELink 编码
            match adapter.encode_message(&serialized) {
                Ok(frame_bytes) => {
                    info!("ELink 编码成功: 总帧大小 {} 字节", frame_bytes.len());

                    // 解码
                    match adapter.process_incoming_bytes(frame_bytes) {
                        Ok(Some(decoded)) => {
                            // 反序列化
                            match postcard::from_bytes::<SplitMessage>(decoded) {
                                Ok(decoded_message) => {
                                    if matches!(decoded_message, SplitMessage::Key(_)) {
                                        info!("✅ SplitMessage 解码成功");
                                    } else {
                                        error!("❌ 解码的消息类型不匹配");
                                    }
                                }
                                Err(e) => {
                                    error!("❌ SplitMessage 反序列化错误: {:?}", e);
                                }
                            }
                        }
                        Ok(None) => error!("❌ 解码返回 None"),
                        Err(e) => error!("❌ 解码错误: {:?}", e),
                    }
                }
                Err(e) => {
                    error!("❌ ELink 编码错误: {:?}", e);
                }
            }
        }
        Err(e) => {
            error!("❌ SplitMessage 序列化错误: {:?}", e);
        }
    }
}

/// Postcard+COBS 编码（模拟原有协议）
/// 简化版：在 no_std 环境中，我们模拟 Postcard+COBS 的编码
/// 实际 RMK 使用: SplitMessage -> Postcard -> COBS -> Serial
fn postcard_cobs_encode(message: &[u8], buffer: &mut [u8]) -> Result<usize, ()> {
    // 简化版 Postcard+COBS 编码
    // Postcard 序列化后，COBS 编码会增加一些开销
    // 最坏情况：n + ceil(n/254) 字节
    // 这里我们简化：假设 COBS 开销约为 1-2 字节（对于小消息）
    let cobs_overhead = if message.len() < 254 { 1 } else { 2 };
    let total_size = message.len() + cobs_overhead;
    
    if buffer.len() < total_size {
        return Err(());
    }
    
    // 模拟 COBS 编码：复制数据并添加结束标志
    // 实际 COBS 会更复杂，但这里我们只对比大小
    buffer[0..message.len()].copy_from_slice(message);
    buffer[message.len()] = 0x00; // COBS 结束标志
    
    Ok(total_size)
}

/// Postcard+COBS 解码（模拟原有协议）
fn postcard_cobs_decode(encoded: &[u8]) -> Result<heapless::Vec<u8, 64>, ()> {
    // 简化版：查找 0x00 作为结束标志（COBS 结束标志）
    if let Some(end_pos) = encoded.iter().position(|&b| b == 0x00) {
        let mut result = heapless::Vec::new();
        result.extend_from_slice(&encoded[..end_pos]).map_err(|_| ())?;
        Ok(result)
    } else {
        Err(())
    }
}

/// 测试 6: ELink vs Postcard+COBS 对比
pub async fn test_protocol_comparison() {
    info!("=== 测试 6: ELink vs Postcard+COBS 对比 ===");
    
    let mut elink_adapter = ElinkAdapter::new(0x1, 0x01, 0x0);
    let mut postcard_buffer = [0u8; 128];
    
    // 测试消息
    let test_messages: [&[u8]; 5] = [
        b"Test 1",
        b"Test Message 2",
        b"Test Message 3 - Longer",
        b"Test Message 4 - Even Longer Message",
        &[0xAA; 32], // 32 字节数据
    ];
    
    info!("");
    info!("消息大小对比:");
    info!("大小    | ELink帧      | Postcard+COBS | 开销差异");
    info!("--------+-------------+---------------+----------");
    
    for msg in test_messages.iter() {
        // ELink 编码
        let elink_frame_size = match elink_adapter.encode_message(msg) {
            Ok(frame_bytes) => frame_bytes.len(),
            Err(_) => {
                error!("ELink 编码失败");
                continue;
            }
        };
        
        // Postcard+COBS 编码
        let postcard_size = match postcard_cobs_encode(msg, &mut postcard_buffer) {
            Ok(size) => size,
            Err(_) => {
                error!("Postcard+COBS 编码失败");
                continue;
            }
        };
        
        let overhead_diff = elink_frame_size as i32 - postcard_size as i32;
        
        if overhead_diff > 0 {
            info!(
                "{:<6} | {:<12} | {:<12} | +{}",
                msg.len(),
                elink_frame_size,
                postcard_size,
                overhead_diff
            );
        } else {
            info!(
                "{:<6} | {:<12} | {:<12} | {}",
                msg.len(),
                elink_frame_size,
                postcard_size,
                overhead_diff
            );
        }
    }
    
    // 性能对比
    info!("");
    info!("性能对比 (100 次循环):");
    
    let test_message = b"Performance Comparison Test";
    const ITERATIONS: u32 = 100;
    
    // ELink 性能
    let mut elink_encode_time = 0u64;
    let mut elink_decode_time = 0u64;
    let mut elink_encode_count = 0u32;
    let mut elink_decode_count = 0u32;
    
    for _ in 0..ITERATIONS {
        // ELink 编码
        let encode_start = Instant::now();
        match elink_adapter.encode_message(test_message) {
            Ok(frame_bytes) => {
                let encode_time = encode_start.elapsed().as_micros() as u64;
                elink_encode_time += encode_time;
                elink_encode_count += 1;
                
                // ELink 解码
                let decode_start = Instant::now();
                match elink_adapter.process_incoming_bytes(frame_bytes) {
                    Ok(Some(_)) => {
                        let decode_time = decode_start.elapsed().as_micros() as u64;
                        elink_decode_time += decode_time;
                        elink_decode_count += 1;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    
    // Postcard+COBS 性能
    let mut postcard_encode_time = 0u64;
    let mut postcard_decode_time = 0u64;
    let mut postcard_encode_count = 0u32;
    let mut postcard_decode_count = 0u32;
    
    for _ in 0..ITERATIONS {
        // Postcard+COBS 编码
        let encode_start = Instant::now();
        match postcard_cobs_encode(test_message, &mut postcard_buffer) {
            Ok(encoded_size) => {
                let encode_time = encode_start.elapsed().as_micros() as u64;
                postcard_encode_time += encode_time;
                postcard_encode_count += 1;
                
                // Postcard+COBS 解码
                let decode_start = Instant::now();
                match postcard_cobs_decode(&postcard_buffer[..encoded_size]) {
                    Ok(_) => {
                        let decode_time = decode_start.elapsed().as_micros() as u64;
                        postcard_decode_time += decode_time;
                        postcard_decode_count += 1;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    
    // 输出对比结果
    info!("");
    info!("编码性能:");
    if elink_encode_count > 0 && postcard_encode_count > 0 {
        let elink_avg = elink_encode_time / elink_encode_count as u64;
        let postcard_avg = postcard_encode_time / postcard_encode_count as u64;
        let diff = elink_avg as i64 - postcard_avg as i64;
        let diff_pct = if postcard_avg > 0 {
            (diff as f32 / postcard_avg as f32) * 100.0
        } else {
            0.0
        };
        
        info!("  ELink:        {} µs/消息", elink_avg);
        info!("  Postcard+COBS: {} µs/消息", postcard_avg);
        if diff > 0 {
            info!("  ELink 慢:     {} µs (+{:.1}%)", diff, diff_pct);
        } else {
            info!("  ELink 快:     {} µs ({:.1}%)", -diff, -diff_pct);
        }
    }
    
    info!("");
    info!("解码性能:");
    if elink_decode_count > 0 && postcard_decode_count > 0 {
        let elink_avg = elink_decode_time / elink_decode_count as u64;
        let postcard_avg = postcard_decode_time / postcard_decode_count as u64;
        let diff = elink_avg as i64 - postcard_avg as i64;
        let diff_pct = if postcard_avg > 0 {
            (diff as f32 / postcard_avg as f32) * 100.0
        } else {
            0.0
        };
        
        info!("  ELink:        {} µs/消息", elink_avg);
        info!("  Postcard+COBS: {} µs/消息", postcard_avg);
        if diff > 0 {
            info!("  ELink 慢:     {} µs (+{:.1}%)", diff, diff_pct);
        } else {
            info!("  ELink 快:     {} µs ({:.1}%)", -diff, -diff_pct);
        }
    }
    
    // 功能对比
    info!("");
    info!("功能对比:");
    info!("  ELink:");
    info!("    ✅ CRC 校验");
    info!("    ✅ 优先级支持");
    info!("    ✅ 多设备支持");
    info!("    ✅ 扩展外设 ID");
    info!("  Postcard+COBS:");
    info!("    ❌ 无 CRC 校验");
    info!("    ❌ 无优先级");
    info!("    ❌ 无多设备支持");
    info!("    ✅ 更小的帧开销");
}

/// 运行所有测试
pub async fn run_all_tests() {
    info!("");
    info!("========================================");
    info!("ELink 单 Pad 测试套件");
    info!("========================================");
    info!("");

    test_basic_encode_decode().await;
    Timer::after(Duration::from_millis(100)).await;

    test_various_message_sizes().await;
    Timer::after(Duration::from_millis(100)).await;

    test_performance().await;
    Timer::after(Duration::from_millis(100)).await;

    test_buffer_management().await;
    Timer::after(Duration::from_millis(100)).await;

    test_split_message().await;
    Timer::after(Duration::from_millis(100)).await;

    test_protocol_comparison().await;

    info!("");
    info!("========================================");
    info!("所有测试完成");
    info!("========================================");
    info!("");
}
