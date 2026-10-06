# 加特性的逐步配方

每个配方都给出「改哪些文件 + 关键代码骨架 + 别忘了什么」。

---

## A. 新语法（新语句 / 新表达式）

以「加一个 `match` 语句」为例（其它语法同理）：

1. **`src/ast.rs`**：加节点

```rust
pub enum StmtKind {
    // ...
    Match { subject: Expr, cases: Vec<MatchCase> },
}
pub struct MatchCase { pub pattern: Pattern, pub guard: Option<Expr>, pub body: Vec<Stmt> }
```

2. **`src/parser.rs`**：解析到该节点
   - 语句级：在 `parse_compound()` 里加分支（`if self.at_kw(Kw::Match) { ... }`），注意 `expect(Op::Colon)` + `parse_block()`
   - 表达式级：想清楚**优先级**挂在哪一层（`parse_ternary` / `parse_or` / `parse_factor` / `parse_postfix`），并处理终止符（逗号、`)`、`:`、`in` 等；`parse_target_list` 是"不能吃掉 `in` 的赋值目标"特例
   - 关键字要先在 `src/lexer.rs` 的 `Kw::keyword()` 里登记

3. **`src/interp.rs`**：`exec_stmt` 加分支执行（语句）或 `eval` 加分支求值（表达式）
   - 要传递控制流就返回 `Err(Signal::Break/Continue/Return(..))`
   - 求值顺序、异常传播都要和 CPython 对齐（例如 `and`/`or` 短路返回**操作数本身**而不是 bool）

4. **测试**：`tests/language.rs` 加断言 + 临时脚本与 CPython 差分

> 别忘了：新增 AST 节点后，`match &e.kind` 的**穷尽性**会逼你补齐全部分派点（这是好事）。

---

## B. 新内置函数

`src/builtins.rs`：

```rust
fn bi_double(i: &mut Interp, args: &[Value], kwargs: &[(String, Value)]) -> EResult<Value> {
    if args.len() != 1 {
        return i.err("TypeError", format!("double() takes exactly one argument ({} given)", args.len()));
    }
    i.binop(crate::lexer::Op::Star, args[0].clone(), Value::Int(2))
}
```

在 `native_functions()` 里注册：`nf!("double", bi_double),`

要点：

- 签名固定：`(&mut Interp, &[Value], &[(String, Value)]) -> EResult<Value>`（`NativeFn` 类型别名在 `value.rs`）
- 取关键字参数用 `kwargs.iter().find(|(k, _)| k == "strict")`
- 参数报错用 `i.err("TypeError", ...)`；异常**类型**必须和 CPython 一致
- 需要走迭代协议就用 `i.collect_iter(&v)`；需要真值判断用 `i.truthy(&v)?`
- 想返回迭代器（而非列表），构造 `Value::Iterator(Rc::new(RefCell::new(IterKind::List(rc, 0))))`

---

## C. 新类型方法

`src/methods.rs`：

```rust
fn str_my_method(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need_between(i, a, "my_method", 1, 2)?;   // 参数个数（含接收者）
    let s = as_str(&a[0]);                     // 接收者永远是 a[0]
    Ok(Value::str_from(s.to_uppercase()))
}
```

挂到 `lookup_method()` 的对应类型分支：`"str" => match name { "my_method" => str_my_method, ... }`

要点与辅助函数：

- `need` / `need_between(i, a, name, lo, hi)` 做参数个数检查
- `as_str` / `as_list` / `as_dict` / `as_set` / `as_file` / `as_iter` 取接收者
- `as_int(i, v)` 走 `__index__`，`int_arg` 用于报错更清晰的场景
- 会改内容的方法（`sort`/`reverse`/`append`）返回 `Value::None`
- 只读方法返回**新对象**，不要就地改（`list.copy`/`set.union` 等）
- 若该类型支持 `dir()`，同步 `method_names()`

---

## D. 新魔术方法

分派点都在 `src/ops.rs`：`binop`（算术/位运算）、`aug_binop`（`+=` 等原地语义）、`eq_values`、`order_compare`、`truthy`、`unop`、`get_index`/`set_index`/`del_index`、`get_attr`/`set_attr`/`del_attr`、`contains`、`value_str`/`value_repr`/`format_with_spec`，以及 `interp.rs` 的 `get_iter`/`iter_next`/`call_class`。

两种写法：

```rust
// 1) 需要"绑定接收者 + 压 class_ctx（供 super() 用）"时，用 call_dunder
if let Some(r) = self.call_dunder(&recv, &cls, "__mydunder__", vec![arg])? { ... }

// 2) 只需要原始函数体时用 find_class_attr + call_value（记得自己补接收者）
```

**关键规则**：默认行为由 `object` 上的原生实现提供，只有在子类**用户自定义**了同名方法时才该改变行为 —— 判定用 `find_user_dunder(&cls, name)`（它跳过 `object` 的条目）。
反例：若无条件调用 `__setattr__`，就会命中 `object.__setattr__`，导致 **property setter 被绕过**。

---

## E. 新内置模块

`src/modules.rs`：

```rust
fn math_my_fn(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> { ... }

// 1) 在 create_builtin_module 里加分支
"mymod" => module_from("mymod", vec![("my_fn", math_my_fn as NativeFn)], vec![("CONST", Value::Int(7))]),

// 2) 在 is_builtin_module 里登记名字
matches!(name, "math" | "random" | "sys" | "string" | "mymod")
```

要点：

- 需要惰性初始化或依赖 `Interp` 状态（如 `sys.argv`、`self.rng`）时，在 `create_builtin_module` 里手动 `md.dict.borrow_mut().insert(...)`
- 嵌套子模块（如 `os.path`）：登记 `"os.path"` 为内置模块名；`import_module` 会先建父模块再把子模块挂上去（`import os.path` 后 `os.path.join` 可用）
- 用户模块导入走 `load_module_from_file`：按 `search_path` 找 `<name>.py` 或 `<name>/__init__.py`；执行前登记占位模块以支持循环导入
- 相对导入依赖模块作用域里的 `__package__`（包自身=包名，普通模块=去掉最后一段，顶层模块=空串，主脚本=None）

---

## F. 内置类型构造 / `type()` 分支

- **可调用**的内置类型（`int("3")`、`dict(a=1)`）：在 `builtins.rs` 的 `call_builtin_type()` 加分支（`ClassData::builtin` 为 `Some(name)` 时 `call_class` 会转到它）
- **三参 `type(name, bases, ns)`**：`call_builtin_type("type")` 的 `3 =>` 分支收集 bases/命名空间后交给 `Interp::build_class`
- 新增内置类型需要：① `init_builtins` 的 `TYPE_NAMES` 注册类型对象 ② `Value::type_name()` 返回同名 ③ `is_instance_of` 的 `builtin` 分支能识别 ④（可选）`methods.rs` 的方法表

---

## G. 异常行为

- 内置异常层级在 `src/error.rs` 的 `BUILTIN_EXCEPTIONS`（名称→直接基类）；`init_builtins` 会据此建类，`exception_bases()` 用于名字匹配
- `BaseException.__init__` 负责把参数存进 `self.args`（`super().__init__(...)` 依赖它）
- 想给某个异常定制 `str()`：`Interp::instance_str`（例如 `KeyError` 用键的 repr）
- 抛出时携带异常对象（`PyError.value = Some(instance)`）会让匹配**只按继承关系**，这是 `raise MyError("x")` 能被 `except MyError` 捕获的原因；内部错误（无实例）则按内置层级名匹配
