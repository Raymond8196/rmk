#!/usr/bin/env python3
"""
ELink 协议监控脚本

用于在 PC 端监控 ELink 协议的性能和错误。

使用方法:
    python3 monitor_elink.py [--port PORT] [--baud BAUD] [--filter FILTER]

示例:
    python3 monitor_elink.py --port /dev/ttyUSB0 --baud 115200
    python3 monitor_elink.py --port COM3 --baud 115200 --filter elink
"""

import serial
import time
import re
import argparse
import sys
from datetime import datetime
from collections import defaultdict

class ElinkMonitor:
    def __init__(self, port, baud_rate=115200, filter_keywords=None):
        self.port = port
        self.baud_rate = baud_rate
        self.filter_keywords = filter_keywords or []
        
        # 统计信息
        self.stats = {
            'messages_sent': 0,
            'messages_received': 0,
            'errors': 0,
            'buffer_overflows': 0,
            'crc_errors': 0,
            'invalid_frames': 0,
            'parse_errors': 0,
            'total_lines': 0,
            'elink_lines': 0,
        }
        
        # 时间统计
        self.start_time = time.time()
        self.last_stats_time = time.time()
        self.message_times = []
        
        # 错误详情
        self.error_details = defaultdict(int)
        
    def parse_log_line(self, line):
        """解析日志行，提取 ELink 相关信息"""
        line = line.strip()
        if not line:
            return
        
        self.stats['total_lines'] += 1
        
        # 检查是否包含过滤关键词
        if self.filter_keywords:
            if not any(keyword.lower() in line.lower() for keyword in self.filter_keywords):
                return
        
        # 检测 ELink 相关日志
        is_elink = False
        if 'elink' in line.lower() or 'ELink' in line:
            is_elink = True
            self.stats['elink_lines'] += 1
            print(f"[{datetime.now().strftime('%H:%M:%S')}] {line}")
        
        # 统计错误
        if 'error' in line.lower() or 'Error' in line or 'ERROR' in line:
            self.stats['errors'] += 1
            # 提取错误类型
            if 'buffer' in line.lower() and ('small' in line.lower() or 'overflow' in line.lower()):
                self.stats['buffer_overflows'] += 1
                self.error_details['buffer_overflow'] += 1
            elif 'CRC' in line or 'crc' in line:
                self.stats['crc_errors'] += 1
                self.error_details['crc_error'] += 1
            elif 'invalid' in line.lower() and 'frame' in line.lower():
                self.stats['invalid_frames'] += 1
                self.error_details['invalid_frame'] += 1
            elif 'parse' in line.lower():
                self.stats['parse_errors'] += 1
                self.error_details['parse_error'] += 1
        
        # 检测消息统计
        if 'message' in line.lower():
            if 'sent' in line.lower() or 'send' in line.lower():
                self.stats['messages_sent'] += 1
                self.message_times.append(time.time())
            if 'received' in line.lower() or 'recv' in line.lower():
                self.stats['messages_received'] += 1
        
        # 检测缓冲区使用
        if 'buffer' in line.lower():
            # 尝试提取缓冲区使用率
            match = re.search(r'buffer.*?(\d+).*?(\d+)', line, re.IGNORECASE)
            if match:
                used = int(match.group(1))
                total = int(match.group(2))
                usage = (used / total) * 100 if total > 0 else 0
                if usage > 80:
                    print(f"  ⚠️  缓冲区使用率: {usage:.1f}%")
    
    def print_stats(self):
        """打印统计信息"""
        elapsed = time.time() - self.start_time
        print("\n" + "="*60)
        print(f"ELink 统计信息 (运行时间: {elapsed:.0f} 秒)")
        print("="*60)
        print(f"  总日志行数: {self.stats['total_lines']}")
        print(f"  ELink 相关日志: {self.stats['elink_lines']}")
        print(f"  发送消息: {self.stats['messages_sent']}")
        print(f"  接收消息: {self.stats['messages_received']}")
        
        if self.stats['messages_sent'] > 0:
            success_rate = (self.stats['messages_received'] / self.stats['messages_sent']) * 100
            print(f"  成功率: {success_rate:.1f}%")
        
        if elapsed > 0:
            msg_rate = self.stats['messages_sent'] / elapsed
            print(f"  消息速率: {msg_rate:.1f} 消息/秒")
        
        print(f"\n  错误统计:")
        print(f"    总错误数: {self.stats['errors']}")
        print(f"    缓冲区溢出: {self.stats['buffer_overflows']}")
        print(f"    CRC 错误: {self.stats['crc_errors']}")
        print(f"    无效帧: {self.stats['invalid_frames']}")
        print(f"    解析错误: {self.stats['parse_errors']}")
        
        if self.error_details:
            print(f"\n  错误详情:")
            for error_type, count in self.error_details.items():
                print(f"    {error_type}: {count}")
        
        print("="*60 + "\n")
    
    def run(self):
        """运行监控"""
        try:
            ser = serial.Serial(self.port, self.baud_rate, timeout=1)
            print(f"连接到 {self.port}，波特率 {self.baud_rate}")
            print("按 Ctrl+C 退出\n")
            
            while True:
                if ser.in_waiting:
                    try:
                        line = ser.readline().decode('utf-8', errors='ignore')
                        self.parse_log_line(line)
                    except UnicodeDecodeError:
                        # 忽略解码错误
                        pass
                
                # 每 10 秒打印一次统计
                if time.time() - self.last_stats_time > 10:
                    self.print_stats()
                    self.last_stats_time = time.time()
                
                time.sleep(0.01)
                
        except KeyboardInterrupt:
            print("\n\n最终统计:")
            self.print_stats()
            if ser.is_open:
                ser.close()
        except serial.SerialException as e:
            print(f"串口错误: {e}")
            print("\n请检查:")
            print(f"  1. 串口设备是否存在: {self.port}")
            print(f"  2. 是否有权限访问串口")
            print(f"  3. 波特率是否正确: {self.baud_rate}")
            sys.exit(1)
        except Exception as e:
            print(f"错误: {e}")
            sys.exit(1)

def main():
    parser = argparse.ArgumentParser(description='ELink 协议监控工具')
    parser.add_argument('--port', '-p', 
                       default='/dev/ttyUSB0',
                       help='串口设备路径 (默认: /dev/ttyUSB0)')
    parser.add_argument('--baud', '-b',
                       type=int,
                       default=115200,
                       help='波特率 (默认: 115200)')
    parser.add_argument('--filter', '-f',
                       nargs='+',
                       default=['elink'],
                       help='过滤关键词 (默认: elink)')
    
    args = parser.parse_args()
    
    monitor = ElinkMonitor(args.port, args.baud, args.filter)
    monitor.run()

if __name__ == '__main__':
    main()
