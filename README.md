# Chrono Harness Runner

`chrono-harness run-command-v1` 在当前目录按宿主提供的命令、环境、时限和输出限额运行外部程序。stdin 第一行是 JSON CommandSpec，另可提供 `expected_sha256` 固定程序身份。其余字节原样送入子进程。命令头最多 1 MiB，输入最多 64 MiB。执行成功后，首行 JSON 含 `exit_code`、`stdout_length` 和 `stderr_length`，随后依次为原始 stdout 和 stderr。执行器失败返回 2，在 stderr 输出原错误。

`chrono-harness judge-command-v1 NAME VERSION PROGRAM OPERATION ERROR_CODE --protocol chrono-judge/v1` 将同一个已封存请求交给单独安装的判官。runner 负责解析请求、校验封存、解析宿主配置、限制进程以及写出原生响应。PROGRAM 的 stdin 含 `request` 和从该宿主选出的 `protocol`，stdout 返回 `status`、`findings` 和 `outputs`。调用方声明接纳或传输失败时对外使用的错误码。`prepare-command-v1 PROGRAM OPERATION` 使用同一套请求和进程限制，接收绑定 request_id 的 judge-pipeline 输入生产者响应，包括业务失败时已记录的 outputs。

`chrono-harness-runner` 提供 `chrono-harness` 命令和同名 Rust 库。宿主提供检查配置，runner 调用判官并返回真实退出状态与报告。

`chrono-harness retained-artifacts-v1` 为外部生产者在当前仓库保持一次读取作用域。每行一个 JSON 请求，回复一行 `Ok` 或 `Err`。涉及创建暂存、写原件、依赖登记、读取与消费者保管的操作使用宿主已有 retention 策略。读取能力持续到调用方关闭 stdin；durable consumer 由后续的实际使用方通过原有 retention 命令释放。


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

已安装的外部判官可将协议处理交给 `chrono-harness judge-pipeline NAME VERSION PRODUCER OPERATION EVALUATOR OPERATION --protocol chrono-judge/v1`。runner 在已登记的限额与环境下，向输入生产者发送封存请求。生产者返回请求 ID、观察到的输出以及准备好的 JSON 或原错误。求值程序只读取该 JSON，返回状态、findings 与 outputs。两个程序均通过标准输入输出通信，响应共享请求身份，重复输出键会报错。

`chrono-harness git-facts-v1` 在当前宿主打开一次 Git 读取会话。stdin 和 stdout 使用逐行 JSON，每行请求得到一个 `{"Ok":...}` 或 `{"Err":"原始诊断"}`，关闭 stdin 会结束会话。首行是 `{"mode":"config","config":"…","base":"完整提交 ID","candidate":"完整提交 ID"}`，或 `{"mode":"request","request":封存请求}`。打开成功返回 `{"bound":true|false}`。单行输入上限为 64 MiB。

后续请求的 `op` 可取 `delta`、`observation`，或带 `endpoint`（`base` 或 `candidate`）的 `verify-tree`、`registry-snapshot`、`checked-out-snapshot`、`json-blob`。`json-blob` 另需相对路径 `path`。快照返回 `originals`、`values`、`effective_path`、`selection`，检出快照还核对 HEAD 和配置原字节。发生操作错误后仍可读取该会话已执行的 Git 观察，再关闭输入并等待进程退出。

`git-facts-v1` 的 `checkout-excluding` 接收宿主登记的 `artifacts` 路径列表，返回当前固定候选的 HEAD、跟踪变更、范围外的未跟踪路径和 index 标志。项目判官在执行前后使用同一 Git 读取会话核对候选输入，并将 Git 观察纳入响应。


独立 routes 通过 `source-coverage-v1` 提交两个固定端点、受影响节点、登记方法的 owner/tool/operation 与选中证明调用的 argv、输出合同和输出上限。runner 返回两端提交、实际 Git 观察及首个验证错误。Git 来源支持原判官封存请求、已有观察和旧式未绑定读取。完整库存与选中根使用同一份连续性证据。

`check-invocation-v1` 在 `template` 模式核对原始入口、参数模板和绑定，在 `request` 模式核对封存请求、宿主配置的规范入口和短式准备原件。成功输出 JSON null，失败返回 2 并写明原因。

`installed-query-v1` 按调用方声明的工作目录、程序、操作与环境执行已安装的 JSON 查询，保留原输出。调用方用 `inherited` 保留启动坐标，或用 `search-path` 指定登记查询只继承 PATH。

`judge-command-v1` 的 `--inherited` 选项供保留原启动坐标和环境的判官使用。它将已核验的请求作为 `{"request":...}` 传入求值命令；准入失败使用 fail 和原错误前缀。

公开的 Git 会话也接受 `verify-commit` 和 `ancestors`（参数 `commit` 为完整 OID）、`check-branch-name`（参数 `branch`）及 `shallow-boundaries`。已封存请求内的读取使用同一个宿主绑定的 Git 进程记录。只检查调用方给出的分支上下文时，可以用 `{"mode":"history"}` 打开无配置的历史读取会话。

`chrono-harness judge-evidence-v1` 读取逐行 JSON 并逐行回复 `{"Ok":值}` 或 `{"Err":"错误"}`。workflow 通过 `expand-record`、`check-process`、`report-transport-bytes` 和 `retained-judges` 重放 runner 报告。`retained-judges` 校验封存的原请求、登记顺序、每个实际进程的原始流与响应，返回所选判官的输入请求和汇总状态。协议还提供严格 JSON 解码（`decode-json`）、原请求校验（`validate-request`）和按选择计算执行单元（`required-units`）。`retained-artifacts-v1` 的 `checked-path` 核对已登记相对路径的符号链接，读取租约仍由同一保管会话持有至 EOF。

`judge-command-v1` 的 `--admission-result` 选项将 `{"request":请求,"admission":{"Ok":null}}` 或带原错误的 `{"Err":"…"}` 交给独立判官。判官可对准入失败使用其已登记的 finding 和严重程度。

`retained-artifacts-v1` 的外部执行者用 `begin-directory` 打开原件目录，`write-directory` 在登记目录中写入意图和结果，`execute-retained` 按命令、SHA-256、超时与输出上限执行一次，并保留 stdout、stderr 和 launch.json。`check-directory-write` 再核对保管预算。`finish-directory` 接受最终结果和输入依赖，在业务进程已结束后封存。生成分配、判定执行结果与决定释放时机由项目负责。

安装的判官通过 `judge-command-v1` 返回决策时，可以在 `evidence` 数组里列出已生成的证据文件。runner 将其路径、SHA-256 和种类随判官响应交给实际调用方，省略该字段表示没有证据文件。

在报告收集过程中，同一个 `retained-artifacts-v1` 会话提供 `resolve-map`、`transport-map` 和 `restage-map` 处理已登记原件，`artifact-identity` 核对摘要与长度，`read-artifact-text` 读取原文，`read-bounded-text` 执行有上限的本地读取。调用 `check-preparation` 核验准备阶段的封存输入、可选转运位置和原件映射。
