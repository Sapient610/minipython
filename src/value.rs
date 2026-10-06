//! 运行时值模型。
//!
//! 这里只放与解释器无关的、纯粹的数据结构和基础运算；
//! 需要调用 `Interp` 的部分（如 `__str__` 分派）放在 `interp.rs`。

use crate::ast::Stmt;
use crate::env::Env;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::rc::Rc;

pub type EnvRef = Rc<Env>;

/// 运行时的函数参数描述（默认值已在定义时求值）。
#[derive(Debug, Clone, Default)]
pub struct RtParams {
    pub args: Vec<String>,
    /// 与 `args` 末尾对齐
    pub defaults: Vec<Value>,
    pub vararg: Option<String>,
    pub kwonly: Vec<(String, Option<Value>)>,
    pub kwarg: Option<String>,
}

pub struct FuncData {
    pub name: String,
    pub params: RtParams,
    pub body: Rc<Vec<Stmt>>,
    /// 定义处的环境（实现闭包）
    pub closure: EnvRef,
    pub is_lambda: bool,
    /// 若该函数定义在类体中，记录所属类（零参 super() 需要）
    pub owner: Option<Rc<ClassData>>,
}

impl fmt::Debug for FuncData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<function {}>", self.name)
    }
}

/// 内置函数签名：`(解释器, 实参, 关键字实参)`。
pub type NativeFn = fn(
    &mut crate::interp::Interp,
    &[Value],
    &[(String, Value)],
) -> Result<Value, crate::interp::Signal>;

pub struct NativeData {
    pub name: String,
    pub func: NativeFn,
}

impl fmt::Debug for NativeData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<built-in function {}>", self.name)
    }
}

pub struct BoundNativeData {
    pub recv: Value,
    pub func: Rc<NativeData>,
}

impl fmt::Debug for BoundNativeData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<built-in method {}>", self.func.name)
    }
}

pub struct BoundMethodData {
    pub func: Value,
    pub recv: Value,
    /// 方法所属的类（用于零参 super()）
    pub owner: Option<Rc<ClassData>>,
}

impl fmt::Debug for BoundMethodData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<bound method>")
    }
}

pub struct ClassData {
    pub name: String,
    /// 定义该类的模块名，用于 repr
    pub module: String,
    pub bases: Vec<Rc<ClassData>>,
    pub dict: RefCell<HashMap<String, Value>>,
    /// 方法解析顺序（C3 线性化），含自身
    pub mro: Vec<Rc<ClassData>>,
    /// 是否为异常类
    pub is_exception: bool,
    /// 若是内置类型（int/str/list...），记录其类型名，用于 isinstance / 构造
    pub builtin: Option<String>,
}

impl fmt::Debug for ClassData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<class '{}'>", self.name)
    }
}

pub struct InstanceData {
    pub class: Rc<ClassData>,
    /// 实例命名空间。
    ///
    /// 用 `DictData` 而不是 `HashMap` 是为了让 `obj.__dict__` 成为**活动视图**：
    /// `self.__dict__[k] = v` 会真正写回实例（`__setattr__` / `__getattr__` 里常见）。
    pub dict: Rc<RefCell<DictData>>,
}

impl InstanceData {
    pub fn new(class: Rc<ClassData>) -> InstanceData {
        InstanceData {
            class,
            dict: Rc::new(RefCell::new(DictData::new())),
        }
    }

    /// 按属性名读取（等价于 `obj.__dict__[name]`）。
    pub fn get(&self, name: &str) -> Option<Value> {
        self.dict.borrow().get(&Value::Str(Rc::from(name)))
    }

    /// 直接写入实例字典（绕过 `__setattr__`，即 `object.__setattr__` 的语义）。
    pub fn set(&self, name: &str, value: Value) {
        self.dict
            .borrow_mut()
            .insert(Value::Str(Rc::from(name)), value);
    }

    pub fn remove(&self, name: &str) -> Option<Value> {
        self.dict.borrow_mut().remove(&Value::Str(Rc::from(name)))
    }

    pub fn contains(&self, name: &str) -> bool {
        self.dict.borrow().contains(&Value::Str(Rc::from(name)))
    }

    /// 实例属性名（保持插入顺序）。
    pub fn names(&self) -> Vec<String> {
        self.dict
            .borrow()
            .entries
            .iter()
            .filter_map(|(k, _)| match k {
                Value::Str(s) => Some(s.to_string()),
                _ => None,
            })
            .collect()
    }
}

impl fmt::Debug for InstanceData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<{} object>", self.class.name)
    }
}

pub struct ModuleData {
    pub name: String,
    pub dict: RefCell<HashMap<String, Value>>,
}

impl fmt::Debug for ModuleData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<module '{}'>", self.name)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SliceData {
    pub lower: Option<i64>,
    pub upper: Option<i64>,
    pub step: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RangeData {
    pub start: i64,
    pub stop: i64,
    pub step: i64,
}

impl RangeData {
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn len(&self) -> i64 {
        if self.step > 0 {
            if self.stop > self.start {
                (self.stop - self.start + self.step - 1) / self.step
            } else {
                0
            }
        } else if self.stop < self.start {
            (self.start - self.stop - self.step - 1) / (-self.step)
        } else {
            0
        }
    }

    pub fn contains(&self, v: i64) -> bool {
        if self.step > 0 {
            v >= self.start && v < self.stop && (v - self.start) % self.step == 0
        } else if self.step < 0 {
            v <= self.start && v > self.stop && (self.start - v) % (-self.step) == 0
        } else {
            false
        }
    }
}

#[derive(Debug)]
pub enum FileBuf {
    Read(BufReader<File>),
    Write(BufWriter<File>),
}

#[derive(Debug)]
pub struct FileData {
    pub name: String,
    pub mode: String,
    pub buf: FileBuf,
    pub closed: bool,
}

/// 迭代器状态。
#[derive(Debug)]
pub enum IterKind {
    List(Rc<RefCell<Vec<Value>>>, usize),
    Tuple(Rc<Vec<Value>>, usize),
    Str(Vec<char>, usize),
    Range {
        cur: i64,
        stop: i64,
        step: i64,
    },
    /// 快照式迭代（字典的键、集合元素等）
    Items(Vec<Value>, usize),
    /// 用户自定义对象的迭代器：持有 `__next__` 绑定方法
    Object {
        obj: Value,
        next: Value,
    },
    /// 旧式序列协议：只有 `__getitem__` 时从 0 开始逐个取，直到 IndexError
    GetItem {
        obj: Value,
        index: i64,
        method: Value,
    },
}

/// 属性描述符（staticmethod / classmethod / property）。
pub struct DescriptorData {
    pub kind: DescriptorKind,
    pub func: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DescriptorKind {
    Static,
    Class,
}

pub struct PropertyData {
    pub name: String,
    pub getter: Value,
    pub setter: Option<Value>,
    pub deleter: Option<Value>,
}

impl fmt::Debug for PropertyData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<property '{}'>", self.name)
    }
}

/// 有序字典：保持插入顺序（与 CPython 一致），查找为线性扫描。
#[derive(Debug, Default, Clone)]
pub struct DictData {
    pub entries: Vec<(Value, Value)>,
}

impl DictData {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn new() -> Self {
        DictData {
            entries: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn get(&self, key: &Value) -> Option<Value> {
        self.entries
            .iter()
            .find(|(k, _)| key_equal(k, key))
            .map(|(_, v)| v.clone())
    }

    pub fn get_mut(&mut self, key: &Value) -> Option<&mut Value> {
        self.entries
            .iter_mut()
            .find(|(k, _)| key_equal(k, key))
            .map(|(_, v)| v)
    }

    pub fn contains(&self, key: &Value) -> bool {
        self.entries.iter().any(|(k, _)| key_equal(k, key))
    }

    pub fn insert(&mut self, key: Value, value: Value) {
        match self.entries.iter_mut().find(|(k, _)| key_equal(k, &key)) {
            Some(slot) => slot.1 = value,
            None => self.entries.push((key, value)),
        }
    }

    pub fn remove(&mut self, key: &Value) -> Option<Value> {
        match self.entries.iter().position(|(k, _)| key_equal(k, key)) {
            Some(i) => Some(self.entries.remove(i).1),
            None => None,
        }
    }
}

/// 集合：有序存储（便于稳定输出）。
#[derive(Debug, Default, Clone)]
pub struct SetData {
    pub items: Vec<Value>,
}

impl SetData {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn new() -> Self {
        SetData { items: Vec::new() }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn contains(&self, key: &Value) -> bool {
        self.items.iter().any(|k| key_equal(k, key))
    }

    pub fn insert(&mut self, key: Value) -> bool {
        if self.contains(&key) {
            false
        } else {
            self.items.push(key);
            true
        }
    }

    pub fn remove(&mut self, key: &Value) -> Option<Value> {
        match self.items.iter().position(|k| key_equal(k, key)) {
            Some(i) => Some(self.items.remove(i)),
            None => None,
        }
    }
}

#[derive(Clone)]
pub enum Value {
    None,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(Rc<str>),
    List(Rc<RefCell<Vec<Value>>>),
    Tuple(Rc<Vec<Value>>),
    Dict(Rc<RefCell<DictData>>),
    Set(Rc<RefCell<SetData>>),
    Range(Rc<RangeData>),
    Func(Rc<FuncData>),
    Native(Rc<NativeData>),
    BoundNative(Rc<BoundNativeData>),
    BoundMethod(Rc<BoundMethodData>),
    Class(Rc<ClassData>),
    Instance(Rc<InstanceData>),
    Module(Rc<ModuleData>),
    Slice(Rc<SliceData>),
    Iterator(Rc<RefCell<IterKind>>),
    File(Rc<RefCell<FileData>>),
    Descriptor(Rc<DescriptorData>),
    Property(Rc<PropertyData>),
    /// `super()` 返回的代理对象
    Super(Rc<SuperData>),
}

/// `super(C, obj)`：从 C 的 MRO 之后开始查找属性。
pub struct SuperData {
    pub class: Rc<ClassData>,
    pub obj: Value,
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::None => write!(f, "None"),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Int(i) => write!(f, "{}", i),
            Value::Float(x) => write!(f, "{}", float_repr(*x)),
            Value::Str(s) => write!(f, "{}", str_repr(s)),
            other => write!(f, "<{}>", other.type_name()),
        }
    }
}

impl Value {
    pub fn str_from(s: impl Into<String>) -> Value {
        Value::Str(Rc::from(s.into().as_str()))
    }

    pub fn list(items: Vec<Value>) -> Value {
        Value::List(Rc::new(RefCell::new(items)))
    }

    pub fn tuple(items: Vec<Value>) -> Value {
        Value::Tuple(Rc::new(items))
    }

    /// Python 中的 `type(x)` 名称。
    pub fn type_name(&self) -> String {
        match self {
            Value::None => "NoneType".to_string(),
            Value::Bool(_) => "bool".to_string(),
            Value::Int(_) => "int".to_string(),
            Value::Float(_) => "float".to_string(),
            Value::Str(_) => "str".to_string(),
            Value::List(_) => "list".to_string(),
            Value::Tuple(_) => "tuple".to_string(),
            Value::Dict(_) => "dict".to_string(),
            Value::Set(_) => "set".to_string(),
            Value::Range(_) => "range".to_string(),
            Value::Func(_) => "function".to_string(),
            Value::Native(_) | Value::BoundNative(_) => "builtin_function_or_method".to_string(),
            Value::BoundMethod(_) => "method".to_string(),
            Value::Class(c) => c.name.clone(),
            Value::Instance(i) => i.class.name.clone(),
            Value::Module(m) => format!("module '{}'", m.name),
            Value::Slice(_) => "slice".to_string(),
            Value::Iterator(_) => "iterator".to_string(),
            Value::File(_) => "TextIOWrapper".to_string(),
            Value::Descriptor(_) => "staticmethod".to_string(),
            Value::Property(_) => "property".to_string(),
            Value::Super(_) => "super".to_string(),
        }
    }

    pub fn is_callable(&self) -> bool {
        matches!(
            self,
            Value::Func(_)
                | Value::Native(_)
                | Value::BoundNative(_)
                | Value::BoundMethod(_)
                | Value::Class(_)
        )
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Value::None)
    }

    /// 是否为“数字”（bool 也算，与 Python 一致）。
    pub fn as_number(&self) -> Option<Num> {
        match self {
            Value::Bool(b) => Some(Num::Int(*b as i64)),
            Value::Int(i) => Some(Num::Int(*i)),
            Value::Float(f) => Some(Num::Float(*f)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Num {
    Int(i64),
    Float(f64),
}

impl Num {
    pub fn as_f64(self) -> f64 {
        match self {
            Num::Int(i) => i as f64,
            Num::Float(f) => f,
        }
    }
    pub fn to_value(self) -> Value {
        match self {
            Num::Int(i) => Value::Int(i),
            Num::Float(f) => Value::Float(f),
        }
    }
}

// ---------------- 基础运算 ----------------

/// 用于字典键 / 集合成员的相等判断（不调用用户 `__eq__`）。
pub fn key_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::None, Value::None) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => x == y,
        (
            Value::Bool(_) | Value::Int(_) | Value::Float(_),
            other @ (Value::Bool(_) | Value::Int(_) | Value::Float(_)),
        ) => match (a.as_number(), other.as_number()) {
            (Some(x), Some(y)) => x.as_f64() == y.as_f64(),
            _ => false,
        },
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Tuple(x), Value::Tuple(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| key_equal(p, q))
        }
        (Value::Range(x), Value::Range(y)) => x == y,
        (Value::Class(x), Value::Class(y)) => Rc::ptr_eq(x, y),
        (Value::Func(x), Value::Func(y)) => Rc::ptr_eq(x, y),
        (Value::Instance(x), Value::Instance(y)) => Rc::ptr_eq(x, y),
        (Value::Super(x), Value::Super(y)) => Rc::ptr_eq(x, y),
        (Value::List(x), Value::List(y)) => {
            let xs = x.borrow();
            let ys = y.borrow();
            xs.len() == ys.len() && xs.iter().zip(ys.iter()).all(|(p, q)| key_equal(p, q))
        }
        (Value::Dict(x), Value::Dict(y)) => {
            let xs = x.borrow();
            let ys = y.borrow();
            xs.len() == ys.len()
                && xs.entries.iter().all(|(k, v)| match ys.get(k) {
                    Some(v2) => key_equal(v, &v2),
                    None => false,
                })
        }
        (Value::Set(x), Value::Set(y)) => {
            let xs = x.borrow();
            let ys = y.borrow();
            xs.len() == ys.len() && xs.items.iter().all(|v| ys.contains(v))
        }
        _ => false,
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Value) -> bool {
        key_equal(self, other)
    }
}

/// 值是否为可哈希类型（字典键 / 集合成员）。
pub fn is_hashable(v: &Value) -> bool {
    match v {
        Value::None | Value::Bool(_) | Value::Int(_) | Value::Float(_) | Value::Str(_) => true,
        Value::Tuple(items) => items.iter().all(is_hashable),
        Value::Range(_) | Value::Class(_) | Value::Func(_) | Value::Native(_) => true,
        Value::Instance(i) => {
            let d = i.class.dict.borrow();
            !d.contains_key("__eq__") || d.contains_key("__hash__")
        }
        _ => false,
    }
}

/// 只处理内置类型的 repr（用于异常对象等不需要解释器的场合）。
pub fn builtin_repr(v: &Value) -> String {
    match v {
        Value::None => "None".to_string(),
        Value::Bool(b) => (if *b { "True" } else { "False" }).to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => float_repr(*f),
        Value::Str(s) => str_repr(s),
        Value::Tuple(t) => {
            let parts: Vec<String> = t.iter().map(builtin_repr).collect();
            if parts.len() == 1 {
                format!("({},)", parts[0])
            } else {
                format!("({})", parts.join(", "))
            }
        }
        Value::List(l) => {
            let parts: Vec<String> = l.borrow().iter().map(builtin_repr).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Dict(d) => {
            let parts: Vec<String> = d
                .borrow()
                .entries
                .iter()
                .map(|(k, val)| format!("{}: {}", builtin_repr(k), builtin_repr(val)))
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
        other => format!("<{} object>", other.type_name()),
    }
}

/// 内置类型的真值判断；返回 `None` 表示需要调用 `__bool__` / `__len__`。
pub fn truthy_builtin(v: &Value) -> Option<bool> {
    Some(match v {
        Value::None => false,
        Value::Bool(b) => *b,
        Value::Int(i) => *i != 0,
        Value::Float(f) => *f != 0.0,
        Value::Str(s) => !s.is_empty(),
        Value::List(l) => !l.borrow().is_empty(),
        Value::Tuple(t) => !t.is_empty(),
        Value::Dict(d) => !d.borrow().entries.is_empty(),
        Value::Set(s) => !s.borrow().items.is_empty(),
        Value::Range(r) => !r.is_empty(),
        Value::Slice(_) => true,
        _ => return None,
    })
}

/// 整数向下取整除法（Python 语义）。
pub fn floor_div_i64(a: i64, b: i64) -> Option<i64> {
    if b == 0 {
        return None;
    }
    let q = a.checked_div(b)?;
    let r = a.checked_rem(b)?;
    if r != 0 && ((r < 0) != (b < 0)) {
        q.checked_sub(1)
    } else {
        Some(q)
    }
}

/// Python 风格的取模。
pub fn mod_i64(a: i64, b: i64) -> Option<i64> {
    if b == 0 {
        return None;
    }
    let r = a.checked_rem(b)?;
    if r != 0 && ((r < 0) != (b < 0)) {
        r.checked_add(b)
    } else {
        Some(r)
    }
}

/// 浮点数的 Python 风格取模。
pub fn mod_f64(a: f64, b: f64) -> f64 {
    if b == 0.0 {
        return f64::NAN;
    }
    let r = a % b;
    if r != 0.0 && (r < 0.0) != (b < 0.0) {
        r + b
    } else {
        r
    }
}

/// 复现 CPython 的 `repr(float)`。
pub fn float_repr(x: f64) -> String {
    if x.is_nan() {
        return "nan".to_string();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf" } else { "-inf" }.to_string();
    }
    if x == 0.0 {
        return if x.is_sign_negative() {
            "-0.0".to_string()
        } else {
            "0.0".to_string()
        };
    }
    let neg = x < 0.0;
    let ax = x.abs();
    // Rust 的 {:e} 给出最短往返表示，如 "1.5e-7" / "1e16"
    let sci = format!("{:e}", ax);
    let (mant, exp) = match sci.split_once('e') {
        Some((m, e)) => (m.to_string(), e.parse::<i32>().unwrap_or(0)),
        None => (sci.clone(), 0),
    };
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    let digits = digits.trim_end_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    if !(-4..16).contains(&exp) {
        // 科学计数法
        out.push_str(&digits[0..1]);
        if digits.len() > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        out.push('e');
        if exp < 0 {
            out.push('-');
        } else {
            out.push('+');
        }
        let ae = exp.abs();
        if ae < 10 {
            out.push('0');
        }
        out.push_str(&ae.to_string());
    } else if exp >= 0 {
        let ip = exp as usize + 1;
        if digits.len() > ip {
            out.push_str(&digits[..ip]);
            out.push('.');
            out.push_str(&digits[ip..]);
        } else {
            out.push_str(digits);
            for _ in 0..(ip - digits.len()) {
                out.push('0');
            }
            out.push_str(".0");
        }
    } else {
        out.push_str("0.");
        for _ in 0..(-exp - 1) {
            out.push('0');
        }
        out.push_str(digits);
    }
    out
}

/// 复现 CPython 的 `repr(str)`。
pub fn str_repr(s: &str) -> String {
    let has_single = s.contains('\'');
    let has_double = s.contains('"');
    let quote = if has_single && !has_double { '"' } else { '\'' };
    let mut out = String::new();
    out.push(quote);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// `format(x, spec)` / f-string 的格式说明符。
pub fn format_value(v: &Value, spec: &str) -> Result<String, String> {
    let (fill, align, sign, alt, zero, width, comma, precision, ty) = parse_format_spec(spec)?;
    let mut body;
    let is_num = matches!(v, Value::Int(_) | Value::Float(_) | Value::Bool(_));
    match ty {
        Some('s') | None if !is_num => {
            let s = match v {
                Value::Str(s) => s.to_string(),
                other => match other {
                    Value::None => "None".to_string(),
                    Value::Bool(b) => if *b { "True" } else { "False" }.to_string(),
                    other => {
                        return Err(format!(
                            "暂不支持对 {} 使用格式说明符，请先用 str() 转换",
                            other.type_name()
                        ))
                    }
                },
            };
            body = s;
            if let Some(p) = precision {
                body = body.chars().take(p).collect();
            }
        }
        _ => {
            let (text, is_neg) = match v {
                Value::Int(i) => (format_int(*i, ty, alt)?, *i < 0),
                Value::Bool(b) => (format_int(*b as i64, ty, alt)?, false),
                Value::Float(f) => (format_float(*f, ty, precision, alt)?, f.is_sign_negative()),
                other => {
                    return Err(format!(
                        "格式说明符 '{}' 不能用于 {}",
                        ty.unwrap_or(' '),
                        other.type_name()
                    ))
                }
            };
            let mut s = text;
            if comma {
                s = add_thousands(&s);
            }
            let sign_str = if is_neg {
                "-".to_string()
            } else {
                match sign {
                    '+' => "+".to_string(),
                    ' ' => " ".to_string(),
                    _ => String::new(),
                }
            };
            if zero && fill == ' ' && align.is_none() {
                // 数字补零：符号在前
                let w = width.unwrap_or(0);
                let total = sign_str.chars().count() + s.chars().count();
                if total < w {
                    body = format!("{}{}{}", sign_str, "0".repeat(w - total), s);
                    return Ok(body);
                }
            }
            if is_neg {
                s = s.trim_start_matches('-').to_string();
            }
            body = format!("{}{}", sign_str, s);
        }
    }
    if let Some(w) = width {
        let len = body.chars().count();
        if len < w {
            let pad = w - len;
            let a = align.unwrap_or(if is_num { '>' } else { '<' });
            match a {
                '<' => body = format!("{}{}", body, fill.to_string().repeat(pad)),
                '>' => body = format!("{}{}", fill.to_string().repeat(pad), body),
                '^' => {
                    let left = pad / 2;
                    let right = pad - left;
                    body = format!(
                        "{}{}{}",
                        fill.to_string().repeat(left),
                        body,
                        fill.to_string().repeat(right)
                    );
                }
                '=' => {
                    // 数字的符号后对齐：简化处理为右对齐
                    body = format!("{}{}", fill.to_string().repeat(pad), body);
                }
                _ => {}
            }
        }
    }
    Ok(body)
}

// 这里用 `% 3 == 0` 而不是 `is_multiple_of(3)`，后者需要 Rust 1.87+，会抬高 MSRV
#[allow(clippy::manual_is_multiple_of)]
fn add_thousands(s: &str) -> String {
    let (sign, digits) = if let Some(rest) = s.strip_prefix('-') {
        ("-", rest)
    } else {
        ("", s)
    };
    let (int_part, rest) = match digits.find('.') {
        Some(i) => (&digits[..i], &digits[i..]),
        None => (digits, ""),
    };
    let mut out = String::new();
    let chars: Vec<char> = int_part.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && (chars.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*c);
    }
    format!("{}{}{}", sign, out, rest)
}

fn format_int(v: i64, ty: Option<char>, alt: bool) -> Result<String, String> {
    let neg = v < 0;
    let mag = v.unsigned_abs();
    let prefix = |p: &str| -> String {
        if alt && !neg {
            p.to_string()
        } else {
            String::new()
        }
    };
    Ok(match ty {
        None | Some('d') => v.to_string(),
        Some('b') => format!("{}{}{:b}", if neg { "-" } else { "" }, prefix("0b"), mag),
        Some('o') => format!("{}{}{:o}", if neg { "-" } else { "" }, prefix("0o"), mag),
        Some('x') => format!("{}{}{:x}", if neg { "-" } else { "" }, prefix("0x"), mag),
        Some('X') => format!("{}{}{:X}", if neg { "-" } else { "" }, prefix("0X"), mag),
        Some('c') => match u32::try_from(v).ok().and_then(char::from_u32) {
            Some(c) => c.to_string(),
            None => return Err("chr() 参数超出范围".to_string()),
        },
        Some('f') | Some('F') => format!("{:.6}", v as f64),
        Some('e') => exp_format(v as f64, 6, true),
        Some('E') => exp_format(v as f64, 6, false),
        Some('g') | Some('G') => format_general(v as f64, 6),
        Some('%') => format!("{:.6}%", (v as f64) * 100.0),
        Some(c) => return Err(format!("未知的格式类型 '{}'", c)),
    })
}

fn exp_format(f: f64, prec: usize, lower: bool) -> String {
    let s = format!("{:.*e}", prec, f);
    let s = if lower { s } else { s.to_uppercase() };
    // Rust: 1.5e2 -> Python: 1.500000e+02
    match s.split_once(['e', 'E']) {
        Some((m, e)) => {
            let e_marker = if lower { 'e' } else { 'E' };
            let (sign, digits) = if let Some(d) = e.strip_prefix('-') {
                ('-', d)
            } else {
                ('+', e)
            };
            format!(
                "{}{}{}{:0>2}",
                m,
                e_marker,
                sign,
                digits.parse::<i64>().unwrap_or(0)
            )
        }
        None => s,
    }
}

fn format_general(f: f64, prec: usize) -> String {
    if f == 0.0 {
        return "0".to_string();
    }
    let exp = f.abs().log10().floor() as i32;
    if exp < -4 || exp >= prec as i32 {
        let s = exp_format(f, prec.saturating_sub(1), true);
        trim_g_zeros(s)
    } else {
        let decimals = (prec as i32 - 1 - exp).max(0) as usize;
        let s = format!("{:.*}", decimals, f);
        trim_g_zeros(s)
    }
}

fn trim_g_zeros(s: String) -> String {
    if let Some(epos) = s.find('e') {
        let (m, e) = s.split_at(epos);
        let m = if m.contains('.') {
            m.trim_end_matches('0').trim_end_matches('.')
        } else {
            m
        };
        format!("{}{}", m, e)
    } else if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

fn format_float(
    f: f64,
    ty: Option<char>,
    precision: Option<usize>,
    _alt: bool,
) -> Result<String, String> {
    if f.is_nan() {
        return Ok("nan".to_string());
    }
    if f.is_infinite() {
        return Ok(if f > 0.0 { "inf" } else { "-inf" }.to_string());
    }
    Ok(match ty {
        None => {
            if let Some(p) = precision {
                format!("{:.*}", p, f)
            } else {
                float_repr(f)
            }
        }
        Some('f') | Some('F') => format!("{:.*}", precision.unwrap_or(6), f),
        Some('e') => exp_format(f, precision.unwrap_or(6), true),
        Some('E') => exp_format(f, precision.unwrap_or(6), false),
        Some('g') | Some('G') => format_general(f, precision.unwrap_or(6)),
        Some('%') => format!("{:.*}%", precision.unwrap_or(6), f * 100.0),
        Some('d') => {
            if precision.is_some() {
                return Err("精度不能用于整数格式".to_string());
            }
            format!("{}", f as i64)
        }
        Some(c) => return Err(format!("未知的格式类型 '{}'", c)),
    })
}

type FormatSpec = (
    char,
    Option<char>,
    char,
    bool,
    bool,
    Option<usize>,
    bool,
    Option<usize>,
    Option<char>,
);

fn parse_format_spec(spec: &str) -> Result<FormatSpec, String> {
    let chars: Vec<char> = spec.chars().collect();
    let mut i = 0usize;
    let mut fill = ' ';
    let mut align = None;
    // [[fill]align]
    if chars.len() >= 2 && matches!(chars[1], '<' | '>' | '^' | '=') {
        fill = chars[0];
        align = Some(chars[1]);
        i = 2;
    } else if !chars.is_empty() && matches!(chars[0], '<' | '>' | '^' | '=') {
        align = Some(chars[0]);
        i = 1;
    }
    let mut sign = '-';
    if i < chars.len() && matches!(chars[i], '+' | '-' | ' ') {
        sign = chars[i];
        if sign == '-' && align.is_none() {
            align = Some('<');
        }
        i += 1;
    }
    let mut alt = false;
    if i < chars.len() && chars[i] == '#' {
        alt = true;
        i += 1;
    }
    let mut zero = false;
    if i < chars.len() && chars[i] == '0' {
        zero = true;
        i += 1;
    }
    let mut width = None;
    let mut w = String::new();
    while i < chars.len() && chars[i].is_ascii_digit() {
        w.push(chars[i]);
        i += 1;
    }
    if !w.is_empty() {
        width = w.parse::<usize>().ok();
    }
    let mut comma = false;
    if i < chars.len() && chars[i] == ',' {
        comma = true;
        i += 1;
    }
    let mut precision = None;
    if i < chars.len() && chars[i] == '.' {
        i += 1;
        let mut p = String::new();
        while i < chars.len() && chars[i].is_ascii_digit() {
            p.push(chars[i]);
            i += 1;
        }
        precision = Some(p.parse::<usize>().map_err(|_| "无效的精度")?);
    }
    let mut ty = None;
    if i < chars.len() {
        ty = Some(chars[i]);
        i += 1;
    }
    if i < chars.len() {
        return Err(format!("无效的格式说明符 '{}'", spec));
    }
    Ok((fill, align, sign, alt, zero, width, comma, precision, ty))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_repr_matches_python() {
        assert_eq!(float_repr(1.0), "1.0");
        assert_eq!(float_repr(0.1 + 0.2), "0.30000000000000004");
        assert_eq!(float_repr(-0.0), "-0.0");
        assert_eq!(float_repr(1e16), "1e+16");
        assert_eq!(float_repr(1e15), "1000000000000000.0");
        assert_eq!(float_repr(1.5e-7), "1.5e-07");
        assert_eq!(float_repr(123.456), "123.456");
        assert_eq!(float_repr(f64::INFINITY), "inf");
    }

    #[test]
    fn str_repr_matches_python() {
        assert_eq!(str_repr("hi"), "'hi'");
        assert_eq!(str_repr("it's"), "\"it's\"");
        assert_eq!(str_repr("a\nb"), "'a\\nb'");
        assert_eq!(str_repr("a'b\"c"), "'a\\'b\"c'");
    }

    #[test]
    fn integer_division_semantics() {
        assert_eq!(floor_div_i64(-7, 2), Some(-4));
        assert_eq!(mod_i64(-7, 2), Some(1));
        assert_eq!(floor_div_i64(7, -2), Some(-4));
        assert_eq!(mod_i64(7, -2), Some(-1));
    }

    #[test]
    fn format_specs() {
        assert_eq!(format_value(&Value::Float(1.23456), ".2f").unwrap(), "1.23");
        assert_eq!(format_value(&Value::Int(42), "5d").unwrap(), "   42");
        assert_eq!(format_value(&Value::Int(42), "<5d").unwrap(), "42   ");
        assert_eq!(format_value(&Value::Int(42), "^5d").unwrap(), " 42  ");
        assert_eq!(format_value(&Value::Int(42), "05d").unwrap(), "00042");
        assert_eq!(format_value(&Value::Int(255), "x").unwrap(), "ff");
        assert_eq!(
            format_value(&Value::Int(1234567), ",").unwrap(),
            "1,234,567"
        );
        assert_eq!(
            format_value(&Value::Str("ab".into()), ">5").unwrap(),
            "   ab"
        );
        assert_eq!(format_value(&Value::Float(0.5), ".1%").unwrap(), "50.0%");
    }

    #[test]
    fn dict_keeps_insertion_order() {
        let mut d = DictData::new();
        d.insert(Value::Int(2), Value::str_from("b"));
        d.insert(Value::Int(1), Value::str_from("a"));
        d.insert(Value::Int(2), Value::str_from("B"));
        assert_eq!(d.len(), 2);
        assert_eq!(d.get(&Value::Int(2)).unwrap(), Value::str_from("B"));
        // 1 和 1.0 视为同一个键
        d.insert(Value::Float(1.0), Value::str_from("A"));
        assert_eq!(d.len(), 2);
    }
}
