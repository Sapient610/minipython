# AGENTS.md

本文件是给 **在本仓库工作的 AI 代理**（以及人类贡献者）的说明。修改代码前请先读完本页。

## 项目是什么

MiniPython：一个**从零手写**的 Python 子集解释器，用 Rust 实现。

- **零第三方依赖**（只用 `std`）——这是硬约束，不要引入任何 crate，包括 `dev-dependencies`
- **不使用 `unsafe`**
- MSRV **Rust 1.75**（CI 的 `msrv` job 会实际验证），开发环境为 1.99
- 目标不是"能跑"，而是**与 CPython 3.13 逐字节一致**：示例程序与差分脚本的输出都必须完全对齐
- 代码注释、错误消息、文档统一用**中文**（异常类型名与消息格式对齐 CPython 的英文）

## 常用命令

```bash
cargo build --release                 # 构建
cargo test                            # 全部测试（单元 + 集成 + 示例 + CPython 差分）
cargo test --test language            # 只跑语言特性测试（改语义时最常用）
cargo test --test examples            # 只跑示例输出比对
cargo test --test cpython_diff        # 与 CPython 逐字节对照（无 python 时自动跳过）

cargo fmt --all                       # 格式化（提交前必须跑）
cargo clippy --all-targets -- -D warnings   # 静态检查（警告即失败）

cargo run --release -- examples/classes.py  # 运行脚本
cargo run --release -- -c "print(sum(range(10)))"
cargo run --release                          # 交互式 REPL
```

## 提交前必须全绿（CI 会卡这三项）

1. `cargo fmt --all -- --check`
2. `cargo clippy --all-targets -- -D warnings`
3. `cargo test`

任何一项不过就不要提交。CI 还会在 ubuntu / windows / **macos(arm64)** 三平台跑同样的测试。

## 仓库结构速查

| 文件 | 负责什么 | 改动频率 |
| --- | --- | --- |
| `src/lexer.rs` | 缩进栈、字符串/f-string 片段抽取、数字字面量、运算符 | 改语法时 |
| `src/ast.rs` | AST 节点定义 | 加语法时 |
| `src/parser.rs` | 递归下降 + 优先级爬升，产出 AST | 加语法时 |
| `src/value.rs` | 值模型、`DictData`/`SetData`、`float`/`str` 的 repr、格式说明符 | 加类型时 |
| `src/env.rs` | 作用域链、`global`/`nonlocal`、海象赋值作用域 | 少 |
| `src/interp.rs` | 语句执行、表达式求值、调用约定、类构造、异常、导入 | 高频 |
| `src/ops.rs` | 运算符、比较、属性访问、下标、魔术方法分派 | 高频 |
| `src/methods.rs` | 内置类型方法表（str/list/dict/set/tuple/int/float/range/file） | 加方法时 |
| `src/builtins.rs` | 内置函数、内置类型构造、`object` 默认实现、内置异常 | 加内置函数时 |
| `src/modules.rs` | `math`/`random`/`sys`/`string` 模块 + 模块导入（含相对导入） | 加模块时 |
| `src/repl.rs` / `src/main.rs` | REPL 与 CLI | 少 |
| `src/lib.rs` | `Session`（大栈工作线程 + 输出捕获）与便捷函数 | 少 |
| `tests/*.rs` | 集成测试（语言特性、示例比对、CPython 差分） | 每次改行为 |
| `examples/*.py` + `*.out` | 示例与其 CPython 期望输出 | 加特性时 |

## 加一个新特性：三步走

**1) 新语法**（例如某种新语句/表达式）

- `ast.rs`：加节点
- `parser.rs`：解析到该节点（注意优先级与终止符，参考 `parse_expr` / `parse_call_args`）
- `interp.rs` 或 `ops.rs`：求值

**2) 新内置函数**：在 `builtins.rs` 写 `fn(...) -> EResult<Value>`，然后在 `native_functions()` 里注册一行。

**3) 新类型方法**：在 `methods.rs` 写函数，再到 `lookup_method()` 对应类型分支挂名字。接收者永远是 `args[0]`。

**4) 新模块**：在 `modules.rs` 的 `create_builtin_module()` 加分支，用 `module_from(...)` 组装。

**5) 新魔术方法**：在 `ops.rs` 的对应分派点（`binop`/`eq_values`/`get_attr`/…）查找并调用；注意**只有用户自定义的挂钩才应优先于默认行为**（见 `find_user_dunder`）。

## 验证方式：与 CPython 差分（本项目的核心手段）

不要只写"我觉得对"的断言，要**真的和 CPython 比**：

```bash
# 1) 写一个临时脚本（可以用 examples/ 下现成的）
# 2) 两边各跑一次，逐行 diff
python -W ignore scratch_x.py > x.py.txt 2>&1
.\target\debug\minipython.exe scratch_x.py > x.mp.txt 2>&1
# 3) 用 Python 比对（PowerShell 的 Compare-Object 在中文/换行上不可靠）
python -c "import io;a=io.open('x.py.txt',encoding='utf-8').read().splitlines();b=io.open('x.mp.txt',encoding='utf-8').read().splitlines();print([(i+1,a[i],b[i]) for i in range(min(len(a),len(b))) if a[i]!=b[i]])"
```

两个坑：

- **stdout/stderr 会被 PowerShell 交错**：`cargo run ... > f 2>&1` 里 stderr 的 traceback 可能出现在文件中间。要看真实顺序就把 stderr 单独重定向（`2>err.txt`），并优先用**已构建的 exe**（`.\target\debug\minipython.exe`）而不是 `cargo run`，避免 cargo 的警告混进来。
- **平台 libm 有末位差异**：`expm1`/`log1p`/`cbrt` 等依赖系统数学库，macOS(arm64) 与 glibc 的最后一位可能不同。写进 `examples/*.out` 或断言里的这类值**必须先 `round(..., 12)`**；整数与 IEEE 精确运算（`prod`/`isqrt`/`comb`/`perm`/`lcm`/`dist`/`modf`/`frexp`/`ldexp`/`nextafter`/`ulp`/`remainder`）可以逐位比较。

新增示例后要重新生成期望输出（**不要手写 `.out`**）：

```bash
python -c "import io,subprocess,os;e=dict(os.environ,PYTHONIOENCODING='utf-8',PYTHONUTF8='1');r=subprocess.run(['python','-W','ignore','examples/新示例.py'],capture_output=True,env=e);io.open('examples/新示例.out','w',encoding='utf-8',newline='\n').write(r.stdout.decode('utf-8').replace('\r\n','\n'))"
```

`.out` 必须用 **LF** 换行（`.gitattributes` 已强制，`tests/examples.rs` 也会归一化）。

## 代码约定与已知陷阱

- **错误构造**：`self.err("TypeError", "……")` / `self.pyerr(...)`；错误消息尽量贴近 CPython，异常**类型**必须完全一致（测试会断言类型）。
- **`RefCell` 借用不要跨调用**：`find_class_attr(...).borrow()` 之后若要调用用户代码，先 `clone()` 出来丢掉借用，否则会 panic（`already borrowed`）。
- **实例字典是活动视图**：`InstanceData.dict` 是 `Rc<RefCell<DictData>>`，`obj.__dict__` 直接返回它（所以 `self.__dict__[k] = v` 会写回）。新增涉及实例属性的代码请用 `InstanceData::get/set/remove/names` 而不是直接操作 HashMap。
- **类的 `mro` 不含自身**（自身字典由 `find_class_attr` 先查）。写查找逻辑时别假设 `mro[0]` 是自己。
- **递归保护**：`call_function` 同时检查深度与**实际栈用量**（`stack_budget`）。debug 构建下每层约 100KB 栈，直接嵌入 `Interp` 的线程栈要够大（`Session`/CLI 用 64MB）。
- **异常匹配**：携带异常对象时**只按类继承关系**匹配（不要用名字前缀，否则 `except 子类` 会误捕父类）。
- **整数是 64 位**，溢出会升级为 `float`（这是有意的简化，别"顺手修掉"）。
- **`set` 的迭代顺序是插入序**（CPython 是哈希序）：任何测试/示例都不要直接打印未排序的集合。
- **`Rc` 循环引用会泄漏**（无 GC）；这是已知取舍。

## 提交与 PR 约定

- 一个提交只做一件事，message 用 `类型: 中文摘要`，类型取 `feat` / `fix` / `test` / `docs` / `ci` / `chore` / `refactor`
- 正文写清"为什么"和"验证方式"（跑了哪些测试、与 CPython 比对的结论）
- 改了行为就同步更新：`README.md`（支持矩阵 / 已知限制）、`CHANGELOG.md`、以及 `examples/modern_python.py` 之类能覆盖新特性的示例
- 修 bug 时优先补一个 `tests/language.rs` 的回归用例
- PR 按 [`.github/pull_request_template.md`](.github/pull_request_template.md) 的清单逐项勾选
  （三道门禁、与 CPython 的对照结论、文档同步）
- 面向人类的贡献流程见 [`CONTRIBUTING.md`](CONTRIBUTING.md)；**安全问题不要开公开 issue**，
  走 [`SECURITY.md`](SECURITY.md) 的私密渠道

## 不要做的事

- ❌ 引入第三方 crate、`build.rs`、`unsafe`
- ❌ 手写 `.out`、或为了让测试过而放宽断言（宁可先记进"已知限制"）
- ❌ 跳过 `fmt`/`clippy` 提交
- ❌ 把 `target/`、`examples/_sample_output.txt` 提交进仓库（已在 `.gitignore`）
- ❌ 为了通过 macOS 而改动与平台无关的行为；平台差异应当收敛在测试的容差里
