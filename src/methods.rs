//! 内置类型的方法表（str / list / dict / set / tuple / int / float / range / file / iterator）。

use crate::interp::{EResult, Interp, Signal};
use crate::value::*;
use std::cell::RefCell;
use std::io::{BufRead, Write};
use std::rc::Rc;

type Nf = NativeFn;

// ---------------- 辅助 ----------------

fn need(interp: &Interp, args: &[Value], name: &str, n: usize) -> Result<(), Signal> {
    if args.len() != n {
        return Err(Signal::Error(interp.pyerr(
            "TypeError",
            format!(
                "{}{} 需要 {} 个参数，收到 {} 个",
                name,
                "()",
                n - 1,
                args.len() - 1
            ),
        )));
    }
    Ok(())
}

fn need_between(
    interp: &Interp,
    args: &[Value],
    name: &str,
    lo: usize,
    hi: usize,
) -> Result<(), Signal> {
    if args.len() < lo || args.len() > hi {
        return Err(Signal::Error(interp.pyerr(
            "TypeError",
            format!(
                "{}() 需要 {}~{} 个参数，收到 {} 个",
                name,
                lo - 1,
                hi - 1,
                args.len() - 1
            ),
        )));
    }
    Ok(())
}

fn as_str(v: &Value) -> &str {
    match v {
        Value::Str(s) => s,
        _ => unreachable!(),
    }
}

fn as_list(v: &Value) -> Rc<RefCell<Vec<Value>>> {
    match v {
        Value::List(l) => l.clone(),
        _ => unreachable!(),
    }
}

fn as_dict(v: &Value) -> Rc<RefCell<DictData>> {
    match v {
        Value::Dict(d) => d.clone(),
        _ => unreachable!(),
    }
}

fn as_set(v: &Value) -> Rc<RefCell<SetData>> {
    match v {
        Value::Set(s) => s.clone(),
        _ => unreachable!(),
    }
}

fn as_file(v: &Value) -> Rc<RefCell<FileData>> {
    match v {
        Value::File(f) => f.clone(),
        _ => unreachable!(),
    }
}

fn as_int(interp: &mut Interp, v: &Value) -> EResult<i64> {
    interp.as_index(v)
}

fn kwarg(kwargs: &[(String, Value)], name: &str) -> Option<Value> {
    kwargs
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.clone())
}

// ---------------- 字符串方法 ----------------

fn str_upper(_: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    Ok(Value::str_from(as_str(&a[0]).to_uppercase()))
}
fn str_lower(_: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    Ok(Value::str_from(as_str(&a[0]).to_lowercase()))
}
fn str_casefold(i: &mut Interp, a: &[Value], k: &[(String, Value)]) -> EResult<Value> {
    str_lower(i, a, k)
}
fn str_capitalize(_: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    let s = as_str(&a[0]);
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i == 0 {
            out.extend(c.to_uppercase());
        } else {
            out.extend(c.to_lowercase());
        }
    }
    Ok(Value::str_from(out))
}
fn str_title(_: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    let s = as_str(&a[0]);
    let mut out = String::new();
    let mut start = true;
    for c in s.chars() {
        if c.is_alphanumeric() {
            if start {
                out.extend(c.to_uppercase());
            } else {
                out.extend(c.to_lowercase());
            }
            start = false;
        } else {
            out.push(c);
            start = true;
        }
    }
    Ok(Value::str_from(out))
}
fn str_swapcase(_: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    let mut out = String::new();
    for c in as_str(&a[0]).chars() {
        if c.is_uppercase() {
            out.extend(c.to_lowercase());
        } else if c.is_lowercase() {
            out.extend(c.to_uppercase());
        } else {
            out.push(c);
        }
    }
    Ok(Value::str_from(out))
}

fn str_strip(
    interp: &mut Interp,
    a: &[Value],
    chars: Option<&Value>,
    left: bool,
    right: bool,
) -> EResult<Value> {
    let s: Vec<char> = as_str(&a[0]).chars().collect();
    let set: Option<Vec<char>> = chars.map(|c| as_str(c).chars().collect());
    let matches = |c: char| match &set {
        Some(set) => set.contains(&c),
        None => c.is_whitespace(),
    };
    let mut start = 0usize;
    let mut end = s.len();
    if left {
        while start < end && matches(s[start]) {
            start += 1;
        }
    }
    if right {
        while end > start && matches(s[end - 1]) {
            end -= 1;
        }
    }
    let _ = interp;
    Ok(Value::str_from(s[start..end].iter().collect::<String>()))
}

fn str_strip_m(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need_between(i, a, "strip", 1, 2)?;
    str_strip(i, a, a.get(1), true, true)
}
fn str_lstrip_m(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need_between(i, a, "lstrip", 1, 2)?;
    str_strip(i, a, a.get(1), true, false)
}
fn str_rstrip_m(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need_between(i, a, "rstrip", 1, 2)?;
    str_strip(i, a, a.get(1), false, true)
}

fn str_split_impl(i: &mut Interp, a: &[Value], from_right: bool) -> EResult<Value> {
    need_between(i, a, "split", 1, 3)?;
    let s = as_str(&a[0]).to_string();
    let sep = a
        .get(1)
        .filter(|v| !v.is_none())
        .map(|v| as_str(v).to_string());
    let maxsplit = match a.get(2) {
        Some(v) => as_int(i, v)?,
        None => -1,
    };
    let mut parts: Vec<Value> = Vec::new();
    match sep {
        None => {
            let mut cur = String::new();
            let mut count = 0i64;
            let mut items: Vec<String> = Vec::new();
            for c in s.chars() {
                if c.is_whitespace() && (maxsplit < 0 || count < maxsplit) {
                    if !cur.is_empty() {
                        items.push(std::mem::take(&mut cur));
                        count += 1;
                    }
                } else {
                    cur.push(c);
                }
            }
            if !cur.is_empty() {
                items.push(cur);
            }
            for it in items {
                parts.push(Value::str_from(it));
            }
        }
        Some(sep) => {
            if sep.is_empty() {
                return i.err("ValueError", "empty separator");
            }
            if maxsplit < 0 {
                for p in s.split(&sep) {
                    parts.push(Value::str_from(p.to_string()));
                }
            } else if from_right {
                let mut idx: Vec<usize> = Vec::new();
                let mut search_from = s.len();
                let mut n = 0i64;
                while n < maxsplit {
                    if search_from == 0 {
                        break;
                    }
                    match s[..search_from].rfind(&sep) {
                        Some(p) => {
                            idx.push(p);
                            search_from = p;
                            n += 1;
                        }
                        None => break,
                    }
                }
                idx.reverse();
                let mut prev = 0usize;
                for p in idx {
                    parts.push(Value::str_from(s[prev..p].to_string()));
                    prev = p + sep.len();
                }
                parts.push(Value::str_from(s[prev..].to_string()));
            } else {
                let mut prev = 0usize;
                let mut n = 0i64;
                while n < maxsplit {
                    match s[prev..].find(&sep) {
                        Some(p) => {
                            parts.push(Value::str_from(s[prev..prev + p].to_string()));
                            prev = prev + p + sep.len();
                            n += 1;
                        }
                        None => break,
                    }
                }
                parts.push(Value::str_from(s[prev..].to_string()));
            }
        }
    }
    Ok(Value::list(parts))
}

fn str_split(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    str_split_impl(i, a, false)
}
fn str_rsplit(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    str_split_impl(i, a, true)
}

fn str_splitlines(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need_between(i, a, "splitlines", 1, 2)?;
    let s = as_str(&a[0]);
    let keepends = a.get(1).map(|v| match v {
        Value::Bool(b) => *b,
        _ => false,
    });
    let keepends = keepends.unwrap_or(false);
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\n' => {
                if keepends {
                    cur.push('\n');
                }
                out.push(Value::str_from(std::mem::take(&mut cur)));
            }
            '\r' => {
                if keepends {
                    cur.push('\r');
                }
                if chars.peek() == Some(&'\n') {
                    if keepends {
                        cur.push('\n');
                    }
                    chars.next();
                }
                out.push(Value::str_from(std::mem::take(&mut cur)));
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(Value::str_from(cur));
    }
    Ok(Value::list(out))
}

fn str_join(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need(i, a, "join", 2)?;
    let sep = as_str(&a[0]);
    let items = i.collect_iter(&a[1])?;
    let mut out = String::new();
    for (n, it) in items.iter().enumerate() {
        match it {
            Value::Str(s) => {
                if n > 0 {
                    out.push_str(sep);
                }
                out.push_str(s);
            }
            other => {
                return i.err(
                    "TypeError",
                    format!(
                        "sequence item {}: expected str instance, {} found",
                        n,
                        other.type_name()
                    ),
                )
            }
        }
    }
    Ok(Value::str_from(out))
}

fn str_replace(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need_between(i, a, "replace", 3, 4)?;
    let s = as_str(&a[0]);
    let old = as_str(&a[1]);
    let new = as_str(&a[2]);
    let count = match a.get(3) {
        Some(v) => as_int(i, v)?,
        None => -1,
    };
    if count < 0 {
        return Ok(Value::str_from(s.replace(old, new)));
    }
    let mut out = String::new();
    let mut rest = s;
    let mut n = 0i64;
    if old.is_empty() {
        return Ok(Value::str_from(s.to_string()));
    }
    while n < count {
        match rest.find(old) {
            Some(p) => {
                out.push_str(&rest[..p]);
                out.push_str(new);
                rest = &rest[p + old.len()..];
                n += 1;
            }
            None => break,
        }
    }
    out.push_str(rest);
    Ok(Value::str_from(out))
}

fn str_find_impl(i: &mut Interp, a: &[Value], from_right: bool, raise: bool) -> EResult<Value> {
    need_between(i, a, "find", 2, 4)?;
    let s = as_str(&a[0]).to_string();
    let sub = as_str(&a[1]).to_string();
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len() as i64;
    let norm = |v: i64| -> i64 {
        if v < 0 {
            (v + n).max(0)
        } else {
            v.min(n)
        }
    };
    let start = match a.get(2) {
        Some(v) => norm(as_int(i, v)?),
        None => 0,
    } as usize;
    let end = match a.get(3) {
        Some(v) => norm(as_int(i, v)?),
        None => n,
    } as usize;
    let found = if start <= end {
        let hay: String = chars[start..end].iter().collect();
        if from_right {
            hay.rfind(&sub)
        } else {
            hay.find(&sub)
        }
    } else {
        None
    };
    match found {
        Some(p) => {
            let char_pos = chars[start..end][..p].len();
            Ok(Value::Int((start + char_pos) as i64))
        }
        None => {
            if raise {
                i.err("ValueError", "substring not found")
            } else {
                Ok(Value::Int(-1))
            }
        }
    }
}

fn str_find(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    str_find_impl(i, a, false, false)
}
fn str_rfind(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    str_find_impl(i, a, true, false)
}
fn str_index(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    str_find_impl(i, a, false, true)
}
fn str_rindex(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    str_find_impl(i, a, true, true)
}

fn str_count(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need_between(i, a, "count", 2, 4)?;
    let s = as_str(&a[0]);
    let sub = as_str(&a[1]);
    if sub.is_empty() {
        return Ok(Value::Int(s.chars().count() as i64 + 1));
    }
    Ok(Value::Int(s.matches(sub).count() as i64))
}

fn str_startswith(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need_between(i, a, "startswith", 2, 4)?;
    let s = as_str(&a[0]);
    let matched = match &a[1] {
        Value::Str(p) => s.starts_with(&**p),
        Value::Tuple(items) => items.iter().any(|it| match it {
            Value::Str(p) => s.starts_with(&**p),
            _ => false,
        }),
        other => {
            return i.err(
                "TypeError",
                format!(
                    "startswith first arg must be str or a tuple of str, not {}",
                    other.type_name()
                ),
            )
        }
    };
    Ok(Value::Bool(matched))
}
fn str_endswith(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need_between(i, a, "endswith", 2, 4)?;
    let s = as_str(&a[0]);
    let matched = match &a[1] {
        Value::Str(p) => s.ends_with(&**p),
        Value::Tuple(items) => items.iter().any(|it| match it {
            Value::Str(p) => s.ends_with(&**p),
            _ => false,
        }),
        other => {
            return i.err(
                "TypeError",
                format!(
                    "endswith first arg must be str or a tuple of str, not {}",
                    other.type_name()
                ),
            )
        }
    };
    Ok(Value::Bool(matched))
}

fn str_predicate(i: &mut Interp, a: &[Value], name: &str) -> EResult<Value> {
    let s = as_str(&a[0]);
    let nonempty = !s.is_empty();
    let r = match name {
        "isdigit" => nonempty && s.chars().all(|c| c.is_ascii_digit()),
        "isalpha" => nonempty && s.chars().all(|c| c.is_alphabetic()),
        "isalnum" => nonempty && s.chars().all(|c| c.is_alphanumeric()),
        "isspace" => nonempty && s.chars().all(|c| c.is_whitespace()),
        "isupper" => s.chars().any(|c| c.is_uppercase()) && !s.chars().any(|c| c.is_lowercase()),
        "islower" => s.chars().any(|c| c.is_lowercase()) && !s.chars().any(|c| c.is_uppercase()),
        "isnumeric" | "isdecimal" => nonempty && s.chars().all(|c| c.is_numeric()),
        "isidentifier" => {
            let mut cs = s.chars();
            match cs.next() {
                Some(c) if c == '_' || c.is_alphabetic() => {
                    cs.all(|c| c == '_' || c.is_alphanumeric())
                }
                _ => false,
            }
        }
        "isprintable" => s.chars().all(|c| !c.is_control()),
        _ => return i.err("ValueError", format!("未知的谓词 {}", name)),
    };
    Ok(Value::Bool(r))
}

macro_rules! pred_method {
    ($fname:ident, $name:literal) => {
        fn $fname(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
            str_predicate(i, a, $name)
        }
    };
}
pred_method!(str_isdigit, "isdigit");
pred_method!(str_isalpha, "isalpha");
pred_method!(str_isalnum, "isalnum");
pred_method!(str_isspace, "isspace");
pred_method!(str_isupper, "isupper");
pred_method!(str_islower, "islower");
pred_method!(str_isnumeric, "isnumeric");
pred_method!(str_isdecimal, "isdecimal");
pred_method!(str_isidentifier, "isidentifier");
pred_method!(str_isprintable, "isprintable");

fn str_pad(i: &mut Interp, a: &[Value], name: &str) -> EResult<Value> {
    need_between(i, a, name, 2, 3)?;
    let s = as_str(&a[0]);
    let width = as_int(i, &a[1])? as usize;
    let fill = match a.get(2) {
        Some(Value::Str(f)) => f.chars().next().unwrap_or(' '),
        _ => ' ',
    };
    let len = s.chars().count();
    if len >= width {
        return Ok(Value::str_from(s.to_string()));
    }
    let pad = width - len;
    let out = match name {
        "ljust" => format!("{}{}", s, fill.to_string().repeat(pad)),
        "rjust" => format!("{}{}", fill.to_string().repeat(pad), s),
        _ => {
            let left = pad / 2;
            let right = pad - left;
            format!(
                "{}{}{}",
                fill.to_string().repeat(left),
                s,
                fill.to_string().repeat(right)
            )
        }
    };
    Ok(Value::str_from(out))
}

fn str_ljust(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    str_pad(i, a, "ljust")
}
fn str_rjust(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    str_pad(i, a, "rjust")
}
fn str_center(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    str_pad(i, a, "center")
}

fn str_zfill(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need(i, a, "zfill", 2)?;
    let s = as_str(&a[0]);
    let width = as_int(i, &a[1])? as usize;
    let len = s.chars().count();
    if len >= width {
        return Ok(Value::str_from(s.to_string()));
    }
    let pad = width - len;
    let (sign, rest) = match s.chars().next() {
        Some(c) if c == '+' || c == '-' => (c.to_string(), s[c.len_utf8()..].to_string()),
        _ => (String::new(), s.to_string()),
    };
    Ok(Value::str_from(format!(
        "{}{}{}",
        sign,
        "0".repeat(pad),
        rest
    )))
}

fn str_removeprefix(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need(i, a, "removeprefix", 2)?;
    let s = as_str(&a[0]);
    let p = as_str(&a[1]);
    Ok(Value::str_from(match s.strip_prefix(p) {
        Some(r) => r.to_string(),
        None => s.to_string(),
    }))
}
fn str_removesuffix(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need(i, a, "removesuffix", 2)?;
    let s = as_str(&a[0]);
    let p = as_str(&a[1]);
    Ok(Value::str_from(match s.strip_suffix(p) {
        Some(r) => r.to_string(),
        None => s.to_string(),
    }))
}

fn str_partition(i: &mut Interp, a: &[Value], from_right: bool) -> EResult<Value> {
    need(i, a, "partition", 2)?;
    let s = as_str(&a[0]);
    let sep = as_str(&a[1]);
    if sep.is_empty() {
        return i.err("ValueError", "empty separator");
    }
    let found = if from_right {
        s.rfind(sep)
    } else {
        s.find(sep)
    };
    Ok(match found {
        Some(p) => Value::tuple(vec![
            Value::str_from(s[..p].to_string()),
            Value::str_from(sep.to_string()),
            Value::str_from(s[p + sep.len()..].to_string()),
        ]),
        None => Value::tuple(vec![
            Value::str_from(s.to_string()),
            Value::str_from(""),
            Value::str_from(""),
        ]),
    })
}
fn str_partition_m(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    str_partition(i, a, false)
}
fn str_rpartition(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    str_partition(i, a, true)
}

fn str_expandtabs(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need_between(i, a, "expandtabs", 1, 2)?;
    let s = as_str(&a[0]);
    let tab = match a.get(1) {
        Some(v) => as_int(i, v)?.max(1) as usize,
        None => 8,
    };
    let mut out = String::new();
    let mut col = 0usize;
    for c in s.chars() {
        match c {
            '\t' => {
                let n = tab - (col % tab);
                out.push_str(&" ".repeat(n));
                col += n;
            }
            '\n' | '\r' => {
                out.push(c);
                col = 0;
            }
            c => {
                out.push(c);
                col += 1;
            }
        }
    }
    Ok(Value::str_from(out))
}

/// `str.format(...)`：支持 `{}`、`{0}`、`{name}`、`{!r}`、`{:spec}`。
fn str_format(i: &mut Interp, a: &[Value], kwargs: &[(String, Value)]) -> EResult<Value> {
    let fmt = as_str(&a[0]).to_string();
    let args: Vec<Value> = a[1..].to_vec();
    let mut out = String::new();
    let mut chars = fmt.chars().peekable();
    let mut auto = 0usize;
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                if chars.peek() == Some(&'{') {
                    chars.next();
                    out.push('{');
                    continue;
                }
                let mut field = String::new();
                let mut depth = 1;
                while let Some(&c2) = chars.peek() {
                    if c2 == '{' {
                        depth += 1;
                    } else if c2 == '}' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    field.push(c2);
                    chars.next();
                }
                if chars.next() != Some('}') {
                    return i.err("ValueError", "Single '{' encountered in format string");
                }
                let (name_part, conv, spec) = split_field(&field);
                let value = if name_part.is_empty() {
                    let v = match args.get(auto) {
                        Some(v) => v.clone(),
                        None => {
                            return i.err(
                                "IndexError",
                                "Replacement index out of range for format string",
                            )
                        }
                    };
                    auto += 1;
                    v
                } else if let Ok(idx) = name_part.parse::<usize>() {
                    match args.get(idx) {
                        Some(v) => v.clone(),
                        None => {
                            return i.err(
                                "IndexError",
                                "Replacement index out of range for format string",
                            )
                        }
                    }
                } else {
                    match kwargs.iter().find(|(k, _)| *k == name_part) {
                        Some((_, v)) => v.clone(),
                        None => return i.err("KeyError", format!("'{}'", name_part)),
                    }
                };
                let text = match conv {
                    Some('r') => i.value_repr(&value)?,
                    Some('s') => i.value_str(&value)?,
                    None => {
                        if spec.is_empty() {
                            i.value_str(&value)?
                        } else {
                            i.format_with_spec(&value, &spec)?
                        }
                    }
                    Some(other_c) => {
                        return i.err(
                            "ValueError",
                            format!("Unknown conversion specifier {}", other_c),
                        )
                    }
                };
                out.push_str(&text);
            }
            '}' => {
                if chars.peek() == Some(&'}') {
                    chars.next();
                    out.push('}');
                } else {
                    return i.err("ValueError", "Single '}' encountered in format string");
                }
            }
            c => out.push(c),
        }
    }
    Ok(Value::str_from(out))
}

fn split_field(field: &str) -> (String, Option<char>, String) {
    let (rest, conv) = match field.find('!') {
        Some(p) => {
            let c = field[p + 1..].chars().next();
            (&field[..p], c)
        }
        None => (field, None),
    };
    let (name, spec) = match rest.find(':') {
        Some(p) => (&rest[..p], rest[p + 1..].to_string()),
        None => (rest, String::new()),
    };
    (name.to_string(), conv, spec)
}

fn str_repr_m(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    let r = i.value_repr(&a[0])?;
    Ok(Value::str_from(r))
}

fn str_encode(i: &mut Interp, _a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    i.err("NotImplementedError", "暂不支持 bytes / encode()")
}

// ---------------- 列表方法 ----------------

fn list_append(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need(i, a, "append", 2)?;
    as_list(&a[0]).borrow_mut().push(a[1].clone());
    Ok(Value::None)
}

fn list_extend(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need(i, a, "extend", 2)?;
    let items = i.collect_iter(&a[1])?;
    as_list(&a[0]).borrow_mut().extend(items);
    Ok(Value::None)
}

fn list_insert(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need(i, a, "insert", 3)?;
    let l = as_list(&a[0]);
    let mut idx = as_int(i, &a[1])?;
    let len = l.borrow().len() as i64;
    if idx < 0 {
        idx += len;
        if idx < 0 {
            idx = 0;
        }
    }
    if idx > len {
        idx = len;
    }
    l.borrow_mut().insert(idx as usize, a[2].clone());
    Ok(Value::None)
}

fn list_remove(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need(i, a, "remove", 2)?;
    let l = as_list(&a[0]);
    let items = l.borrow().clone();
    for (idx, it) in items.iter().enumerate() {
        if i.eq_values(it, &a[1])? {
            l.borrow_mut().remove(idx);
            return Ok(Value::None);
        }
    }
    i.err("ValueError", "list.remove(x): x not in list")
}

fn list_pop(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need_between(i, a, "pop", 1, 2)?;
    let l = as_list(&a[0]);
    let len = l.borrow().len() as i64;
    if len == 0 {
        return i.err("IndexError", "pop from empty list");
    }
    let mut idx = match a.get(1) {
        Some(v) => as_int(i, v)?,
        None => -1,
    };
    if idx < 0 {
        idx += len;
    }
    if idx < 0 || idx >= len {
        return i.err("IndexError", "pop index out of range");
    }
    let v = l.borrow_mut().remove(idx as usize);
    Ok(v)
}

fn list_clear(_: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    as_list(&a[0]).borrow_mut().clear();
    Ok(Value::None)
}

fn list_index(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need_between(i, a, "index", 2, 4)?;
    let items = as_list(&a[0]).borrow().clone();
    let start = match a.get(2) {
        Some(v) => as_int(i, v)?.max(0) as usize,
        None => 0,
    };
    let end = match a.get(3) {
        Some(v) => as_int(i, v)?.max(0) as usize,
        None => items.len(),
    };
    for (idx, it) in items.iter().enumerate() {
        if idx < start || idx >= end {
            continue;
        }
        if i.eq_values(it, &a[1])? {
            return Ok(Value::Int(idx as i64));
        }
    }
    i.err("ValueError", "x is not in list")
}

fn list_count(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need(i, a, "count", 2)?;
    let items = as_list(&a[0]).borrow().clone();
    let mut n = 0i64;
    for it in items.iter() {
        if i.eq_values(it, &a[1])? {
            n += 1;
        }
    }
    Ok(Value::Int(n))
}

fn list_reverse(_: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    as_list(&a[0]).borrow_mut().reverse();
    Ok(Value::None)
}

fn list_copy(_: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    let items = as_list(&a[0]).borrow().clone();
    Ok(Value::list(items))
}

fn list_sort(i: &mut Interp, a: &[Value], kwargs: &[(String, Value)]) -> EResult<Value> {
    let l = as_list(&a[0]);
    let key = kwarg(kwargs, "key").filter(|v| !v.is_none());
    let reverse = match kwarg(kwargs, "reverse") {
        Some(v) => i.truthy(&v)?,
        None => false,
    };
    let mut items = l.borrow().clone();
    sort_values(i, &mut items, key.as_ref(), reverse)?;
    *l.borrow_mut() = items;
    Ok(Value::None)
}

/// 稳定归并排序（只用 `<` 比较，与 Python 一致）。
pub fn sort_values(
    i: &mut Interp,
    items: &mut Vec<Value>,
    key: Option<&Value>,
    reverse: bool,
) -> EResult<()> {
    let mut pairs: Vec<(Value, Value)> = Vec::with_capacity(items.len());
    for it in items.iter() {
        let k = match key {
            Some(f) => i.call_value(f, vec![it.clone()], Vec::new())?,
            None => it.clone(),
        };
        pairs.push((it.clone(), k));
    }
    merge_sort(i, &mut pairs, reverse)?;
    *items = pairs.into_iter().map(|(v, _)| v).collect();
    Ok(())
}

fn merge_sort(i: &mut Interp, pairs: &mut Vec<(Value, Value)>, reverse: bool) -> EResult<()> {
    let n = pairs.len();
    if n <= 1 {
        return Ok(());
    }
    let mut right = pairs.split_off(n / 2);
    merge_sort(i, pairs, reverse)?;
    merge_sort(i, &mut right, reverse)?;
    let left = std::mem::take(pairs);
    let mut out: Vec<(Value, Value)> = Vec::with_capacity(n);
    let (mut li, mut ri) = (0usize, 0usize);
    while li < left.len() && ri < right.len() {
        let take_right = if !reverse {
            i.lt_values(&right[ri].1, &left[li].1)?
        } else {
            i.lt_values(&left[li].1, &right[ri].1)?
        };
        if take_right {
            out.push(right[ri].clone());
            ri += 1;
        } else {
            out.push(left[li].clone());
            li += 1;
        }
    }
    out.extend(left[li..].iter().cloned());
    out.extend(right[ri..].iter().cloned());
    *pairs = out;
    Ok(())
}

// ---------------- 字典方法 ----------------

fn dict_get(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need_between(i, a, "get", 2, 3)?;
    let d = as_dict(&a[0]);
    let found = d.borrow().get(&a[1]);
    Ok(match found {
        Some(v) => v,
        None => a.get(2).cloned().unwrap_or(Value::None),
    })
}

fn dict_keys(_: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    let keys: Vec<Value> = as_dict(&a[0])
        .borrow()
        .entries
        .iter()
        .map(|(k, _)| k.clone())
        .collect();
    Ok(Value::list(keys))
}

fn dict_values(_: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    let vals: Vec<Value> = as_dict(&a[0])
        .borrow()
        .entries
        .iter()
        .map(|(_, v)| v.clone())
        .collect();
    Ok(Value::list(vals))
}

fn dict_items(_: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    let items: Vec<Value> = as_dict(&a[0])
        .borrow()
        .entries
        .iter()
        .map(|(k, v)| Value::tuple(vec![k.clone(), v.clone()]))
        .collect();
    Ok(Value::list(items))
}

fn dict_pop(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need_between(i, a, "pop", 2, 3)?;
    let d = as_dict(&a[0]);
    let removed = d.borrow_mut().remove(&a[1]);
    match removed {
        Some(v) => Ok(v),
        None => match a.get(2) {
            Some(v) => Ok(v.clone()),
            None => Err(Signal::Error(i.key_error(&a[1]))),
        },
    }
}

fn dict_popitem(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    let d = as_dict(&a[0]);
    let mut dd = d.borrow_mut();
    match dd.entries.pop() {
        Some((k, v)) => Ok(Value::tuple(vec![k, v])),
        None => i.err("KeyError", "popitem(): dictionary is empty"),
    }
}

fn dict_update(i: &mut Interp, a: &[Value], kwargs: &[(String, Value)]) -> EResult<Value> {
    need_between(i, a, "update", 1, 2)?;
    let d = as_dict(&a[0]);
    if let Some(other) = a.get(1) {
        match other {
            Value::Dict(_) => {
                for (k, v) in i.dict_items(other)? {
                    d.borrow_mut().insert(k, v);
                }
            }
            Value::Instance(_) => {
                for (k, v) in i.dict_items(other)? {
                    d.borrow_mut().insert(k, v);
                }
            }
            _ => {
                for pair in i.collect_iter(other)? {
                    let ps = i.collect_iter(&pair)?;
                    if ps.len() != 2 {
                        return i.err(
                            "ValueError",
                            "dictionary update sequence element has length != 2",
                        );
                    }
                    d.borrow_mut().insert(ps[0].clone(), ps[1].clone());
                }
            }
        }
    }
    for (k, v) in kwargs {
        d.borrow_mut().insert(Value::str_from(k.clone()), v.clone());
    }
    Ok(Value::None)
}

fn dict_setdefault(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need_between(i, a, "setdefault", 2, 3)?;
    let d = as_dict(&a[0]);
    if let Some(v) = d.borrow().get(&a[1]) {
        return Ok(v);
    }
    let v = a.get(2).cloned().unwrap_or(Value::None);
    d.borrow_mut().insert(a[1].clone(), v.clone());
    Ok(v)
}

fn dict_clear(_: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    as_dict(&a[0]).borrow_mut().entries.clear();
    Ok(Value::None)
}

fn dict_copy(_: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    let entries = as_dict(&a[0]).borrow().entries.clone();
    let mut d = DictData::new();
    d.entries = entries;
    Ok(Value::Dict(Rc::new(RefCell::new(d))))
}

/// `dict.fromkeys(seq, value=None)`（作为类方法调用）。
fn dict_fromkeys(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    if a.is_empty() {
        return i.err("TypeError", "dict.fromkeys 需要至少一个参数");
    }
    let keys = i.collect_iter(&a[0])?;
    let value = a.get(1).cloned().unwrap_or(Value::None);
    let mut d = DictData::new();
    for k in keys {
        if !is_hashable(&k) {
            return i.err("TypeError", format!("unhashable type: '{}'", k.type_name()));
        }
        d.insert(k, value.clone());
    }
    Ok(Value::Dict(Rc::new(RefCell::new(d))))
}

// ---------------- 集合方法 ----------------

fn set_add(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need(i, a, "add", 2)?;
    if !is_hashable(&a[1]) {
        return i.err(
            "TypeError",
            format!("unhashable type: '{}'", a[1].type_name()),
        );
    }
    as_set(&a[0]).borrow_mut().insert(a[1].clone());
    Ok(Value::None)
}

fn set_remove(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need(i, a, "remove", 2)?;
    let removed = as_set(&a[0]).borrow_mut().remove(&a[1]);
    match removed {
        Some(_) => Ok(Value::None),
        None => Err(Signal::Error(i.key_error(&a[1]))),
    }
}

fn set_discard(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need(i, a, "discard", 2)?;
    as_set(&a[0]).borrow_mut().remove(&a[1]);
    Ok(Value::None)
}

fn set_pop(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    let s = as_set(&a[0]);
    let v = s.borrow_mut().items.pop();
    match v {
        Some(v) => Ok(v),
        None => i.err("KeyError", "pop from an empty set"),
    }
}

fn set_clear(_: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    as_set(&a[0]).borrow_mut().items.clear();
    Ok(Value::None)
}

fn set_copy(_: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    let items = as_set(&a[0]).borrow().items.clone();
    let mut s = SetData::new();
    s.items = items;
    Ok(Value::Set(Rc::new(RefCell::new(s))))
}

fn set_op(i: &mut Interp, a: &[Value], name: &str) -> EResult<Value> {
    let cur = as_set(&a[0]).borrow().items.clone();
    let mut others: Vec<Vec<Value>> = Vec::new();
    for v in &a[1..] {
        others.push(i.collect_iter(v)?);
    }
    let has = |items: &Vec<Value>, v: &Value| items.iter().any(|w| key_equal(w, v));
    let mut out = SetData::new();
    match name {
        "union" => {
            for v in cur.iter() {
                out.insert(v.clone());
            }
            for o in &others {
                for v in o.iter() {
                    out.insert(v.clone());
                }
            }
        }
        "intersection" => {
            for v in cur.iter() {
                if others.iter().all(|o| has(o, v)) {
                    out.insert(v.clone());
                }
            }
        }
        "difference" => {
            for v in cur.iter() {
                if others.iter().all(|o| !has(o, v)) {
                    out.insert(v.clone());
                }
            }
        }
        "symmetric_difference" => {
            let o = others.first().cloned().unwrap_or_default();
            for v in cur.iter() {
                if !has(&o, v) {
                    out.insert(v.clone());
                }
            }
            for v in o.iter() {
                if !has(&cur, v) {
                    out.insert(v.clone());
                }
            }
        }
        _ => return i.err("ValueError", format!("未知的集合运算 {}", name)),
    }
    Ok(Value::Set(Rc::new(RefCell::new(out))))
}

fn set_union(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    set_op(i, a, "union")
}
fn set_intersection(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    set_op(i, a, "intersection")
}
fn set_difference(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    set_op(i, a, "difference")
}
fn set_symmetric_difference(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    set_op(i, a, "symmetric_difference")
}

fn set_update(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    let s = as_set(&a[0]);
    for o in &a[1..] {
        let items = i.collect_iter(o)?;
        for v in items {
            s.borrow_mut().insert(v);
        }
    }
    Ok(Value::None)
}

fn set_issubset(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need(i, a, "issubset", 2)?;
    let cur = as_set(&a[0]).borrow().items.clone();
    let other = match &a[1] {
        Value::Set(s) => s.borrow().items.clone(),
        other => i.collect_iter(other)?,
    };
    Ok(Value::Bool(
        cur.iter().all(|v| other.iter().any(|w| key_equal(v, w))),
    ))
}

fn set_issuperset(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need(i, a, "issuperset", 2)?;
    let cur = as_set(&a[0]).borrow().items.clone();
    let other = match &a[1] {
        Value::Set(s) => s.borrow().items.clone(),
        other => i.collect_iter(other)?,
    };
    Ok(Value::Bool(
        other.iter().all(|v| cur.iter().any(|w| key_equal(v, w))),
    ))
}

fn set_isdisjoint(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need(i, a, "isdisjoint", 2)?;
    let cur = as_set(&a[0]).borrow().items.clone();
    let other = match &a[1] {
        Value::Set(s) => s.borrow().items.clone(),
        other => i.collect_iter(other)?,
    };
    Ok(Value::Bool(
        !cur.iter().any(|v| other.iter().any(|w| key_equal(v, w))),
    ))
}

// ---------------- 元组 / 数字 ----------------

fn tuple_count(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need(i, a, "count", 2)?;
    let items = match &a[0] {
        Value::Tuple(t) => t.as_ref().clone(),
        _ => unreachable!(),
    };
    let mut n = 0i64;
    for it in items.iter() {
        if i.eq_values(it, &a[1])? {
            n += 1;
        }
    }
    Ok(Value::Int(n))
}

fn tuple_index(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need(i, a, "index", 2)?;
    let items = match &a[0] {
        Value::Tuple(t) => t.as_ref().clone(),
        _ => unreachable!(),
    };
    for (idx, it) in items.iter().enumerate() {
        if i.eq_values(it, &a[1])? {
            return Ok(Value::Int(idx as i64));
        }
    }
    i.err("ValueError", "tuple.index(x): x not in tuple")
}

fn int_bit_count(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    let n = as_int(i, &a[0])?;
    Ok(Value::Int(n.unsigned_abs().count_ones() as i64))
}

fn int_bit_length(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    let n = as_int(i, &a[0])?;
    Ok(Value::Int(if n == 0 {
        0
    } else {
        64 - n.unsigned_abs().leading_zeros() as i64
    }))
}

fn float_is_integer(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    match &a[0] {
        Value::Float(f) => Ok(Value::Bool(f.fract() == 0.0 && f.is_finite())),
        _ => i.err("TypeError", "is_integer() 只能用于 float"),
    }
}

fn num_conjugate(_: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    Ok(a[0].clone())
}

fn range_count(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need(i, a, "count", 2)?;
    let r = match &a[0] {
        Value::Range(r) => **r,
        _ => unreachable!(),
    };
    let v = as_int(i, &a[1])?;
    Ok(Value::Int(if r.contains(v) { 1 } else { 0 }))
}

fn range_index(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need(i, a, "index", 2)?;
    let r = match &a[0] {
        Value::Range(r) => **r,
        _ => unreachable!(),
    };
    let v = as_int(i, &a[1])?;
    if r.contains(v) {
        Ok(Value::Int((v - r.start) / r.step))
    } else {
        i.err("ValueError", format!("{} is not in range", v))
    }
}

// ---------------- 迭代器 / 文件 ----------------

fn iter_next_method(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    match i.iter_next(&a[0])? {
        Some(v) => Ok(v),
        None => i.err("StopIteration", ""),
    }
}

fn iter_self(_: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    Ok(a[0].clone())
}

fn iter_len(i: &mut Interp, _a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    i.err("TypeError", "object of type 'iterator' has no len()")
}

fn file_read(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    let f = as_file(&a[0]);
    let mut fd = f.borrow_mut();
    if fd.closed {
        return i.err("ValueError", "I/O operation on closed file.");
    }
    let n = match a.get(1) {
        Some(v) if !v.is_none() => Some(as_int(i, v)?.max(0) as usize),
        _ => None,
    };
    let mut out = String::new();
    match &mut fd.buf {
        FileBuf::Read(r) => match n {
            Some(n) => {
                let mut buf = vec![0u8; n];
                let got = std::io::Read::read(r, &mut buf)
                    .map_err(|e| Signal::Error(i.pyerr("OSError", e.to_string())))?;
                buf.truncate(got);
                out = String::from_utf8_lossy(&buf).to_string();
            }
            None => {
                std::io::Read::read_to_string(r, &mut out)
                    .map_err(|e| Signal::Error(i.pyerr("OSError", e.to_string())))?;
            }
        },
        FileBuf::Write(_) => {
            return i.err("OSError", "not readable");
        }
    }
    Ok(Value::str_from(out))
}

fn read_line_from(fd: &mut FileData) -> std::io::Result<Option<String>> {
    match &mut fd.buf {
        FileBuf::Read(r) => {
            let mut line = String::new();
            let n = r.read_line(&mut line)?;
            if n == 0 {
                Ok(None)
            } else {
                Ok(Some(line))
            }
        }
        FileBuf::Write(_) => Ok(None),
    }
}

fn file_readline(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    let f = as_file(&a[0]);
    let mut fd = f.borrow_mut();
    if fd.closed {
        return i.err("ValueError", "I/O operation on closed file.");
    }
    match read_line_from(&mut fd) {
        Ok(Some(l)) => Ok(Value::str_from(l)),
        Ok(None) => Ok(Value::str_from("")),
        Err(e) => i.err("OSError", e.to_string()),
    }
}

fn file_readlines(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    let f = as_file(&a[0]);
    let mut out = Vec::new();
    loop {
        let mut fd = f.borrow_mut();
        if fd.closed {
            return i.err("ValueError", "I/O operation on closed file.");
        }
        match read_line_from(&mut fd) {
            Ok(Some(l)) => out.push(Value::str_from(l)),
            Ok(None) => break,
            Err(e) => return i.err("OSError", e.to_string()),
        }
    }
    Ok(Value::list(out))
}

fn file_next(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    let f = as_file(&a[0]);
    let mut fd = f.borrow_mut();
    if fd.closed {
        return i.err("ValueError", "I/O operation on closed file.");
    }
    match read_line_from(&mut fd) {
        Ok(Some(l)) => {
            if l.is_empty() {
                i.err("StopIteration", "")
            } else {
                Ok(Value::str_from(l))
            }
        }
        Ok(None) => i.err("StopIteration", ""),
        Err(e) => i.err("OSError", e.to_string()),
    }
}

fn file_write(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need(i, a, "write", 2)?;
    let f = as_file(&a[0]);
    let mut fd = f.borrow_mut();
    if fd.closed {
        return i.err("ValueError", "I/O operation on closed file.");
    }
    let text = match &a[1] {
        Value::Str(s) => s.to_string(),
        other => {
            return i.err(
                "TypeError",
                format!("write() argument must be str, not {}", other.type_name()),
            )
        }
    };
    match &mut fd.buf {
        FileBuf::Write(w) => match w.write_all(text.as_bytes()) {
            Ok(()) => Ok(Value::Int(text.chars().count() as i64)),
            Err(e) => i.err("OSError", e.to_string()),
        },
        FileBuf::Read(_) => i.err("OSError", "not writable"),
    }
}

fn file_writelines(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need(i, a, "writelines", 2)?;
    let lines = i.collect_iter(&a[1])?;
    for l in lines {
        let text = match &l {
            Value::Str(s) => s.to_string(),
            other => {
                return i.err(
                    "TypeError",
                    format!("write() argument must be str, not {}", other.type_name()),
                )
            }
        };
        let f = as_file(&a[0]);
        let mut fd = f.borrow_mut();
        match &mut fd.buf {
            FileBuf::Write(w) => {
                if let Err(e) = w.write_all(text.as_bytes()) {
                    return i.err("OSError", e.to_string());
                }
            }
            FileBuf::Read(_) => return i.err("OSError", "not writable"),
        }
    }
    Ok(Value::None)
}

fn file_flush(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    let f = as_file(&a[0]);
    let mut fd = f.borrow_mut();
    if let FileBuf::Write(w) = &mut fd.buf {
        if let Err(e) = w.flush() {
            return i.err("OSError", e.to_string());
        }
    }
    Ok(Value::None)
}

fn file_close(_i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    let f = as_file(&a[0]);
    let mut fd = f.borrow_mut();
    if !fd.closed {
        if let FileBuf::Write(w) = &mut fd.buf {
            let _ = w.flush();
        }
        fd.closed = true;
    }
    Ok(Value::None)
}

fn file_enter(_: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    Ok(a[0].clone())
}

fn file_exit(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    file_close(i, a, &[])?;
    Ok(Value::Bool(false))
}

fn file_iter(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    i.get_iter(&a[0])
}

// ---------------- property ----------------

pub fn property_getter(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need(i, a, "property.getter", 2)?;
    let p = match &a[0] {
        Value::Property(p) => p.clone(),
        _ => unreachable!(),
    };
    Ok(Value::Property(Rc::new(PropertyData {
        name: p.name.clone(),
        getter: a[1].clone(),
        setter: p.setter.clone(),
        deleter: p.deleter.clone(),
    })))
}

pub fn property_setter(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need(i, a, "property.setter", 2)?;
    let p = match &a[0] {
        Value::Property(p) => p.clone(),
        _ => unreachable!(),
    };
    Ok(Value::Property(Rc::new(PropertyData {
        name: p.name.clone(),
        getter: p.getter.clone(),
        setter: Some(a[1].clone()),
        deleter: p.deleter.clone(),
    })))
}

pub fn property_deleter(i: &mut Interp, a: &[Value], _: &[(String, Value)]) -> EResult<Value> {
    need(i, a, "property.deleter", 2)?;
    let p = match &a[0] {
        Value::Property(p) => p.clone(),
        _ => unreachable!(),
    };
    Ok(Value::Property(Rc::new(PropertyData {
        name: p.name.clone(),
        getter: p.getter.clone(),
        setter: p.setter.clone(),
        deleter: Some(a[1].clone()),
    })))
}

// ---------------- 属性（非方法） ----------------

/// 内置类型的普通属性（range.start / file.name / int.real ...）。
pub fn lookup_attr(type_name: &str, name: &str, recv: &Value) -> Option<Value> {
    match type_name {
        "int" => match name {
            "real" | "numerator" => Some(recv.clone()),
            "imag" | "denominator" => Some(Value::Int(0)),
            _ => None,
        },
        "float" => match name {
            "real" => Some(recv.clone()),
            "imag" => Some(Value::Float(0.0)),
            _ => None,
        },
        "range" => match (name, recv) {
            ("start", Value::Range(r)) => Some(Value::Int(r.start)),
            ("stop", Value::Range(r)) => Some(Value::Int(r.stop)),
            ("step", Value::Range(r)) => Some(Value::Int(r.step)),
            _ => None,
        },
        "TextIOWrapper" => match (name, recv) {
            ("name", Value::File(f)) => Some(Value::str_from(f.borrow().name.clone())),
            ("mode", Value::File(f)) => Some(Value::str_from(f.borrow().mode.clone())),
            ("closed", Value::File(f)) => Some(Value::Bool(f.borrow().closed)),
            _ => None,
        },
        _ => None,
    }
}

// ---------------- 方法表 ----------------

pub fn lookup_method(type_name: &str, name: &str) -> Option<Nf> {
    let f: Nf = match type_name {
        "str" => match name {
            "upper" => str_upper,
            "lower" => str_lower,
            "casefold" => str_casefold,
            "capitalize" => str_capitalize,
            "title" => str_title,
            "swapcase" => str_swapcase,
            "strip" => str_strip_m,
            "lstrip" => str_lstrip_m,
            "rstrip" => str_rstrip_m,
            "split" => str_split,
            "rsplit" => str_rsplit,
            "splitlines" => str_splitlines,
            "join" => str_join,
            "replace" => str_replace,
            "find" => str_find,
            "rfind" => str_rfind,
            "index" => str_index,
            "rindex" => str_rindex,
            "count" => str_count,
            "startswith" => str_startswith,
            "endswith" => str_endswith,
            "isdigit" => str_isdigit,
            "isalpha" => str_isalpha,
            "isalnum" => str_isalnum,
            "isspace" => str_isspace,
            "isupper" => str_isupper,
            "islower" => str_islower,
            "isnumeric" => str_isnumeric,
            "isdecimal" => str_isdecimal,
            "isidentifier" => str_isidentifier,
            "isprintable" => str_isprintable,
            "ljust" => str_ljust,
            "rjust" => str_rjust,
            "center" => str_center,
            "zfill" => str_zfill,
            "removeprefix" => str_removeprefix,
            "removesuffix" => str_removesuffix,
            "partition" => str_partition_m,
            "rpartition" => str_rpartition,
            "expandtabs" => str_expandtabs,
            "format" => str_format,
            "__repr__" => str_repr_m,
            "encode" => str_encode,
            _ => return None,
        },
        "list" => match name {
            "append" => list_append,
            "extend" => list_extend,
            "insert" => list_insert,
            "remove" => list_remove,
            "pop" => list_pop,
            "clear" => list_clear,
            "index" => list_index,
            "count" => list_count,
            "reverse" => list_reverse,
            "copy" => list_copy,
            "sort" => list_sort,
            "__iter__" => iter_self,
            _ => return None,
        },
        "dict" => match name {
            "get" => dict_get,
            "keys" => dict_keys,
            "values" => dict_values,
            "items" => dict_items,
            "pop" => dict_pop,
            "popitem" => dict_popitem,
            "update" => dict_update,
            "setdefault" => dict_setdefault,
            "clear" => dict_clear,
            "copy" => dict_copy,
            "__iter__" => iter_self,
            _ => return None,
        },
        "set" => match name {
            "add" => set_add,
            "remove" => set_remove,
            "discard" => set_discard,
            "pop" => set_pop,
            "clear" => set_clear,
            "copy" => set_copy,
            "union" => set_union,
            "intersection" => set_intersection,
            "difference" => set_difference,
            "symmetric_difference" => set_symmetric_difference,
            "update" => set_update,
            "issubset" => set_issubset,
            "issuperset" => set_issuperset,
            "isdisjoint" => set_isdisjoint,
            "__iter__" => iter_self,
            _ => return None,
        },
        "tuple" => match name {
            "count" => tuple_count,
            "index" => tuple_index,
            "__iter__" => iter_self,
            _ => return None,
        },
        "int" | "bool" => match name {
            "bit_length" => int_bit_length,
            "bit_count" => int_bit_count,
            "conjugate" => num_conjugate,
            _ => return None,
        },
        "float" => match name {
            "is_integer" => float_is_integer,
            "conjugate" => num_conjugate,
            _ => return None,
        },
        "range" => match name {
            "count" => range_count,
            "index" => range_index,
            "__iter__" => iter_self,
            _ => return None,
        },
        "iterator" => match name {
            "__next__" => iter_next_method,
            "__iter__" => iter_self,
            "__len__" => iter_len,
            _ => return None,
        },
        "TextIOWrapper" => match name {
            "read" => file_read,
            "readline" => file_readline,
            "readlines" => file_readlines,
            "write" => file_write,
            "writelines" => file_writelines,
            "flush" => file_flush,
            "close" => file_close,
            "__enter__" => file_enter,
            "__exit__" => file_exit,
            "__iter__" => file_iter,
            "__next__" => file_next,
            _ => return None,
        },
        _ => return None,
    };
    Some(f)
}

/// 内置类型的“类方法”，例如 `dict.fromkeys`。
pub fn lookup_class_method(type_name: &str, name: &str) -> Option<Nf> {
    match (type_name, name) {
        ("dict", "fromkeys") => Some(dict_fromkeys),
        _ => None,
    }
}

/// 供 `dir()` 使用的方法名列表。
pub fn method_names(type_name: &str) -> Vec<&'static str> {
    match type_name {
        "list" => vec![
            "append", "clear", "copy", "count", "extend", "index", "insert", "pop", "remove",
            "reverse", "sort",
        ],
        "dict" => vec![
            "clear",
            "copy",
            "get",
            "items",
            "keys",
            "pop",
            "popitem",
            "setdefault",
            "update",
            "values",
        ],
        "set" => vec![
            "add",
            "clear",
            "copy",
            "difference",
            "discard",
            "intersection",
            "isdisjoint",
            "issubset",
            "issuperset",
            "pop",
            "remove",
            "symmetric_difference",
            "union",
            "update",
        ],
        "str" => vec![
            "capitalize",
            "casefold",
            "center",
            "count",
            "endswith",
            "expandtabs",
            "find",
            "format",
            "index",
            "isalnum",
            "isalpha",
            "isdigit",
            "islower",
            "isspace",
            "isupper",
            "join",
            "ljust",
            "lower",
            "lstrip",
            "partition",
            "removeprefix",
            "removesuffix",
            "replace",
            "rfind",
            "rindex",
            "rjust",
            "rpartition",
            "rsplit",
            "rstrip",
            "split",
            "splitlines",
            "startswith",
            "strip",
            "swapcase",
            "title",
            "upper",
            "zfill",
        ],
        "tuple" => vec!["count", "index"],
        _ => vec![],
    }
}

/// 供类型构造使用：把 map 参数转换为字典键值对。
pub fn kwargs_to_pairs(kwargs: &[(String, Value)]) -> Vec<(Value, Value)> {
    kwargs
        .iter()
        .map(|(k, v)| (Value::str_from(k.clone()), v.clone()))
        .collect()
}
