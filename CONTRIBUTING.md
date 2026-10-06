# 参与开发

欢迎贡献！本项目是**纯 `std`（零第三方依赖）** 手写的 Python 子集解释器，
验收标准是**与 CPython 3.13 输出逐字节一致**。

> 如果你是用 AI 代理改这个仓库，请让它**先读 [`AGENTS.md`](AGENTS.md)**；
> 仓库里还有一份项目技能 `.agents/skills/minipython-dev/`，包含架构、各扩展点的配方与差分测试流程。

## 环境准备

| 需要 | 说明 |
| --- | --- |
| Rust | **1.75+**（CI 的 `msrv` job 会验证；开发用 stable 即可） |
| CPython 3.13 | **可选但强烈建议**：差分测试与生成 `examples/*.out` 都要用；没有它 `tests/cpython_diff.rs` 会自动跳过 |
| git | 任意版本 |

```bash
git clone git@github.com:Sapient610/minipython.git
cd minipython
cargo build --release
cargo test                 # 首次会稍慢；69 项测试
```

## 提交前必须全绿（CI 会卡这三项）

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

任何一项不过就不要提交。CI 还会在 **ubuntu / windows / macos(arm64)** 三平台跑同一套测试。

## 开发流程

1. **先看**：README 的「语言支持矩阵 / 已知限制」→ `AGENTS.md` → 技能文档
   `references/architecture.md`（核心类型与不变量）与 `references/feature-recipes.md`（各扩展点配方）
2. **改代码**：新语法走 `ast.rs → parser.rs → interp.rs/ops.rs`；新内置函数/方法/模块各只改一处
   （配方里给了代码骨架）
3. **验证**：不要只写自认为对的断言，**真跑一遍 CPython 对照**
   （命令与结果判读见 `references/differential-testing.md`）
4. **补测试**：新行为补 `tests/language.rs` 用例；值得长期展示的特性顺手加进 `examples/`
   （`.out` 必须由 CPython 生成，LF 换行）
5. **同步文档**：`README.md`（支持矩阵/已知限制）、`CHANGELOG.md`；新增约定再更新 `AGENTS.md`

## 代码约定（摘要）

- 注释、文档、错误消息一律**中文**；异常**类型名**与消息格式尽量对齐 CPython
- 不使用 `unsafe`，不引入任何 crate（含 dev-dependencies）
- 不用第三方格式化工具：`cargo fmt` 是唯一格式标准
- 关键陷阱（详见 `AGENTS.md`）：
  - `RefCell` 借用**不要跨调用**用户代码
  - 类的 `mro` **不含自身**；只有**用户自定义**的魔术方法才优先于默认行为
  - 实例属性统一走 `InstanceData::get/set/remove/names`
  - 整数是 i64，溢出**有意**升级为 float；`set` 迭代序是插入序（测试里一律 `sorted()`）
  - 依赖平台 libm 的浮点（`expm1`/`log1p`/`cbrt`…）在断言里要先 `round(..., 12)`

## 提交信息规范

```
类型: 中文摘要

正文：说明「为什么」与「怎么验证的」（跑了哪些测试、与 CPython 比对的结论）
```

类型取 `feat` / `fix` / `test` / `docs` / `ci` / `chore` / `refactor`。
一个提交只做一件事；大改动请拆成可独立通过 CI 的若干提交。

## PR 流程

1. Fork 或从分支提交 PR（目标分支 `main`），按 [PR 模板](.github/pull_request_template.md) 填完清单
2. 等 CI 三平台全绿；如果只有某个平台红，先看是不是**平台差异**（历史案例：macOS arm64 的
   `expm1` 末位不同）而不是急着改实现
3. Reviewer 会重点看：是否引入依赖、是否有回归用例、是否与 CPython 逐字节一致

## 反馈问题

- **普通缺陷 / 特性建议**：用 [issue 表单](.github/ISSUE_TEMPLATE/)（请附最小复现与 CPython 对照）
- **安全问题**：**不要开公开 issue**，走 [`SECURITY.md`](SECURITY.md) 里的私密渠道
- **行为准则**：见 [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md)
