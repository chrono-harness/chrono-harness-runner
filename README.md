# Chrono Harness Runner

`chrono-harness run-command-v1` 在当前目录按宿主提供的命令、环境、时限和输出限额运行外部程序。stdin 第一行是 JSON CommandSpec，另可提供 `expected_sha256` 固定程序身份。其余字节原样送入子进程。命令头最多 1 MiB，输入最多 64 MiB。执行成功后，首行 JSON 含 `exit_code`、`stdout_length` 和 `stderr_length`，随后依次为原始 stdout 和 stderr。执行器失败返回 2，在 stderr 输出原错误。

`chrono-harness observed-command-v1` 用于发布流程的实时转发和进程回执。请求首行含 `command`、`expected_sha256`、`timeout: {"secs": ..., "nanos": ...}`，其余字节是原 stdin。大小限制同上。回复每帧为一个类型字节、八字节大端长度和载荷。O 和 E 分别是当场读取的有界 stdout、stderr。唯一末帧 T 是 JSON，含两流的长度、SHA-256、完整性标记、实际 PID、执行身份、退出码、信号和执行错误。启动前失败或传输错误在 stderr 报告，程序退出 2。

登记判官在历史格式迁移时调用 `chrono-harness record-command-v1`。命令使用与 `run-command-v1` 相同的有界请求头和原始 stdin，必须提供 `expected_sha256`。stdout 返回执行引擎生成的完整 JSON 进程记录，包括实际启动信息、子进程回收结果、原始输出及退出状态。消费者从记录读取子进程的失败。获取记录前发生的错误写到 stderr，CLI 退出 2。

独立 bootstrap 通过 `compiler-output-v1` 提交编译器合同、原始 Cargo-Verus 输出行、dep-info 和逐文件绑定。操作为 `validate-contract`、`proof-rows`、`dependency-paths`、`dependency-header`、`dependency-matches`。对生命周期报告中的 `managed_process`，`judge-evidence-v1` 的 `expand-process` 校验压缩流和原进程格式，返回 stdout、stderr 原始字节、失败原因和退出码。

bootstrap 通过 `retained-artifacts-v1` 中的 `publication` 操作创建或接管宿主登记的目录，保留读取租约、诊断和报告引用，并在完成后封存。关闭未完成的发布会释放使用权，原始状态继续交给保管者处理。需要把新取得的租约交给原生子进程时，调用方将匿名 Unix socket 用作会话 stdin，stdout 仍返回逐行 JSON。`scoped-descriptors` 从给定 `offset` 返回最多 32 个描述符。计数大于零时，同一个 socket 传递一个字节 `01` 和相应的 `SCM_RIGHTS`。计数达到 32 时，调用方继续读下一批。

工作树启用原件保管时，在同一 `retained-artifacts-v1` 会话中登记文件、目录及读取它们的操作。普通 JSON 请求每行返回一个 `Result`。显式发出 `explicit-read-scopes` 后，调用方为每次操作打开并关闭自己的读取范围。固定报告的 `reference-held` 返回独立的租约句柄，在指针落地后释放。

`observe-maintenance` 先逐行发出 `{"observation": ...}`，最后返回本轮汇总或错误。维护进程可以边读边写 inventory。对于任意长度的原件，`publish-stream` 请求给出 producer、path、length、sha256 和 outcome。收到 `{"receive": length}` 后发送恰好 length 字节，再读取最终结果。提前拒绝、复用已有原件及内容校验失败都有明确返回值。

工作树清理器使用 `retained-artifacts-v1` 的 `open-removal` 登记一组宿主相对路径。返回 true 时，保管进程持有这组路径的删除租约，直到输入关闭。调用方在实际删除前用 `check-removal` 再次核对租约。若要将保护传给子进程，调用方通过同一 Unix 会话的 `scoped-descriptors` 接收并传递原文件描述符。

`chrono-harness inherited-descriptors-v1` 校验并报告当前是否继承了所有权描述符。`chrono-harness exec-with-descriptors-v1 PROGRAM [ARGS...]` 将租约交给实际程序，在同一 PID 上执行它，沿用调用方提供的当前目录、环境和标准流。

`chrono-harness judge-command-v1 NAME VERSION PROGRAM OPERATION ERROR_CODE --protocol chrono-judge/v1` 将同一个已封存请求交给单独安装的判官。runner 负责解析请求、校验封存、解析宿主配置、限制进程以及写出原生响应。PROGRAM 的 stdin 含 `request` 和从该宿主选出的 `protocol`，stdout 返回 `status`、`findings` 和 `outputs`。调用方声明接纳或传输失败时对外使用的错误码。`prepare-command-v1 PROGRAM OPERATION` 使用同一套请求和进程限制，接收绑定 request_id 的 judge-pipeline 输入生产者响应，包括业务失败时已记录的 outputs。

`chrono-harness-runner` 提供 `chrono-harness` 命令和同名 Rust 库。宿主提供检查配置，runner 调用判官并返回真实退出状态与报告。

`chrono-harness retained-artifacts-v1` 为外部生产者在当前仓库保持一次读取作用域。每行一个 JSON 请求，回复一行 `Ok` 或 `Err`。涉及创建暂存、写原件、依赖登记、读取与消费者保管的操作使用宿主已有 retention 策略。读取能力持续到调用方关闭 stdin；durable consumer 由后续的实际使用方通过原有 retention 命令释放。
CI 输入生产者可用 `validate-preparation-request` 核对当前检出的请求并取得保留目录，使用 `retain-original-text` 按内容登记 UTF-8 原件。`identify-original` 和 `verify-original` 分别取得及核对路径与摘要。`validate-preparation-result` 核对返回结果、所用请求与实际原件。
同一会话可以在创建 publication 时传入 `consumer_prefix`，将待复核的消费者与目录一起登记。`create-member` 在该目录的配额内建子目录；`complete` 的 `seal: true` 核对及封存最后的成员身份。


开发机可在现有 `chrono-harness retention --host-root . --request PATH` 的 `maintain` 请求中选定成功的本地检查报告，例如 `{"operation":"maintain","expire_local_check_slots":[".chrono-harness/state/collection/check.json"]}`。宿主需启用有限的 `max_lifetime_seconds`，报告使用 `retained-reference/v2`。保留期限从原件创建时计算。定时维护在到期后核对当前代际、发布状态、持有者及读者，释放已结束的本地入口，普通回收随后清理无引用的原件。需要长期使用的结果应在窗口内转存；有外部交接义务的结果沿已有交接流程。每个槽位的处置由本轮 `local_check_slots` 报告，后续维护可重试。

## 使用

在仓库根运行 `cargo build --locked --package chrono-harness`，再运行 `target/debug/chrono-harness --help`。日志和报告的位置由调用宿主选择。

## 验证

在仓库根运行 `mise exec -- ./check.sh` 执行格式、锁定构建、原生测试及整 crate 证明。Rust 固定为 1.95.0，Verus 固定为 0.2026.04.24.f8e1704。开发检查使用该分发包中的 `cargo-verus`。本地入口需要 Python 3.8+ 和支持 BSD flock 的本地文件系统。宿主调用固定版本的 chrono-host-bootstrap native-scratch。每次检查在本工作树的 `target/native-checks/` 下独占一个目录；标准错误会输出路径。原生编译和 Verus 结束并释放租约后，成功目录立即清理。自选 `CARGO_TARGET_DIR` 由调用者管理，应放在本工具的专用目录以外。

失败和中断的可重建目录默认保留 24 小时。需要保存的诊断应在窗口内复制到其他存储，读回并核对摘要。失败从进程返回时计时，中断按目录最后一次修改时间计时。检查进程及继承租约的子进程仍在使用时，维护跳过该目录。长期检出在该用户的 crontab 加一条每小时的任务，将路径替换为仓库的绝对路径。

```cron
17 * * * * cd "/absolute/path/to/chrono-harness-runner" && ./check.sh --maintain
```

机器在线、任务已安装、目录已到期且读者释放后，下一次成功维护回收整次检查的目录。现有 crontab 可继续使用。第一次新版检查会在 target/native-check-bootstrap 创建指向工作树外已安装程序的链接，供定时维护使用。新目录由共享工具依据标记和目录身份清理。旧版检查在专属业务目录分配的 32 位小写十六进制目录由既有 Python 维护逻辑核对租约、时间和 Git 跟踪后回收。较早版本生成的 `target/check.*` 没有租约，只在核实旧检查已经退出后按实际归属回收。

[本仓 Cargo 工作区](Cargo.toml) 使用一份根 Cargo.lock 管理 runner、测试、进程替身及三个证明包。证明包登记 `chrono-identifiers`、`chrono-verifier` 及其余各根对应的 Cargo 库目标。生产 runner、Cargo 的原生测试及 Verus 均使用这两个包的同一份源码。runner 保留从固定 Git 端点取数的适配器；验证器包负责根据已经观察到的数据作判断。

| 证明根 | 真实使用方与边界 |
| --- | --- |
| `chrono_identifiers`、`identifiers_tests` | runner 解码提交 OID、请求头、状态路径及摘要；验证器以相同依赖判断文本与字节身份 |
| `chrono_verifier`、`verifier_tests` | `compiler-output-v1` 接纳版本、commit、整根报告和编译原件；验证器测试还覆盖固定源连续性和候选判定 |
| `runner_paths`、`runner_paths_tests` | 发布、保留和输出路径的准入，逐段处理路径、符号链接及发布错误 |
| `runner_status`、`runner_status_tests` | 判官响应、固定 Git 事实与运行状态；状态解释使用实际读到的端点和结果 |
| `runner_limits`、`runner_limits_tests` | 执行计划的正超时、输出量下界和上界 |
| `retention_release`、`retention_release_tests` | 保管对象的选择、取得及释放，外部文件系统的观察与操作由适配层执行 |
| `runner_git_binding`、`runner_git_binding_tests` | Git 工具及登记绑定，拒绝无效版本、选择器或输入边 |
| `runner_cli`、`runner_cli_tests` | CLI 选择、参数解码、判官请求和输出；发布失败会保留产生它的写入或 flush 阶段 |
| `runner_fixture_tests` | 原生测试的进程替身与输入预期，由测试框架使用 |

每个整根的实际输入见 [Cargo 目标](crates/proofs/expectations/Cargo.toml) 和 [原生和证明检查](.chrono-harness/native-check.sh)。原生测试核对换用系统接口后可观察的业务结果，Verus 的结论依赖所登记的工具、编译器和外部模型。

运行库与测试从 [原实现的固定提交](https://github.com/ChronoAIProject/chrono-harness/tree/5f6892dfa2f73675a84e4ebdc3b06eb83b824d6e/crates/runner) 分出。旧宿主的登记表检查由新的采用宿主负责。工作树、CI 生成器及安装器各自负责其命令替身的断言。

The runner checks its own pull requests with a digest-pinned released binary. The check compares the first parent with the PR merge commit, then runs the registered native and Verus check for changes affecting the runner. After a successful operation the freshly built candidate checks the same commits. For a local replay put the released `chrono-harness` and `chrono-judge-ci` binaries on PATH, set `VERUS`, and run:

```sh
chrono-harness check --config .chrono-harness/check.json --base "$base_oid" --candidate "$(git rev-parse HEAD)"
```

The local checkout must match the candidate. The report and operation streams are written to `.chrono-harness/state/`.

预览版从已合入 dev 的提交打标签，正式版从 main 发布。每个版本在本仓构建 Linux x86_64 和 macOS arm64 的单个 `chrono-harness` 二进制，并附 SHA256SUMS。下载所需标签的二进制和校验文件，验算目标平台的 SHA-256 后将它安装为 `chrono-harness`。升级或回退时按同样方式替换该程序。宿主启用运行日志时，runner 通过已安装的 chrono-diagnostics 命令写入日志。

已安装的外部判官可将协议处理交给 `chrono-harness judge-pipeline NAME VERSION PRODUCER OPERATION EVALUATOR OPERATION --protocol chrono-judge/v1`。runner 在已登记的限额与环境下，向输入生产者发送封存请求。生产者返回请求 ID、观察到的输出以及准备好的 JSON 或原错误。求值程序只读取该 JSON，返回状态、findings 与 outputs。两个程序均通过标准输入输出通信，响应共享请求身份，重复输出键会报错。

`chrono-harness git-facts-v1` 在当前宿主打开一次 Git 读取会话。stdin 和 stdout 使用逐行 JSON，每行请求得到一个 `{"Ok":...}` 或 `{"Err":"原始诊断"}`，关闭 stdin 会结束会话。首行是 `{"mode":"config","config":"…","base":"完整提交 ID","candidate":"完整提交 ID"}`，或 `{"mode":"request","request":封存请求}`。打开成功返回 `{"bound":true|false}`。单行输入上限为 64 MiB。

登记判官用 `observed` 模式提交 `config`、`candidate`、原请求的 `observations` 和可选 `base`。runner 再次核对已观察的 Git 工具与配置。`checkout-root` 读取该工具报告的检出根。`candidate-inventory` 返回候选树中各路径的 `kind`、`mode` 以及相对基线的 `delta`。首次登记没有基线，此时 `delta` 为 null。

后续请求的 `op` 可取 `delta`、`observation`，或带 `endpoint`（`base` 或 `candidate`）的 `verify-tree`、`registry-snapshot`、`checked-out-snapshot`、`json-blob`。`json-blob` 另需相对路径 `path`。快照返回 `originals`、`values`、`effective_path`、`selection`，检出快照还核对 HEAD 和配置原字节。发生操作错误后仍可读取该会话已执行的 Git 观察，再关闭输入并等待进程退出。

`git-facts-v1` 的 `checkout-excluding` 接收宿主登记的 `artifacts` 路径列表，返回当前固定候选的 HEAD、跟踪变更、范围外的未跟踪路径和 index 标志。项目判官在执行前后使用同一 Git 读取会话核对候选输入，并将 Git 观察纳入响应。


独立 routes 通过 `source-coverage-v1` 提交两个固定端点、受影响节点、登记方法的 owner/tool/operation 与选中证明调用的 argv、输出合同和输出上限。runner 返回两端提交、实际 Git 观察及首个验证错误。Git 来源支持原判官封存请求、已有观察和旧式未绑定读取。完整库存与选中根使用同一份连续性证据。

`check-invocation-v1` 在 `template` 模式核对原始入口、参数模板和绑定，在 `request` 模式核对封存请求、宿主配置的规范入口和短式准备原件。成功输出 JSON null，失败返回 2 并写明原因。

`installed-query-v1` 按调用方声明的工作目录、程序、操作与环境执行已安装的 JSON 查询，保留原输出。调用方用 `inherited` 保留启动坐标，或用 `search-path` 指定登记查询只继承 PATH。

`judge-command-v1` 的 `--inherited` 选项供保留原启动坐标和环境的判官使用。它将已核验的请求作为 `{"request":...}` 传入求值命令；准入失败使用 fail 和原错误前缀。

项目收集器通过 `judge-evidence-v1` 的 `unit-tests`、`collection-paths`、`artifact-directories` 和 `retained-entry` 核对已登记的测试、收集路径与封存的启动命令。`selected-host-config-v1` 从标准输入读取宿主配置的相对路径，输出当前平台选中的配置。

scoped CI 在 `judge-evidence-v1` 中使用 `scoped-report-path` 和 `scoped-reference-publication` 查询当前 profile 的发布合同，使用 `checkout-index-issue` 核对固定工作树的 index 标志。`check-invocation-v1` 的 `scoped-request` 模式接收完整 scoped request、其登记配置和 `admit_entry`。本地执行和离线收集由同一入口核对适用的原调用参数和准备阶段身份。


执行者用 `retained-artifacts-v1` 的 `directory-evidence` 读取操作的 stdout、stderr 原件大小与保管记录，用 `process-completion` 判断子进程是否已结束。`compact-process` 保留原字段及压缩流。`check-artifact-path` 检查登记的相对地址。规范编译命令成功返回并写入结果后，`dispose-compiler-originals` 核对同次执行的库和依赖文件的长度及 SHA-256，依照宿主保管政策释放它们。

CI 提供者用 `git-facts-v1` 的 `config` 模式打开登记的 Git 配置。它可以验证配置与提交的一致性（`verify-config`），读取 HEAD、父提交、原始文件及差异路径，按完整提交 ID 从 origin 获取已登记历史。所有原执行记录通过同一会话的 `observation` 返回。设置 `observe_failure:true` 时，打开失败也返回原始 Git 观察。

需要读取作业身份、调度结果等 CI 输入的宿主，可在 schema 4 的 `canonical_check.inputs.inherit_environment` 中列出变量名。准备器接收实际值，原始进程回执记录各值的 SHA-256。业务进程及两端输入快照继续使用 `environment.inherit`；同名重复登记或同时设置固定值会报错。

独立安装的判官在宿主 `.chrono-harness/bin/<程序名>` 登记入口。末端链接可指向工作树外同名普通文件。initial 和 full 先按登记 SHA-256 调用 `--version`，逐字节核对 `<程序名> <登记版本>\n`，再按同一摘要启动判官。报告中的 `process.judge_version_process` 保存版本进程原件，归档读取只需原请求、登记和报告。宿主内普通文件沿用直接调用。

登记判官可以在候选根目录向 `judge-evidence-v1` 提交 `{"operation":"bound-judge-path","binding":...}`。成功返回已核登记 SHA-256 的可执行文件绝对路径，调用方据此核对自己的执行身份。登记判官通过 `judge-evidence-v1` 的 `registry-identity` 解析固定登记文档的配置身份，通过 `report-paths` 检查报告路径与证据路径冲突。`validate-git-declaration`、`validate-preparation-declaration` 和 `credential-environment` 接受原配置文档并执行 runner 的对应规则。

公开的 Git 会话也接受 `verify-commit` 和 `ancestors`（参数 `commit` 为完整 OID）、`check-branch-name`（参数 `branch`）及 `shallow-boundaries`。已封存请求内的读取使用同一个宿主绑定的 Git 进程记录。只检查调用方给出的分支上下文时，可以用 `{"mode":"history"}` 打开无配置的历史读取会话。历史调度可用同一会话的 `blob-bytes` 读取完整提交 ID 与仓内相对路径指定的 Git 原字节。

`chrono-harness judge-evidence-v1` 读取逐行 JSON 并逐行回复 `{"Ok":值}` 或 `{"Err":"错误"}`。workflow 通过 `expand-record`、`check-process`、`report-transport-bytes` 和 `retained-judges` 重放 runner 报告。`retained-judges` 校验封存的原请求、登记顺序、每个实际进程的原始流与响应，返回所选判官的输入请求和汇总状态。首次登记判官通过 `validate-initial-request` 和 `validate-initial-profile` 校验单端点请求的封存身份、执行限制及判官登记。协议还提供严格 JSON 解码（`decode-json`）、原请求校验（`validate-request`）和按选择计算执行单元（`required-units`）。`retained-artifacts-v1` 的 `checked-path` 核对已登记相对路径的符号链接，读取租约仍由同一保管会话持有至 EOF。

`judge-command-v1` 的 `--admission-result` 选项将 `{"request":请求,"admission":{"Ok":null}}` 或带原错误的 `{"Err":"…"}` 交给独立判官。判官可对准入失败使用其已登记的 finding 和严重程度。

`retained-artifacts-v1` 的外部执行者用 `begin-directory` 打开原件目录，`write-directory` 在登记目录中写入意图和结果，`execute-retained` 按命令、SHA-256、超时与输出上限执行一次，并保留 stdout、stderr 和 launch.json。`check-directory-write` 再核对保管预算。`finish-directory` 接受最终结果和输入依赖，在业务进程已结束后封存。生成分配、判定执行结果与决定释放时机由项目负责。 当外部生产者需要在创建前登记消费者时，`begin-directory` 接受 `rooted:true` 并返回库存 ID。用 `create-directory-member` 创建多个成员，每个成员可单独执行一次，stdout、stderr 和 launch.json 均归属该成员。生产者用 `reserve-directory-file` 在写入前按字节和节点占用预算，用 `validate-launch` 核对精确原件路径与 SHA-256，用 `seal-directory` 封存结果。库存查询与消费引用通过 `inventory` 操作执行，活跃读取租约由该进程持有到会话结束。

安装的判官通过 `judge-command-v1` 返回决策时，可以在 `evidence` 数组里列出已生成的证据文件。runner 将其路径、SHA-256 和种类随判官响应交给实际调用方，省略该字段表示没有证据文件。

在报告收集过程中，同一个 `retained-artifacts-v1` 会话提供 `resolve-map`、`transport-map` 和 `restage-map` 处理已登记原件，`artifact-identity` 核对摘要与长度，`read-artifact-text` 读取原文，`read-bounded-text` 执行有上限的本地读取。调用 `check-preparation` 核验准备阶段的封存输入、可选转运位置和原件映射。

离线 CI 收集器使用 `validate-portable-binding` 核验准备结果和可选转运目录，使用 `restore-scoped-process` 按调用方给定的剩余字节数重建进程回执。两项操作逐件发出 `{"read_original":{"path":"…","sha256":"…"}}`。收集器持有读取租约、核对宿主路径与配额，回复一行 `{"Ok":字节数}` 或 `{"Err":"读取错误"}`。runner 接受长度后发出一行 `{"send_original":true}`，收集器随后写入指定长度的原始字节。最后一行 `{"Ok":结果}` 或 `{"Err":"校验错误"}` 结束本次操作。

`compiler-output-v1` 的 `canonical` 操作返回参数是否对应登记的编译器输出；`observe` 在原工作目录读取声明的依赖文件并核对库的实际 SHA-256 与长度，在进程失败时保留原始判断结果。

runner 的生产和专属测试包各有 Cargo manifest，构建时读取仓库根目录的 Cargo.lock。使用宿主日志时，在 PATH 中安装本仓清单固定的 chrono-diagnostics。它通过 cli-events-v1 写入宿主选择的滚动文件，接收真实进程的 PID、span、退出状态和诊断。原生输出发布失败时，runner 把同一个 Rust I/O 错误留给调用方，并将错误记录送到 diagnostics 的显式 stderr 输出接口。

CI 判官通过同一 `git-facts-v1` 进程读取两个固定提交的原件。开场 `{"mode":"scoped","config":null,"base":null,"candidate":"完整 OID"}` 使用普通 Git。登记了 Git 观察时提供 `config`，打开失败会返回原观察。需要为收集准备请求时，使用 `prepared-collection`，提供 `config`、`base`、`candidate`、`context`、`scope` 和目标 `judge`。回复同时包含封存的 `request`、选定的 `config` 和读取绑定 `bound`。后续 `tree-entries` 返回指定端点每个路径的 mode、kind、OID，`registry-at` 按给定登记路径返回原文和有效配置，`blob-batch` 按输入路径顺序返回原始字节。所有这些读取都记入该进程的 `observation`。

收集会话持有准备出来的基线快照，直至宿主关闭 stdin。启用保管政策时，最后一个读取租约结束后由已登记的维护程序按期回收。普通宿主的临时目录在会话退出时回收。
登记判官通过 `compiler-output-v1` 的 `source-declarations` 操作提交带有验证声明的文件、已登记操作及引用路径（`files`、`operations`、`reference_paths`）。runner 校验每项声明及证明根所用操作，并按输入顺序返回每条引用路径的规范性布尔值。无效声明返回原文件名和错误。

工作树参与本地完整检查时，`check-invocation-v1` 接受 `{"mode":"local-preparation","request":...}`，按 runner 的当前合同核对输入、选定 profile 和原件绑定并返回 `null`。`{"mode":"participation","config":...}` 返回配置中登记的工作树参与动作或 `null`。两者均为可选的 runner 检查集成。

CI 的编译器输出由固定候选的 `git-facts-v1` 会话核验。候选 `tree-entries` 传入 `retain_output_modes=true` 为本次核验保留原文件模式，`prepare-output-binding` 登记本次计划的方法和 FILEMAP，`bind-operation-output` 逐操作核对原参数、进程终态和适用的编译报告、库及候选源码。被阻断的操作和通过的普通操作都返回 JSON null，原错误通过同一会话返回。

`compiler-output-v1` 的 `dependency-header` 从原生 dep-info 的首条规则读取含字面空格的目标主名，解码来源序列及其中的转义空格。返回 `[目标字节, 来源字节数组]`，不合格时为 `null`。与 Cargo 当前选择的库关联时，由读取文件的宿主核实所声明目标与当前选中 dep-info 的身份。
