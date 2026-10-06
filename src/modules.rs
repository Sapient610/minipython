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

fn math_prod(i: &mut Interp, a: &[Value], kwargs: &[(String, Value)]) -> EResult<Value> {
    if a.is_empty() {
        return i.err("TypeError", "prod() 至少需要 1 个参数");
    }
    let start = match a
        .get(1)
        .or_else(|| kwargs.iter().find(|(k, _)| k == "start").map(|(_, v)| v))
    {
        Some(v) => v.clone(),
        None => Value::Int(1),
    };
    let items = i.collect_iter(&a[0])?;
    let mut acc = start;
    for it in items {
        acc = i.binop(crate::lexer::Op::Star, acc, it)?;
    }
    Ok(acc)
}

fn math_isqrt(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 1, "isqrt")?;
    let n = match &a[0] {
        Value::Int(v) => *v,
        Value::Bool(b) => *b as i64,
        other => {
            return i.err(
                "TypeError",
                format!(
                    "isqrt() argument must be integer, not {}",
                    other.type_name()
                ),
            )
        }
    };
    if n < 0 {
        return i.err("ValueError", "isqrt() argument must be nonnegative");
    }
    // 牛顿迭代求整数平方根
    if n < 2 {
        return Ok(Value::Int(n));
    }
    let mut x = (n as f64).sqrt() as i64;
    while (x + 1).checked_mul(x + 1).map(|v| v <= n).unwrap_or(false) {
        x += 1;
    }
    while x.checked_mul(x).map(|v| v > n).unwrap_or(true) {
        x -= 1;
    }
    Ok(Value::Int(x))
}

fn math_comb(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 2, "comb")?;
    let n = to_i(i, &a[0], "comb")?;
    let k = to_i(i, &a[1], "comb")?;
    if n < 0 || k < 0 {
        return i.err("ValueError", "comb() 的参数不能为负数");
    }
    if k > n {
        return Ok(Value::Int(0));
    }
    let k = k.min(n - k);
    let mut result: i64 = 1;
    for idx in 0..k {
        match result.checked_mul(n - idx) {
            Some(v) => match v.checked_div(idx + 1) {
                Some(v2) => result = v2,
                None => return Ok(Value::Float(comb_f64(n, k))),
            },
            None => return Ok(Value::Float(comb_f64(n, k))),
        }
    }
    Ok(Value::Int(result))
}

fn comb_f64(n: i64, k: i64) -> f64 {
    let mut r = 1.0f64;
    for idx in 0..k {
        r = r * (n - idx) as f64 / (idx + 1) as f64;
    }
    r.round()
}

fn math_perm(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if a.is_empty() || a.len() > 2 {
        return i.err("TypeError", "perm() 需要 1~2 个参数");
    }
    let n = to_i(i, &a[0], "perm")?;
    let k = match a.get(1) {
        Some(v) => to_i(i, v, "perm")?,
        None => n,
    };
    if n < 0 || k < 0 {
        return i.err("ValueError", "perm() 的参数不能为负数");
    }
    if k > n {
        return Ok(Value::Int(0));
    }
    let mut result: i64 = 1;
    for idx in 0..k {
        match result.checked_mul(n - idx) {
            Some(v) => result = v,
            None => {
                let mut r = 1.0f64;
                for j in 0..k {
                    r *= (n - j) as f64;
                }
                return Ok(Value::Float(r));
            }
        }
    }
    Ok(Value::Int(result))
}

fn math_lcm(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    let mut result: i64 = 1;
    for v in a {
        let n = to_i(i, v, "lcm")?.abs();
        if n == 0 {
            return Ok(Value::Int(0));
        }
        let g = gcd(result, n);
        result = match result.checked_div(g).and_then(|q| q.checked_mul(n)) {
            Some(v) => v,
            None => {
                let mut r = result as f64 / g as f64 * n as f64;
                r = r.abs();
                return Ok(Value::Float(r));
            }
        };
    }
    Ok(Value::Int(result))
}

fn math_dist(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 2, "dist")?;
    let p = i.collect_iter(&a[0])?;
    let q = i.collect_iter(&a[1])?;
    if p.len() != q.len() {
        return i.err("ValueError", "dist() 的两个点维度必须相同");
    }
    let mut sum = 0.0;
    for (x, y) in p.iter().zip(q.iter()) {
        let d = to_f(i, x, "dist")? - to_f(i, y, "dist")?;
        sum += d * d;
    }
    Ok(Value::Float(sum.sqrt()))
}

fn math_modf(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 1, "modf")?;
    let x = to_f(i, &a[0], "modf")?;
    let int_part = x.trunc();
    Ok(Value::tuple(vec![
        Value::Float(x - int_part),
        Value::Float(int_part),
    ]))
}

fn math_frexp(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 1, "frexp")?;
    let x = to_f(i, &a[0], "frexp")?;
    if x == 0.0 || !x.is_finite() {
        return Ok(Value::tuple(vec![Value::Float(x), Value::Int(0)]));
    }
    let bits = x.to_bits();
    let exp_bits = ((bits >> 52) & 0x7ff) as i64;
    let (m, e) = if exp_bits == 0 {
        // 次正规数
        let scaled = x * 2f64.powi(64);
        let b = scaled.to_bits();
        let eb = ((b >> 52) & 0x7ff) as i64;
        let mant = f64::from_bits((b & !(0x7ffu64 << 52)) | (1022u64 << 52));
        (mant, eb - 1022 - 64)
    } else {
        let mant = f64::from_bits((bits & !(0x7ffu64 << 52)) | (1022u64 << 52));
        (mant, exp_bits - 1022)
    };
    Ok(Value::tuple(vec![Value::Float(m), Value::Int(e)]))
}

fn math_ldexp(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 2, "ldexp")?;
    let x = to_f(i, &a[0], "ldexp")?;
    let e = to_i(i, &a[1], "ldexp")?;
    Ok(Value::Float(ldexp(x, e)))
}

fn ldexp(x: f64, e: i64) -> f64 {
    let mut result = x;
    let mut remaining = e;
    while remaining > 1000 {
        result *= 2f64.powi(1000);
        remaining -= 1000;
    }
    while remaining < -1000 {
        result *= 2f64.powi(-1000);
        remaining += 1000;
    }
    result * 2f64.powi(remaining as i32)
}

fn math_isclose(i: &mut Interp, a: &[Value], kwargs: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 2, "isclose")?;
    let x = to_f(i, &a[0], "isclose")?;
    let y = to_f(i, &a[1], "isclose")?;
    let rel_tol = match kwargs.iter().find(|(k, _)| k == "rel_tol") {
        Some((_, v)) => to_f(i, v, "isclose")?,
        None => 1e-9,
    };
    let abs_tol = match kwargs.iter().find(|(k, _)| k == "abs_tol") {
        Some((_, v)) => to_f(i, v, "isclose")?,
        None => 0.0,
    };
    if x == y {
        return Ok(Value::Bool(true));
    }
    if x.is_nan() || y.is_nan() {
        return Ok(Value::Bool(false));
    }
    if x.is_infinite() || y.is_infinite() {
        return Ok(Value::Bool(false));
    }
    let diff = (x - y).abs();
    Ok(Value::Bool(
        diff <= (rel_tol * x.abs().max(y.abs())).max(abs_tol),
    ))
}

fn math_remainder(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 2, "remainder")?;
    let x = to_f(i, &a[0], "remainder")?;
    let y = to_f(i, &a[1], "remainder")?;
    if y == 0.0 {
        return i.err("ValueError", "math domain error");
    }
    if x.is_nan() || y.is_nan() || x.is_infinite() {
        return Ok(Value::Float(f64::NAN));
    }
    if y.is_infinite() {
        return Ok(Value::Float(x));
    }
    // IEEE remainder：x - n*y，n 为 x/y 四舍五入到最近偶数
    let q = x / y;
    let n = round_half_even_f64(q);
    let mut r = x - n * y;
    if r == 0.0 {
        r = x.copysign(y) * 0.0;
    }
    Ok(Value::Float(r))
}

fn round_half_even_f64(x: f64) -> f64 {
    let r = format!("{:.0}", x);
    r.parse::<f64>().unwrap_or(x)
}

fn math_nextafter(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 2, "nextafter")?;
    let x = to_f(i, &a[0], "nextafter")?;
    let y = to_f(i, &a[1], "nextafter")?;
    Ok(Value::Float(nextafter(x, y)))
}

fn nextafter(x: f64, y: f64) -> f64 {
    if x.is_nan() || y.is_nan() {
        return f64::NAN;
    }
    if x == y {
        return y;
    }
    if x == 0.0 {
        let tiny = f64::from_bits(1);
        return if y > 0.0 { tiny } else { -tiny };
    }
    let bits = x.to_bits();
    let go_up = (y > x) == (x > 0.0);
    f64::from_bits(if go_up { bits + 1 } else { bits - 1 })
}

fn math_ulp(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 1, "ulp")?;
    let x = to_f(i, &a[0], "ulp")?;
    if !x.is_finite() {
        return Ok(Value::Float(f64::INFINITY));
    }
    let ax = x.abs();
    Ok(Value::Float(nextafter(ax, f64::INFINITY) - ax))
}

fn math_cbrt(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 1, "cbrt")?;
    Ok(Value::Float(to_f(i, &a[0], "cbrt")?.cbrt()))
}

fn math_expm1(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 1, "expm1")?;
    Ok(Value::Float(to_f(i, &a[0], "expm1")?.exp_m1()))
}

fn math_log1p(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 1, "log1p")?;
    let x = to_f(i, &a[0], "log1p")?;
    if x <= -1.0 {
        return i.err("ValueError", "math domain error");
    }
    Ok(Value::Float(x.ln_1p()))
}

fn math_exp2(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    m(i, a, 1, "exp2")?;
    Ok(Value::Float(to_f(i, &a[0], "exp2")?.exp2()))
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
                ("prod", math_prod),
                ("isqrt", math_isqrt),
                ("comb", math_comb),
                ("perm", math_perm),
                ("lcm", math_lcm),
                ("dist", math_dist),
                ("modf", math_modf),
                ("frexp", math_frexp),
                ("ldexp", math_ldexp),
                ("isclose", math_isclose),
                ("remainder", math_remainder),
                ("nextafter", math_nextafter),
                ("ulp", math_ulp),
                ("cbrt", math_cbrt),
                ("expm1", math_expm1),
                ("log1p", math_log1p),
                ("exp2", math_exp2),
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
        // 点分模块：先导入父模块，再把子模块挂到父模块上
        if let Some((parent, child)) = name.rsplit_once('.') {
            let p = self.import_module(parent)?;
            // 导入父包的过程中可能已经顺带加载了子模块
            let full = match self.modules.get(name) {
                Some(m) => Value::Module(m.clone()),
                None => self.load_module_from_file(name)?,
            };
            if let Value::Module(pm) = &p {
                pm.dict.borrow_mut().insert(child.to_string(), full.clone());
            }
            return Ok(full);
        }
        self.load_module_from_file(name)
    }

    /// 从磁盘加载模块文件（不处理点分前缀）。
    fn load_module_from_file(&mut self, name: &str) -> EResult<Value> {
        if let Some(m) = self.modules.get(name) {
            return Ok(Value::Module(m.clone()));
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
        let is_package = path
            .file_name()
            .map(|f| f == "__init__.py")
            .unwrap_or(false);
        let package = if is_package {
            name.to_string()
        } else {
            match name.rsplit_once('.') {
                Some((p, _)) => p.to_string(),
                None => String::new(),
            }
        };
        let env = Env::new_module();
        env.define("__name__", Value::str_from(name.to_string()));
        env.define("__package__", Value::str_from(package));
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
