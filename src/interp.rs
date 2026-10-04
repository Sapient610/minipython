//! 树遍历解释器：执行 AST、管理作用域、调用函数、类与异常。

use crate::ast::*;
use crate::env::{Env, EnvRef, ScopeKind};
use crate::error::{exception_bases, PyError, TraceFrame};
use crate::parser::Parser;
use crate::value::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;

/// 控制流信号：错误 / break / continue / return。
#[derive(Debug)]
pub enum Signal {
    Error(PyError),
    Break,
    Continue,
    Return(Value),
}

impl From<PyError> for Signal {
    fn from(e: PyError) -> Signal {
        Signal::Error(e)
    }
}

pub type EResult<T> = Result<T, Signal>;

/// 调用实参：位置参数 + 关键字参数
pub type CallArgs = (Vec<Value>, Vec<(String, Value)>);

/// 递归深度硬上限（作为栈用量检查之外的兜底）。
const DEFAULT_MAX_DEPTH: usize = 500;

/// 默认的栈用量预算：通用场景（例如 2MB 栈的测试线程）下安全。
/// 命令行入口使用 64MB 栈，会显式调大该预算。
const DEFAULT_STACK_BUDGET: usize = 1024 * 1024;

pub struct Interp {
    pub builtins: EnvRef,
    /// 已加载的模块
    pub modules: HashMap<String, Rc<ModuleData>>,
    /// 内置类型 / 内置函数使用的类对象
    pub type_classes: HashMap<String, Rc<ClassData>>,
    /// 内置类型构造函数
    pub ctor_names: Vec<String>,
    pub frames: Vec<TraceFrame>,
    pub sources: HashMap<String, Vec<String>>,
    pub out: Box<dyn Write>,
    /// 当前正在执行的模块作用域
    pub main_env: Option<EnvRef>,
    /// 当前文件名
    pub filename: String,
    /// 零参 super() 需要知道当前类与实例
    pub class_ctx: Vec<(Rc<ClassData>, Value)>,
    /// 正在执行的类体（用于给方法记录所属类）
    pub defining_class: Option<Rc<ClassData>>,
    /// 正在处理的异常（供 `raise` 重新抛出）
    pub exc_stack: Vec<PyError>,
    pub depth: usize,
    pub max_depth: usize,
    /// 递归时允许消耗的栈空间（字节）
    pub stack_budget: usize,
    /// 执行起点处的栈地址
    pub stack_base: usize,
    pub search_path: Vec<PathBuf>,
    pub argv: Vec<String>,
    /// 当前模块名（用于类 repr）
    pub module_name: String,
    /// 随机数状态（供 random 模块使用）
    pub rng: std::cell::Cell<u64>,
    /// 防止重复导入
    pub importing: Vec<String>,
}

impl Default for Interp {
    fn default() -> Self {
        Self::new()
    }
}

impl Interp {
    pub fn new() -> Interp {
        let builtins = Env::new_module();
        let mut it = Interp {
            builtins,
            modules: HashMap::new(),
            type_classes: HashMap::new(),
            ctor_names: Vec::new(),
            frames: Vec::new(),
            sources: HashMap::new(),
            out: Box::new(std::io::stdout()),
            main_env: None,
            filename: "<stdin>".to_string(),
            class_ctx: Vec::new(),
            defining_class: None,
            exc_stack: Vec::new(),
            depth: 0,
            max_depth: DEFAULT_MAX_DEPTH,
            stack_budget: DEFAULT_STACK_BUDGET,
            stack_base: 0,
            search_path: vec![PathBuf::from(".")],
            argv: Vec::new(),
            module_name: "__main__".to_string(),
            rng: std::cell::Cell::new(0x2545F4914F6CDD1D),
            importing: Vec::new(),
        };
        it.init_builtins();
        it
    }

    /// 使用自定义输出（便于测试捕获）。
    pub fn with_writer(mut self, w: Box<dyn Write>) -> Interp {
        self.out = w;
        self
    }

    /// 设置递归可消耗的栈预算（字节）。
    ///
    /// 解释器在调用函数时同时检查递归深度与栈用量，避免栈溢出。
    pub fn with_stack_budget(mut self, bytes: usize) -> Interp {
        self.stack_budget = bytes;
        self
    }

    pub fn set_source(&mut self, file: &str, src: &str) {
        self.sources
            .insert(file.to_string(), src.lines().map(|s| s.to_string()).collect());
    }

    // ---------------- 错误构造 ----------------

    pub fn pyerr(&self, kind: &str, message: impl Into<String>) -> PyError {
        PyError {
            kind: kind.to_string(),
            message: message.into(),
            value: None,
            trace: self.frames.clone(),
        }
    }

    pub fn err<T>(&self, kind: &str, message: impl Into<String>) -> EResult<T> {
        Err(Signal::Error(self.pyerr(kind, message)))
    }


    /// 构造 KeyError：消息用键的 repr，异常对象携带原始键。
    pub fn key_error(&self, key: &Value) -> PyError {
        let msg = crate::value::builtin_repr(key);
        let mut e = self.pyerr("KeyError", msg);
        if let Some(Value::Class(c)) = self.builtins.lookup("KeyError") {
            e.value = Some(self.instantiate_exception(&c, vec![key.clone()]));
        }
        e
    }

    pub fn signal_to_error(&mut self, sig: Signal) -> PyError {
        match sig {
            Signal::Error(e) => e,
            Signal::Break => self.pyerr("SyntaxError", "'break' outside loop"),
            Signal::Continue => self.pyerr("SyntaxError", "'continue' not properly in loop"),
            Signal::Return(_) => self.pyerr("SyntaxError", "'return' outside function"),
        }
    }

    // ---------------- 顶层执行 ----------------

    /// 估计当前已消耗的解释器栈空间。
    fn stack_used(&self) -> usize {
        let marker = 0u8;
        let addr = &marker as *const u8 as usize;
        self.stack_base.saturating_sub(addr)
    }

    /// 重置栈基点（每次顶层执行时调用）。
    fn reset_stack_base(&mut self) {
        let marker = 0u8;
        self.stack_base = &marker as *const u8 as usize;
    }

    pub fn run_source(&mut self, src: &str, filename: &str) -> Result<(), PyError> {
        self.reset_stack_base();
        let stmts = Parser::parse_source(src).map_err(|e| {
            PyError::new("SyntaxError", format!("{} (line {})", e.msg, e.line))
        })?;
        self.set_source(filename, src);
        self.filename = filename.to_string();
        let env = match &self.main_env {
            Some(e) => e.clone(),
            None => {
                let e = Env::new_module();
                e.define("__name__", Value::str_from("__main__"));
                e.define("__file__", Value::str_from(filename));
                self.main_env = Some(e.clone());
                e
            }
        };
        self.frames.push(TraceFrame {
            file: filename.to_string(),
            line: 0,
            func: "<module>".to_string(),
        });
        let r = self.exec_block(&stmts, &env);
        self.frames.pop();
        match r {
            Ok(()) => Ok(()),
            Err(sig) => Err(self.signal_to_error(sig)),
        }
    }

    /// 保证主模块作用域存在。
    pub fn ensure_main_env(&mut self, filename: &str) -> EnvRef {
        match &self.main_env {
            Some(e) => e.clone(),
            None => {
                let e = Env::new_module();
                e.define("__name__", Value::str_from("__main__"));
                e.define("__file__", Value::str_from(filename));
                self.main_env = Some(e.clone());
                e
            }
        }
    }

    /// REPL 用：在主模块作用域执行语句；`echo` 为真时返回最后一个表达式的值。
    pub fn exec_repl_stmts(
        &mut self,
        stmts: &[Stmt],
        echo: bool,
    ) -> Result<Option<Value>, PyError> {
        self.reset_stack_base();
        let env = self.ensure_main_env("<stdin>");
        self.frames.push(TraceFrame {
            file: "<stdin>".to_string(),
            line: 0,
            func: "<module>".to_string(),
        });
        let mut result: Result<Option<Value>, Signal> = Ok(None);
        for s in stmts {
            match &s.kind {
                StmtKind::Expr(e) if echo => match self.eval(e, &env) {
                    Ok(v) => result = Ok(Some(v)),
                    Err(sig) => {
                        result = Err(sig);
                        break;
                    }
                },
                _ => {
                    if let Err(sig) = self.exec_stmt(s, &env) {
                        result = Err(sig);
                        break;
                    }
                }
            }
        }
        self.frames.pop();
        match result {
            Ok(v) => Ok(v),
            Err(sig) => Err(self.signal_to_error(sig)),
        }
    }

    pub fn run_file(&mut self, path: &str) -> Result<(), PyError> {        let src = std::fs::read_to_string(path)
            .map_err(|e| PyError::new("OSError", format!("无法读取 {}: {}", path, e)))?;
        if let Some(dir) = std::path::Path::new(path).parent() {
            let d = if dir.as_os_str().is_empty() {
                PathBuf::from(".")
            } else {
                dir.to_path_buf()
            };
            if !self.search_path.contains(&d) {
                self.search_path.insert(0, d);
            }
        }
        self.run_source(&src, path)
    }

    /// 打印 traceback（与 CPython 风格接近）。
    pub fn format_traceback(&self, err: &PyError) -> String {
        let mut s = String::new();
        if err.trace.is_empty() {
            s.push_str(&err.summary());
            return s;
        }
        s.push_str("Traceback (most recent call last):\n");
        for f in &err.trace {
            s.push_str(&format!(
                "  File \"{}\", line {}, in {}\n",
                f.file, f.line, f.func
            ));
            if let Some(lines) = self.sources.get(&f.file) {
                if f.line > 0 && (f.line as usize) <= lines.len() {
                    let text = lines[(f.line - 1) as usize].trim();
                    if !text.is_empty() {
                        s.push_str(&format!("    {}\n", text));
                    }
                }
            }
        }
        s.push_str(&err.summary());
        s
    }

    // ---------------- 语句执行 ----------------

    fn set_line(&mut self, line: u32) {
        if let Some(f) = self.frames.last_mut() {
            f.line = line;
        }
    }

    pub fn exec_block(&mut self, stmts: &[Stmt], env: &EnvRef) -> EResult<()> {
        for s in stmts {
            self.exec_stmt(s, env)?;
        }
        Ok(())
    }

    fn exec_stmt(&mut self, s: &Stmt, env: &EnvRef) -> EResult<()> {
        self.set_line(s.line);
        match &s.kind {
            StmtKind::Expr(e) => {
                self.eval(e, env)?;
                Ok(())
            }
            StmtKind::Pass => Ok(()),
            StmtKind::Break => Err(Signal::Break),
            StmtKind::Continue => Err(Signal::Continue),
            StmtKind::Return(v) => {
                let val = match v {
                    Some(e) => self.eval(e, env)?,
                    None => Value::None,
                };
                Err(Signal::Return(val))
            }
            StmtKind::Assign { targets, value } => {
                let v = self.eval(value, env)?;
                for t in targets {
                    self.assign_target(t, v.clone(), env)?;
                }
                Ok(())
            }
            StmtKind::AnnAssign { target, value, .. } => {
                if let Some(e) = value {
                    let v = self.eval(e, env)?;
                    self.assign_target(target, v, env)?;
                }
                Ok(())
            }
            StmtKind::AugAssign { target, op, value } => {
                let rhs = self.eval(value, env)?;
                match &target.kind {
                    ExprKind::Name(n) => {
                        let cur = self.lookup_name(n, env)?;
                        let newv = self.aug_binop(*op, cur, rhs)?;
                        env.assign(n, newv);
                        Ok(())
                    }
                    ExprKind::Attr { obj, name } => {
                        let o = self.eval(obj, env)?;
                        let cur = self.get_attr(&o, name)?;
                        let newv = self.aug_binop(*op, cur, rhs)?;
                        self.set_attr(&o, name, newv)
                    }
                    ExprKind::Index { obj, index } => {
                        let o = self.eval(obj, env)?;
                        let i = self.eval(index, env)?;
                        let cur = self.get_index(&o, &i)?;
                        let newv = self.aug_binop(*op, cur, rhs)?;
                        self.set_index(&o, &i, newv)
                    }
                    _ => self.err("SyntaxError", "非法的增量赋值目标"),
                }
            }
            StmtKind::If { test, body, orelse } => {
                let c = self.eval(test, env)?;
                if self.truthy(&c)? {
                    self.exec_block(body, env)
                } else {
                    self.exec_block(orelse, env)
                }
            }
            StmtKind::While { test, body, orelse } => {
                let mut broke = false;
                loop {
                    let c = self.eval(test, env)?;
                    if !self.truthy(&c)? {
                        break;
                    }
                    match self.exec_block(body, env) {
                        Ok(()) => {}
                        Err(Signal::Break) => {
                            broke = true;
                            break;
                        }
                        Err(Signal::Continue) => continue,
                        Err(e) => return Err(e),
                    }
                }
                if !broke {
                    self.exec_block(orelse, env)?;
                }
                Ok(())
            }
            StmtKind::For {
                target,
                iter,
                body,
                orelse,
            } => {
                let iterable = self.eval(iter, env)?;
                let it = self.get_iter(&iterable)?;
                let mut broke = false;
                loop {
                    let item = match self.iter_next(&it)? {
                        Some(v) => v,
                        None => break,
                    };
                    self.assign_target(target, item, env)?;
                    match self.exec_block(body, env) {
                        Ok(()) => {}
                        Err(Signal::Break) => {
                            broke = true;
                            break;
                        }
                        Err(Signal::Continue) => continue,
                        Err(e) => return Err(e),
                    }
                }
                if !broke {
                    self.exec_block(orelse, env)?;
                }
                Ok(())
            }
            StmtKind::FuncDef(fd) => {
                let f = self.make_function(fd, env, &fd.name, false)?;
                let f = self.apply_decorators(&fd.decorators, f, env)?;
                env.assign(&fd.name, f);
                Ok(())
            }
            StmtKind::ClassDef(cd) => {
                let c = self.make_class(cd, env)?;
                let c = self.apply_decorators(&cd.decorators, c, env)?;
                env.assign(&cd.name, c);
                Ok(())
            }
            StmtKind::Global(names) => {
                let mut g = env.global_names.borrow_mut();
                for n in names {
                    g.insert(n.clone());
                }
                Ok(())
            }
            StmtKind::Nonlocal(names) => {
                let mut g = env.nonlocal_names.borrow_mut();
                for n in names {
                    g.insert(n.clone());
                }
                Ok(())
            }
            StmtKind::Assert { test, msg } => {
                let c = self.eval(test, env)?;
                if self.truthy(&c)? {
                    return Ok(());
                }
                let m = match msg {
                    Some(e) => {
                        let v = self.eval(e, env)?;
                        self.value_str(&v)?
                    }
                    None => String::new(),
                };
                self.err("AssertionError", m)
            }
            StmtKind::Delete(targets) => {
                for t in targets {
                    match &t.kind {
                        ExprKind::Name(n) => {
                            if !env.delete(n) {
                                return self.err(
                                    "NameError",
                                    format!("name '{}' is not defined", n),
                                );
                            }
                        }
                        ExprKind::Attr { obj, name } => {
                            let o = self.eval(obj, env)?;
                            self.del_attr(&o, name)?;
                        }
                        ExprKind::Index { obj, index } => {
                            let o = self.eval(obj, env)?;
                            let i = self.eval(index, env)?;
                            self.del_index(&o, &i)?;
                        }
                        _ => return self.err("SyntaxError", "无法删除的表达式"),
                    }
                }
                Ok(())
            }
            StmtKind::Raise { exc, cause } => {
                let _ = cause;
                match exc {
                    None => match self.exc_stack.last() {
                        Some(e) => Err(Signal::Error(e.clone())),
                        None => self.err("RuntimeError", "No active exception to reraise"),
                    },
                    Some(e) => {
                        let v = self.eval(e, env)?;
                        Err(Signal::Error(self.make_raise(v)?))
                    }
                }
            }
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                let mut pending: Option<Signal> = None;
                match self.exec_block(body, env) {
                    Ok(()) => {
                        if !orelse.is_empty() {
                            if let Err(sig) = self.exec_block(orelse, env) {
                                pending = Some(sig);
                            }
                        }
                    }
                    Err(Signal::Error(e)) => {
                        if let Err(sig) = self.run_handlers(handlers, e, env) {
                            pending = Some(sig);
                        }
                    }
                    Err(sig) => pending = Some(sig),
                }
                if !finalbody.is_empty() {
                    self.exec_block(finalbody, env)?;
                }
                match pending {
                    Some(sig) => Err(sig),
                    None => Ok(()),
                }
            }
            StmtKind::With { items, body } => {
                let mut entered: Vec<(Value, Value)> = Vec::new();
                for (ctx, target) in items {
                    let cm = self.eval(ctx, env)?;
                    let enter = self.get_attr(&cm, "__enter__")?;
                    match self.call_value(&enter, Vec::new(), Vec::new()) {
                        Ok(v) => {
                            if let Some(t) = target {
                                self.assign_target(t, v.clone(), env)?;
                            }
                            let exit = self.get_attr(&cm, "__exit__")?;
                            entered.push((cm, exit));
                        }
                        Err(e) => {
                            // __enter__ 失败时，之前进入的上下文仍要退出
                            let sig = self.unwind_with(&mut entered, None)?;
                            return match sig {
                                Some(s) => Err(s),
                                None => Err(e),
                            };
                        }
                    }
                }
                let result = self.exec_block(body, env);
                let err = match &result {
                    Err(Signal::Error(e)) => Some(e.clone()),
                    _ => None,
                };
                let had_error = err.is_some();
                let raised = self.unwind_with(&mut entered, err)?;
                if let Some(sig) = raised {
                    return Err(sig);
                }
                if had_error {
                    // __exit__ 返回真值：异常被抑制
                    return Ok(());
                }
                result
            }
            StmtKind::Import { names } => {
                for (path, alias) in names {
                    let full = path.join(".");
                    let m = self.import_module(&full)?;
                    match alias {
                        Some(a) => env.assign(a, m),
                        None => {
                            let root = self.import_module(&path[0])?;
                            env.assign(&path[0], root);
                        }
                    }
                }
                Ok(())
            }
            StmtKind::ImportFrom {
                module,
                names,
                level,
            } => {
                if *level > 0 {
                    return self.err("ImportError", "暂不支持相对导入");
                }
                let m = self.import_module(&module.join("."))?;
                for (name, alias) in names {
                    if name == "*" {
                        if let Value::Module(md) = &m {
                            let d = md.dict.borrow();
                            for (k, v) in d.iter() {
                                if !k.starts_with('_') {
                                    env.assign(k, v.clone());
                                }
                            }
                        }
                        continue;
                    }
                    let v = self.get_attr(&m, name)?;
                    env.assign(alias.as_ref().unwrap_or(name), v);
                }
                Ok(())
            }
        }
    }

    fn unwind_with(
        &mut self,
        entered: &mut Vec<(Value, Value)>,
        err: Option<PyError>,
    ) -> EResult<Option<Signal>> {
        let mut suppressed = false;
        while let Some((cm, exit)) = entered.pop() {
            let args: Vec<Value> = match &err {
                Some(e) => vec![
                    self.exc_type_value(e),
                    self.exc_value_of(e),
                    Value::None,
                ],
                None => vec![Value::None, Value::None, Value::None],
            };
            let _ = cm;
            match self.call_value(&exit, args, Vec::new()) {
                Ok(v) => {
                    if self.truthy(&v)? {
                        suppressed = true;
                    }
                }
                Err(sig) => return Ok(Some(sig)),
            }
        }
        match (err, suppressed) {
            (Some(e), false) => Ok(Some(Signal::Error(e))),
            _ => Ok(None),
        }
    }

    fn exc_type_value(&self, e: &PyError) -> Value {
        if let Some(Value::Instance(i)) = &e.value {
            return Value::Class(i.class.clone());
        }
        match self.builtins.lookup(&e.kind) {
            Some(v) => v,
            None => Value::str_from(e.kind.clone()),
        }
    }

    fn exc_value_of(&self, e: &PyError) -> Value {
        match &e.value {
            Some(v) => v.clone(),
            None => match self.builtins.lookup(&e.kind) {
                Some(Value::Class(c)) => self.instantiate_exception(&c, vec![Value::str_from(e.message.clone())]),
                _ => Value::str_from(e.message.clone()),
            },
        }
    }

    /// 按顺序尝试 except 处理块。
    fn run_handlers(
        &mut self,
        handlers: &[ExceptHandler],
        err: PyError,
        env: &EnvRef,
    ) -> EResult<()> {
        for h in handlers {
            let matched = match &h.types {
                None => true,
                Some(types) => {
                    let mut m = false;
                    for t in types {
                        let tv = self.eval(t, env)?;
                        if self.exception_matches(&err, &tv) {
                            m = true;
                            break;
                        }
                    }
                    m
                }
            };
            if !matched {
                continue;
            }
            self.exc_stack.push(err.clone());
            if let Some(name) = &h.name {
                let v = self.exc_value_of(&err);
                env.assign(name, v);
            }
            let r = self.exec_block(&h.body, env);
            self.exc_stack.pop();
            return r;
        }
        Err(Signal::Error(err))
    }

    pub fn exception_matches(&self, err: &PyError, cls: &Value) -> bool {
        let c = match cls {
            Value::Class(c) => c.clone(),
            Value::Tuple(items) => {
                return items.iter().any(|i| self.exception_matches(err, i));
            }
            _ => return false,
        };
        if let Some(Value::Instance(i)) = &err.value {
            // 携带异常对象时，完全按类的继承关系判断
            return self.class_is_subclass(&i.class, &c);
        }
        if c.name == err.kind {
            return true;
        }
        if c.mro.iter().any(|b| b.name == err.kind) {
            return true;
        }
        exception_bases(&err.kind).iter().any(|b| *b == c.name)
    }

    fn make_raise(&mut self, v: Value) -> EResult<PyError> {
        match v {
            Value::Class(c) => {
                if !self.class_is_exception(&c) {
                    return self.err("TypeError", "exceptions must derive from BaseException");
                }
                let inst = self.instantiate_exception(&c, Vec::new());
                let msg = match &inst {
                    Value::Instance(i) => self.instance_str(i).unwrap_or_default(),
                    _ => String::new(),
                };
                let mut e = self.pyerr(&c.name, msg);
                e.value = Some(inst);
                Ok(e)
            }
            Value::Instance(i) => {
                if !self.class_is_exception(&i.class) {
                    return self.err("TypeError", "exceptions must derive from BaseException");
                }
                let msg = self.instance_str(&i).unwrap_or_default();
                let mut e = self.pyerr(&i.class.name, msg);
                e.value = Some(Value::Instance(i));
                Ok(e)
            }
            other => self.err(
                "TypeError",
                format!("exceptions must derive from BaseException, not {}", other.type_name()),
            ),
        }
    }

    pub fn class_is_exception(&self, c: &Rc<ClassData>) -> bool {
        if c.name == "BaseException" {
            return true;
        }
        c.mro.iter().any(|b| b.name == "BaseException")
    }

    fn instantiate_exception(&self, c: &Rc<ClassData>, args: Vec<Value>) -> Value {
        let inst = Rc::new(InstanceData {
            class: c.clone(),
            dict: RefCell::new(HashMap::new()),
        });
        inst.dict
            .borrow_mut()
            .insert("args".to_string(), Value::tuple(args));
        Value::Instance(inst)
    }

    pub fn instance_str(&self, i: &Rc<InstanceData>) -> Option<String> {
        // 异常对象默认的 str 是空串或参数拼接
        let d = i.dict.borrow();
        if let Some(Value::Tuple(args)) = d.get("args") {
            if args.is_empty() {
                return Some(String::new());
            }
            if args.len() == 1 {
                if i.class.name == "KeyError" {
                    return Some(crate::value::builtin_repr(&args[0]));
                }
                if let Ok(s) = crate::value::format_value(&args[0], "s") {
                    return Some(s);
                }
                return Some(match &args[0] {
                    Value::Str(s) => s.to_string(),
                    Value::Int(n) => n.to_string(),
                    _ => String::new(),
                });
            }
            let parts: Vec<String> = args
                .iter()
                .map(|a| match a {
                    Value::Str(s) => s.to_string(),
                    Value::Int(n) => n.to_string(),
                    Value::Float(f) => float_repr(*f),
                    other => other.type_name(),
                })
                .collect();
            return Some(format!("({})", parts.join(", ")));
        }
        None
    }

    fn apply_decorators(
        &mut self,
        decorators: &[Expr],
        mut value: Value,
        env: &EnvRef,
    ) -> EResult<Value> {
        if decorators.is_empty() {
            return Ok(value);
        }
        let mut fns = Vec::new();
        for d in decorators {
            fns.push(self.eval(d, env)?);
        }
        for f in fns.into_iter().rev() {
            value = self.call_value(&f, vec![value], Vec::new())?;
        }
        Ok(value)
    }

    // ---------------- 函数与类 ----------------

    fn function_closure(&self, env: &EnvRef) -> EnvRef {
        if env.scope_kind() == ScopeKind::Class {
            env.parent.clone().unwrap_or_else(|| env.clone())
        } else {
            env.clone()
        }
    }

    fn make_function(
        &mut self,
        fd: &FuncDef,
        env: &EnvRef,
        name: &str,
        is_lambda: bool,
    ) -> EResult<Value> {
        let mut defaults = Vec::new();
        for d in &fd.params.defaults {
            defaults.push(self.eval(d, env)?);
        }
        let mut kwonly = Vec::new();
        for (n, d) in &fd.params.kwonly {
            let v = match d {
                Some(e) => Some(self.eval(e, env)?),
                None => None,
            };
            kwonly.push((n.clone(), v));
        }
        let owner = if env.scope_kind() == ScopeKind::Class {
            self.defining_class.clone()
        } else {
            None
        };
        Ok(Value::Func(Rc::new(FuncData {
            name: name.to_string(),
            params: RtParams {
                args: fd.params.args.clone(),
                defaults,
                vararg: fd.params.vararg.clone(),
                kwonly,
                kwarg: fd.params.kwarg.clone(),
            },
            body: Rc::new(fd.body.clone()),
            closure: self.function_closure(env),
            is_lambda,
            owner,
        })))
    }

    fn make_class(&mut self, cd: &ClassDef, env: &EnvRef) -> EResult<Value> {
        let mut bases = Vec::new();
        for b in &cd.bases {
            let bv = self.eval(b, env)?;
            match bv {
                Value::Class(c) => {
                    if c.builtin.is_some() {
                        return self.err(
                            "TypeError",
                            format!("暂不支持继承内置类型 '{}'", c.name),
                        );
                    }
                    bases.push(c);
                }
                other => {
                    return self.err(
                        "TypeError",
                        format!("类继承的基类必须是类，而不是 {}", other.type_name()),
                    )
                }
            }
        }
        if bases.is_empty() {
            bases.push(self.object_class());
        }
        let mro = self.linearize(&cd.name, &bases)?;
        let cls = Rc::new(ClassData {
            name: cd.name.clone(),
            module: self.module_name.clone(),
            bases: bases.clone(),
            dict: RefCell::new(HashMap::new()),
            mro: mro.clone(),
            builtin: None,
            is_exception: mro.iter().any(|c| c.name == "BaseException"),
        });
        let cenv = Env::new(Some(env.clone()), ScopeKind::Class);
        let saved_defining = self.defining_class.take();
        self.defining_class = Some(cls.clone());
        let body_result = self.exec_block(&cd.body, &cenv);
        self.defining_class = saved_defining;
        body_result?;
        {
            let vars = cenv.vars.borrow();
            let mut d = cls.dict.borrow_mut();
            for (k, v) in vars.iter() {
                d.insert(k.clone(), v.clone());
            }
        }
        cls.dict
            .borrow_mut()
            .insert("__name__".to_string(), Value::str_from(cd.name.clone()));
        Ok(Value::Class(cls))
    }

    fn object_class(&self) -> Rc<ClassData> {
        self.type_classes
            .get("object")
            .expect("object 类必须存在")
            .clone()
    }

    /// C3 线性化。
    fn linearize(&mut self, name: &str, bases: &[Rc<ClassData>]) -> EResult<Vec<Rc<ClassData>>> {
        let mut seqs: Vec<Vec<Rc<ClassData>>> = Vec::new();
        for b in bases {
            let mut s = vec![b.clone()];
            s.extend(b.mro.iter().cloned());
            seqs.push(s);
        }
        let mut result: Vec<Rc<ClassData>> = Vec::new();
        loop {
            seqs.retain(|s| !s.is_empty());
            if seqs.is_empty() {
                break;
            }
            let mut candidate: Option<Rc<ClassData>> = None;
            for s in &seqs {
                let head = s[0].clone();
                let in_tail = seqs
                    .iter()
                    .any(|o| o.iter().skip(1).any(|c| Rc::ptr_eq(c, &head)));
                if !in_tail {
                    candidate = Some(head);
                    break;
                }
            }
            let cand = match candidate {
                Some(c) => c,
                None => {
                    return self.err(
                        "TypeError",
                        format!("类 {} 的继承关系无法线性化（MRO 冲突）", name),
                    )
                }
            };
            result.push(cand.clone());
            for s in seqs.iter_mut() {
                if !s.is_empty() && Rc::ptr_eq(&s[0], &cand) {
                    s.remove(0);
                }
            }
        }
        Ok(result)
    }

    // ---------------- 表达式求值 ----------------

    pub fn eval(&mut self, e: &Expr, env: &EnvRef) -> EResult<Value> {
        match &e.kind {
            ExprKind::Int(i) => Ok(Value::Int(*i)),
            ExprKind::Float(f) => Ok(Value::Float(*f)),
            ExprKind::Str(s) => Ok(Value::str_from(s.clone())),
            ExprKind::Bool(b) => Ok(Value::Bool(*b)),
            ExprKind::None_ => Ok(Value::None),
            ExprKind::Name(n) => self.lookup_name(n, env),
            ExprKind::FStr(pieces) => {
                let mut out = String::new();
                for p in pieces {
                    match p {
                        FStrPiece::Lit(s) => out.push_str(s),
                        FStrPiece::Value { expr, conv, spec } => {
                            let v = self.eval(expr, env)?;
                            let spec_text = if spec.is_empty() {
                                String::new()
                            } else {
                                let mut s = String::new();
                                for p in spec {
                                    match p {
                                        SpecPart::Lit(t) => s.push_str(t),
                                        SpecPart::Field(fe) => {
                                            let fv = self.eval(fe, env)?;
                                            let t = self.value_str(&fv)?;
                                            s.push_str(&t);
                                        }
                                    }
                                }
                                s
                            };
                            let s = match conv {
                                Some('r') => self.value_repr(&v)?,
                                Some('s') | None => {
                                    if spec_text.is_empty() {
                                        self.value_str(&v)?
                                    } else {
                                        self.format_with_spec(&v, &spec_text)?
                                    }
                                }
                                Some('a') => self.value_repr(&v)?,
                                Some(c) => {
                                    return self
                                        .err("ValueError", format!("未知的转换符 !{}", c))
                                }
                            };
                            out.push_str(&s);
                        }
                    }
                }
                Ok(Value::str_from(out))
            }
            ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
                let mut vals = Vec::new();
                for it in items {
                    match &it.kind {
                        ExprKind::Starred(inner) => {
                            let v = self.eval(inner, env)?;
                            vals.extend(self.collect_iter(&v)?);
                        }
                        _ => vals.push(self.eval(it, env)?),
                    }
                }
                Ok(match &e.kind {
                    ExprKind::List(_) => Value::list(vals),
                    ExprKind::Tuple(_) => Value::tuple(vals),
                    _ => {
                        let mut s = SetData::new();
                        for v in vals {
                            if !is_hashable(&v) {
                                return self.err(
                                    "TypeError",
                                    format!("unhashable type: '{}'", v.type_name()),
                                );
                            }
                            s.insert(v);
                        }
                        Value::Set(Rc::new(RefCell::new(s)))
                    }
                })
            }
            ExprKind::Dict(entries) => {
                let mut d = DictData::new();
                for (k, v) in entries {
                    match k {
                        Some(ke) => {
                            let kv = self.eval(ke, env)?;
                            if !is_hashable(&kv) {
                                return self.err(
                                    "TypeError",
                                    format!("unhashable type: '{}'", kv.type_name()),
                                );
                            }
                            let vv = self.eval(v, env)?;
                            d.insert(kv, vv);
                        }
                        None => {
                            let other = self.eval(v, env)?;
                            for (k2, v2) in self.dict_items(&other)? {
                                d.insert(k2, v2);
                            }
                        }
                    }
                }
                Ok(Value::Dict(Rc::new(RefCell::new(d))))
            }
            ExprKind::Starred(_) => self.err("SyntaxError", "此处不能使用 * 展开"),
            ExprKind::BinOp { left, op, right } => {
                let a = self.eval(left, env)?;
                let b = self.eval(right, env)?;
                self.binop(*op, a, b)
            }
            ExprKind::UnaryOp { op, operand } => {
                let v = self.eval(operand, env)?;
                self.unop(*op, v)
            }
            ExprKind::BoolOp { op, values } => {
                let mut last = Value::None;
                for (i, v) in values.iter().enumerate() {
                    last = self.eval(v, env)?;
                    let t = self.truthy(&last)?;
                    let stop = match op {
                        BoolOpKind::And => !t,
                        BoolOpKind::Or => t,
                    };
                    if stop {
                        return Ok(last);
                    }
                    if i + 1 == values.len() {
                        return Ok(last);
                    }
                }
                Ok(last)
            }
            ExprKind::Compare {
                left,
                ops,
                comparators,
            } => {
                let mut cur = self.eval(left, env)?;
                for (op, c) in ops.iter().zip(comparators.iter()) {
                    let rhs = self.eval(c, env)?;
                    if !self.compare(*op, &cur, &rhs)? {
                        return Ok(Value::Bool(false));
                    }
                    cur = rhs;
                }
                Ok(Value::Bool(true))
            }
            ExprKind::IfExp { test, body, orelse } => {
                let c = self.eval(test, env)?;
                if self.truthy(&c)? {
                    self.eval(body, env)
                } else {
                    self.eval(orelse, env)
                }
            }
            ExprKind::Lambda(ld) => {
                let params = ld.params.clone();
                let mut defaults = Vec::new();
                for d in &params.defaults {
                    defaults.push(self.eval(d, env)?);
                }
                let mut kwonly = Vec::new();
                for (n, d) in &params.kwonly {
                    let v = match d {
                        Some(x) => Some(self.eval(x, env)?),
                        None => None,
                    };
                    kwonly.push((n.clone(), v));
                }
                let body_stmts = vec![Stmt::new(
                    StmtKind::Return(Some((*ld.body).clone())),
                    e.line,
                )];
                let owner = if env.scope_kind() == ScopeKind::Class {
                    self.defining_class.clone()
                } else {
                    None
                };
                Ok(Value::Func(Rc::new(FuncData {
                    name: "<lambda>".to_string(),
                    params: RtParams {
                        args: params.args.clone(),
                        defaults,
                        vararg: params.vararg.clone(),
                        kwonly,
                        kwarg: params.kwarg.clone(),
                    },
                    body: Rc::new(body_stmts),
                    closure: self.function_closure(env),
                    is_lambda: true,
                    owner,
                })))
            }
            ExprKind::Call { func, args } => {
                let f = self.eval(func, env)?;
                let (pos, kw) = self.eval_call_args(args, env)?;
                self.call_value(&f, pos, kw)
            }
            ExprKind::Attr { obj, name } => {
                let o = self.eval(obj, env)?;
                self.get_attr(&o, name)
            }
            ExprKind::Index { obj, index } => {
                let o = self.eval(obj, env)?;
                let i = self.eval(index, env)?;
                self.get_index(&o, &i)
            }
            ExprKind::Slice {
                lower,
                upper,
                step,
            } => {
                let lo = match lower {
                    Some(e) => Some(self.eval_int(e, env)?),
                    None => None,
                };
                let hi = match upper {
                    Some(e) => Some(self.eval_int(e, env)?),
                    None => None,
                };
                let st = match step {
                    Some(e) => Some(self.eval_int(e, env)?),
                    None => None,
                };
                Ok(Value::Slice(Rc::new(SliceData {
                    lower: lo,
                    upper: hi,
                    step: st,
                })))
            }
            ExprKind::ListComp { elt, generators } => {
                let mut out: Vec<Value> = Vec::new();
                {
                    let cenv = Env::new(Some(env.clone()), ScopeKind::Comprehension);
                    self.run_generators(generators, &cenv, &mut |it, e| {
                        let v = it.eval(elt, e)?;
                        out.push(v);
                        Ok(())
                    })?;
                }
                Ok(Value::list(out))
            }
            ExprKind::SetComp { elt, generators } => {
                let mut out = SetData::new();
                {
                    let cenv = Env::new(Some(env.clone()), ScopeKind::Comprehension);
                    self.run_generators(generators, &cenv, &mut |it, e| {
                        let v = it.eval(elt, e)?;
                        if !is_hashable(&v) {
                            return it.err(
                                "TypeError",
                                format!("unhashable type: '{}'", v.type_name()),
                            );
                        }
                        out.insert(v);
                        Ok(())
                    })?;
                }
                Ok(Value::Set(Rc::new(RefCell::new(out))))
            }
            ExprKind::DictComp {
                key,
                value,
                generators,
            } => {
                let mut out = DictData::new();
                {
                    let cenv = Env::new(Some(env.clone()), ScopeKind::Comprehension);
                    self.run_generators(generators, &cenv, &mut |it, e| {
                        let k = it.eval(key, e)?;
                        let v = it.eval(value, e)?;
                        if !is_hashable(&k) {
                            return it.err(
                                "TypeError",
                                format!("unhashable type: '{}'", k.type_name()),
                            );
                        }
                        out.insert(k, v);
                        Ok(())
                    })?;
                }
                Ok(Value::Dict(Rc::new(RefCell::new(out))))
            }
            ExprKind::GenExp { elt, generators } => {
                // 简化实现：立即求值成列表迭代器（不惰性）
                let mut out: Vec<Value> = Vec::new();
                {
                    let cenv = Env::new(Some(env.clone()), ScopeKind::Comprehension);
                    self.run_generators(generators, &cenv, &mut |it, e| {
                        let v = it.eval(elt, e)?;
                        out.push(v);
                        Ok(())
                    })?;
                }
                let rc = Rc::new(RefCell::new(out));
                Ok(Value::Iterator(Rc::new(RefCell::new(IterKind::List(rc, 0)))))
            }
        }
    }

    fn eval_int(&mut self, e: &Expr, env: &EnvRef) -> EResult<i64> {
        let v = self.eval(e, env)?;
        match v {
            Value::Int(i) => Ok(i),
            Value::Bool(b) => Ok(b as i64),
            other => self.err(
                "TypeError",
                format!("切片下标必须是整数，而不是 {}", other.type_name()),
            ),
        }
    }

    fn eval_call_args(&mut self, args: &[Arg], env: &EnvRef) -> EResult<CallArgs> {
        let mut pos = Vec::new();
        let mut kw: Vec<(String, Value)> = Vec::new();
        for a in args {
            match a {
                Arg::Pos(e) => pos.push(self.eval(e, env)?),
                Arg::Kw(n, e) => {
                    let v = self.eval(e, env)?;
                    if kw.iter().any(|(k, _)| k == n) {
                        return self.err(
                            "TypeError",
                            format!("重复的关键字参数 '{}'", n),
                        );
                    }
                    kw.push((n.clone(), v));
                }
                Arg::Star(e) => {
                    let v = self.eval(e, env)?;
                    pos.extend(self.collect_iter(&v)?);
                }
                Arg::DoubleStar(e) => {
                    let v = self.eval(e, env)?;
                    for (k, val) in self.dict_items(&v)? {
                        let key = match &k {
                            Value::Str(s) => s.to_string(),
                            other => {
                                return self.err(
                                    "TypeError",
                                    format!(
                                        "关键字参数名必须是字符串，而不是 {}",
                                        other.type_name()
                                    ),
                                )
                            }
                        };
                        kw.push((key, val));
                    }
                }
            }
        }
        Ok((pos, kw))
    }

    fn run_generators<F>(
        &mut self,
        gens: &[CompFor],
        env: &EnvRef,
        f: &mut F,
    ) -> EResult<()>
    where
        F: FnMut(&mut Interp, &EnvRef) -> EResult<()>,
    {
        if gens.is_empty() {
            return f(self, env);
        }
        let g = &gens[0];
        let iterable = self.eval(&g.iter, env)?;
        let it = self.get_iter(&iterable)?;
        loop {
            let item = match self.iter_next(&it)? {
                Some(v) => v,
                None => break,
            };
            self.assign_target(&g.target, item, env)?;
            let mut ok = true;
            for cond in &g.ifs {
                let c = self.eval(cond, env)?;
                if !self.truthy(&c)? {
                    ok = false;
                    break;
                }
            }
            if !ok {
                continue;
            }
            self.run_generators(&gens[1..], env, f)?;
        }
        Ok(())
    }

    pub fn lookup_name(&mut self, name: &str, env: &EnvRef) -> EResult<Value> {
        if let Some(v) = env.lookup(name) {
            return Ok(v);
        }
        if let Some(v) = self.builtins.lookup(name) {
            return Ok(v);
        }
        self.err("NameError", format!("name '{}' is not defined", name))
    }

    // ---------------- 赋值 ----------------

    pub fn assign_target(&mut self, target: &Expr, value: Value, env: &EnvRef) -> EResult<()> {
        match &target.kind {
            ExprKind::Name(n) => {
                env.assign(n, value);
                Ok(())
            }
            ExprKind::Attr { obj, name } => {
                let o = self.eval(obj, env)?;
                self.set_attr(&o, name, value)
            }
            ExprKind::Index { obj, index } => {
                let o = self.eval(obj, env)?;
                let i = self.eval(index, env)?;
                self.set_index(&o, &i, value)
            }
            ExprKind::Tuple(items) | ExprKind::List(items) => {
                let vals = self.collect_iter(&value)?;
                let star_pos = items
                    .iter()
                    .position(|t| matches!(t.kind, ExprKind::Starred(_)));
                match star_pos {
                    None => {
                        if vals.len() != items.len() {
                            let (what, verb) = if vals.len() < items.len() {
                                ("not enough", "expected")
                            } else {
                                ("too many", "expected")
                            };
                            return self.err(
                                "ValueError",
                                format!(
                                    "{} values to unpack ({} {}, got {})",
                                    what,
                                    verb,
                                    items.len(),
                                    vals.len()
                                ),
                            );
                        }
                        for (t, v) in items.iter().zip(vals) {
                            self.assign_target(t, v, env)?;
                        }
                    }
                    Some(sp) => {
                        let before = &items[..sp];
                        let after = &items[sp + 1..];
                        if vals.len() < before.len() + after.len() {
                            return self.err(
                                "ValueError",
                                format!(
                                    "not enough values to unpack (expected at least {}, got {})",
                                    before.len() + after.len(),
                                    vals.len()
                                ),
                            );
                        }
                        let n_after = after.len();
                        for (t, v) in before.iter().zip(vals.iter().take(before.len())) {
                            self.assign_target(t, v.clone(), env)?;
                        }
                        let mid: Vec<Value> = vals[before.len()..vals.len() - n_after].to_vec();
                        if let ExprKind::Starred(inner) = &items[sp].kind {
                            self.assign_target(inner, Value::list(mid), env)?;
                        }
                        for (t, v) in after.iter().zip(vals.iter().skip(vals.len() - n_after)) {
                            self.assign_target(t, v.clone(), env)?;
                        }
                    }
                }
                Ok(())
            }
            ExprKind::Starred(inner) => self.assign_target(inner, value, env),
            _ => self.err("SyntaxError", "非法的赋值目标"),
        }
    }

    // ---------------- 调用 ----------------

    pub fn call_value(
        &mut self,
        f: &Value,
        args: Vec<Value>,
        kwargs: Vec<(String, Value)>,
    ) -> EResult<Value> {
        match f {
            Value::Func(fd) => self.call_function(fd, args, kwargs),
            Value::Native(nd) => (nd.func)(self, &args, &kwargs),
            Value::BoundNative(bn) => {
                let mut a = vec![bn.recv.clone()];
                a.extend(args);
                (bn.func.func)(self, &a, &kwargs)
            }
            Value::BoundMethod(bm) => {
                let mut a = vec![bm.recv.clone()];
                a.extend(args);
                self.call_value(&bm.func, a, kwargs)
            }
            Value::Class(c) => self.call_class(c, args, kwargs),
            Value::Instance(i) => match self.find_class_attr(&i.class, "__call__") {
                Some(m) => {
                    let mut a = vec![Value::Instance(i.clone())];
                    a.extend(args);
                    self.call_value(&m, a, kwargs)
                }
                None => self.err(
                    "TypeError",
                    format!("'{}' object is not callable", i.class.name),
                ),
            },
            other => self.err(
                "TypeError",
                format!("'{}' object is not callable", other.type_name()),
            ),
        }
    }

    fn call_function(
        &mut self,
        fd: &Rc<FuncData>,
        args: Vec<Value>,
        kwargs: Vec<(String, Value)>,
    ) -> EResult<Value> {
        if self.depth >= self.max_depth || self.stack_used() > self.stack_budget {
            return self.err("RecursionError", "maximum recursion depth exceeded");
        }
        let env = Env::new(Some(fd.closure.clone()), ScopeKind::Function);
        // 方法调用时记录 (所属类, self)，供零参 super() 使用
        let ctx_recv = match &fd.owner {
            Some(_) => args.first().cloned(),
            None => None,
        };
        if let (Some(owner), Some(recv)) = (&fd.owner, &ctx_recv) {
            self.class_ctx.push((owner.clone(), recv.clone()));
        }
        self.bind_params(fd, &env, args, kwargs)?;
        self.depth += 1;
        self.frames.push(TraceFrame {
            file: self.filename.clone(),
            line: 0,
            func: fd.name.clone(),
        });
        let r = self.exec_block(&fd.body, &env);
        self.frames.pop();
        self.depth -= 1;
        if fd.owner.is_some() {
            self.class_ctx.pop();
        }
        match r {
            Ok(()) => Ok(Value::None),
            Err(Signal::Return(v)) => Ok(v),
            Err(e) => Err(e),
        }
    }

    fn bind_params(
        &mut self,
        fd: &Rc<FuncData>,
        env: &EnvRef,
        args: Vec<Value>,
        kwargs: Vec<(String, Value)>,
    ) -> EResult<()> {
        let p = &fd.params;
        let n_named = p.args.len();
        let mut filled: Vec<Option<Value>> = vec![None; n_named];
        let mut extra: Vec<Value> = Vec::new();
        for (i, a) in args.into_iter().enumerate() {
            if i < n_named {
                filled[i] = Some(a);
            } else if p.vararg.is_some() {
                extra.push(a);
            } else {
                let given = i + 1;
                return self.err(
                    "TypeError",
                    format!(
                        "{}() takes {} positional argument{} but {} were given",
                        fd.name,
                        n_named,
                        if n_named == 1 { "" } else { "s" },
                        given
                    ),
                );
            }
        }
        let mut kw_map: HashMap<String, Value> = HashMap::new();
        for (k, v) in kwargs {
            if let Some(pos) = p.args.iter().position(|n| *n == k) {
                if filled[pos].is_some() {
                    return self.err(
                        "TypeError",
                        format!("{}() got multiple values for argument '{}'", fd.name, k),
                    );
                }
                filled[pos] = Some(v);
            } else if p.kwonly.iter().any(|(n, _)| *n == k) || p.kwarg.is_some() {
                kw_map.insert(k, v);
            } else {
                return self.err(
                    "TypeError",
                    format!("{}() got an unexpected keyword argument '{}'", fd.name, k),
                );
            }
        }
        // 普通位置参数
        for (i, name) in p.args.iter().enumerate() {
            match &filled[i] {
                Some(v) => env.define(name.clone(), v.clone()),
                None => {
                    let n_def = p.defaults.len();
                    let first_default = n_named - n_def;
                    if i >= first_default {
                        env.define(name.clone(), p.defaults[i - first_default].clone());
                    } else {
                        return self.err(
                            "TypeError",
                            format!(
                                "{}() missing 1 required positional argument: '{}'",
                                fd.name, name
                            ),
                        );
                    }
                }
            }
        }
        if let Some(va) = &p.vararg {
            env.define(va.clone(), Value::tuple(extra));
        }
        for (name, def) in &p.kwonly {
            match kw_map.remove(name) {
                Some(v) => env.define(name.clone(), v),
                None => match def {
                    Some(d) => env.define(name.clone(), d.clone()),
                    None => {
                        return self.err(
                            "TypeError",
                            format!(
                                "{}() missing 1 required keyword-only argument: '{}'",
                                fd.name, name
                            ),
                        )
                    }
                },
            }
        }
        if let Some(kw) = &p.kwarg {
            let mut d = DictData::new();
            for (k, v) in kw_map.iter() {
                d.insert(Value::str_from(k.clone()), v.clone());
            }
            env.define(kw.clone(), Value::Dict(Rc::new(RefCell::new(d))));
        }
        Ok(())
    }

    fn call_class(
        &mut self,
        c: &Rc<ClassData>,
        args: Vec<Value>,
        kwargs: Vec<(String, Value)>,
    ) -> EResult<Value> {
        if let Some(b) = &c.builtin {
            return self.call_builtin_type(b, args, kwargs);
        }
        let inst = Rc::new(InstanceData {
            class: c.clone(),
            dict: RefCell::new(HashMap::new()),
        });
        let v = Value::Instance(inst.clone());
        if let Some(init) = self.find_class_attr(c, "__init__") {
            let mut a = vec![v.clone()];
            a.extend(args);
            let r = self.call_value(&init, a, kwargs)?;
            if !r.is_none() {
                return self.err("TypeError", "__init__() should return None");
            }
        } else if !args.is_empty() || !kwargs.is_empty() {
            // 异常类可以带参数
            if self.class_is_exception(c) {
                let mut d = inst.dict.borrow_mut();
                d.insert("args".to_string(), Value::tuple(args));
                return Ok(v);
            }
            return self.err(
                "TypeError",
                format!("{}() takes no arguments", c.name),
            );
        } else if self.class_is_exception(c) {
            inst.dict
                .borrow_mut()
                .insert("args".to_string(), Value::tuple(Vec::new()));
        }
        Ok(v)
    }

    // ---------------- 迭代 ----------------

    pub fn get_iter(&mut self, v: &Value) -> EResult<Value> {
        Ok(match v {
            Value::List(l) => Value::Iterator(Rc::new(RefCell::new(IterKind::List(l.clone(), 0)))),
            Value::Tuple(t) => {
                Value::Iterator(Rc::new(RefCell::new(IterKind::Tuple(t.clone(), 0))))
            }
            Value::Str(s) => Value::Iterator(Rc::new(RefCell::new(IterKind::Str(
                s.chars().collect(),
                0,
            )))),
            Value::Range(r) => Value::Iterator(Rc::new(RefCell::new(IterKind::Range {
                cur: r.start,
                stop: r.stop,
                step: r.step,
            }))),
            Value::Dict(d) => {
                let keys: Vec<Value> = d.borrow().entries.iter().map(|(k, _)| k.clone()).collect();
                Value::Iterator(Rc::new(RefCell::new(IterKind::Items(keys, 0))))
            }
            Value::Set(s) => {
                let items = s.borrow().items.clone();
                Value::Iterator(Rc::new(RefCell::new(IterKind::Items(items, 0))))
            }
            Value::Iterator(_) => v.clone(),
            Value::File(_) => {
                let next = self.get_attr(v, "__next__")?;
                Value::Iterator(Rc::new(RefCell::new(IterKind::Object {
                    obj: v.clone(),
                    next,
                })))
            }
            Value::Instance(i) => {
                let f = self.find_class_attr(&i.class, "__iter__");
                match f {
                    Some(m) => {
                        let it = self.call_value(&m, vec![v.clone()], Vec::new())?;
                        match &it {
                            Value::Iterator(_) => it,
                            other => {
                                if self.find_class_attr_of_value(other, "__next__").is_some() {
                                    let next = self.get_attr(other, "__next__")?;
                                    Value::Iterator(Rc::new(RefCell::new(IterKind::Object {
                                        obj: other.clone(),
                                        next,
                                    })))
                                } else {
                                    it
                                }
                            }
                        }
                    }
                    None => {
                        return self.err(
                            "TypeError",
                            format!("'{}' object is not iterable", i.class.name),
                        )
                    }
                }
            }
            other => {
                return self.err(
                    "TypeError",
                    format!("'{}' object is not iterable", other.type_name()),
                )
            }
        })
    }

    fn find_class_attr_of_value(&mut self, v: &Value, name: &str) -> Option<Value> {
        match v {
            Value::Instance(i) => self.find_class_attr(&i.class, name),
            _ => None,
        }
    }

    pub fn iter_next(&mut self, it: &Value) -> EResult<Option<Value>> {
        let cell = match it {
            Value::Iterator(c) => c.clone(),
            other => {
                return self.err(
                    "TypeError",
                    format!("'{}' object is not an iterator", other.type_name()),
                )
            }
        };
        // 先取一步（避免在调用用户代码时持有借用）
        enum Step {
            Item(Value),
            Done,
            CallObject(Value),
        }
        let step = {
            let mut c = cell.borrow_mut();
            match &mut *c {
                IterKind::List(l, i) => {
                    let v = l.borrow().get(*i).cloned();
                    match v {
                        Some(v) => {
                            *i += 1;
                            Step::Item(v)
                        }
                        None => Step::Done,
                    }
                }
                IterKind::Tuple(t, i) => {
                    let v = t.get(*i).cloned();
                    match v {
                        Some(v) => {
                            *i += 1;
                            Step::Item(v)
                        }
                        None => Step::Done,
                    }
                }
                IterKind::Str(chars, i) => {
                    let v = chars.get(*i).copied();
                    match v {
                        Some(ch) => {
                            *i += 1;
                            Step::Item(Value::str_from(ch.to_string()))
                        }
                        None => Step::Done,
                    }
                }
                IterKind::Range { cur, stop, step } => {
                    let done = if *step > 0 { *cur >= *stop } else { *cur <= *stop };
                    if done {
                        Step::Done
                    } else {
                        let v = Value::Int(*cur);
                        *cur += *step;
                        Step::Item(v)
                    }
                }
                IterKind::Items(items, i) => {
                    let v = items.get(*i).cloned();
                    match v {
                        Some(v) => {
                            *i += 1;
                            Step::Item(v)
                        }
                        None => Step::Done,
                    }
                }
                IterKind::Object { next, .. } => Step::CallObject(next.clone()),
            }
        };
        match step {
            Step::Item(v) => Ok(Some(v)),
            Step::Done => Ok(None),
            Step::CallObject(next) => match self.call_value(&next, Vec::new(), Vec::new()) {
                Ok(v) => Ok(Some(v)),
                Err(Signal::Error(e)) => {
                    if e.kind == "StopIteration" || self.exception_matches(&e, &self.stop_iteration_class())
                    {
                        Ok(None)
                    } else {
                        Err(Signal::Error(e))
                    }
                }
                Err(sig) => Err(sig),
            },
        }
    }

    fn stop_iteration_class(&self) -> Value {
        self.builtins
            .lookup("StopIteration")
            .unwrap_or(Value::None)
    }

    pub fn collect_iter(&mut self, v: &Value) -> EResult<Vec<Value>> {
        let it = self.get_iter(v)?;
        let mut out = Vec::new();
        while let Some(x) = self.iter_next(&it)? {
            out.push(x);
        }
        Ok(out)
    }

    pub fn dict_items(&mut self, v: &Value) -> EResult<Vec<(Value, Value)>> {
        match v {
            Value::Dict(d) => Ok(d.borrow().entries.clone()),
            Value::Instance(i) => match self.find_class_attr(&i.class, "items") {
                Some(_) => {
                    let it = self.collect_iter(v)?;
                    let mut out = Vec::new();
                    for pair in it {
                        let ps = self.collect_iter(&pair)?;
                        if ps.len() != 2 {
                            return self.err(
                                "ValueError",
                                "dict update sequence element has length != 2",
                            );
                        }
                        out.push((ps[0].clone(), ps[1].clone()));
                    }
                    Ok(out)
                }
                None => self.err(
                    "TypeError",
                    format!("'{}' object is not a mapping", i.class.name),
                ),
            },
            other => self.err(
                "TypeError",
                format!("'{}' object is not a mapping", other.type_name()),
            ),
        }
    }
}
