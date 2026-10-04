//! 内置函数、内置类型与内置异常。

use crate::error::BUILTIN_EXCEPTIONS;
use crate::interp::{EResult, Interp, Signal};
use crate::value::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Write;
use std::rc::Rc;

/// 需要预先创建类型对象的名称。
const TYPE_NAMES: &[&str] = &[
    "object",
    "int",
    "float",
    "str",
    "bool",
    "list",
    "dict",
    "set",
    "tuple",
    "range",
    "type",
    "NoneType",
    "function",
    "builtin_function_or_method",
    "method",
    "module",
    "iterator",
    "slice",
    "TextIOWrapper",
    "property",
];

impl Interp {
    /// 建立内置命名空间。
    pub fn init_builtins(&mut self) {
        // 1. object（所有内置类型的基类，mro 不包含自身）
        let object = Rc::new(ClassData {
            name: "object".to_string(),
            module: "builtins".to_string(),
            bases: Vec::new(),
            dict: RefCell::new(HashMap::new()),
            mro: Vec::new(),
            is_exception: false,
            builtin: None,
        });
        self.type_classes
            .insert("object".to_string(), object.clone());
        self.builtins.define("object", Value::Class(object.clone()));

        // 2. 内置类型（以 object 为基类）
        for name in TYPE_NAMES {
            if *name == "object" {
                continue;
            }
            let cls = Rc::new(ClassData {
                name: name.to_string(),
                module: "builtins".to_string(),
                bases: vec![object.clone()],
                dict: RefCell::new(HashMap::new()),
                mro: Vec::new(),
                is_exception: false,
                builtin: Some(name.to_string()),
            });
            let cls = with_mro(cls);
            self.type_classes.insert(name.to_string(), cls.clone());
            self.builtins.define(*name, Value::Class(cls));
        }

        // 3. 内置异常类
        for (name, base) in BUILTIN_EXCEPTIONS {
            let bases: Vec<Rc<ClassData>> = if base.is_empty() {
                vec![object.clone()]
            } else {
                match self.builtins.lookup(base) {
                    Some(Value::Class(c)) => vec![c],
                    _ => vec![object.clone()],
                }
            };
            let cls = Rc::new(ClassData {
                name: name.to_string(),
                module: "builtins".to_string(),
                bases,
                dict: RefCell::new(HashMap::new()),
                mro: Vec::new(),
                is_exception: true,
                builtin: None,
            });
            let cls = with_mro(cls);
            self.builtins.define(*name, Value::Class(cls));
        }

        // 3.5 给 BaseException 加上 __init__（保存 args），使 super().__init__(...) 可用
        if let Some(Value::Class(base)) = self.builtins.lookup("BaseException") {
            base.dict.borrow_mut().insert(
                "__init__".to_string(),
                Value::Native(Rc::new(NativeData {
                    name: "BaseException.__init__".to_string(),
                    func: exception_init,
                })),
            );
        }

        // 4. 内置函数
        for (name, f) in native_functions() {
            self.builtins.define(
                name,
                Value::Native(Rc::new(NativeData {
                    name: name.to_string(),
                    func: f,
                })),
            );
        }
        self.builtins
            .define("__name__", Value::str_from("builtins"));

        // 5. 内置模块
        crate::modules::register_builtin_modules(self);
    }

    /// 取得某个类型名对应的类对象。
    pub fn type_class(&self, name: &str) -> Rc<ClassData> {
        match self.type_classes.get(name) {
            Some(c) => c.clone(),
            None => self
                .type_classes
                .get("object")
                .expect("object 类必须存在")
                .clone(),
        }
    }

    /// 与 Python 的 `type(x)` 对应。
    pub fn type_of(&self, v: &Value) -> Value {
        match v {
            Value::Instance(i) => Value::Class(i.class.clone()),
            // 类的类型就是 type（本实现不支持元类）
            Value::Class(_) => Value::Class(self.type_class("type")),
            other => Value::Class(self.type_class(&other.type_name())),
        }
    }

    /// 调用属性取值（供内置函数使用）。
    pub fn attr_of(&mut self, v: &Value, name: &str) -> EResult<Value> {
        self.get_attr(v, name)
    }

    // ---------------- 内置类型构造 ----------------

    pub fn call_builtin_type(
        &mut self,
        name: &str,
        args: Vec<Value>,
        kwargs: Vec<(String, Value)>,
    ) -> EResult<Value> {
        match name {
            "int" => self.ctor_int(args),
            "float" => self.ctor_float(args),
            "str" => self.ctor_str(args),
            "bool" => {
                if args.is_empty() {
                    return Ok(Value::Bool(false));
                }
                let b = self.truthy(&args[0])?;
                Ok(Value::Bool(b))
            }
            "list" => {
                if args.is_empty() {
                    return Ok(Value::list(Vec::new()));
                }
                let items = self.collect_iter(&args[0])?;
                Ok(Value::list(items))
            }
            "tuple" => {
                if args.is_empty() {
                    return Ok(Value::tuple(Vec::new()));
                }
                let items = self.collect_iter(&args[0])?;
                Ok(Value::tuple(items))
            }
            "set" => {
                let items = if args.is_empty() {
                    Vec::new()
                } else {
                    self.collect_iter(&args[0])?
                };
                let mut s = SetData::new();
                for v in items {
                    if !is_hashable(&v) {
                        return self
                            .err("TypeError", format!("unhashable type: '{}'", v.type_name()));
                    }
                    s.insert(v);
                }
                Ok(Value::Set(Rc::new(RefCell::new(s))))
            }
            "dict" => {
                let mut d = DictData::new();
                if let Some(other) = args.first() {
                    let is_mapping = match other {
                        Value::Dict(_) => true,
                        Value::Instance(inst) => {
                            self.find_class_attr(&inst.class, "keys").is_some()
                        }
                        _ => false,
                    };
                    if is_mapping {
                        for (k, v) in self.dict_items(other)? {
                            d.insert(k, v);
                        }
                    } else {
                        for pair in self.collect_iter(other)? {
                            let ps = self.collect_iter(&pair)?;
                            if ps.len() != 2 {
                                return self.err(
                                    "ValueError",
                                    "dictionary update sequence element has length != 2",
                                );
                            }
                            d.insert(ps[0].clone(), ps[1].clone());
                        }
                    }
                }
                for (k, v) in kwargs {
                    d.insert(Value::str_from(k), v);
                }
                Ok(Value::Dict(Rc::new(RefCell::new(d))))
            }
            "range" => {
                let ints: Vec<i64> = {
                    let mut out = Vec::new();
                    for a in &args {
                        out.push(self.as_index(a)?);
                    }
                    out
                };
                let (start, stop, step) = match ints.len() {
                    0 => (0, 0, 1),
                    1 => (0, ints[0], 1),
                    2 => (ints[0], ints[1], 1),
                    3 => (ints[0], ints[1], ints[2]),
                    _ => return self.err("TypeError", "range() 参数过多"),
                };
                if step == 0 {
                    return self.err("ValueError", "range() arg 3 must not be zero");
                }
                Ok(Value::Range(Rc::new(RangeData { start, stop, step })))
            }
            "type" => match args.len() {
                1 => Ok(self.type_of(&args[0])),
                _ => self.err("NotImplementedError", "暂不支持三参数 type()"),
            },
            "slice" => {
                let mut nums: Vec<Option<i64>> = Vec::new();
                for a in &args {
                    if a.is_none() {
                        nums.push(None);
                    } else {
                        nums.push(Some(self.as_index(a)?));
                    }
                }
                let (lower, upper, step) = match nums.len() {
                    1 => (None, nums[0], None),
                    2 => (nums[0], nums[1], None),
                    3 => (nums[0], nums[1], nums[2]),
                    _ => return self.err("TypeError", "slice() 参数个数错误"),
                };
                Ok(Value::Slice(Rc::new(SliceData { lower, upper, step })))
            }
            "property" => {
                if args.is_empty() {
                    return self.err("TypeError", "property() 至少需要 1 个参数");
                }
                Ok(Value::Property(Rc::new(PropertyData {
                    name: "<property>".to_string(),
                    getter: args[0].clone(),
                    setter: args.get(1).filter(|v| !v.is_none()).cloned(),
                    deleter: args.get(2).filter(|v| !v.is_none()).cloned(),
                })))
            }
            "object" => {
                let cls = self.type_class("object");
                Ok(Value::Instance(Rc::new(InstanceData {
                    class: cls,
                    dict: RefCell::new(HashMap::new()),
                })))
            }
            "NoneType" => self.err("TypeError", "cannot create 'NoneType' instances"),
            "TextIOWrapper" => self.err("TypeError", "请使用 open() 打开文件"),
            other => self.err("TypeError", format!("暂不支持构造内置类型 '{}'", other)),
        }
    }

    fn ctor_int(&mut self, args: Vec<Value>) -> EResult<Value> {
        if args.is_empty() {
            return Ok(Value::Int(0));
        }
        if args.len() > 2 {
            return self.err("TypeError", "int() 参数过多");
        }
        let base = match args.get(1) {
            Some(v) => Some(self.as_index(v)?),
            None => None,
        };
        match (&args[0], base) {
            (Value::Str(s), b) => {
                let text = s.trim();
                let base = b.unwrap_or(10);
                if base != 0 && !(2..=36).contains(&base) {
                    return self.err("ValueError", "int() base must be >= 2 and <= 36, or 0");
                }
                parse_int_str(text, base as u32)
                    .ok_or_else(|| {
                        Signal::Error(self.pyerr(
                            "ValueError",
                            format!("invalid literal for int() with base {}: '{}'", base, text),
                        ))
                    })
                    .map(Value::Int)
            }
            (Value::Int(i), None) => Ok(Value::Int(*i)),
            (Value::Bool(b), None) => Ok(Value::Int(*b as i64)),
            (Value::Float(f), None) => {
                if !f.is_finite() {
                    return self.err(
                        "ValueError",
                        format!("cannot convert float {} to integer", float_repr(*f)),
                    );
                }
                if *f > i64::MAX as f64 || *f < i64::MIN as f64 {
                    return self.err("OverflowError", "int 过大");
                }
                Ok(Value::Int(*f as i64))
            }
            (Value::Instance(i), None) => {
                if let Some(m) = self.find_class_attr(&i.class, "__int__") {
                    let r = self.call_value(&m, vec![args[0].clone()], Vec::new())?;
                    return match r {
                        Value::Int(v) => Ok(Value::Int(v)),
                        other => self.err(
                            "TypeError",
                            format!("__int__ 应返回 int，而不是 {}", other.type_name()),
                        ),
                    };
                }
                self.err(
                    "TypeError",
                    format!(
                        "int() argument must be a string or a number, not '{}'",
                        args[0].type_name()
                    ),
                )
            }
            (other, _) => self.err(
                "TypeError",
                format!(
                    "int() argument must be a string or a number, not '{}'",
                    other.type_name()
                ),
            ),
        }
    }

    fn ctor_float(&mut self, args: Vec<Value>) -> EResult<Value> {
        if args.is_empty() {
            return Ok(Value::Float(0.0));
        }
        match &args[0] {
            Value::Str(s) => {
                let t = s.trim().to_lowercase();
                let v = match t.as_str() {
                    "inf" | "infinity" | "+inf" | "+infinity" => f64::INFINITY,
                    "-inf" | "-infinity" => f64::NEG_INFINITY,
                    "nan" | "+nan" | "-nan" => f64::NAN,
                    _ => match t.replace('_', "").parse::<f64>() {
                        Ok(v) => v,
                        Err(_) => {
                            return self.err(
                                "ValueError",
                                format!("could not convert string to float: '{}'", s),
                            )
                        }
                    },
                };
                Ok(Value::Float(v))
            }
            Value::Int(i) => Ok(Value::Float(*i as f64)),
            Value::Bool(b) => Ok(Value::Float(*b as i64 as f64)),
            Value::Float(f) => Ok(Value::Float(*f)),
            Value::Instance(i) => {
                if let Some(m) = self.find_class_attr(&i.class, "__float__") {
                    let r = self.call_value(&m, vec![args[0].clone()], Vec::new())?;
                    return match r {
                        Value::Float(v) => Ok(Value::Float(v)),
                        other => self.err(
                            "TypeError",
                            format!("__float__ 应返回 float，而不是 {}", other.type_name()),
                        ),
                    };
                }
                self.err(
                    "TypeError",
                    format!(
                        "float() argument must be a string or a number, not '{}'",
                        i.class.name
                    ),
                )
            }
            other => self.err(
                "TypeError",
                format!(
                    "float() argument must be a string or a number, not '{}'",
                    other.type_name()
                ),
            ),
        }
    }

    fn ctor_str(&mut self, args: Vec<Value>) -> EResult<Value> {
        if args.is_empty() {
            return Ok(Value::str_from(""));
        }
        let s = self.value_str(&args[0])?;
        Ok(Value::str_from(s))
    }
}

/// 由基类推导 MRO（不含自身，与 `ClassData::mro` 的约定一致）。
fn with_mro(c: Rc<ClassData>) -> Rc<ClassData> {
    let mut mro: Vec<Rc<ClassData>> = Vec::new();
    for b in &c.bases {
        if !mro.iter().any(|x| Rc::ptr_eq(x, b)) {
            mro.push(b.clone());
        }
        for x in &b.mro {
            if !mro.iter().any(|y| Rc::ptr_eq(y, x)) {
                mro.push(x.clone());
            }
        }
    }
    Rc::new(ClassData {
        name: c.name.clone(),
        module: c.module.clone(),
        bases: c.bases.clone(),
        dict: RefCell::new(c.dict.borrow().clone()),
        mro,
        is_exception: c.is_exception,
        builtin: c.builtin.clone(),
    })
}

/// 按给定进制解析整数字符串。
pub fn parse_int_str(text: &str, base: u32) -> Option<i64> {
    let mut t = text.trim().replace('_', "");
    let mut sign = 1i64;
    if let Some(rest) = t.strip_prefix('-') {
        sign = -1;
        t = rest.to_string();
    } else if let Some(rest) = t.strip_prefix('+') {
        t = rest.to_string();
    }
    let mut base = base;
    if base == 0 {
        let lower = t.to_lowercase();
        if let Some(rest) = lower.strip_prefix("0x") {
            base = 16;
            t = rest.to_string();
        } else if let Some(rest) = lower.strip_prefix("0o") {
            base = 8;
            t = rest.to_string();
        } else if let Some(rest) = lower.strip_prefix("0b") {
            base = 2;
            t = rest.to_string();
        } else {
            base = 10;
        }
    } else {
        let lower = t.to_lowercase();
        let prefix = match base {
            16 => Some("0x"),
            8 => Some("0o"),
            2 => Some("0b"),
            _ => None,
        };
        if let Some(p) = prefix {
            if let Some(rest) = lower.strip_prefix(p) {
                t = rest.to_string();
            }
        }
    }
    if t.is_empty() {
        return None;
    }
    let v = i64::from_str_radix(&t, base).ok()?;
    Some(sign.wrapping_mul(v))
}

// ---------------- 内置函数 ----------------

macro_rules! nf {
    ($name:literal, $f:path) => {
        ($name, $f as NativeFn)
    };
}

pub fn native_functions() -> Vec<(&'static str, NativeFn)> {
    vec![
        nf!("print", bi_print),
        nf!("len", bi_len),
        nf!("repr", bi_repr),
        nf!("abs", bi_abs),
        nf!("min", bi_min),
        nf!("max", bi_max),
        nf!("sum", bi_sum),
        nf!("sorted", bi_sorted),
        nf!("reversed", bi_reversed),
        nf!("enumerate", bi_enumerate),
        nf!("zip", bi_zip),
        nf!("map", bi_map),
        nf!("filter", bi_filter),
        nf!("all", bi_all),
        nf!("any", bi_any),
        nf!("round", bi_round),
        nf!("divmod", bi_divmod),
        nf!("pow", bi_pow),
        nf!("chr", bi_chr),
        nf!("ord", bi_ord),
        nf!("hex", bi_hex),
        nf!("oct", bi_oct),
        nf!("bin", bi_bin),
        nf!("id", bi_id),
        nf!("input", bi_input),
        nf!("open", bi_open),
        nf!("callable", bi_callable),
        nf!("getattr", bi_getattr),
        nf!("setattr", bi_setattr),
        nf!("hasattr", bi_hasattr),
        nf!("delattr", bi_delattr),
        nf!("isinstance", bi_isinstance),
        nf!("issubclass", bi_issubclass),
        nf!("iter", bi_iter),
        nf!("next", bi_next),
        nf!("format", bi_format),
        nf!("hash", bi_hash),
        nf!("dir", bi_dir),
        nf!("globals", bi_globals),
        nf!("vars", bi_vars),
        nf!("staticmethod", bi_staticmethod),
        nf!("classmethod", bi_classmethod),
        nf!("property", bi_property),
        nf!("super", bi_super),
        nf!("exit", bi_exit),
    ]
}

/// `BaseException.__init__(self, *args)`：把参数保存到 `self.args`。
fn exception_init(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    match args.first() {
        Some(Value::Instance(inst)) => {
            let rest: Vec<Value> = args[1..].to_vec();
            inst.dict
                .borrow_mut()
                .insert("args".to_string(), Value::tuple(rest));
            Ok(Value::None)
        }
        _ => i.err("TypeError", "BaseException.__init__ 需要异常实例"),
    }
}

fn bi_print(i: &mut Interp, args: &[Value], kwargs: &[(String, Value)]) -> EResult<Value> {
    let sep = match kwargs.iter().find(|(k, _)| k == "sep") {
        Some((_, Value::Str(s))) => s.to_string(),
        Some((_, Value::None)) => " ".to_string(),
        Some((_, other)) => {
            return i.err(
                "TypeError",
                format!("sep must be None or a string, not {}", other.type_name()),
            )
        }
        None => " ".to_string(),
    };
    let end = match kwargs.iter().find(|(k, _)| k == "end") {
        Some((_, Value::Str(s))) => s.to_string(),
        Some((_, Value::None)) => "\n".to_string(),
        Some((_, other)) => {
            return i.err(
                "TypeError",
                format!("end must be None or a string, not {}", other.type_name()),
            )
        }
        None => "\n".to_string(),
    };
    let mut text = String::new();
    for (n, a) in args.iter().enumerate() {
        if n > 0 {
            text.push_str(&sep);
        }
        text.push_str(&i.value_str(a)?);
    }
    text.push_str(&end);
    let file_kw = kwargs
        .iter()
        .find(|(k, _)| k == "file")
        .map(|(_, v)| v.clone());
    match file_kw {
        Some(Value::None) | None => {
            if i.out.write_all(text.as_bytes()).is_err() {
                return Err(Signal::Error(i.pyerr("OSError", "写入标准输出失败")));
            }
            let _ = i.out.flush();
        }
        Some(f) => {
            let w = i.get_attr(&f, "write")?;
            i.call_value(&w, vec![Value::str_from(text)], Vec::new())?;
        }
    }
    Ok(Value::None)
}

fn bi_len(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.len() != 1 {
        return i.err("TypeError", "len() 需要 1 个参数");
    }
    let n = match &args[0] {
        Value::Str(s) => s.chars().count() as i64,
        Value::List(l) => l.borrow().len() as i64,
        Value::Tuple(t) => t.len() as i64,
        Value::Dict(d) => d.borrow().len() as i64,
        Value::Set(s) => s.borrow().len() as i64,
        Value::Range(r) => r.len(),
        Value::Instance(inst) => {
            if let Some(m) = i.find_class_attr(&inst.class, "__len__") {
                let r = i.call_value(&m, vec![args[0].clone()], Vec::new())?;
                i.as_index(&r)?
            } else {
                return i.err(
                    "TypeError",
                    format!("object of type '{}' has no len()", inst.class.name),
                );
            }
        }
        other => {
            return i.err(
                "TypeError",
                format!("object of type '{}' has no len()", other.type_name()),
            )
        }
    };
    Ok(Value::Int(n))
}

fn bi_repr(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.len() != 1 {
        return i.err("TypeError", "repr() 需要 1 个参数");
    }
    let s = i.value_repr(&args[0])?;
    Ok(Value::str_from(s))
}

fn bi_abs(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.len() != 1 {
        return i.err("TypeError", "abs() 需要 1 个参数");
    }
    match &args[0] {
        Value::Int(n) => Ok(match n.checked_abs() {
            Some(v) => Value::Int(v),
            None => Value::Float((*n as f64).abs()),
        }),
        Value::Bool(b) => Ok(Value::Int(*b as i64)),
        Value::Float(f) => Ok(Value::Float(f.abs())),
        Value::Instance(inst) => match i.find_class_attr(&inst.class, "__abs__") {
            Some(m) => i.call_value(&m, vec![args[0].clone()], Vec::new()),
            None => i.err(
                "TypeError",
                format!("bad operand type for abs(): '{}'", inst.class.name),
            ),
        },
        other => i.err(
            "TypeError",
            format!("bad operand type for abs(): '{}'", other.type_name()),
        ),
    }
}

fn min_max(
    i: &mut Interp,
    args: &[Value],
    kwargs: &[(String, Value)],
    want_max: bool,
) -> EResult<Value> {
    let key = kwargs
        .iter()
        .find(|(k, _)| k == "key")
        .map(|(_, v)| v.clone());
    let default = kwargs
        .iter()
        .find(|(k, _)| k == "default")
        .map(|(_, v)| v.clone());
    let items: Vec<Value> = if args.len() == 1 {
        i.collect_iter(&args[0])?
    } else {
        args.to_vec()
    };
    if items.is_empty() {
        return match default {
            Some(d) => Ok(d),
            None => i.err(
                "ValueError",
                format!(
                    "{}() iterable argument is empty",
                    if want_max { "max" } else { "min" }
                ),
            ),
        };
    }
    let mut best = items[0].clone();
    let mut best_key = match &key {
        Some(k) if !k.is_none() => i.call_value(k, vec![best.clone()], Vec::new())?,
        _ => best.clone(),
    };
    for it in items.iter().skip(1) {
        let k = match &key {
            Some(f) if !f.is_none() => i.call_value(f, vec![it.clone()], Vec::new())?,
            _ => it.clone(),
        };
        let better = if want_max {
            i.lt_values(&best_key, &k)?
        } else {
            i.lt_values(&k, &best_key)?
        };
        if better {
            best = it.clone();
            best_key = k;
        }
    }
    Ok(best)
}

fn bi_min(i: &mut Interp, args: &[Value], kwargs: &[(String, Value)]) -> EResult<Value> {
    if args.is_empty() {
        return i.err("TypeError", "min expected at least 1 argument, got 0");
    }
    min_max(i, args, kwargs, false)
}

fn bi_max(i: &mut Interp, args: &[Value], kwargs: &[(String, Value)]) -> EResult<Value> {
    if args.is_empty() {
        return i.err("TypeError", "max expected at least 1 argument, got 0");
    }
    min_max(i, args, kwargs, true)
}

fn bi_sum(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.is_empty() {
        return i.err("TypeError", "sum() 至少需要 1 个参数");
    }
    let items = i.collect_iter(&args[0])?;
    let mut acc = args.get(1).cloned().unwrap_or(Value::Int(0));
    if matches!(acc, Value::Str(_)) {
        return i.err(
            "TypeError",
            "sum() can't sum strings [use ''.join(seq) instead]",
        );
    }
    for it in items {
        acc = i.binop(crate::lexer::Op::Plus, acc, it)?;
    }
    Ok(acc)
}

fn bi_sorted(i: &mut Interp, args: &[Value], kwargs: &[(String, Value)]) -> EResult<Value> {
    if args.len() != 1 {
        return i.err("TypeError", "sorted() 需要 1 个参数");
    }
    let mut items = i.collect_iter(&args[0])?;
    let key = kwargs
        .iter()
        .find(|(k, _)| k == "key")
        .map(|(_, v)| v.clone());
    let reverse = match kwargs.iter().find(|(k, _)| k == "reverse") {
        Some((_, v)) => i.truthy(v)?,
        None => false,
    };
    let key_ref = key.as_ref().filter(|v| !v.is_none());
    crate::methods::sort_values(i, &mut items, key_ref, reverse)?;
    Ok(Value::list(items))
}

fn bi_reversed(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.len() != 1 {
        return i.err("TypeError", "reversed() 需要 1 个参数");
    }
    let mut items = i.collect_iter(&args[0])?;
    items.reverse();
    let rc = Rc::new(RefCell::new(items));
    Ok(Value::Iterator(Rc::new(RefCell::new(IterKind::List(
        rc, 0,
    )))))
}

fn bi_enumerate(i: &mut Interp, args: &[Value], kwargs: &[(String, Value)]) -> EResult<Value> {
    if args.is_empty() {
        return i.err("TypeError", "enumerate() 至少需要 1 个参数");
    }
    let start = match args.get(1) {
        Some(v) => i.as_index(v)?,
        None => match kwargs.iter().find(|(k, _)| k == "start") {
            Some((_, v)) => i.as_index(v)?,
            None => 0,
        },
    };
    let items = i.collect_iter(&args[0])?;
    let mut out = Vec::new();
    for (offset, it) in items.into_iter().enumerate() {
        out.push(Value::tuple(vec![Value::Int(start + offset as i64), it]));
    }
    let rc = Rc::new(RefCell::new(out));
    Ok(Value::Iterator(Rc::new(RefCell::new(IterKind::List(
        rc, 0,
    )))))
}

fn bi_zip(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.is_empty() {
        return i.err("TypeError", "zip() 至少需要 1 个参数");
    }
    let mut all: Vec<Vec<Value>> = Vec::new();
    for a in args {
        all.push(i.collect_iter(a)?);
    }
    let n = all.iter().map(|v| v.len()).min().unwrap_or(0);
    let mut out = Vec::new();
    for idx in 0..n {
        let row: Vec<Value> = all.iter().map(|v| v[idx].clone()).collect();
        out.push(Value::tuple(row));
    }
    let rc = Rc::new(RefCell::new(out));
    Ok(Value::Iterator(Rc::new(RefCell::new(IterKind::List(
        rc, 0,
    )))))
}

fn bi_map(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.len() < 2 {
        return i.err("TypeError", "map() 至少需要 2 个参数");
    }
    let f = args[0].clone();
    let mut all: Vec<Vec<Value>> = Vec::new();
    for a in &args[1..] {
        all.push(i.collect_iter(a)?);
    }
    let n = all.iter().map(|v| v.len()).min().unwrap_or(0);
    let mut out = Vec::new();
    for idx in 0..n {
        let row: Vec<Value> = all.iter().map(|v| v[idx].clone()).collect();
        out.push(i.call_value(&f, row, Vec::new())?);
    }
    let rc = Rc::new(RefCell::new(out));
    Ok(Value::Iterator(Rc::new(RefCell::new(IterKind::List(
        rc, 0,
    )))))
}

fn bi_filter(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.len() != 2 {
        return i.err("TypeError", "filter() 需要 2 个参数");
    }
    let items = i.collect_iter(&args[1])?;
    let mut out = Vec::new();
    for it in items {
        let keep = if args[0].is_none() {
            i.truthy(&it)?
        } else {
            let r = i.call_value(&args[0], vec![it.clone()], Vec::new())?;
            i.truthy(&r)?
        };
        if keep {
            out.push(it);
        }
    }
    let rc = Rc::new(RefCell::new(out));
    Ok(Value::Iterator(Rc::new(RefCell::new(IterKind::List(
        rc, 0,
    )))))
}

fn bi_all(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.len() != 1 {
        return i.err("TypeError", "all() 需要 1 个参数");
    }
    let items = i.collect_iter(&args[0])?;
    for it in items {
        if !i.truthy(&it)? {
            return Ok(Value::Bool(false));
        }
    }
    Ok(Value::Bool(true))
}

fn bi_any(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.len() != 1 {
        return i.err("TypeError", "any() 需要 1 个参数");
    }
    let items = i.collect_iter(&args[0])?;
    for it in items {
        if i.truthy(&it)? {
            return Ok(Value::Bool(true));
        }
    }
    Ok(Value::Bool(false))
}

fn bi_round(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.is_empty() {
        return i.err("TypeError", "round() 至少需要 1 个参数");
    }
    let ndigits = match args.get(1) {
        Some(Value::None) | None => None,
        Some(v) => Some(i.as_index(v)?),
    };
    let x = match &args[0] {
        Value::Int(n) => return Ok(Value::Int(*n)),
        Value::Bool(b) => return Ok(Value::Int(*b as i64)),
        Value::Float(f) => *f,
        Value::Instance(inst) => {
            if let Some(m) = i.find_class_attr(&inst.class, "__round__") {
                let mut a = vec![args[0].clone()];
                if let Some(n) = ndigits {
                    a.push(Value::Int(n));
                }
                return i.call_value(&m, a, Vec::new());
            }
            return i.err(
                "TypeError",
                format!("type {} doesn't define __round__ method", inst.class.name),
            );
        }
        other => {
            return i.err(
                "TypeError",
                format!("type {} doesn't define __round__ method", other.type_name()),
            )
        }
    };
    match ndigits {
        None => {
            // Rust 的 {:.*} 使用“四舍六入五成双”，与 Python 的 round 一致
            let s = format!("{:.0}", x);
            let v: f64 = s.parse().unwrap_or(x);
            Ok(Value::Int(v as i64))
        }
        Some(n) => {
            if n <= 0 {
                let factor = 10f64.powi(-(n as i32));
                let s = format!("{:.0}", x / factor);
                let v: f64 = s.parse().unwrap_or(x);
                return Ok(Value::Float(v * factor));
            }
            let s = format!("{:.*}", n as usize, x);
            Ok(Value::Float(s.parse().unwrap_or(x)))
        }
    }
}

fn bi_divmod(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.len() != 2 {
        return i.err(
            "TypeError",
            format!("divmod expected 2 arguments, got {}", args.len()),
        );
    }
    let q = i.binop(
        crate::lexer::Op::DoubleSlash,
        args[0].clone(),
        args[1].clone(),
    )?;
    let r = i.binop(crate::lexer::Op::Percent, args[0].clone(), args[1].clone())?;
    Ok(Value::tuple(vec![q, r]))
}

fn bi_pow(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.len() < 2 {
        return i.err("TypeError", "pow() 至少需要 2 个参数");
    }
    let mut v = i.binop(
        crate::lexer::Op::DoubleStar,
        args[0].clone(),
        args[1].clone(),
    )?;
    if let Some(m) = args.get(2) {
        v = i.binop(crate::lexer::Op::Percent, v, m.clone())?;
    }
    Ok(v)
}

fn bi_chr(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.len() != 1 {
        return i.err("TypeError", "chr() 需要 1 个参数");
    }
    let v = i.as_index(&args[0])?;
    match u32::try_from(v).ok().and_then(char::from_u32) {
        Some(c) => Ok(Value::str_from(c.to_string())),
        None => i.err("ValueError", "chr() arg not in range(0x110000)"),
    }
}

fn bi_ord(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.len() != 1 {
        return i.err("TypeError", "ord() 需要 1 个参数");
    }
    match &args[0] {
        Value::Str(s) => {
            let mut it = s.chars();
            match (it.next(), it.next()) {
                (Some(c), None) => Ok(Value::Int(c as i64)),
                (Some(_), Some(_)) => i.err(
                    "TypeError",
                    format!(
                        "ord() expected a character, but string of length {} found",
                        s.chars().count()
                    ),
                ),
                _ => i.err(
                    "TypeError",
                    "ord() expected a character, but string of length 0 found",
                ),
            }
        }
        other => i.err(
            "TypeError",
            format!(
                "ord() expected string of length 1, but {} found",
                other.type_name()
            ),
        ),
    }
}

fn bi_hex(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.len() != 1 {
        return i.err("TypeError", "hex() 需要 1 个参数");
    }
    let n = i.as_index(&args[0])?;
    Ok(Value::str_from(if n < 0 {
        format!("-0x{:x}", n.unsigned_abs())
    } else {
        format!("0x{:x}", n)
    }))
}

fn bi_oct(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.len() != 1 {
        return i.err("TypeError", "oct() 需要 1 个参数");
    }
    let n = i.as_index(&args[0])?;
    Ok(Value::str_from(if n < 0 {
        format!("-0o{:o}", n.unsigned_abs())
    } else {
        format!("0o{:o}", n)
    }))
}

fn bi_bin(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.len() != 1 {
        return i.err("TypeError", "bin() 需要 1 个参数");
    }
    let n = i.as_index(&args[0])?;
    Ok(Value::str_from(if n < 0 {
        format!("-0b{:b}", n.unsigned_abs())
    } else {
        format!("0b{:b}", n)
    }))
}

fn value_id(v: &Value) -> usize {
    match v {
        Value::List(l) => Rc::as_ptr(l) as *const u8 as usize,
        Value::Dict(d) => Rc::as_ptr(d) as *const u8 as usize,
        Value::Set(s) => Rc::as_ptr(s) as *const u8 as usize,
        Value::Tuple(t) => Rc::as_ptr(t) as *const u8 as usize,
        Value::Str(s) => Rc::as_ptr(s) as *const u8 as usize,
        Value::Func(f) => Rc::as_ptr(f) as *const u8 as usize,
        Value::Instance(i) => Rc::as_ptr(i) as *const u8 as usize,
        Value::Class(c) => Rc::as_ptr(c) as *const u8 as usize,
        Value::Module(m) => Rc::as_ptr(m) as *const u8 as usize,
        Value::Iterator(it) => Rc::as_ptr(it) as *const u8 as usize,
        Value::File(f) => Rc::as_ptr(f) as *const u8 as usize,
        Value::Range(r) => Rc::as_ptr(r) as *const u8 as usize,
        Value::None => 0,
        Value::Bool(b) => *b as usize + 1,
        Value::Int(i) => (*i as usize).wrapping_add(0x1000),
        Value::Float(f) => f.to_bits() as usize,
        Value::Native(n) => Rc::as_ptr(n) as *const u8 as usize,
        Value::BoundNative(b) => Rc::as_ptr(b) as *const u8 as usize,
        Value::BoundMethod(b) => Rc::as_ptr(b) as *const u8 as usize,
        Value::Slice(s) => Rc::as_ptr(s) as *const u8 as usize,
        Value::Descriptor(d) => Rc::as_ptr(d) as *const u8 as usize,
        Value::Property(p) => Rc::as_ptr(p) as *const u8 as usize,
        Value::Super(s) => Rc::as_ptr(s) as *const u8 as usize,
    }
}

fn bi_id(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.len() != 1 {
        return i.err("TypeError", "id() 需要 1 个参数");
    }
    Ok(Value::Int(value_id(&args[0]) as i64))
}

fn bi_hash(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.len() != 1 {
        return i.err("TypeError", "hash() 需要 1 个参数");
    }
    match &args[0] {
        Value::Int(n) => Ok(Value::Int(*n)),
        Value::Bool(b) => Ok(Value::Int(*b as i64)),
        Value::Float(f) => Ok(Value::Int(*f as i64)),
        Value::Str(s) => {
            let mut h: u64 = 1469598103934665603;
            for b in s.as_bytes() {
                h ^= *b as u64;
                h = h.wrapping_mul(1099511628211);
            }
            Ok(Value::Int(h as i64))
        }
        Value::None => Ok(Value::Int(0)),
        Value::Tuple(t) => {
            let mut h: u64 = 0;
            for x in t.iter() {
                h = h.wrapping_mul(31).wrapping_add(value_id(x) as u64);
            }
            Ok(Value::Int(h as i64))
        }
        Value::Instance(inst) => {
            if let Some(m) = i.find_class_attr(&inst.class, "__hash__") {
                let r = i.call_value(&m, vec![args[0].clone()], Vec::new())?;
                return i.as_index(&r).map(Value::Int);
            }
            if i.find_class_attr(&inst.class, "__eq__").is_some() {
                return i.err(
                    "TypeError",
                    format!("unhashable type: '{}'", inst.class.name),
                );
            }
            Ok(Value::Int(value_id(&args[0]) as i64))
        }
        other => i.err(
            "TypeError",
            format!("unhashable type: '{}'", other.type_name()),
        ),
    }
}

fn bi_callable(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.len() != 1 {
        return i.err("TypeError", "callable() 需要 1 个参数");
    }
    let _ = i;
    Ok(Value::Bool(args[0].is_callable()))
}

fn bi_getattr(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.len() < 2 || args.len() > 3 {
        return i.err("TypeError", "getattr() 需要 2~3 个参数");
    }
    let name = match &args[1] {
        Value::Str(s) => s.to_string(),
        other => {
            return i.err(
                "TypeError",
                format!("attribute name must be string, not '{}'", other.type_name()),
            )
        }
    };
    match i.get_attr(&args[0], &name) {
        Ok(v) => Ok(v),
        Err(Signal::Error(e)) => {
            if e.kind == "AttributeError" {
                match args.get(2) {
                    Some(d) => Ok(d.clone()),
                    None => Err(Signal::Error(e)),
                }
            } else {
                Err(Signal::Error(e))
            }
        }
        Err(sig) => Err(sig),
    }
}

fn bi_setattr(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.len() != 3 {
        return i.err("TypeError", "setattr() 需要 3 个参数");
    }
    let name = match &args[1] {
        Value::Str(s) => s.to_string(),
        other => {
            return i.err(
                "TypeError",
                format!("attribute name must be string, not '{}'", other.type_name()),
            )
        }
    };
    i.set_attr(&args[0], &name, args[2].clone())?;
    Ok(Value::None)
}

fn bi_hasattr(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.len() != 2 {
        return i.err("TypeError", "hasattr() 需要 2 个参数");
    }
    let name = match &args[1] {
        Value::Str(s) => s.to_string(),
        other => {
            return i.err(
                "TypeError",
                format!("attribute name must be string, not '{}'", other.type_name()),
            )
        }
    };
    Ok(Value::Bool(i.get_attr(&args[0], &name).is_ok()))
}

fn bi_delattr(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.len() != 2 {
        return i.err("TypeError", "delattr() 需要 2 个参数");
    }
    let name = match &args[1] {
        Value::Str(s) => s.to_string(),
        other => {
            return i.err(
                "TypeError",
                format!("attribute name must be string, not '{}'", other.type_name()),
            )
        }
    };
    i.del_attr(&args[0], &name)?;
    Ok(Value::None)
}

fn bi_isinstance(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.len() != 2 {
        return i.err("TypeError", "isinstance() 需要 2 个参数");
    }
    let r = i.is_instance_of(&args[0], &args[1])?;
    Ok(Value::Bool(r))
}

fn bi_issubclass(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.len() != 2 {
        return i.err("TypeError", "issubclass() 需要 2 个参数");
    }
    let sub = match &args[0] {
        Value::Class(c) => c.clone(),
        other => {
            return i.err(
                "TypeError",
                format!(
                    "issubclass() arg 1 must be a class, not {}",
                    other.type_name()
                ),
            )
        }
    };
    let ok = match &args[1] {
        Value::Class(c) => i.class_is_subclass(&sub, c),
        Value::Tuple(items) => items.iter().any(|it| match it {
            Value::Class(c) => i.class_is_subclass(&sub, c),
            _ => false,
        }),
        other => {
            return i.err(
                "TypeError",
                format!(
                    "issubclass() arg 2 must be a class or tuple, not {}",
                    other.type_name()
                ),
            )
        }
    };
    Ok(Value::Bool(ok))
}

fn bi_iter(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.len() != 1 {
        return i.err("TypeError", "iter() 需要 1 个参数");
    }
    i.get_iter(&args[0])
}

fn bi_next(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.is_empty() || args.len() > 2 {
        return i.err("TypeError", "next() 需要 1~2 个参数");
    }
    match i.iter_next(&args[0])? {
        Some(v) => Ok(v),
        None => match args.get(1) {
            Some(d) => Ok(d.clone()),
            None => i.err("StopIteration", ""),
        },
    }
}

fn bi_format(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.is_empty() || args.len() > 2 {
        return i.err("TypeError", "format() 需要 1~2 个参数");
    }
    let spec = match args.get(1) {
        Some(Value::Str(s)) => s.to_string(),
        Some(other) => {
            return i.err(
                "TypeError",
                format!(
                    "format() 的格式说明符必须是字符串，而不是 {}",
                    other.type_name()
                ),
            )
        }
        None => String::new(),
    };
    let s = i.format_with_spec(&args[0], &spec)?;
    Ok(Value::str_from(s))
}

fn bi_dir(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    let mut names: Vec<String> = Vec::new();
    if args.is_empty() {
        names = i.builtins.visible_names();
    } else {
        match &args[0] {
            Value::Module(m) => {
                names = m.dict.borrow().keys().cloned().collect();
            }
            Value::Instance(inst) => {
                names.extend(inst.dict.borrow().keys().cloned());
                for c in &inst.class.mro {
                    for k in c.dict.borrow().keys() {
                        names.push(k.clone());
                    }
                }
                names.extend(
                    crate::methods::method_names(&inst.class.name)
                        .iter()
                        .map(|s| s.to_string()),
                );
            }
            Value::Class(c) => {
                for x in &c.mro {
                    for k in x.dict.borrow().keys() {
                        names.push(k.clone());
                    }
                }
            }
            other => {
                names.extend(
                    crate::methods::method_names(&other.type_name())
                        .iter()
                        .map(|s| s.to_string()),
                );
            }
        }
    }
    names.sort();
    names.dedup();
    Ok(Value::list(
        names.into_iter().map(Value::str_from).collect(),
    ))
}

fn bi_globals(i: &mut Interp, _args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    let mut d = DictData::new();
    if let Some(env) = &i.main_env {
        for (k, v) in env.vars.borrow().iter() {
            d.insert(Value::str_from(k.clone()), v.clone());
        }
    }
    Ok(Value::Dict(Rc::new(RefCell::new(d))))
}

fn bi_vars(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.is_empty() {
        return bi_globals(i, args, &[]);
    }
    let mut d = DictData::new();
    match &args[0] {
        Value::Instance(inst) => {
            for (k, v) in inst.dict.borrow().iter() {
                d.insert(Value::str_from(k.clone()), v.clone());
            }
        }
        Value::Class(c) => {
            for (k, v) in c.dict.borrow().iter() {
                d.insert(Value::str_from(k.clone()), v.clone());
            }
        }
        Value::Module(m) => {
            for (k, v) in m.dict.borrow().iter() {
                d.insert(Value::str_from(k.clone()), v.clone());
            }
        }
        _ => return i.err("TypeError", "vars() argument must have __dict__ attribute"),
    }
    Ok(Value::Dict(Rc::new(RefCell::new(d))))
}

fn bi_staticmethod(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.len() != 1 {
        return i.err("TypeError", "staticmethod 需要 1 个参数");
    }
    Ok(Value::Descriptor(Rc::new(DescriptorData {
        kind: DescriptorKind::Static,
        func: args[0].clone(),
    })))
}

fn bi_classmethod(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.len() != 1 {
        return i.err("TypeError", "classmethod 需要 1 个参数");
    }
    Ok(Value::Descriptor(Rc::new(DescriptorData {
        kind: DescriptorKind::Class,
        func: args[0].clone(),
    })))
}

fn bi_property(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.is_empty() {
        return i.err("TypeError", "property() 至少需要 1 个参数");
    }
    Ok(Value::Property(Rc::new(PropertyData {
        name: "<property>".to_string(),
        getter: args[0].clone(),
        setter: args.get(1).filter(|v| !v.is_none()).cloned(),
        deleter: args.get(2).filter(|v| !v.is_none()).cloned(),
    })))
}

/// `super()` / `super(C, obj)`
fn bi_super(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    match args.len() {
        0 => match i.class_ctx.last() {
            Some((c, obj)) => Ok(Value::Super(Rc::new(SuperData {
                class: c.clone(),
                obj: obj.clone(),
            }))),
            None => i.err(
                "RuntimeError",
                "super(): no arguments（只能在方法内部使用零参 super()）",
            ),
        },
        2 => {
            let c = match &args[0] {
                Value::Class(c) => c.clone(),
                other => {
                    return i.err(
                        "TypeError",
                        format!("super() 的第一个参数必须是类，而不是 {}", other.type_name()),
                    )
                }
            };
            Ok(Value::Super(Rc::new(SuperData {
                class: c,
                obj: args[1].clone(),
            })))
        }
        _ => i.err("TypeError", "super() 需要 0 或 2 个参数"),
    }
}

fn bi_exit(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    let code = match args.first() {
        Some(Value::Int(n)) => *n,
        Some(Value::None) | None => 0,
        Some(Value::Str(s)) => {
            let _ = s;
            1
        }
        Some(_) => 1,
    };
    let mut e = i.pyerr("SystemExit", String::new());
    e.value = Some(Value::Int(code));
    Err(Signal::Error(e))
}

fn bi_input(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if let Some(p) = args.first() {
        let text = i.value_str(p)?;
        let _ = i.out.write_all(text.as_bytes());
        let _ = i.out.flush();
    }
    let mut line = String::new();
    match std::io::stdin().read_line(&mut line) {
        Ok(0) => Err(Signal::Error(
            i.pyerr("EOFError", "EOF when reading a line"),
        )),
        Ok(_) => {
            while line.ends_with('\n') || line.ends_with('\r') {
                line.pop();
            }
            Ok(Value::str_from(line))
        }
        Err(e) => i.err("OSError", e.to_string()),
    }
}

fn bi_open(i: &mut Interp, args: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if args.is_empty() {
        return i.err("TypeError", "open() 至少需要 1 个参数");
    }
    let name = match &args[0] {
        Value::Str(s) => s.to_string(),
        other => {
            return i.err(
                "TypeError",
                format!("open() 的文件名必须是字符串，而不是 {}", other.type_name()),
            )
        }
    };
    let mode = match args.get(1) {
        Some(Value::Str(s)) => s.to_string(),
        Some(other) => {
            return i.err(
                "TypeError",
                format!("open() 的模式必须是字符串，而不是 {}", other.type_name()),
            )
        }
        None => "r".to_string(),
    };
    let path = std::path::Path::new(&name);
    let file = if mode.starts_with('r') {
        match std::fs::File::open(path) {
            Ok(f) => f,
            Err(e) => {
                let kind = if e.kind() == std::io::ErrorKind::NotFound {
                    "FileNotFoundError"
                } else {
                    "OSError"
                };
                return i.err(
                    kind,
                    format!("[Errno 2] No such file or directory: '{}'", name),
                );
            }
        }
    } else if mode.starts_with('w') {
        match std::fs::File::create(path) {
            Ok(f) => f,
            Err(e) => return i.err("OSError", e.to_string()),
        }
    } else if mode.starts_with('a') {
        match std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
        {
            Ok(f) => f,
            Err(e) => return i.err("OSError", e.to_string()),
        }
    } else {
        return i.err("ValueError", format!("不支持的打开模式 '{}'", mode));
    };
    let buf = if mode.starts_with('r') {
        FileBuf::Read(std::io::BufReader::new(file))
    } else {
        FileBuf::Write(std::io::BufWriter::new(file))
    };
    Ok(Value::File(Rc::new(RefCell::new(FileData {
        name,
        mode,
        buf,
        closed: false,
    }))))
}
