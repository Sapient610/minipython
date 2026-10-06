# MiniPython —— 用 Rust 实现的 Python 子集解释器

[![CI](https://github.com/Sapient610/minipython/actions/workflows/ci.yml/badge.svg)](https://github.com/Sapient610/minipython/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://opensource.org/licenses/MIT)

一个**从零手写**的 Python 解释器：缩进敏感的词法分析器 → 递归下降 / 优先级爬升语法分析器 → AST → 树遍历求值器。
**不依赖任何第三方 crate**（纯 `std`），约 12000 行 Rust 代码，另含 600 余行测试与 10 个示例程序。

```
$ cargo run --release -- examples/fizzbuzz.py
1 2 Fizz 4 Buzz
Fizz 7 8 Fizz Buzz
...

$ cargo run --release -- -c "print(sum(x*x for x in range(10)))"
285

$ cargo run --release
MiniPython 0.1.0 （输入 exit() 或 Ctrl-D 退出）
>>> import math
>>> math.sqrt(2)
1.4142135623730951
>>> [x for x in range(5) if x % 2]
[1, 3]
```

## 目录

- [特性一览](#特性一览)
- [快速开始](#快速开始)
- [命令行用法](#命令行用法)
- [语言支持矩阵](#语言支持矩阵)
- [代码结构](#代码结构)
- [实现要点](#实现要点)
- [测试与 CPython 对照](#测试与-cpython-对照)
- [已知限制](#已知限制)
- [扩展指南](#扩展指南)

## 特性一览

| 领域 | 支持内容 |
| --- | --- |
| 词法 | 缩进 / 反缩进（Tab 按 8 列对齐）、注释、括号内隐式续行、`\` 续行、三引号字符串、raw 字符串、f-string（含 `!r`、格式说明符、嵌套 `{}` 字段、`f'{x=}'` 调试语法）、二/八/十六进制与下划线数字 |
| 语法 | 赋值（含链式、解包、`*rest`）、增量赋值、`if/elif/else`、`while/else`、`for/else`、`break`、`continue`、`pass`、`def`（默认值、`*args`、`**kwargs`、关键字参数、装饰器）、`class`（多继承）、`lambda`、`return`、`try/except/else/finally`、`raise ... from`、`assert`、`del`、`global`、`nonlocal`、`import` / `from ... import`、`with`、海象运算符 `:=`（含 PEP 572 作用域规则）、`type(name, bases, ns)` 动态建类 |
| 表达式 | 完整运算符优先级（含 `**` 右结合、链式比较 `a < b < c`）、切片 `a[i:j:k]`、三目、`and/or/not` 短路、成员与身份判断、字典合并 `a | b` 与 `|=`、列表/集合/字典推导式、生成器表达式、`*`/`**` 实参展开 |
| 数据类型 | `None`、`bool`、`int`（64 位）、`float`、`str`、`list`、`tuple`、`dict`（保持插入顺序）、`set`、`range`、`slice`、函数、类、实例、模块、迭代器、文件对象 |
| 面向对象 | 实例属性、类属性、实例方法、`staticmethod`、`classmethod`、`property`（含 setter/deleter）、继承、C3 线性化 MRO、`super()`（零参与双参）、`__getattr__`/`__setattr__`/`__delattr__`/`__getattribute__` 钩子与 `object.__setattr__` 等默认实现、活动的 `obj.__dict__`、运算符重载、`__str__`/`__repr__`/`__eq__`/`__len__`/`__call__`/`__getitem__`/`__setitem__`/`__contains__`/`__iter__`/`__next__`/`__hash__` 等魔术方法 |
| 异常 | 完整的内置异常体系（`BaseException` → `Exception` → …）、自定义异常、异常匹配按继承关系、`finally` 覆盖语义、`raise` 重新抛出、traceback（含源码行） |
| 内置函数 | `print`、`len`、`range`、`int`、`float`、`str`、`bool`、`list`、`tuple`、`dict`、`set`、`type`、`isinstance`、`issubclass`、`repr`、`abs`、`min`、`max`、`sum`、`sorted`、`reversed`、`enumerate`、`zip`、`map`、`filter`、`all`、`any`、`round`、`divmod`、`pow`、`chr`、`ord`、`hex`、`oct`、`bin`、`id`、`hash`、`input`、`open`、`iter`、`next`、`format`、`dir`、`globals`、`vars`、`getattr`、`setattr`、`hasattr`、`delattr`、`callable`、`staticmethod`、`classmethod`、`property`、`super`、`exit` |
| 内置方法 | `str` 约 38 个（`split`/`join`/`replace`/`find`/`format`/`strip`/`zfill`/`startswith` …）、`list` 11 个、`dict` 10 个、`set` 14 个、`tuple` 2 个、`int.bit_length`/`bit_count`、只实现 `__getitem__` 也能迭代（旧式序列协议）、文件对象的 `read`/`readline`/`readlines`/`write`/`writelines`/`flush`/`close` 与上下文管理器 / 迭代协议 |
| 内置模块 | `math`（50 个函数 + 常量：`prod`/`isqrt`/`comb`/`perm`/`lcm`/`dist`/`modf`/`frexp`/`ldexp`/`isclose`/`remainder`/`nextafter`/`ulp`/`cbrt`/`expm1`/`log1p`/`exp2` 等）、`random`（`random`/`seed`/`randint`/`randrange`/`uniform`/`choice`/`shuffle`）、`string`（常量）、`sys`（`argv`/`version`/`platform`/`maxsize`/`exit`），以及**导入自定义模块与包**（`import foo`、`from foo import bar`、`import foo as f`、`from foo import *`、`import pkg.mod`、包内**相对导入** `from . import x` / `from ..pkg import y`） |
| 工具 | 脚本执行、`-c` 代码片段、多行续行的交互式 REPL、与 CPython 风格一致的 traceback 与异常消息、`sys.argv` |

## 快速开始

需要 Rust **1.75+**（CI 中的 `msrv` job 会在 1.75.0 上实际编译并跑测试；开发环境使用 1.99）。

```bash
cargo build --release          # 构建
cargo run --release -- examples/classes.py
cargo test                     # 运行全部测试（含与 CPython 的对照测试）
```

作为库使用：

```rust
use minipython::Session;

fn main() {
    let mut s = Session::new();
    s.run_source("x = 21\nprint(x * 2)", "<demo>").unwrap();
    assert_eq!(s.output(), "42\n");
}
```

`Session` 会在一个拥有 64MB 栈的工作线程中运行解释器，因此递归可以比较深，
并且会捕获标准输出，方便测试。

## 命令行用法

```
minipython [选项] [脚本.py] [脚本参数...]

  -c CODE        执行一段代码
  -i             执行完脚本后进入交互模式
  -h, --help     显示帮助
  -V, --version  显示版本
```

- 不带参数进入 REPL：`>>>` 提示符，语句未写完时自动出现 `...` 续行提示，表达式语句会回显结果。
- 脚本参数可通过 `sys.argv` 获取（`argv[0]` 是脚本名）。
- 运行出错时打印 CPython 风格的 traceback 并以退出码 1 结束；`sys.exit(n)` 以 `n` 作为退出码。

## 语言支持矩阵

**暂不支持**（会给出明确的错误信息，而不是静默算错）：

| 特性 | 说明 |
| --- | --- |
| `yield` / 生成器函数 | 会报 `暂不支持 yield / 生成器`（生成器表达式可用，但会立即求值） |
| `async` / `await` | 词法阶段直接报错 |
| 大整数 | `int` 为 64 位；`+ - * **` 溢出时自动转为 `float`（详见[已知限制](#已知限制)） |
| `bytes` / `bytearray` | 不支持，`b''` 字面量报错 |
| 元类、`__slots__` 的实际生效（可写但不做限制）、`__set_name__` | 不支持 |
| `match` 语句、`except*`、`async` 推导式 | 不支持 |
| 命名空间包（无 `__init__.py` 的包） | 不支持，包需要 `__init__.py` |
| `set`/`dict` 对自定义对象的哈希 | 使用身份/结构比较，不调用用户 `__hash__`/`__eq__` |
| `complex`、`decimal`、`fractions`、`datetime` 等标准库 | 未实现 |

## 代码结构

```
src/
├── main.rs      命令行入口（参数解析、退出码、大栈线程）
├── lib.rs       库接口：Session（大栈工作线程 + 输出捕获）、run_capture
├── repl.rs      交互式解释器（多行续行、表达式回显）
├── lexer.rs     词法分析：缩进栈、f-string 片段抽取、字符串转义、数字字面量
├── ast.rs       AST 定义（语句 / 表达式 / 参数 / 推导式）
├── parser.rs    递归下降 + 优先级爬升，语法错误带行列号
├── value.rs     运行时值模型、字典/集合、float/str 的 repr、格式化迷你语言
├── env.rs       作用域链（LEGB、global/nonlocal）
├── interp.rs    求值器主体：语句执行、表达式求值、调用约定、类构造、异常、导入
├── ops.rs       运算符、比较、真值、属性访问、下标、字符串化、魔术方法分派
├── methods.rs   内置类型的方法表（str/list/dict/set/tuple/int/float/range/file）
├── modules.rs   内置模块 + 用户模块导入（文件查找、缓存、循环导入占位）
└── builtins.rs  内置函数、内置类型构造、内置异常体系
```

测试：

```
tests/language.rs     19 个语言特性测试（结果、异常类型、traceback、文件 IO、模块导入）
tests/examples.rs     10 个示例程序与期望输出逐字节比对
tests/cpython_diff.rs 若本机有 CPython，则同一脚本两边执行并逐字节比对（无则跳过）
```

## 实现要点

1. **缩进敏感的词法分析**：维护缩进宽度栈（Tab 前进到下一个 8 的倍数），空行与纯注释行不产生
   `NEWLINE`/`INDENT`；括号深度不为 0 时换行不结束逻辑行；反缩进不匹配任何外层级别时报
   `IndentationError`。

2. **f-string**：词法阶段只做「片段抽取」——把 `{...}` 内部的表达式源码原样取出（正确处理嵌套括号、
   字符串字面量、`!r` 转换与 `:` 之后的格式说明符），在语法阶段再递归调用解析器生成真正的 AST。
   格式说明符里还可以嵌套替换字段（`f"{x:>{w}}"`），同样在语法阶段解析。

3. **作用域与闭包**：作用域是 `Rc<Env>` 链，闭包直接捕获定义处的环境引用，因此
   `for` 循环里创建的 lambda 与 CPython 一样是「迟绑定」的。`global` / `nonlocal` 通过在
   环境上记录名字集合来实现写穿透。

4. **调用约定**：实现 CPython 的参数绑定顺序——位置参数按序填充、`*args` 收集多余位置参数、
   关键字参数回填位置参数（重复报 `got multiple values`）、关键字专用参数、`**kwargs` 收集，
   并复现了 `missing 1 required positional argument` 一类消息。

5. **类的构造**：先建立 `ClassData`（含 C3 线性化的 MRO），再在类作用域中执行类体，最后把
   类命名空间搬进类字典；类体中的函数定义记录「所属类」，于是零参 `super()` 在任意调用路径
   （含 `__init__`、魔术方法、继承来的方法）中都能正确定位 `__class__`。

6. **异常**：异常对象是真正的实例，异常匹配按 `isinstance` 语义（而不是名字前缀），因此
   `except 父类` 能捕获子类、`except 子类` 不会误捕父类。traceback 在异常产生处快照调用栈，
   打印时再从源码缓存中取出对应行。

7. **输出与数值细节**：`repr(float)` 复刻 CPython 的最短往返表示（含 `1e+16`、`1.5e-07`、
   `-0.0`）；`//` 与 `%` 使用 Python 的向下取整语义（`-7 // 2 == -4`、`-7 % 2 == 1`）；
   `round` 借助 Rust 的「四舍六入五成双」格式化，得到与 CPython 一致的结果
   （`round(2.675, 2) == 2.67`）。

8. **属性协议**：`object` 上挂有真正的 `__getattribute__` / `__setattr__` / `__delattr__` /
   `__repr__` / `__eq__` / `__hash__` 等默认实现，因此 `object.__setattr__(self, ...)` 这类
   绕过挂钩的写法可用；只有**用户自定义**的挂钩才会被调用（否则每次属性访问都要多绕一层，
   property 也会被绕过）。`obj.__dict__` 返回**活动视图**（实例命名空间本身就是 `DictData`），
   所以 `self.__dict__[k] = v` 会真正写回实例。

9. **相对导入**：解析基准是**定义处模块**的 `__package__`（与 CPython 一致），而不是
   「当前正在执行的模块」——因此包内函数被别的模块调用时，函数体里的 `from . import x`
   仍然正确。`from pkg import name` 在属性缺失时会回退为导入子模块。

10. **递归保护**：调用函数时同时检查递归深度与**实际栈用量**（栈基点由顶层执行时记录），
    超出预算抛 `RecursionError`，而不是让进程栈溢出。命令行与 `Session` 使用 64MB 栈。

## 持续集成

`.github/workflows/ci.yml` 在 push / PR 时运行四个 job：

| Job | 内容 |
| --- | --- |
| `test`（ubuntu / windows / macos 三平台矩阵） | `cargo build --release --locked`、`cargo test --all-targets --locked`，并用 `examples/*.py` 与仓库中的 `.out` 期望输出**逐字节 diff**（失败时把实际输出作为 artifact 上传） |
| `lint` | `cargo fmt --all -- --check` 与 `cargo clippy --all-targets --locked -- -D warnings`（警告即失败） |
| `cross-check-python` | 用 runner 自带的 CPython 重跑示例并与 `.out` 比对，确认期望输出不是「本机特有」的 |
| `msrv` | 在 Rust 1.75.0 上 `cargo check --all-targets --locked`，保证最低支持版本声明不失效 |

`tests/cpython_diff.rs` 会在有 CPython 的环境里自动做双引擎差分对照，没有则跳过，因此本地与 CI 行为一致。
另外 `.github/dependabot.yml` 会按月提交 action 版本升级的 PR。

## 测试与 CPython 对照

```bash
cargo test                      # 全部测试
cargo test --test cpython_diff  # 只跑与 CPython 的逐字节对照
```

示例覆盖：`fizzbuzz`、`fibonacci`、`classes`、`exceptions`、`functional`、`text_stats`、
`algorithms`、`stdlib_io`、`mandelbrot`、`modern_python`（海象运算符 / f-string 调试语法 /
属性钩子 / 活动 `__dict__` / 字典合并 / 动态建类 / 旧式序列协议 / math 扩充）。

`examples/*.out` 是由 CPython 生成的期望输出：

```bash
python -W ignore examples/fizzbuzz.py > examples/fizzbuzz.out
```

开发过程中用同一套脚本在两边执行并逐行 diff，覆盖了数字语义、字符串与格式化、容器、
控制流、函数与闭包、类与继承、异常、推导式、内置函数、模块、文件 IO 等场景；
除下述「已知限制」中的差异外，输出与 CPython 3.13 完全一致
（异常消息文字的细微差别除外）。

## 已知限制

1. **没有大整数**：`int` 是 64 位有符号整数。`2 ** 100`、`fact(25)` 这类溢出的运算会自动
   升级为 `float`（因此会损失精度），而不是像 CPython 那样保持精确整数。
2. **集合/字典对自定义对象使用身份比较**：`{Obj(1), Obj(1)}` 会有两个元素，
   用户的 `__hash__`/`__eq__` 不参与容器查找（内置类型与元组不受影响）。
3. **`set` 的打印顺序是插入顺序**，CPython 是哈希顺序，因此打印集合内容的输出可能不同。
4. **生成器表达式不惰性**：`(x for x in range(10**9))` 会立即展开成列表；
   `yield` 生成器函数完全不支持。
5. **不支持 bytes / 大整数 / 元类 / 异步**，详见[语言支持矩阵](#语言支持矩阵)。
6. **标准库很小**：仅 `math`、`random`、`sys`、`string` 与用户自定义模块。
7. **`random` 使用 xorshift 而非 Mersenne Twister**：同一种子在本实现内可复现，
   但与 CPython 的随机序列不同。
8. **没有垃圾回收**：使用 `Rc`，循环引用（例如父对象与子对象互指）会泄漏内存。
   对短命脚本无影响，长时间运行的服务需要注意。
9. **`math` 中少数函数与 CPython 有末位差异**：`expm1` / `log1p` / `cbrt` 等依赖底层数学库
   （Rust 与 glibc 的实现不同），在 1e-10 这类量级上最后一位可能不同；整数类函数
   （`prod`/`isqrt`/`comb`/`perm`/`lcm`）与 `math.pi` 等常量完全一致。
10. **`__slots__` 只是普通类属性**：写了不会报错，但也不会限制实例属性。
11. **性能**：树遍历解释器，未做 inline cache、字节码等优化。实测在「循环 + 函数调用 + 类实例化 +
   字符串拼接」的混合负载上约为 CPython 3.13 的 1/4 速度（约慢 4 倍），
   在重度递归或属性访问密集的代码上差距会更大。对脚本与教学用途完全够用。

## 扩展指南

**加一个内置函数**：在 `src/builtins.rs` 写一个

```rust
fn bi_double(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.len() != 1 {
        return i.err("TypeError", "double() 需要 1 个参数");
    }
    i.binop(crate::lexer::Op::Star, args[0].clone(), Value::Int(2))
}
```

再在 `native_functions()` 里注册 `nf!("double", bi_double)` 即可。

**加一个类型方法**：在 `src/methods.rs` 写好函数，然后在 `lookup_method()` 的对应类型分支里
挂上名字；接收者始终是 `args[0]`。

**加一个模块**：在 `src/modules.rs` 的 `create_builtin_module()` 里增加分支，
用 `module_from("名字", vec![("函数名", 函数指针)], vec![("常量名", 值)])` 组装。

**加语法特性**：`ast.rs` 增加节点 → `parser.rs` 解析 → `interp.rs`（或 `ops.rs`）求值，
三步各加一处即可。

## 许可证

MIT
