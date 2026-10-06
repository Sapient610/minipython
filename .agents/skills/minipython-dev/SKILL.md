---
name: minipython-dev
description: Use when changing the MiniPython interpreter (a pure-std Rust implementation of a Python 3 subset) — adding syntax, builtins, type methods, magic methods or stdlib modules, fixing semantics, or when a behavior must be verified against CPython 3.13.
---

# MiniPython 开发技能

MiniPython（本仓库）是用 **纯 `std` Rust** 手写的 Python 子集解释器，验收标准是**与 CPython 3.13 输出逐字节一致**。
本技能给出改动时的固定套路与踩坑清单；细节见 `references/`。

## 何时用

- 要加/改**语法**（新语句、新表达式、新运算符）
- 要加/改**内置函数、类型方法、魔术方法、内置模块**
- 修**语义 bug**，或"和 CPython 表现不一致"
- 需要为某个行为**建立可信验证**（差分测试、示例 `.out`）

## 三条铁律

1. **零依赖、无 `unsafe`**：只能 `std`。不要加 crate（含 dev-dependencies）。
2. **提交前三项全绿**：`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`。
3. **不接受"看起来对"**：新行为必须真跑一遍 CPython 对照（见下）。

## 改动后的标准动作

```bash
cargo test --test language     # 语义相关：最快的反馈
cargo test                     # 全量（含 examples 逐字节比对 + CPython 差分）
cargo fmt --all && cargo clippy --all-targets -- -D warnings
```

行为有变化时同步三处：`README.md`（支持矩阵/已知限制）、`CHANGELOG.md`、`tests/language.rs`（回归用例）。

## 差分验证（本项目最核心的手段）

```bash
# 用已构建的 exe，不要用 cargo run（避免 cargo 警告混入 stdout）
python -W ignore t.py > t.py.txt 2>&1
./target/debug/minipython.exe t.py > t.mp.txt 2>&1
# 用 Python 比对，不要用 PowerShell 的 Compare-Object（中文/换行不可靠）
python -c "import io;a=io.open('t.py.txt',encoding='utf-8').read().splitlines();b=io.open('t.mp.txt',encoding='utf-8').read().splitlines();print([(i+1,a[i],b[i]) for i in range(min(len(a),len(b))) if a[i]!=b[i]])"
```

- stderr 会与 stdout 交错：怀疑顺序问题时把 `2>err.txt` 分开重定向
- 浮点陷阱：`expm1`/`log1p`/`cbrt` 依赖平台 libm（macOS arm64 ≠ glibc），**这类值必须 `round(..., 12)` 后再比较**；整数与 IEEE 精确运算可逐位比
- 新增示例的 `.out` **必须由 CPython 生成**且为 LF 换行，别手写

## 高危区域（改之前先看一眼）

| 区域 | 风险 |
| --- | --- |
| `ops.rs` 属性分派 | 只有**用户自定义**的 `__getattribute__`/`__setattr__`/`__delattr__` 才可优先于默认逻辑（用 `find_user_dunder`）；否则 property 被绕过、每次访问多绕一层 |
| `RefCell` | 借用在手时**不要调用用户代码**，先 `clone()` 出来；实例属性统一用 `InstanceData::get/set/remove/names` |
| 类的 `mro` | **不含自身**；查找顺序是「自身 dict → mro」 |
| 异常匹配 | 携带异常对象时只按**类继承关系**匹配；用名字前缀会让 `except 子类` 误捕父类 |
| 递归/栈 | `call_function` 同时校验深度与栈用量；debug 下每层约 100KB 栈，`Session`/CLI 用 64MB 线程 |
| 整数 | i64，溢出**有意**升级为 float；不要"顺手修" |
| `set` 顺序 | 迭代序=插入序；测试/示例里一律 `sorted()` 后再打印 |

## 参考文件

- `references/architecture.md` —— 文件职责、核心类型、求值流程、关键不变量
- `references/feature-recipes.md` —— 加语法 / 内置函数 / 类型方法 / 模块 / 魔术方法的逐步配方（含代码骨架）
- `references/differential-testing.md` —— 差分测试与期望输出生成的完整流程、结果判读与分诊
