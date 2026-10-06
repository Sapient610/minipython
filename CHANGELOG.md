# 更新日志

本文件记录 MiniPython 的所有重要变更，格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

暂无。

## [0.1.0] - 2026-10-06

首个版本：一个从零手写、**纯 `std`（零第三方依赖）** 的 Python 子集解释器。

### Added · 解释器核心

- **词法分析**：缩进敏感（INDENT/DEDENT，Tab 按 8 列对齐）、注释、括号内隐式续行、`\` 续行、
  三引号与 raw 字符串、f-string（`!r`/`!s`/格式说明符/嵌套字段/`f'{x=}'` 调试语法）、
  二/八/十六进制与下划线数字
- **语法分析**：递归下降 + 优先级爬升，语法错误带行列号；支持链式赋值、解包（含 `*rest`）、
  注解赋值、`if/elif/else`、`while/else`、`for/else`、`def`（默认值/`*args`/`**kwargs`/关键字专用参数/装饰器）、
  `class`（多继承）、`lambda`、`try/except/else/finally`、`raise ... from`、`assert`、`del`、
  `global`/`nonlocal`、`import`/`from ... import`（含包内相对导入）、`with`、海象运算符 `:=`
- **求值器**：树遍历；LEGB 作用域链与闭包（迟绑定）、C3 线性化 MRO、`super()`（零参与双参）、
  `staticmethod`/`classmethod`/`property`、`__getattr__`/`__setattr__`/`__delattr__`/`__getattribute__`
  钩子与 `object.__setattr__` 等默认实现、活动的 `obj.__dict__`、`type(name, bases, ns)` 动态建类、
  旧式序列协议（只实现 `__getitem__` 也能迭代）、`dict | dict` 与 `|=`
- **异常**：完整内置异常层级、自定义异常、按继承关系匹配、`finally` 覆盖语义、`raise` 重新抛出、
  CPython 风格 traceback（含源码行）
- **数据类型**：`None`/`bool`/`int`(i64)/`float`/`str`/`list`/`tuple`/`dict`(保持插入序)/`set`/`range`/`slice`/
  函数/类/实例/模块/迭代器/文件对象
- **内置函数**：`print`、`len`、`range`、类型构造（`int`/`float`/`str`/`bool`/`list`/`tuple`/`dict`/`set`/`type`）、
  `isinstance`、`issubclass`、`repr`、`abs`、`min`、`max`、`sum`、`sorted`、`reversed`、`enumerate`、
  `zip`（含 `strict`）、`map`、`filter`、`all`、`any`、`round`、`divmod`、`pow`、`chr`、`ord`、`hex`、`oct`、`bin`、
  `id`、`hash`、`input`、`open`、`iter`、`next`、`format`、`dir`、`globals`、`vars`、`getattr`/`setattr`/
  `hasattr`/`delattr`、`callable`、`staticmethod`、`classmethod`、`property`、`super`、`exit`
- **内置方法**：`str` 约 38 个、`list` 11 个、`dict` 10 个、`set` 14 个、`tuple` 2 个，
  `int.bit_length`/`bit_count`，文件对象的 `read`/`readline`/`readlines`/`write`/`writelines`/`flush`/`close`
  与上下文管理器/迭代协议
- **内置模块**：`math`（50 个函数 + 常量，含 `prod`/`isqrt`/`comb`/`perm`/`lcm`/`dist`/`modf`/`frexp`/`ldexp`/
  `isclose`/`remainder`/`nextafter`/`ulp`/`cbrt`/`expm1`/`log1p`）、`random`、`string`、`sys`，
  以及自定义模块与包（`import pkg.mod`、包内相对导入）
- **工具**：脚本执行、`-c` 片段、多行续行的交互式 REPL、可嵌入的 `Session`（大栈工作线程 + 输出捕获）、
  `sys.argv`、`sys.exit(n)` 退出码

### Added · 工程与文档

- 10 个示例程序（`examples/*.py`）及其由 CPython 3.13 生成的期望输出（`examples/*.out`）
- 69 项测试：单元测试、语言特性测试（35 项）、示例逐字节比对、与 CPython 的自动差分对照
- GitHub Actions：ubuntu / windows / macos 三平台测试矩阵、`fmt`+`clippy -D warnings` 门禁、
  期望输出与 runner 自带 CPython 的交叉验证、Rust 1.75 MSRV 检查；Dependabot 按月升级 action
- 中文 `README.md`（特性矩阵、实现要点、已知限制、扩展指南）、`AGENTS.md`（代理与贡献者须知）、
  项目技能 `.agents/skills/minipython-dev/`

### Fixed · 开发期通过与 CPython 差分发现并修复的语义问题

- f-string 字段解析（`!r`/格式说明符/`f'{x=}'` 的位置与转义）
- `lambda` 参数被误当作带注解的参数解析
- 切片赋值 `a[1:3] = [...]` 未删除被替换元素
- 异常匹配按名字前缀导致 `except 子类` 误捕父类
- `with` 语句中 `__exit__` 返回真值未抑制异常
- 属性挂钩分派顺序（`__getattribute__` 抛 `AttributeError` 未回退到 `__getattr__`）
- `object.__str__` 未委派给最派生的 `__repr__`
- 定义了 `__eq__` 却没定义 `__hash__` 的对象仍可哈希
- `round()` 的十进制舍入（`round(2.675, 2) == 2.67`）
- `%` 格式化的宽度/对齐/`#` 前缀（`%-45s`、`{:08b}`、`{:#x}`）
- 整数除法与取模的 Python 语义（`-7 // 2 == -4`、`-7 % 2 == 1`）
- `type(Class)` 应返回 `type`
- 词法分析器在文件以换行结尾时死循环

### Known limitations

详见 `README.md` 的「已知限制」：无 bigint（i64 溢出升级为 float）、容器对自定义对象用身份比较、
`set` 迭代为插入序、生成器表达式不惰性、无 `yield`/`async`/`bytes`/元类、标准库仅 4 个模块、
`Rc` 循环引用不回收、`math` 中少数函数依赖平台 libm 有末位差异。

[Unreleased]: https://github.com/Sapient610/minipython/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Sapient610/minipython/releases/tag/v0.1.0
