# Chrono Harness Runner

`chrono-harness-runner` 提供 `chrono-harness` 命令和同名 Rust 库。宿主提供检查配置，runner 调用判官并返回真实退出状态与报告。

## 使用

在仓库根运行 `cargo build --locked --manifest-path crates/runner/Cargo.toml`，再运行 `crates/runner/target/debug/chrono-harness --help`。日志和报告的位置由调用宿主选择。

## 验证

`VERUS=/path/to/verus ./check.sh` 执行格式检查、锁定构建、测试及 Verus 验证。Rust 使用 1.95.0；Verus 使用 0.2026.04.24.f8e1704。验证从 Cargo 锁定的 identifiers 源码生成证明元数据，runner 的证明直接导入该库。

运行库与测试从 [原实现的固定提交](https://github.com/ChronoAIProject/chrono-harness/tree/5f6892dfa2f73675a84e4ebdc3b06eb83b824d6e/crates/runner) 分出。旧宿主的登记表检查由新的采用宿主负责。工作树、CI 生成器及安装器各自负责其命令替身的断言。
