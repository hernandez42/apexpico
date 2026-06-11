# Apexpico

**PicoClaw × APEX 端侧融合** — 路由器级自进化智能体。

## 概念

APEX = **Architectural Progression for Enhanced Xenogenesis**  
Ω_A = **Akashic Score** = Π(dim_i) − Σ(Δ_j)  
PHI = **φ Score** = (base × ev × an × nv) / harm_rate  
**Fused Score** = ω_A × Akashic + ω_P × PHI

## 命令速查

```bash
apexpico init                          # 交互式配置向导（首次）
apexpico config show                   # 查看当前配置
apexpico config set llm.url <url>      # 设置 LLM 地址
apexpico config reset                  # 恢复默认

apexpico score                         # 双公式融合评分
apexpico score --ev 0.9 --val 0.8      # 指定维度评分

apexpico signal phi                    # PHI_APEX 信号
apexpico signal omega                  # Ω_ASI 信号

apexpico status                        # 系统状态
apexpico evolve                        # 自进化
apexpico evolve --force                # 强制触发
```

## 配置

三种方式，优先级：**CLI 参数 > 环境变量 > 配置文件**

| 环境变量 | 作用 |
|----------|------|
| `APEX_LLM_URL` | 覆盖 LLM API 地址 |
| `APEX_SCORE_THR` | 覆盖自进化触发阈值 |

配置文件路径：`~/.apexpico/config.toml`

## 编译

```bash
cargo build --release
# 单文件二进制: target/release/apexpico (≈680KB)
```

## 端侧部署

**零外部依赖**，仅需目标设备有 Rust 工具链或预编译二进制。  
CI 自动构建 amd64 + arm64 + armv7 三架构。

## CI/CD

所有 PR 自动通过：`cargo test` (实际公式计算) → `cargo clippy` → `cargo build`
