# 提交计划

## 当前状态总结

### 主仓库 (rmk) 变更

#### 已修改的文件
1. **rmk/Cargo.toml** - 添加 elink feature 配置
   - 修改了 elink-rmk-adapter 依赖配置
   - 添加了 elink feature 说明注释

2. **rmk/src/split/mod.rs** - 添加 elink 模块条件编译
   - 添加了 `#[cfg(feature = "elink")] pub mod elink;`

3. **examples/use_rust/nrf52840_ble_split_dongle/Cargo.lock** - 自动生成的锁文件
   - 可以忽略或提交

#### 新增的文件
1. **rmk/src/split/elink/mod.rs** (~191 行)
   - ELink split driver 实现
   - 实现了 SplitReader 和 SplitWriter trait

2. **ELINK_INTEGRATION_PLAN.md** - 集成计划文档
3. **ELINK_PROTOCOL_EVALUATION.md** - 协议评估文档
4. **ELINK_USAGE.md** - 使用指南

### 子模块 (elink-protocol) 变更

#### 已修改的文件
1. **elink-rmk-adapter/Cargo.toml**
   - 移除了 rmk 依赖（解决循环依赖）
   - 移除了 rmk feature

2. **elink-rmk-adapter/src/message_mapper.rs**
   - 移除了 `#[cfg(feature = "rmk")]` 条件
   - 简化了函数签名

3. **elink-rmk-adapter/src/test_utils.rs**
   - 修改了 `send_central_to_peripheral` 和 `send_peripheral_to_central` 返回类型

#### 新增的文件
1. **elink-rmk-adapter/README.md** - 适配器 README
2. **elink-rmk-adapter/examples/benchmark.rs** - 性能基准测试

## 建议的提交分组

### 方案 A: 按功能分组（推荐）

#### 提交 1: 子模块 - 修复循环依赖
```bash
cd elink-protocol
git add elink-rmk-adapter/Cargo.toml elink-rmk-adapter/src/message_mapper.rs
git commit -m "refactor: 移除 rmk 依赖，解决循环依赖问题"
```

#### 提交 2: 子模块 - 测试工具改进
```bash
cd elink-protocol
git add elink-rmk-adapter/src/test_utils.rs
git commit -m "feat: 改进测试工具，返回帧大小用于统计"
```

#### 提交 3: 子模块 - 文档和基准测试
```bash
cd elink-protocol
git add elink-rmk-adapter/README.md elink-rmk-adapter/examples/benchmark.rs
git commit -m "docs: 添加 README 和性能基准测试示例"
```

#### 提交 4: 主仓库 - ELink 集成核心代码
```bash
cd ..
git add rmk/Cargo.toml rmk/src/split/mod.rs rmk/src/split/elink/
git commit -m "feat: 集成 ELink 协议到 RMK split 模块

- 添加 elink feature flag，支持一键启用/禁用
- 实现 ElinkSplitDriver，支持 SplitReader/SplitWriter trait
- 禁用时完全排除编译，节省固件大小"
```

#### 提交 5: 主仓库 - 文档
```bash
git add ELINK_*.md
git commit -m "docs: 添加 ELink 协议相关文档

- ELINK_INTEGRATION_PLAN.md: 集成计划
- ELINK_PROTOCOL_EVALUATION.md: 协议评估分析
- ELINK_USAGE.md: 使用指南"
```

#### 提交 6: 主仓库 - 更新子模块引用
```bash
git add elink-protocol
git commit -m "chore: 更新 elink-protocol 子模块引用"
```

### 方案 B: 按仓库分组（更简单）

#### 提交 1: 子模块所有变更
```bash
cd elink-protocol
git add .
git commit -m "feat: 修复循环依赖，添加测试工具和文档

- 移除 rmk 依赖，解决循环依赖问题
- 改进测试工具，支持帧大小统计
- 添加 README 和性能基准测试示例"
```

#### 提交 2: 主仓库核心代码
```bash
cd ..
git add rmk/Cargo.toml rmk/src/split/mod.rs rmk/src/split/elink/
git commit -m "feat: 集成 ELink 协议到 RMK split 模块"
```

#### 提交 3: 主仓库文档
```bash
git add ELINK_*.md
git commit -m "docs: 添加 ELink 协议相关文档"
```

#### 提交 4: 更新子模块引用
```bash
git add elink-protocol
git commit -m "chore: 更新 elink-protocol 子模块引用"
```

## 文件大小统计

- `rmk/src/split/elink/mod.rs`: ~191 行
- `ELINK_INTEGRATION_PLAN.md`: ~200+ 行
- `ELINK_PROTOCOL_EVALUATION.md`: ~400+ 行
- `ELINK_USAGE.md`: ~200+ 行

## 注意事项

1. **Cargo.lock**: `examples/use_rust/nrf52840_ble_split_dongle/Cargo.lock` 是自动生成的，可以选择提交或忽略
2. **子模块**: 需要先在子模块中提交，然后在主仓库中更新引用
3. **测试**: 建议在提交前运行 `cargo check --features split,elink` 确保编译通过
