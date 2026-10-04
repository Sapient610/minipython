//! 内置模块（math / random / sys / string）与模块导入。

use crate::env::Env;
use crate::error::TraceFrame;
use crate::interp::{EResult, Interp, Signal};
use crate::parser::Parser;
use crate::value::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

fn m(i: &Interp, args: &[Value], n: usize, name: &str) -> Result<(), Signal> {
    if args.len() != n {
        return Err(Signal::Error(
            i.pyerr("TypeError", format!("{}() 需要 {} 个参数", name, n)),
        ));
    }
    Ok(())
}

fn to_f(i: &mut Interp, v: &Value, name: &str) -> EResult<f64> {
    match v.as_number() {
        Some(n) => Ok(n.as_f64()),
        None => i.err(
            "TypeError",
            format!("{}() 的参数必须是数字，而不是 {}", name, v.type_name()),
        ),
    }
}

fn to_i(i: &mut Interp, v: &Value, name: &str) -> EResult<i64> {
    match v {
        Value::Int(n) => Ok(*n),
        Value::Bool(b) => Ok(*b as i64),
        Value::Float(f) => Ok(*f as i64),
        other => i.err(
            "TypeError",
            format!("{}() 的参数必须是整数，而不是 {}", name, other.type_name()),
        ),
    }
}

// ---------------- math ----------------

fn math_sqrt(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 1, "sqrt")?;
    let x = to_f(i, &a[0], "sqrt")?;
    if x < 0.0 {
        return i.err("ValueError", "math domain error");
    }
    Ok(Value::Float(x.sqrt()))
}

fn math_floor(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 1, "floor")?;
    let x = to_f(i, &a[0], "floor")?;
    Ok(Value::Int(x.floor() as i64))
}

fn math_ceil(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 1, "ceil")?;
    let x = to_f(i, &a[0], "ceil")?;
    Ok(Value::Int(x.ceil() as i64))
}

fn math_trunc(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 1, "trunc")?;
    let x = to_f(i, &a[0], "trunc")?;
    Ok(Value::Int(x.trunc() as i64))
}

fn math_fabs(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 1, "fabs")?;
    Ok(Value::Float(to_f(i, &a[0], "fabs")?.abs()))
}

fn math_factorial(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 1, "factorial")?;
    let n = to_i(i, &a[0], "factorial")?;
    if n < 0 {
        return i.err("ValueError", "factorial() not defined for negative values");
    }
    let mut acc: i64 = 1;
    for k in 2..=n {
        acc = match acc.checked_mul(k) {
            Some(v) => v,
            None => return Ok(Value::Float(factorial_f64(n))),
        };
    }
    Ok(Value::Int(acc))
}

fn factorial_f64(n: i64) -> f64 {
    let mut acc = 1.0f64;
    for k in 2..=n {
        acc *= k as f64;
    }
    acc
}

fn math_gcd(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    let mut g: i64 = 0;
    for v in a {
        let n = to_i(i, v, "gcd")?.abs();
        g = gcd(g, n);
    }
    Ok(Value::Int(g))
}

fn gcd(mut a: i64, mut b: i64) -> i64 {
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a.abs()
}

fn math_pow(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 2, "pow")?;
    let x = to_f(i, &a[0], "pow")?;
    let y = to_f(i, &a[1], "pow")?;
    Ok(Value::Float(x.powf(y)))
}

fn math_exp(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 1, "exp")?;
    Ok(Value::Float(to_f(i, &a[0], "exp")?.exp()))
}

fn math_log(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if a.is_empty() || a.len() > 2 {
        return i.err("TypeError", "log() 需要 1~2 个参数");
    }
    let x = to_f(i, &a[0], "log")?;
    if x <= 0.0 {
        return i.err("ValueError", "math domain error");
    }
    match a.get(1) {
        Some(b) => {
            let base = to_f(i, b, "log")?;
            Ok(Value::Float(x.log(base)))
        }
        None => Ok(Value::Float(x.ln())),
    }
}

fn math_log2(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 1, "log2")?;
    let x = to_f(i, &a[0], "log2")?;
    if x <= 0.0 {
        return i.err("ValueError", "math domain error");
    }
    Ok(Value::Float(x.log2()))
}

fn math_log10(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 1, "log10")?;
    let x = to_f(i, &a[0], "log10")?;
    if x <= 0.0 {
        return i.err("ValueError", "math domain error");
    }
    Ok(Value::Float(x.log10()))
}

macro_rules! math_unary {
    ($fname:ident, $name:literal, $method:ident) => {
        fn $fname(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
            m(i, a, 1, $name)?;
            Ok(Value::Float(to_f(i, &a[0], $name)?.$method()))
        }
    };
}

math_unary!(math_sin, "sin", sin);
math_unary!(math_cos, "cos", cos);
math_unary!(math_tan, "tan", tan);
math_unary!(math_asin, "asin", asin);
math_unary!(math_acos, "acos", acos);
math_unary!(math_atan, "atan", atan);
math_unary!(math_sinh, "sinh", sinh);
math_unary!(math_cosh, "cosh", cosh);
math_unary!(math_tanh, "tanh", tanh);

fn math_atan2(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 2, "atan2")?;
    let y = to_f(i, &a[0], "atan2")?;
    let x = to_f(i, &a[1], "atan2")?;
    Ok(Value::Float(y.atan2(x)))
}

fn math_hypot(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    let mut sum = 0.0;
    for v in a {
        let x = to_f(i, v, "hypot")?;
        sum += x * x;
    }
    Ok(Value::Float(sum.sqrt()))
}

fn math_degrees(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 1, "degrees")?;
    Ok(Value::Float(to_f(i, &a[0], "degrees")?.to_degrees()))
}

fn math_radians(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 1, "radians")?;
    Ok(Value::Float(to_f(i, &a[0], "radians")?.to_radians()))
}

fn math_isnan(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 1, "isnan")?;
    Ok(Value::Bool(to_f(i, &a[0], "isnan")?.is_nan()))
}

fn math_isinf(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 1, "isinf")?;
    Ok(Value::Bool(to_f(i, &a[0], "isinf")?.is_infinite()))
}

fn math_isfinite(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 1, "isfinite")?;
    Ok(Value::Bool(to_f(i, &a[0], "isfinite")?.is_finite()))
}

fn math_fmod(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 2, "fmod")?;
    let x = to_f(i, &a[0], "fmod")?;
    let y = to_f(i, &a[1], "fmod")?;
    Ok(Value::Float(x % y))
}

fn math_copysign(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 2, "copysign")?;
    let x = to_f(i, &a[0], "copysign")?;
    let y = to_f(i, &a[1], "copysign")?;
    Ok(Value::Float(
        x.abs() * if y.is_sign_negative() { -1.0 } else { 1.0 },
    ))
}

fn math_fsum(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 1, "fsum")?;
    let items = i.collect_iter(&a[0])?;
    let mut sum = 0.0;
    for it in items {
        sum += to_f(i, &it, "fsum")?;
    }
    Ok(Value::Float(sum))
}

// ---------------- random ----------------

fn next_rand(i: &mut Interp) -> f64 {
    let mut x = i.rng.get();
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    i.rng.set(x);
    // 取 53 位作为 [0,1) 的浮点数
    let bits = x >> 11;
    (bits as f64) / ((1u64 << 53) as f64)
}

fn random_random(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 0, "random")?;
    Ok(Value::Float(next_rand(i)))
}

fn random_seed(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    let seed = match a.first() {
        Some(Value::Int(n)) => *n as u64,
        Some(Value::Float(f)) => f.to_bits(),
        Some(Value::Str(s)) => {
            let mut h: u64 = 1469598103934665603;
            for b in s.as_bytes() {
                h ^= *b as u64;
                h = h.wrapping_mul(1099511628211);
            }
            h
        }
        _ => 0x2545F4914F6CDD1D,
    };
    i.rng.set(if seed == 0 { 0x9E3779B97F4A7C15 } else { seed });
    Ok(Value::None)
}

fn random_randint(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 2, "randint")?;
    let lo = to_i(i, &a[0], "randint")?;
    let hi = to_i(i, &a[1], "randint")?;
    if lo > hi {
        return i.err("ValueError", "empty range for randrange()");
    }
    let span = (hi - lo + 1) as f64;
    let v = lo + (next_rand(i) * span) as i64;
    Ok(Value::Int(v.min(hi)))
}

fn random_randrange(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if a.is_empty() || a.len() > 3 {
        return i.err("TypeError", "randrange() 需要 1~3 个参数");
    }
    let (start, stop, step) = match a.len() {
        1 => (0, to_i(i, &a[0], "randrange")?, 1),
        2 => (
            to_i(i, &a[0], "randrange")?,
            to_i(i, &a[1], "randrange")?,
            1,
        ),
        _ => (
            to_i(i, &a[0], "randrange")?,
            to_i(i, &a[1], "randrange")?,
            to_i(i, &a[2], "randrange")?,
        ),
    };
    if step == 0 {
        return i.err("ValueError", "range() arg 3 must not be zero");
    }
    let n = if step > 0 {
        ((stop - start) + step - 1) / step
    } else {
        ((start - stop) - step - 1) / (-step)
    };
    if n <= 0 {
        return i.err("ValueError", "empty range for randrange()");
    }
    let k = (next_rand(i) * n as f64) as i64;
    Ok(Value::Int(start + k.min(n - 1) * step))
}

fn random_uniform(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 2, "uniform")?;
    let lo = to_f(i, &a[0], "uniform")?;
    let hi = to_f(i, &a[1], "uniform")?;
    Ok(Value::Float(lo + next_rand(i) * (hi - lo)))
}

fn random_choice(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 1, "choice")?;
    let items = i.collect_iter(&a[0])?;
    if items.is_empty() {
        return i.err("IndexError", "Cannot choose from an empty sequence");
    }
    let k = (next_rand(i) * items.len() as f64) as usize;
    Ok(items[k.min(items.len() - 1)].clone())
}

fn random_shuffle(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 1, "shuffle")?;
    let l = match &a[0] {
        Value::List(l) => l.clone(),
        other => {
            return i.err(
                "TypeError",
                format!("shuffle() 需要 list，而不是 {}", other.type_name()),
            )
        }
    };
    let n = l.borrow().len();
    for idx in (1..n).rev() {
        let j = (next_rand(i) * (idx + 1) as f64) as usize;
        let j = j.min(idx);
        l.borrow_mut().swap(idx, j);
    }
    Ok(Value::None)
}

// ---------------- sys ----------------

fn sys_exit(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    let code = match a.first() {
        Some(Value::Int(n)) => *n,
        Some(Value::None) | None => 0,
        Some(_) => 1,
    };
    let mut e = i.pyerr("SystemExit", String::new());
    e.value = Some(Value::Int(code));
    Err(Signal::Error(e))
}

// ---------------- 模块注册 ----------------

fn module_from(name: &str, funcs: Vec<(&str, NativeFn)>, consts: Vec<(&str, Value)>) -> Value {
    let mut d = HashMap::new();
    for (n, f) in funcs {
        d.insert(
            n.to_string(),
            Value::Native(Rc::new(NativeData {
                name: format!("{}.{}", name, n),
                func: f,
            })),
        );
    }
    for (n, v) in consts {
        d.insert(n.to_string(), v);
    }
    Value::Module(Rc::new(ModuleData {
        name: name.to_string(),
        dict: RefCell::new(d),
    }))
}

/// 注册所有内置模块（惰性创建，这里仅注册表项）。
pub fn register_builtin_modules(_i: &mut Interp) {}

/// 内置模块名。
pub fn is_builtin_module(name: &str) -> bool {
    matches!(name, "math" | "random" | "sys" | "string")
}

/// 创建内置模块。
pub fn create_builtin_module(name: &str) -> Option<Value> {
    Some(match name {
        "math" => module_from(
            "math",
            vec![
                ("sqrt", math_sqrt as NativeFn),
                ("floor", math_floor),
                ("ceil", math_ceil),
                ("trunc", math_trunc),
                ("fabs", math_fabs),
                ("factorial", math_factorial),
                ("gcd", math_gcd),
                ("pow", math_pow),
                ("exp", math_exp),
                ("log", math_log),
                ("log2", math_log2),
                ("log10", math_log10),
                ("sin", math_sin),
                ("cos", math_cos),
                ("tan", math_tan),
                ("asin", math_asin),
                ("acos", math_acos),
                ("atan", math_atan),
                ("atan2", math_atan2),
                ("sinh", math_sinh),
                ("cosh", math_cosh),
                ("tanh", math_tanh),
                ("hypot", math_hypot),
                ("degrees", math_degrees),
                ("radians", math_radians),
                ("isnan", math_isnan),
                ("isinf", math_isinf),
                ("isfinite", math_isfinite),
                ("fmod", math_fmod),
                ("copysign", math_copysign),
                ("fsum", math_fsum),
            ],
            vec![
                ("pi", Value::Float(std::f64::consts::PI)),
                ("e", Value::Float(std::f64::consts::E)),
                ("tau", Value::Float(std::f64::consts::TAU)),
                ("inf", Value::Float(f64::INFINITY)),
                ("nan", Value::Float(f64::NAN)),
            ],
        ),
        "random" => module_from(
            "random",
            vec![
                ("random", random_random as NativeFn),
                ("seed", random_seed),
                ("randint", random_randint),
                ("randrange", random_randrange),
                ("uniform", random_uniform),
                ("choice", random_choice),
                ("shuffle", random_shuffle),
            ],
            vec![],
        ),
        "string" => module_from(
            "string",
            vec![],
            vec![
                (
                    "ascii_lowercase",
                    Value::str_from("abcdefghijklmnopqrstuvwxyz"),
                ),
                (
                    "ascii_uppercase",
                    Value::str_from("ABCDEFGHIJKLMNOPQRSTUVWXYZ"),
                ),
                (
                    "ascii_letters",
                    Value::str_from("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"),
                ),
                ("digits", Value::str_from("0123456789")),
                ("hexdigits", Value::str_from("0123456789abcdefABCDEF")),
                ("octdigits", Value::str_from("01234567")),
                (
                    "punctuation",
                    Value::str_from("!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~"),
                ),
                ("whitespace", Value::str_from(" \t\n\r\x0b\x0c")),
                (
                    "printable",
                    Value::str_from(
                        "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~ \t\n\r\x0b\x0c",
                    ),
                ),
            ],
        ),
        "sys" => {
            let mut v = module_from("sys", vec![("exit", sys_exit as NativeFn)], vec![]);
            if let Value::Module(md) = &mut v {
                let mut d = md.dict.borrow_mut();
                d.insert("version".to_string(), Value::str_from("3.13.0 (minipython 0.1.0)"));
                d.insert("version_info".to_string(), Value::tuple(vec![
                    Value::Int(3),
                    Value::Int(13),
                    Value::Int(0),
                ]));
                d.insert("platform".to_string(), Value::str_from(std::env::consts::OS));
                d.insert("maxsize".to_string(), Value::Int(i64::MAX));
            }
            v
        }
        _ => return None,
    })
}

impl Interp {
    /// 导入模块：支持内置模块与同目录下的 .py 文件。
    pub fn import_module(&mut self, name: &str) -> EResult<Value> {
        if let Some(m) = self.modules.get(name) {
            return Ok(Value::Module(m.clone()));
        }
        // 点分模块：先导入父模块
        if let Some((parent, child)) = name.rsplit_once('.') {
            let p = self.import_module(parent)?;
            let full = self.import_module_child(parent, child)?;
            if let Value::Module(pm) = &p {
                pm.dict.borrow_mut().insert(child.to_string(), full.clone());
            }
            return Ok(full);
        }
        if is_builtin_module(name) {
            let v = create_builtin_module(name).expect("内置模块必须存在");
            if let Value::Module(md) = &v {
                self.modules.insert(name.to_string(), md.clone());
                if name == "sys" {
                    md.dict.borrow_mut().insert(
                        "argv".to_string(),
                        Value::list(
                            self.argv
                                .iter()
                                .map(|a| Value::str_from(a.clone()))
                                .collect(),
                        ),
                    );
                }
            }
            return Ok(v);
        }
        // 从文件加载
        let path = self.find_module_file(name);
        let path = match path {
            Some(p) => p,
            None => return self.err("ModuleNotFoundError", format!("No module named '{}'", name)),
        };
        let src = std::fs::read_to_string(&path)
            .map_err(|e| Signal::Error(self.pyerr("OSError", e.to_string())))?;
        let file = path.to_string_lossy().to_string();
        let stmts = Parser::parse_source(&src).map_err(|e| {
            Signal::Error(self.pyerr("SyntaxError", format!("{} ({}:{})", e.msg, file, e.line)))
        })?;
        self.set_source(&file, &src);
        let env = Env::new_module();
        env.define("__name__", Value::str_from(name.to_string()));
        env.define("__file__", Value::str_from(file.clone()));
        let placeholder = Rc::new(ModuleData {
            name: name.to_string(),
            dict: RefCell::new(HashMap::new()),
        });
        self.modules.insert(name.to_string(), placeholder.clone());
        let old_module_name = std::mem::replace(&mut self.module_name, name.to_string());
        let old_filename = std::mem::replace(&mut self.filename, file.clone());
        let old_main = self.main_env.take();
        let prev_depth = self.depth;
        self.depth = 0;
        self.frames.push(TraceFrame {
            file: file.clone(),
            line: 0,
            func: "<module>".to_string(),
        });
        let r = self.exec_block(&stmts, &env);
        self.frames.pop();
        self.depth = prev_depth;
        self.main_env = old_main;
        self.filename = old_filename;
        self.module_name = old_module_name;
        if let Err(sig) = r {
            self.modules.remove(name);
            return Err(sig);
        }
        {
            let mut d = placeholder.dict.borrow_mut();
            for (k, v) in env.vars.borrow().iter() {
                d.insert(k.clone(), v.clone());
            }
        }
        Ok(Value::Module(placeholder))
    }

    fn import_module_child(&mut self, parent: &str, child: &str) -> EResult<Value> {
        let full = format!("{}.{}", parent, child);
        self.import_module(&full)
    }

    /// 在搜索路径中查找模块文件。
    fn find_module_file(&self, name: &str) -> Option<PathBuf> {
        let rel: PathBuf = name.split('.').collect();
        for dir in &self.search_path {
            let py = dir.join(format!("{}.py", rel.to_string_lossy()));
            if py.is_file() {
                return Some(py);
            }
            let pkg = dir.join(&rel).join("__init__.py");
            if pkg.is_file() {
                return Some(pkg);
            }
        }
        None
    }
}

/// 供 `Interp` 持有的随机数状态（放在这里便于模块实现）。
pub struct RngState(pub Cell<u64>);
