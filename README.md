# Chrono Harness Runner

`chrono-harness-runner` 提供 `chrono-harness` 命令和同名 Rust 库。宿主提供检查配置，runner 调用判官并返回真实退出状态与报告。

## 使用

在仓库根运行 `cargo build --locked --manifest-path crates/runner/Cargo.toml`，再运行 `crates/runner/target/debug/chrono-harness --help`。日志和报告的位置由调用宿主选择。

## 验证

`VERUS=/path/to/verus ./check.sh` 执行格式检查、锁定构建、测试及 Verus 验证。Rust 使用 1.95.0；Verus 使用 0.2026.04.24.f8e1704。标识符的生产实现和边界用例由 runner 维护。Verus 从本仓的 `verified_identifiers.rs` 生成证明元数据，其他证明根按需导入。

运行库与测试从 [原实现的固定提交](https://github.com/ChronoAIProject/chrono-harness/tree/5f6892dfa2f73675a84e4ebdc3b06eb83b824d6e/crates/runner) 分出。旧宿主的登记表检查由新的采用宿主负责。工作树、CI 生成器及安装器各自负责其命令替身的断言。

The runner checks its own pull requests with a digest-pinned released binary. The check compares the first parent with the PR merge commit, then runs the registered native and Verus check for changes affecting the runner. After a successful operation the freshly built candidate checks the same commits. For a local replay put the released `chrono-harness` and `chrono-judge-ci` binaries on PATH, set `VERUS`, and run:

```sh
chrono-harness check --config .chrono-harness/check.json --base "$base_oid" --candidate "$(git rev-parse HEAD)"
```

The local checkout must match the candidate. The report and operation streams are written to `.chrono-harness/state/`.

预览版从已合入 dev 的提交打标签，正式版从 main 发布。每个版本在本仓构建 Linux x86_64 和 macOS arm64 的单个 `chrono-harness` 二进制，并附 SHA256SUMS。下载所需标签的二进制和校验文件，验算目标平台的 SHA-256 后将它安装为 `chrono-harness`。升级或回退时按同样方式替换该程序。当前源码构建仍使用 Cargo.lock 中固定的 diagnostics 修订。
