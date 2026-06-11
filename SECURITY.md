# 安全策略

## 报告漏洞

Apexclaw 是端侧运行的本地智能体，不接收外部网络请求（LLM 调用除外）。

如果发现安全问题：

1. **不要**在公开 Issue 中披露
2. 发邮件或通过 GitHub Security Advisory 私密上报

## 安全边界

- 所有配置存储在 `~/.apexclaw/config.toml`，权限 600
- LLM URL 仅由用户手动配置，默认不连接任何外部服务
- 无 telemetry、无 analytics、无自动更新
