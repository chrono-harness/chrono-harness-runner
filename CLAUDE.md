# 仓库工作说明

工作前阅读本仓的[公共约定和工具入口](.chrono-harness/USAGE.md)。产品职责见本仓 README。

## 本仓开发

产品位于 [`crates/runner`](crates/runner)。原生测试位于 [`crates/runner-tests`](crates/runner-tests)。检查配置位于 [`.chrono-harness/check.json`](.chrono-harness/check.json)，本仓 PR 会用已发布的 runner 检查候选。在仓库根目录运行 `mise exec -- ./check.sh` 执行本仓的检查。
