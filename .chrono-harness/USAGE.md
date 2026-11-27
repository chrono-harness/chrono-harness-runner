# chrono-harness-runner 使用说明

组件的用途、开发仓库和示例见 [Chrono Harness 用户与开发者入口](https://github.com/chrono-harness/chrono-harness-main)。

`chrono-harness-runner` 提供 `chrono-harness` 命令和同名 Rust 库。宿主提供检查配置，runner 调用判官并返回真实退出状态与报告。

开发分支为 `dev`。日常 PR 合入开发分支，正式版从 `main` 发布。

首次检出运行 `mise trust` 和 `mise run harness-setup`。本仓默认安装策略为 `pinned`。固定配方读取 [.config/mise/conf.d/chrono-harness.toml](../.config/mise/conf.d/chrono-harness.toml)。模板答案 `tool_versions` 可以为每个工具选择 stable、preview 或精确标签。临时运行 `CHRONO_TOOL_CHANNEL=preview mise run harness-setup` 会为已选组织工具安装各自最新测试版，也可以把 `preview` 换成 `stable` 或精确的 `v` 标签。再次运行普通安装即可恢复本仓选择。各分支、标签和 detached HEAD 使用相同规则。

开始任务时运行 `mise run harness-setup` 刷新根指南，成功后重新阅读本仓的 `AGENTS.md` 或 `CLAUDE.md`。完整规则和本仓开发步骤见根 [CLAUDE.md](../CLAUDE.md)。

用 `mise exec -- chrono-pr pr-reminders` 查看本地提醒，再运行 `mise exec -- chrono-pr pr-status --pr NUMBER --head-sha SHA` 查询 GitHub。

## 本仓验证

产品位于 [`crates/runner`](../crates/runner)。原生测试位于 [`crates/runner-tests`](../crates/runner-tests)。检查配置位于 [`.chrono-harness/check.json`](../.chrono-harness/check.json)，本仓 PR 会用已发布的 runner 检查候选。在仓库根目录运行 `mise exec -- ./check.sh` 执行本仓的检查。

本仓 PR 使用已发布的 runner 核对候选。复验 PR 的两个固定提交时，在候选检出中设置 `VERUS` 和基线提交 `base_oid`，运行 `mise exec -- ./check.sh --scoped "$(mise which chrono-harness)" "$base_oid" "$(git rev-parse HEAD)"`。
