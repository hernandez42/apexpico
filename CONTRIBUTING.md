# 贡献指南

感谢你对 Apexclaw 感兴趣！

## 开发流程

1. Fork 本仓库
2. 从 `main` 创建特性分支：`git checkout -b feature/xxx`
3. 提交前保证：
   - `cargo test` 全部通过
   - `cargo clippy -- -D warnings` 无警告
   - `cargo build --release` 编译成功
4. 提交 PR → CI 自动跑测试

## 测试要求

- 新公式必须带真实计算测试（禁止 mock/模拟）
- 配置变更必须带解析/序列化测试
- PR 合并前所有 CI 门禁必须绿

## 代码风格

- Rust 2024 edition
- 变量命名：snake_case
- 类型命名：PascalCase
- 常量命名：SCREAMING_SNAKE_CASE
- 公开 API 必须有 doc comment
