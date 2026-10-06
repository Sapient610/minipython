# 架构与关键不变量

## 数据流

```
源码 &str
  → Lexer::tokenize            src/lexer.rs      Vec<Token>（含 INDENT/DEDENT/NEWLINE）
  → Parser::parse_source       src/parser.rs     Vec<Stmt>（AST）
  → Interp::run_source         src/interp.rs
      exec_block(&[Stmt], &EnvRef)  执行语句，维护 self.frames（traceback 用）
      eval(&Expr, &EnvRef)          求值表达式
  → Value                      src/value.rs     运行时值
```

REPL 走 `Interp::exec_repl_stmts`（保留主模块作用域，表达式语句回显 repr）；
嵌入场景走 `Session`（64MB 栈工作线程 + 捕获 stdout，`src/lib.rs`）。

## 核心类型

| 类型 | 位置 | 要点 |
| --- | --- | --- |
| `Value` | value.rs | 20+ 个变体：`None/Bool/Int(i64)/Float/Str(Rc<str>)/List/Tuple/Dict/Set/Range/Slice/Func/Native/BoundNative/BoundMethod/Class/Instance/Module/Iterator/File/Descriptor/Property/Super` |
| `DictData` | value.rs | **Vec 支撑的有序字典**（线性查找，`key_equal` 比较），因此 `1` 与 `1.0` 是同一个键 |
| `SetData` | value.rs | 同样是 Vec，迭代序=插入序（与 CPython 的哈希序不同，测试里必须 `sorted()`） |
| `ClassData` | value.rs | `name/module/bases/dict/mro/is_exception/builtin`；`builtin: Some("int")` 表示内置类型（用于 `isinstance` 与构造分派） |
| `InstanceData` | value.rs | `class` + `dict: Rc<RefCell<DictData>>`；`get/set/remove/names` 是唯一推荐入口（`__dict__` 返回同一份存储） |
| `Env` / `EnvRef` | env.rs | `Rc<Env>` 链：`vars/parent/globals/global_names/nonlocal_names/kind`；`assign_walrus` 跳过 Comprehension 作用域 |
| `IterKind` | value.rs | `List/Tuple/Str/Range/Items/Object(__next__)/GetItem(旧式序列协议)` |
| `FuncData` | value.rs | `params(RtParams)/body/closure/is_lambda/owner`；**`owner` 是方法所属类**，`call_function` 用它压 `class_ctx` 供零参 `super()` 使用 |
| `Signal` | interp.rs | `Error(PyError)/Break/Continue/Return`；几乎处处是 `EResult<T> = Result<T, Signal>` |
| `PyError` | error.rs | `kind/message/value/trace`；`trace` 在**异常产生时**快照调用栈（`interp.frames`） |

## 求值流程要点

1. **traceback 行号**：`exec_stmt` 开头调用 `set_line(s.line)` 更新 `frames.last_mut().line`；`call_function` 压/弹帧并带函数名。所以错误在**产生处**快照即可得到"外层→内层"的顺序。
2. **调用约定**（`bind_params`）：位置参数按序填充 → 多余给 `*args` → 关键字回填位置参数（重复报 `got multiple values for argument`）→ 关键字专用 → `**kwargs`；缺失报 `missing 1 required positional argument: 'x'`。错误消息贴近 CPython。
3. **类构造**（`make_class`）：求 bases（拒内置类型与 object 之外的非法基类）→ C3 线性化 → **先建 `ClassData`**（`defining_class` 置为它，使类体中的函数拿到 `owner`）→ 在类作用域执行类体（`function_closure` 让方法闭包指向**类外**环境，这是 CPython 的语义）→ 把类命名空间搬进类字典 → 补 `__name__`。
4. **属性访问**（`get_instance_attr`）：用户自定义的 `__getattribute__` → `get_instance_attr_raw`（自身 dict → mro → `__class__`/`__dict__`）→ 失败且是 `AttributeError` 时走 `__getattr__` → 否则抛最初的错误。
5. **异常**：`run_handlers` 按子句顺序匹配（`exception_matches`：有异常对象时**只比类继承关系**）；`finally` 的异常覆盖前面挂起的信号；`with` 的 `__exit__` 返回真值即抑制异常（`unwind_with`）。
6. **导入**（`import_module`）：缓存于 `self.modules` → 内置模块 → 点分模块先导父包并挂子模块 → 文件加载（先登记占位模块以支持循环导入）。执行期把 `__name__`/`__package__`/`__file__` 写进模块作用域；**相对导入读的是定义处模块作用域的 `__package__`**，因此包内函数被别处调用时依然正确。

## 必须遵守的不变量

- **`ClassData::mro` 不含自身**；查找顺序永远是「自身 dict → mro 各项 dict」。
- **只有用户自定义的挂钩**才优先于默认行为：判定用 `find_user_dunder`（它会跳过 `object` 上的默认实现）。`object` 上真的有 `__getattribute__/__setattr__/__delattr__/__init__/__repr__/__str__/__eq__/__hash__/__format__`，这样 `object.__setattr__(self, ...)` 才能用。
- **`RefCell` 借用不跨调用**：拿到 `find_class_attr(...)` 的结果后先 `clone()`/`drop` 借用，再调用用户代码。
- **实例属性走 `InstanceData` 方法**，别自己 `borrow_mut` 一个 HashMap（类型已不是 HashMap）。
- **整数是 i64**，`+ - * **` 溢出**有意**升级为 `float`（README 已列进已知限制）。
- **repr 防环**：`value_repr_inner` 用 `seen: Vec<usize>` 检测自引用容器，输出 `[...]`/`{...}`。
- **序列重复上限**：`MAX_REPEAT`（`ops.rs`）限制 `[1]*n` / `"x"*n` 的规模，避免 OOM；新增类似操作请沿用。
- **递归保护**：`call_function` 检查 `depth >= max_depth || stack_used() > stack_budget`；`Interp` 直接嵌入小栈线程时请调大 `stack_budget`（debug 构建每层约 100KB 栈）。
- **无 GC**：`Rc` 循环引用会泄漏（父↔子互指），已知取舍。

## 有意的简化（别当 bug 修）

- `int` 是 64 位；无 bigint
- `set`/`dict` 对自定义对象用**身份/结构**比较，不调用用户 `__hash__`/`__eq__`
- 生成器表达式立即求值；无 `yield`
- 无 `bytes`、无元类、`__slots__` 不生效
- 标准库仅 `math/random/sys/string` + 用户模块
- `random` 是 xorshift 而非 Mersenne Twister（同种子可复现，但与 CPython 序列不同）
