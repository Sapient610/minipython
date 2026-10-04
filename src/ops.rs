//! 运算符、属性访问、下标访问、字符串化等对象协议。

use crate::ast::{CmpOp, UnOp};
use crate::interp::{EResult, Interp, Signal};
use crate::lexer::Op;
use crate::value::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// 大容器重复的上限，防止内存被瞬间打满。
const MAX_REPEAT: usize = 5_000_000;

impl Interp {
    // ---------------- 真值 ----------------

    pub fn truthy(&mut self, v: &Value) -> EResult<bool> {
        if let Some(b) = truthy_builtin(v) {
            return Ok(b);
        }
        if let Value::Instance(i) = v {
            if let Some(m) = self.find_class_attr(&i.class, "__bool__") {
                let r = self.call_value(&m, vec![v.clone()], Vec::new())?;
                return self.truthy(&r);
            }
            if let Some(m) = self.find_class_attr(&i.class, "__len__") {
                let r = self.call_value(&m, vec![v.clone()], Vec::new())?;
                let n = self.as_index(&r)?;
                return Ok(n != 0);
            }
        }
        Ok(true)
    }

    /// 把值当成整数使用（`__index__`）。
    pub fn as_index(&mut self, v: &Value) -> EResult<i64> {
        match v {
            Value::Int(i) => Ok(*i),
            Value::Bool(b) => Ok(*b as i64),
            Value::Instance(i) => {
                if let Some(m) = self.find_class_attr(&i.class, "__index__") {
                    let r = self.call_value(&m, vec![v.clone()], Vec::new())?;
                    return self.as_index(&r);
                }
                self.err(
                    "TypeError",
                    format!(
                        "'{}' object cannot be interpreted as an integer",
                        i.class.name
                    ),
                )
            }
            other => self.err(
                "TypeError",
                format!(
                    "'{}' object cannot be interpreted as an integer",
                    other.type_name()
                ),
            ),
        }
    }

    // ---------------- 类方法查找 ----------------

    pub fn find_class_attr(&self, c: &Rc<ClassData>, name: &str) -> Option<Value> {
        if let Some(v) = c.dict.borrow().get(name) {
            return Some(v.clone());
        }
        for k in &c.mro {
            if let Some(v) = k.dict.borrow().get(name) {
                return Some(v.clone());
            }
        }
        None
    }

    pub fn class_is_subclass(&self, c: &Rc<ClassData>, base: &Rc<ClassData>) -> bool {
        if let Some(b) = &base.builtin {
            return match b.as_str() {
                "object" => true,
                "int" => c.name == "bool" || c.name == "int",
                _ => c.name == *b,
            };
        }
        Rc::ptr_eq(c, base) || c.mro.iter().any(|x| Rc::ptr_eq(x, base))
    }

    pub fn is_instance_of(&mut self, v: &Value, cls: &Value) -> EResult<bool> {
        match cls {
            Value::Tuple(items) => {
                for i in items.iter() {
                    if self.is_instance_of(v, i)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Value::Class(c) => {
                if let Some(b) = &c.builtin {
                    let tn = v.type_name();
                    return Ok(match b.as_str() {
                        "object" => true,
                        "int" => tn == "int" || tn == "bool",
                        "type" => matches!(v, Value::Class(_)),
                        "bool" => tn == "bool",
                        other => tn == other,
                    });
                }
                match v {
                    Value::Instance(i) => Ok(self.class_is_subclass(&i.class, c)),
                    _ => Ok(false),
                }
            }
            other => self.err(
                "TypeError",
                format!(
                    "isinstance() arg 2 must be a type or tuple of types, not {}",
                    other.type_name()
                ),
            ),
        }
    }

    // ---------------- 属性访问 ----------------

    pub fn get_attr(&mut self, obj: &Value, name: &str) -> EResult<Value> {
        match obj {
            Value::Instance(i) => self.get_instance_attr(i, name),
            Value::Class(c) => self.get_class_attr(c, name),
            Value::Module(m) => match m.dict.borrow().get(name) {
                Some(v) => Ok(v.clone()),
                None => self.err(
                    "AttributeError",
                    format!("module '{}' has no attribute '{}'", m.name, name),
                ),
            },
            Value::Super(s) => {
                let mro = s.class.mro.clone();
                for c in mro {
                    let found = c.dict.borrow().get(name).cloned();
                    if let Some(v) = found {
                        return self.bind_class_attr(v, &s.obj, &c);
                    }
                }
                self.err(
                    "AttributeError",
                    format!("'super' object has no attribute '{}'", name),
                )
            }
            Value::Property(p) => match name {
                "getter" | "setter" | "deleter" => {
                    let f = match name {
                        "getter" => crate::methods::property_getter,
                        "setter" => crate::methods::property_setter,
                        _ => crate::methods::property_deleter,
                    };
                    Ok(Value::BoundNative(Rc::new(BoundNativeData {
                        recv: Value::Property(p.clone()),
                        func: Rc::new(NativeData {
                            name: format!("property.{}", name),
                            func: f,
                        }),
                    })))
                }
                _ => self.err(
                    "AttributeError",
                    format!("'property' object has no attribute '{}'", name),
                ),
            },
            Value::Slice(s) => match name {
                "start" => Ok(opt_int(s.lower)),
                "stop" => Ok(opt_int(s.upper)),
                "step" => Ok(opt_int(s.step)),
                _ => self.err(
                    "AttributeError",
                    format!("'slice' object has no attribute '{}'", name),
                ),
            },
            Value::File(_) | Value::Iterator(_) => match self.builtin_method(obj, name) {
                Some(v) => Ok(v),
                None => self.err(
                    "AttributeError",
                    format!("'{}' object has no attribute '{}'", obj.type_name(), name),
                ),
            },
            Value::Func(f) => match name {
                "__name__" => Ok(Value::str_from(f.name.clone())),
                _ => self.err(
                    "AttributeError",
                    format!("'function' object has no attribute '{}'", name),
                ),
            },
            _ => match self.builtin_method(obj, name) {
                Some(v) => Ok(v),
                None => self.err(
                    "AttributeError",
                    format!("'{}' object has no attribute '{}'", obj.type_name(), name),
                ),
            },
        }
    }

    /// 把从类字典中找到的值绑定到实例（处理描述符）。
    pub fn bind_class_attr(
        &mut self,
        v: Value,
        recv: &Value,
        owner: &Rc<ClassData>,
    ) -> EResult<Value> {
        Ok(match &v {
            Value::Property(p) => {
                let g = p.getter.clone();
                return self.call_value(&g, vec![recv.clone()], Vec::new());
            }
            Value::Descriptor(d) => match d.kind {
                DescriptorKind::Static => d.func.clone(),
                DescriptorKind::Class => Value::BoundMethod(Rc::new(BoundMethodData {
                    func: d.func.clone(),
                    recv: Value::Class(owner.clone()),
                    owner: Some(owner.clone()),
                })),
            },
            Value::Func(_) => Value::BoundMethod(Rc::new(BoundMethodData {
                func: v.clone(),
                recv: recv.clone(),
                owner: Some(owner.clone()),
            })),
            Value::Native(_) => match &v {
                Value::Native(n) => Value::BoundNative(Rc::new(BoundNativeData {
                    recv: recv.clone(),
                    func: n.clone(),
                })),
                _ => v,
            },
            _ => v,
        })
    }

    fn builtin_method(&mut self, obj: &Value, name: &str) -> Option<Value> {
        let tn = obj.type_name();
        let f = crate::methods::lookup_method(&tn, name)?;
        Some(Value::BoundNative(Rc::new(BoundNativeData {
            recv: obj.clone(),
            func: Rc::new(NativeData {
                name: format!("{}.{}", tn, name),
                func: f,
            }),
        })))
    }

    fn get_instance_attr(&mut self, i: &Rc<InstanceData>, name: &str) -> EResult<Value> {
        if let Some(v) = self.find_class_attr(&i.class, name) {
            match &v {
                Value::Property(p) => {
                    let g = p.getter.clone();
                    return self.call_value(&g, vec![Value::Instance(i.clone())], Vec::new());
                }
                Value::Descriptor(d) => {
                    return Ok(match d.kind {
                        DescriptorKind::Static => d.func.clone(),
                        DescriptorKind::Class => Value::BoundMethod(Rc::new(BoundMethodData {
                            func: d.func.clone(),
                            recv: Value::Class(i.class.clone()),
                            owner: Some(i.class.clone()),
                        })),
                    })
                }
                Value::Func(_) | Value::Native(_) => {
                    if let Some(v2) = i.dict.borrow().get(name) {
                        return Ok(v2.clone());
                    }
                    return self.bind_class_attr(
                        v.clone(),
                        &Value::Instance(i.clone()),
                        &i.class.clone(),
                    );
                }
                _ => {}
            }
            if let Some(v2) = i.dict.borrow().get(name) {
                return Ok(v2.clone());
            }
            return Ok(v);
        }
        if let Some(v) = i.dict.borrow().get(name) {
            return Ok(v.clone());
        }
        match name {
            "__class__" => Ok(Value::Class(i.class.clone())),
            "__dict__" => {
                let mut d = DictData::new();
                for (k, v) in i.dict.borrow().iter() {
                    d.insert(Value::str_from(k.clone()), v.clone());
                }
                Ok(Value::Dict(Rc::new(RefCell::new(d))))
            }
            _ => self.err(
                "AttributeError",
                format!("'{}' object has no attribute '{}'", i.class.name, name),
            ),
        }
    }

    fn get_class_attr(&mut self, c: &Rc<ClassData>, name: &str) -> EResult<Value> {
        match name {
            "__name__" => return Ok(Value::str_from(c.name.clone())),
            "__bases__" => {
                return Ok(Value::tuple(
                    c.bases.iter().map(|b| Value::Class(b.clone())).collect(),
                ))
            }
            "__mro__" => {
                let mut all = vec![Value::Class(c.clone())];
                all.extend(c.mro.iter().map(|b| Value::Class(b.clone())));
                return Ok(Value::tuple(all));
            }
            "__dict__" => {
                let mut d = DictData::new();
                for (k, v) in c.dict.borrow().iter() {
                    d.insert(Value::str_from(k.clone()), v.clone());
                }
                return Ok(Value::Dict(Rc::new(RefCell::new(d))));
            }
            _ => {}
        }
        if let Some(v) = self.find_class_attr(c, name) {
            return Ok(match &v {
                Value::Func(_) => v.clone(),
                Value::Descriptor(d) => match d.kind {
                    DescriptorKind::Static => d.func.clone(),
                    DescriptorKind::Class => Value::BoundMethod(Rc::new(BoundMethodData {
                        func: d.func.clone(),
                        recv: Value::Class(c.clone()),
                        owner: Some(c.clone()),
                    })),
                },
                _ => v,
            });
        }
        self.err(
            "AttributeError",
            format!("type object '{}' has no attribute '{}'", c.name, name),
        )
    }

    pub fn set_attr(&mut self, obj: &Value, name: &str, value: Value) -> EResult<()> {
        match obj {
            Value::Instance(i) => {
                if let Some(Value::Property(p)) = self.find_class_attr(&i.class, name) {
                    match &p.setter {
                        Some(s) => {
                            let s = s.clone();
                            self.call_value(
                                &s,
                                vec![Value::Instance(i.clone()), value],
                                Vec::new(),
                            )?;
                            return Ok(());
                        }
                        None => {
                            return self.err(
                                "AttributeError",
                                format!(
                                    "property '{}' of '{}' object has no setter",
                                    name, i.class.name
                                ),
                            )
                        }
                    }
                }
                i.dict.borrow_mut().insert(name.to_string(), value);
                Ok(())
            }
            Value::Class(c) => {
                c.dict.borrow_mut().insert(name.to_string(), value);
                Ok(())
            }
            Value::Module(m) => {
                m.dict.borrow_mut().insert(name.to_string(), value);
                Ok(())
            }
            other => self.err(
                "AttributeError",
                format!("'{}' object has no attribute '{}'", other.type_name(), name),
            ),
        }
    }

    pub fn del_attr(&mut self, obj: &Value, name: &str) -> EResult<()> {
        match obj {
            Value::Instance(i) => {
                if i.dict.borrow_mut().remove(name).is_some() {
                    Ok(())
                } else {
                    self.err(
                        "AttributeError",
                        format!("'{}' object has no attribute '{}'", i.class.name, name),
                    )
                }
            }
            Value::Class(c) => {
                c.dict.borrow_mut().remove(name);
                Ok(())
            }
            Value::Module(m) => {
                m.dict.borrow_mut().remove(name);
                Ok(())
            }
            other => self.err(
                "AttributeError",
                format!("'{}' object has no attribute '{}'", other.type_name(), name),
            ),
        }
    }

    // ---------------- 下标 ----------------

    pub fn get_index(&mut self, obj: &Value, idx: &Value) -> EResult<Value> {
        match obj {
            Value::List(l) => {
                let items = l.borrow();
                match idx {
                    Value::Slice(s) => {
                        let pos = self.slice_positions(items.len() as i64, s)?;
                        Ok(Value::list(
                            pos.into_iter().map(|i| items[i as usize].clone()).collect(),
                        ))
                    }
                    _ => {
                        let i = self.index_of(idx, items.len(), "list")?;
                        Ok(items[i as usize].clone())
                    }
                }
            }
            Value::Tuple(t) => match idx {
                Value::Slice(s) => {
                    let pos = self.slice_positions(t.len() as i64, s)?;
                    Ok(Value::tuple(
                        pos.into_iter().map(|i| t[i as usize].clone()).collect(),
                    ))
                }
                _ => {
                    let i = self.index_of(idx, t.len(), "tuple")?;
                    Ok(t[i as usize].clone())
                }
            },
            Value::Str(s) => {
                let chars: Vec<char> = s.chars().collect();
                match idx {
                    Value::Slice(sl) => {
                        let pos = self.slice_positions(chars.len() as i64, sl)?;
                        Ok(Value::str_from(
                            pos.into_iter()
                                .map(|i| chars[i as usize])
                                .collect::<String>(),
                        ))
                    }
                    _ => {
                        let i = self.index_of(idx, chars.len(), "str")?;
                        Ok(Value::str_from(chars[i as usize].to_string()))
                    }
                }
            }
            Value::Dict(d) => {
                if matches!(idx, Value::Slice(_)) {
                    return self.err("TypeError", "字典不支持切片");
                }
                if !is_hashable(idx) {
                    return self.err(
                        "TypeError",
                        format!("unhashable type: '{}'", idx.type_name()),
                    );
                }
                let found = d.borrow().get(idx);
                match found {
                    Some(v) => Ok(v),
                    None => Err(Signal::Error(self.key_error(idx))),
                }
            }
            Value::Range(r) => match idx {
                Value::Slice(s) => {
                    let pos = self.slice_positions(r.len(), s)?;
                    let items: Vec<Value> = pos
                        .into_iter()
                        .map(|i| Value::Int(r.start + i * r.step))
                        .collect();
                    Ok(Value::list(items))
                }
                _ => {
                    let i = self.index_of(idx, r.len() as usize, "range")?;
                    Ok(Value::Int(r.start + i * r.step))
                }
            },
            Value::Instance(i) => {
                let m = match self.find_class_attr(&i.class, "__getitem__") {
                    Some(m) => m,
                    None => {
                        return self.err(
                            "TypeError",
                            format!("'{}' object is not subscriptable", i.class.name),
                        )
                    }
                };
                self.call_value(&m, vec![obj.clone(), idx.clone()], Vec::new())
            }
            other => self.err(
                "TypeError",
                format!("'{}' object is not subscriptable", other.type_name()),
            ),
        }
    }

    pub fn set_index(&mut self, obj: &Value, idx: &Value, value: Value) -> EResult<()> {
        match obj {
            Value::List(l) => match idx {
                Value::Slice(s) => {
                    let vals = self.collect_iter(&value)?;
                    let step = s.step.unwrap_or(1);
                    if step == 0 {
                        return self.err("ValueError", "slice step cannot be zero");
                    }
                    let len = l.borrow().len() as i64;
                    let pos = self.slice_positions(len, s)?;
                    if step == 1 {
                        let start = if pos.is_empty() {
                            self.slice_indices(len, s)?.0
                        } else {
                            pos[0]
                        };
                        let count = pos.len() as i64;
                        let mut items = l.borrow_mut();
                        let mut tail = items.split_off((start + count) as usize);
                        items.truncate(start as usize);
                        items.extend(vals);
                        items.append(&mut tail);
                    } else {
                        if pos.len() != vals.len() {
                            return self.err(
                                    "ValueError",
                                    format!(
                                        "attempt to assign sequence of size {} to extended slice of size {}",
                                        vals.len(),
                                        pos.len()
                                    ),
                                );
                        }
                        let mut items = l.borrow_mut();
                        for (i, v) in pos.into_iter().zip(vals) {
                            items[i as usize] = v;
                        }
                    }
                    Ok(())
                }
                _ => {
                    let len = l.borrow().len();
                    let i = self.index_of(idx, len, "list")?;
                    l.borrow_mut()[i as usize] = value;
                    Ok(())
                }
            },
            Value::Dict(d) => {
                if !is_hashable(idx) {
                    return self.err(
                        "TypeError",
                        format!("unhashable type: '{}'", idx.type_name()),
                    );
                }
                d.borrow_mut().insert(idx.clone(), value);
                Ok(())
            }
            Value::Instance(i) => {
                let m = match self.find_class_attr(&i.class, "__setitem__") {
                    Some(m) => m,
                    None => {
                        return self.err(
                            "TypeError",
                            format!("'{}' object does not support item assignment", i.class.name),
                        )
                    }
                };
                self.call_value(&m, vec![obj.clone(), idx.clone(), value], Vec::new())?;
                Ok(())
            }
            other => self.err(
                "TypeError",
                format!(
                    "'{}' object does not support item assignment",
                    other.type_name()
                ),
            ),
        }
    }

    pub fn del_index(&mut self, obj: &Value, idx: &Value) -> EResult<()> {
        match obj {
            Value::List(l) => {
                let len = l.borrow().len();
                let i = self.index_of(idx, len, "list")?;
                l.borrow_mut().remove(i as usize);
                Ok(())
            }
            Value::Dict(d) => {
                let removed = d.borrow_mut().remove(idx);
                if removed.is_some() {
                    Ok(())
                } else {
                    Err(Signal::Error(self.key_error(idx)))
                }
            }
            Value::Set(s) => {
                let removed = s.borrow_mut().remove(idx);
                if removed.is_some() {
                    Ok(())
                } else {
                    Err(Signal::Error(self.key_error(idx)))
                }
            }
            Value::Instance(i) => {
                let m = match self.find_class_attr(&i.class, "__delitem__") {
                    Some(m) => m,
                    None => {
                        return self.err(
                            "TypeError",
                            format!("'{}' object does not support item deletion", i.class.name),
                        )
                    }
                };
                self.call_value(&m, vec![obj.clone(), idx.clone()], Vec::new())?;
                Ok(())
            }
            other => self.err(
                "TypeError",
                format!(
                    "'{}' object does not support item deletion",
                    other.type_name()
                ),
            ),
        }
    }

    pub fn key_repr(&mut self, k: &Value) -> EResult<String> {
        let r = self.value_repr(k)?;
        Ok(r)
    }

    /// 把下标规范化为非负整数。
    pub fn index_of(&mut self, idx: &Value, len: usize, what: &str) -> EResult<i64> {
        let i = self.as_index(idx)?;
        let i = if i < 0 { i + len as i64 } else { i };
        if i < 0 || i >= len as i64 {
            let msg = match what {
                "list" => "list index out of range".to_string(),
                "tuple" => "tuple index out of range".to_string(),
                "str" => "string index out of range".to_string(),
                "range" => "range object index out of range".to_string(),
                other => format!("{} index out of range", other),
            };
            return self.err("IndexError", msg);
        }
        Ok(i)
    }

    /// CPython 的 slice.indices 逻辑。
    pub fn slice_indices(&self, len: i64, s: &SliceData) -> EResult<(i64, i64, i64)> {
        let step = s.step.unwrap_or(1);
        if step == 0 {
            return Err(Signal::Error(
                self.pyerr("ValueError", "slice step cannot be zero"),
            ));
        }
        let normalize = |i: i64| if i < 0 { i + len } else { i };
        if step > 0 {
            let start = match s.lower {
                Some(i) => normalize(i).clamp(0, len),
                None => 0,
            };
            let stop = match s.upper {
                Some(i) => normalize(i).clamp(0, len),
                None => len,
            };
            Ok((start, stop, step))
        } else {
            let start = match s.lower {
                Some(i) => normalize(i).clamp(-1, len - 1),
                None => len - 1,
            };
            let stop = match s.upper {
                Some(i) => normalize(i).clamp(-1, len - 1),
                None => -1,
            };
            Ok((start, stop, step))
        }
    }

    pub fn slice_positions(&self, len: i64, s: &SliceData) -> EResult<Vec<i64>> {
        let (start, stop, step) = self.slice_indices(len, s)?;
        let mut out = Vec::new();
        let mut i = start;
        if step > 0 {
            while i < stop {
                out.push(i);
                i += step;
            }
        } else {
            while i > stop {
                out.push(i);
                i += step;
            }
        }
        Ok(out)
    }

    // ---------------- 字符串化 ----------------

    pub fn value_repr(&mut self, v: &Value) -> EResult<String> {
        let mut seen = Vec::new();
        self.value_repr_inner(v, &mut seen)
    }

    pub fn value_str(&mut self, v: &Value) -> EResult<String> {
        match v {
            Value::Str(s) => Ok(s.to_string()),
            Value::Instance(i) => {
                if self.class_is_exception(&i.class) {
                    if let Some(s) = self.instance_str(i) {
                        return Ok(s);
                    }
                }
                if let Some(m) = self.find_class_attr(&i.class, "__str__") {
                    let r = self.call_value(&m, vec![v.clone()], Vec::new())?;
                    return match r {
                        Value::Str(s) => Ok(s.to_string()),
                        other => self.err(
                            "TypeError",
                            format!("__str__ returned non-string (type {})", other.type_name()),
                        ),
                    };
                }
                if let Some(m) = self.find_class_attr(&i.class, "__repr__") {
                    let r = self.call_value(&m, vec![v.clone()], Vec::new())?;
                    return match r {
                        Value::Str(s) => Ok(s.to_string()),
                        other => self.err(
                            "TypeError",
                            format!("__repr__ returned non-string (type {})", other.type_name()),
                        ),
                    };
                }
                self.default_instance_repr(i)
            }
            other => self.value_repr(other),
        }
    }

    /// 异常对象的 repr：`ValueError('bad')`
    pub fn exception_repr(&self, i: &Rc<InstanceData>) -> EResult<String> {
        let d = i.dict.borrow();
        let args = match d.get("args") {
            Some(Value::Tuple(t)) => t.as_ref().clone(),
            _ => Vec::new(),
        };
        let mut parts = Vec::new();
        for a in &args {
            parts.push(crate::value::builtin_repr(a));
        }
        Ok(format!("{}({})", i.class.name, parts.join(", ")))
    }

    fn default_instance_repr(&self, i: &Rc<InstanceData>) -> EResult<String> {
        Ok(format!(
            "<{} object at 0x{:012x}>",
            i.class.name,
            Rc::as_ptr(i) as usize
        ))
    }

    fn value_repr_inner(&mut self, v: &Value, seen: &mut Vec<usize>) -> EResult<String> {
        Ok(match v {
            Value::None => "None".to_string(),
            Value::Bool(true) => "True".to_string(),
            Value::Bool(false) => "False".to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => float_repr(*f),
            Value::Str(s) => str_repr(s),
            Value::List(l) => {
                let ptr = Rc::as_ptr(l) as *const u8 as usize;
                if seen.contains(&ptr) {
                    return Ok("[...]".to_string());
                }
                seen.push(ptr);
                let items = l.borrow().clone();
                let mut parts = Vec::new();
                for it in &items {
                    parts.push(self.value_repr_inner(it, seen)?);
                }
                seen.pop();
                format!("[{}]", parts.join(", "))
            }
            Value::Tuple(t) => {
                let ptr = Rc::as_ptr(t) as *const u8 as usize;
                if seen.contains(&ptr) {
                    return Ok("(...)".to_string());
                }
                seen.push(ptr);
                let mut parts = Vec::new();
                for it in t.iter() {
                    parts.push(self.value_repr_inner(it, seen)?);
                }
                seen.pop();
                if parts.len() == 1 {
                    format!("({},)", parts[0])
                } else {
                    format!("({})", parts.join(", "))
                }
            }
            Value::Dict(d) => {
                let ptr = Rc::as_ptr(d) as *const u8 as usize;
                if seen.contains(&ptr) {
                    return Ok("{...}".to_string());
                }
                seen.push(ptr);
                let entries = d.borrow().entries.clone();
                let mut parts = Vec::new();
                for (k, val) in &entries {
                    parts.push(format!(
                        "{}: {}",
                        self.value_repr_inner(k, seen)?,
                        self.value_repr_inner(val, seen)?
                    ));
                }
                seen.pop();
                format!("{{{}}}", parts.join(", "))
            }
            Value::Set(s) => {
                let ptr = Rc::as_ptr(s) as *const u8 as usize;
                if seen.contains(&ptr) {
                    return Ok("{...}".to_string());
                }
                seen.push(ptr);
                let items = s.borrow().items.clone();
                if items.is_empty() {
                    seen.pop();
                    return Ok("set()".to_string());
                }
                let mut parts = Vec::new();
                for it in &items {
                    parts.push(self.value_repr_inner(it, seen)?);
                }
                seen.pop();
                format!("{{{}}}", parts.join(", "))
            }
            Value::Range(r) => {
                if r.step == 1 {
                    format!("range({}, {})", r.start, r.stop)
                } else {
                    format!("range({}, {}, {})", r.start, r.stop, r.step)
                }
            }
            Value::Func(f) => {
                if f.is_lambda {
                    format!("<function <lambda> at 0x{:012x}>", Rc::as_ptr(f) as usize)
                } else {
                    format!("<function {} at 0x{:012x}>", f.name, Rc::as_ptr(f) as usize)
                }
            }
            Value::Native(n) => format!("<built-in function {}>", n.name),
            Value::BoundNative(b) => format!(
                "<built-in method {} of {} object at 0x{:012x}>",
                b.func.name,
                b.recv.type_name(),
                Rc::as_ptr(b) as usize
            ),
            Value::BoundMethod(b) => {
                let name = match &b.func {
                    Value::Func(f) => f.name.clone(),
                    other => other.type_name(),
                };
                format!(
                    "<bound method {} of {}>",
                    name,
                    self.value_repr_inner(&b.recv, seen)?
                )
            }
            Value::Class(c) => format!("<class '{}.{}'>", c.module, c.name),
            Value::Instance(i) => {
                if let Some(m) = self.find_class_attr(&i.class, "__repr__") {
                    let r = self.call_value(&m, vec![v.clone()], Vec::new())?;
                    return match r {
                        Value::Str(s) => Ok(s.to_string()),
                        other => self.err(
                            "TypeError",
                            format!("__repr__ returned non-string (type {})", other.type_name()),
                        ),
                    };
                }
                if self.class_is_exception(&i.class) {
                    return self.exception_repr(i);
                }
                return self.default_instance_repr(i);
            }
            Value::Module(m) => format!("<module '{}'>", m.name),
            Value::Slice(s) => format!(
                "slice({}, {}, {})",
                opt_int_repr(s.lower),
                opt_int_repr(s.upper),
                opt_int_repr(s.step)
            ),
            Value::Iterator(_) => format!("<iterator object at 0x{:012x}>", 0),
            Value::File(f) => format!(
                "<_io.TextIOWrapper name='{}' mode='{}'>",
                f.borrow().name,
                f.borrow().mode
            ),
            Value::Descriptor(d) => match d.kind {
                DescriptorKind::Static => {
                    format!("<staticmethod object at 0x{:012x}>", Rc::as_ptr(d) as usize)
                }
                DescriptorKind::Class => {
                    format!("<classmethod object at 0x{:012x}>", Rc::as_ptr(d) as usize)
                }
            },
            Value::Property(p) => format!("<property object at 0x{:012x}>", Rc::as_ptr(p) as usize),
            Value::Super(s) => {
                format!("<super: <class '{}'>, {}>", s.class.name, s.obj.type_name())
            }
        })
    }

    pub fn format_with_spec(&mut self, v: &Value, spec: &str) -> EResult<String> {
        if let Value::Instance(i) = v {
            if let Some(m) = self.find_class_attr(&i.class, "__format__") {
                let r = self.call_value(&m, vec![v.clone(), Value::str_from(spec)], Vec::new())?;
                return match r {
                    Value::Str(s) => Ok(s.to_string()),
                    other => self.err(
                        "TypeError",
                        format!("__format__ 必须返回字符串，而不是 {}", other.type_name()),
                    ),
                };
            }
        }
        match format_value(v, spec) {
            Ok(s) => Ok(s),
            Err(e) => self.err("ValueError", e),
        }
    }

    // ---------------- 比较 ----------------

    pub fn is_identical(&self, a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::None, Value::None) => true,
            (Value::Bool(x), Value::Bool(y)) => x == y,
            (Value::Int(x), Value::Int(y)) => x == y && (-5..=256).contains(x),
            (Value::Float(x), Value::Float(y)) => x.to_bits() == y.to_bits(),
            (Value::Str(x), Value::Str(y)) => Rc::ptr_eq(x, y),
            (Value::List(x), Value::List(y)) => Rc::ptr_eq(x, y),
            (Value::Tuple(x), Value::Tuple(y)) => Rc::ptr_eq(x, y),
            (Value::Dict(x), Value::Dict(y)) => Rc::ptr_eq(x, y),
            (Value::Set(x), Value::Set(y)) => Rc::ptr_eq(x, y),
            (Value::Func(x), Value::Func(y)) => Rc::ptr_eq(x, y),
            (Value::Class(x), Value::Class(y)) => Rc::ptr_eq(x, y),
            (Value::Instance(x), Value::Instance(y)) => Rc::ptr_eq(x, y),
            (Value::Module(x), Value::Module(y)) => Rc::ptr_eq(x, y),
            (Value::Slice(x), Value::Slice(y)) => Rc::ptr_eq(x, y),
            _ => false,
        }
    }

    pub fn eq_values(&mut self, a: &Value, b: &Value) -> EResult<bool> {
        match (a, b) {
            (Value::Instance(_), _) | (_, Value::Instance(_)) => {
                if let Value::Instance(i) = a {
                    if let Some(m) = self.find_class_attr(&i.class, "__eq__") {
                        let r = self.call_value(&m, vec![a.clone(), b.clone()], Vec::new())?;
                        return self.truthy(&r);
                    }
                }
                if let Value::Instance(i) = b {
                    if let Some(m) = self.find_class_attr(&i.class, "__eq__") {
                        let r = self.call_value(&m, vec![b.clone(), a.clone()], Vec::new())?;
                        return self.truthy(&r);
                    }
                }
                Ok(self.is_identical(a, b))
            }
            (Value::List(x), Value::List(y)) => {
                let xs = x.borrow().clone();
                let ys = y.borrow().clone();
                if xs.len() != ys.len() {
                    return Ok(false);
                }
                for (p, q) in xs.iter().zip(ys.iter()) {
                    if !self.eq_values(p, q)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            (Value::Tuple(x), Value::Tuple(y)) => {
                if x.len() != y.len() {
                    return Ok(false);
                }
                for (p, q) in x.iter().zip(y.iter()) {
                    if !self.eq_values(p, q)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            (Value::Dict(x), Value::Dict(y)) => {
                let xs = x.borrow().entries.clone();
                let ys = y.borrow().entries.clone();
                if xs.len() != ys.len() {
                    return Ok(false);
                }
                for (k, v) in xs.iter() {
                    match ys.iter().find(|(k2, _)| key_equal(k, k2)) {
                        Some((_, v2)) => {
                            if !self.eq_values(v, v2)? {
                                return Ok(false);
                            }
                        }
                        None => return Ok(false),
                    }
                }
                Ok(true)
            }
            (Value::Set(x), Value::Set(y)) => {
                let xs = x.borrow().items.clone();
                let ys = y.borrow().items.clone();
                if xs.len() != ys.len() {
                    return Ok(false);
                }
                Ok(xs.iter().all(|v| ys.iter().any(|w| key_equal(v, w))))
            }
            _ => Ok(key_equal(a, b)),
        }
    }

    /// 供排序使用的 `<` 比较。
    pub fn lt_values(&mut self, a: &Value, b: &Value) -> EResult<bool> {
        self.order_compare(a, b, Op::Lt)
    }

    pub fn compare(&mut self, op: CmpOp, a: &Value, b: &Value) -> EResult<bool> {
        match op {
            CmpOp::Is => Ok(self.is_identical(a, b)),
            CmpOp::IsNot => Ok(!self.is_identical(a, b)),
            CmpOp::Eq => self.eq_values(a, b),
            CmpOp::NotEq => Ok(!self.eq_values(a, b)?),
            CmpOp::In => self.contains(b, a),
            CmpOp::NotIn => Ok(!self.contains(b, a)?),
            CmpOp::Lt => self.order_compare(a, b, Op::Lt),
            CmpOp::LtE => self.order_compare(a, b, Op::Le),
            CmpOp::Gt => self.order_compare(a, b, Op::Gt),
            CmpOp::GtE => self.order_compare(a, b, Op::Ge),
        }
    }

    fn order_compare(&mut self, a: &Value, b: &Value, op: Op) -> EResult<bool> {
        // 数字
        if let (Some(x), Some(y)) = (a.as_number(), b.as_number()) {
            return Ok(match op {
                Op::Lt => x.as_f64() < y.as_f64(),
                Op::Le => x.as_f64() <= y.as_f64(),
                Op::Gt => x.as_f64() > y.as_f64(),
                _ => x.as_f64() >= y.as_f64(),
            });
        }
        if let (Value::Str(x), Value::Str(y)) = (a, b) {
            return Ok(match op {
                Op::Lt => x < y,
                Op::Le => x <= y,
                Op::Gt => x > y,
                _ => x >= y,
            });
        }
        match (a, b) {
            (Value::List(x), Value::List(y)) => {
                let xs = x.borrow().clone();
                let ys = y.borrow().clone();
                return self.seq_compare(&xs, &ys, op);
            }
            (Value::Tuple(x), Value::Tuple(y)) => {
                let xs = x.as_ref().clone();
                let ys = y.as_ref().clone();
                return self.seq_compare(&xs, &ys, op);
            }
            (Value::Set(x), Value::Set(y)) => {
                let xs = x.borrow().items.clone();
                let ys = y.borrow().items.clone();
                let subset = |p: &Vec<Value>, q: &Vec<Value>| {
                    p.iter().all(|v| q.iter().any(|w| key_equal(v, w)))
                };
                return Ok(match op {
                    Op::Lt => xs.len() < ys.len() && subset(&xs, &ys),
                    Op::Le => subset(&xs, &ys),
                    Op::Gt => xs.len() > ys.len() && subset(&ys, &xs),
                    _ => subset(&ys, &xs),
                });
            }
            _ => {}
        }
        // 用户自定义比较
        let name = match op {
            Op::Lt => "__lt__",
            Op::Le => "__le__",
            Op::Gt => "__gt__",
            _ => "__ge__",
        };
        if let Value::Instance(i) = a {
            if let Some(m) = self.find_class_attr(&i.class, name) {
                let r = self.call_value(&m, vec![a.clone(), b.clone()], Vec::new())?;
                return self.truthy(&r);
            }
        }
        if let Value::Instance(i) = b {
            if let Some(m) = self.find_class_attr(&i.class, name) {
                let r = self.call_value(&m, vec![b.clone(), a.clone()], Vec::new())?;
                return self.truthy(&r);
            }
        }
        self.err(
            "TypeError",
            format!(
                "'{}' not supported between instances of '{}' and '{}'",
                op.as_str(),
                a.type_name(),
                b.type_name()
            ),
        )
    }

    fn seq_compare(&mut self, xs: &[Value], ys: &[Value], op: Op) -> EResult<bool> {
        let n = xs.len().min(ys.len());
        for i in 0..n {
            if self.eq_values(&xs[i], &ys[i])? {
                continue;
            }
            let less = self.order_compare(&xs[i], &ys[i], Op::Lt)?;
            return Ok(match op {
                Op::Lt => less,
                Op::Le => less,
                Op::Gt => !less,
                _ => !less,
            });
        }
        Ok(match op {
            Op::Lt => xs.len() < ys.len(),
            Op::Le => xs.len() <= ys.len(),
            Op::Gt => xs.len() > ys.len(),
            _ => xs.len() >= ys.len(),
        })
    }

    pub fn contains(&mut self, haystack: &Value, needle: &Value) -> EResult<bool> {
        match haystack {
            Value::Str(s) => match needle {
                Value::Str(n) => Ok(s.contains(&**n)),
                other => self.err(
                    "TypeError",
                    format!(
                        "'in <string>' requires string as left operand, not {}",
                        other.type_name()
                    ),
                ),
            },
            Value::List(l) => {
                let items = l.borrow().clone();
                for it in &items {
                    if self.eq_values(it, needle)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Value::Tuple(t) => {
                for it in t.iter() {
                    if self.eq_values(it, needle)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Value::Dict(d) => Ok(d.borrow().contains(needle)),
            Value::Set(s) => Ok(s.borrow().contains(needle)),
            Value::Range(r) => match needle {
                Value::Int(i) => Ok(r.contains(*i)),
                Value::Bool(b) => Ok(r.contains(*b as i64)),
                _ => Ok(false),
            },
            Value::Instance(i) => {
                if let Some(m) = self.find_class_attr(&i.class, "__contains__") {
                    let r =
                        self.call_value(&m, vec![haystack.clone(), needle.clone()], Vec::new())?;
                    return self.truthy(&r);
                }
                let items = self.collect_iter(haystack)?;
                for it in &items {
                    if self.eq_values(it, needle)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            other => self.err(
                "TypeError",
                format!("argument of type '{}' is not iterable", other.type_name()),
            ),
        }
    }

    // ---------------- 算术运算 ----------------

    pub fn aug_binop(&mut self, op: Op, a: Value, b: Value) -> EResult<Value> {
        // 列表 / 集合的原地修改（与 Python 的 += 语义一致）
        if op == Op::Plus {
            if let (Value::List(x), Value::List(y)) = (&a, &b) {
                let items = y.borrow().clone();
                x.borrow_mut().extend(items);
                return Ok(a.clone());
            }
            if let (Value::Set(x), Value::Set(y)) = (&a, &b) {
                let items = y.borrow().items.clone();
                x.borrow_mut().extend_distinct(items);
                return Ok(a.clone());
            }
        }
        if op == Op::Star {
            if let (Value::List(x), Value::Int(n)) = (&a, &b) {
                let items = x.borrow().clone();
                let n = (*n).max(0) as usize;
                if items.len().saturating_mul(n) > MAX_REPEAT {
                    return self.err("MemoryError", "列表重复次数过大");
                }
                let mut new_items = Vec::with_capacity(items.len() * n);
                for _ in 0..n {
                    new_items.extend(items.iter().cloned());
                }
                x.borrow_mut().extend(new_items);
                return Ok(a.clone());
            }
        }
        self.binop(op, a, b)
    }

    pub fn binop(&mut self, op: Op, a: Value, b: Value) -> EResult<Value> {
        // 用户自定义类型
        if matches!(a, Value::Instance(_)) {
            if let Some(r) = self.try_dunder_binop(op, &a, &b, false)? {
                return Ok(r);
            }
        }
        if matches!(b, Value::Instance(_)) {
            if let Some(r) = self.try_dunder_binop(op, &a, &b, true)? {
                return Ok(r);
            }
        }
        use Op::*;
        match op {
            Plus => match (&a, &b) {
                (Value::Str(_), other) if !matches!(other, Value::Str(_)) => self.err(
                    "TypeError",
                    format!(
                        "can only concatenate str (not \"{}\") to str",
                        other.type_name()
                    ),
                ),
                (Value::Str(x), Value::Str(y)) => {
                    let mut s = String::with_capacity(x.len() + y.len());
                    s.push_str(x);
                    s.push_str(y);
                    Ok(Value::str_from(s))
                }
                (Value::List(x), Value::List(y)) => {
                    let mut v = x.borrow().clone();
                    v.extend(y.borrow().iter().cloned());
                    Ok(Value::list(v))
                }
                (Value::Tuple(x), Value::Tuple(y)) => {
                    let mut v = x.as_ref().clone();
                    v.extend(y.iter().cloned());
                    Ok(Value::tuple(v))
                }
                _ => self.arith(op, &a, &b),
            },
            Minus | Star | Slash | DoubleSlash | Percent | DoubleStar => match (&a, &b) {
                (Value::Set(x), Value::Set(y)) if op == Minus => {
                    let xs = x.borrow().items.clone();
                    let ys = y.borrow().items.clone();
                    let mut out = SetData::new();
                    for v in xs.iter() {
                        if !ys.iter().any(|w| key_equal(v, w)) {
                            out.insert(v.clone());
                        }
                    }
                    Ok(Value::Set(Rc::new(RefCell::new(out))))
                }
                (Value::Str(s), Value::Int(n)) if op == Star => self.str_repeat(s, &Value::Int(*n)),
                (Value::Int(n), Value::Str(s)) if op == Star => self.str_repeat(s, &Value::Int(*n)),
                (Value::Str(fmt), _) if op == Percent => self.percent_format(fmt, &b),
                (Value::List(_), Value::Int(_)) if op == Star => {
                    let items = match &a {
                        Value::List(l) => l.borrow().clone(),
                        _ => unreachable!(),
                    };
                    let n = match b {
                        Value::Int(n) => n.max(0) as usize,
                        _ => 0,
                    };
                    if items.len().saturating_mul(n) > MAX_REPEAT {
                        return self.err("MemoryError", "列表重复次数过大");
                    }
                    let mut out = Vec::with_capacity(items.len() * n);
                    for _ in 0..n {
                        out.extend(items.iter().cloned());
                    }
                    Ok(Value::list(out))
                }
                (Value::Int(_), Value::List(_)) if op == Star => {
                    self.binop(op, b.clone(), a.clone())
                }
                (Value::Tuple(_), Value::Int(_)) if op == Star => {
                    let items = match &a {
                        Value::Tuple(t) => t.as_ref().clone(),
                        _ => unreachable!(),
                    };
                    let n = match b {
                        Value::Int(n) => n.max(0) as usize,
                        _ => 0,
                    };
                    if items.len().saturating_mul(n) > MAX_REPEAT {
                        return self.err("MemoryError", "元组重复次数过大");
                    }
                    let mut out = Vec::with_capacity(items.len() * n);
                    for _ in 0..n {
                        out.extend(items.iter().cloned());
                    }
                    Ok(Value::tuple(out))
                }
                (Value::Int(_), Value::Tuple(_)) if op == Star => {
                    self.binop(op, b.clone(), a.clone())
                }
                _ => self.arith(op, &a, &b),
            },
            Amp | Pipe | Caret => match (&a, &b) {
                (Value::Set(x), Value::Set(y)) => {
                    let xs = x.borrow().items.clone();
                    let ys = y.borrow().items.clone();
                    let mut out = SetData::new();
                    match op {
                        Amp => {
                            for v in xs.iter() {
                                if ys.iter().any(|w| key_equal(v, w)) {
                                    out.insert(v.clone());
                                }
                            }
                        }
                        Pipe => {
                            for v in xs.iter().chain(ys.iter()) {
                                out.insert(v.clone());
                            }
                        }
                        _ => {
                            for v in xs.iter() {
                                if !ys.iter().any(|w| key_equal(v, w)) {
                                    out.insert(v.clone());
                                }
                            }
                            for v in ys.iter() {
                                if !xs.iter().any(|w| key_equal(v, w)) {
                                    out.insert(v.clone());
                                }
                            }
                        }
                    }
                    Ok(Value::Set(Rc::new(RefCell::new(out))))
                }
                _ => self.arith(op, &a, &b),
            },
            Shl | Shr => self.arith(op, &a, &b),
            _ => self.err(
                "TypeError",
                format!(
                    "unsupported operand type(s) for {}: '{}' and '{}'",
                    op.as_str(),
                    a.type_name(),
                    b.type_name()
                ),
            ),
        }
    }

    fn str_repeat(&mut self, s: &str, times: &Value) -> EResult<Value> {
        let n = match times {
            Value::Int(i) => (*i).max(0) as usize,
            other => {
                return self.err(
                    "TypeError",
                    format!(
                        "can't multiply sequence by non-int of type '{}'",
                        other.type_name()
                    ),
                )
            }
        };
        if s.len().saturating_mul(n) > MAX_REPEAT * 8 {
            return self.err("MemoryError", "字符串重复次数过大");
        }
        Ok(Value::str_from(s.repeat(n)))
    }

    fn arith(&mut self, op: Op, a: &Value, b: &Value) -> EResult<Value> {
        let (x, y) = match (a.as_number(), b.as_number()) {
            (Some(x), Some(y)) => (x, y),
            _ => {
                return self.err(
                    "TypeError",
                    format!(
                        "unsupported operand type(s) for {}: '{}' and '{}'",
                        op.as_str(),
                        a.type_name(),
                        b.type_name()
                    ),
                )
            }
        };
        use Op::*;
        let r = match (x, y) {
            (Num::Int(p), Num::Int(q)) => match op {
                Plus => p
                    .checked_add(q)
                    .map(Value::Int)
                    .unwrap_or(Value::Float(p as f64 + q as f64)),
                Minus => p
                    .checked_sub(q)
                    .map(Value::Int)
                    .unwrap_or(Value::Float(p as f64 - q as f64)),
                Star => p
                    .checked_mul(q)
                    .map(Value::Int)
                    .unwrap_or(Value::Float(p as f64 * q as f64)),
                Slash => {
                    if q == 0 {
                        return self.err("ZeroDivisionError", "division by zero");
                    }
                    Value::Float(p as f64 / q as f64)
                }
                DoubleSlash => {
                    if q == 0 {
                        return self.err("ZeroDivisionError", "integer division or modulo by zero");
                    }
                    let _ = &x;
                    match floor_div_i64(p, q) {
                        Some(v) => Value::Int(v),
                        None => Value::Float((p as f64 / q as f64).floor()),
                    }
                }
                Percent => {
                    if q == 0 {
                        return self.err("ZeroDivisionError", "integer modulo by zero");
                    }
                    match mod_i64(p, q) {
                        Some(v) => Value::Int(v),
                        None => Value::Float(mod_f64(p as f64, q as f64)),
                    }
                }
                DoubleStar => {
                    if q < 0 {
                        if p == 0 {
                            return self.err(
                                "ZeroDivisionError",
                                "0.0 cannot be raised to a negative power",
                            );
                        }
                        Value::Float((p as f64).powf(q as f64))
                    } else if let Ok(e) = u32::try_from(q) {
                        match p.checked_pow(e) {
                            Some(v) => Value::Int(v),
                            None => Value::Float((p as f64).powf(q as f64)),
                        }
                    } else {
                        Value::Float((p as f64).powf(q as f64))
                    }
                }
                Amp => Value::Int(p & q),
                Pipe => Value::Int(p | q),
                Caret => Value::Int(p ^ q),
                Shl => {
                    if q < 0 {
                        return self.err("ValueError", "negative shift count");
                    }
                    if q >= 64 {
                        return self.err("OverflowError", "shift count too large");
                    }
                    p.checked_shl(q as u32)
                        .map(Value::Int)
                        .unwrap_or(Value::Float(p as f64 * 2f64.powi(q as i32)))
                }
                Shr => {
                    if q < 0 {
                        return self.err("ValueError", "negative shift count");
                    }
                    if q >= 64 {
                        Value::Int(if p < 0 { -1 } else { 0 })
                    } else {
                        Value::Int(p >> q)
                    }
                }
                _ => unreachable!(),
            },
            _ => {
                let (p, q) = (x.as_f64(), y.as_f64());
                match op {
                    Plus => Value::Float(p + q),
                    Minus => Value::Float(p - q),
                    Star => Value::Float(p * q),
                    Slash => {
                        if q == 0.0 {
                            return self.err("ZeroDivisionError", "float division by zero");
                        }
                        Value::Float(p / q)
                    }
                    DoubleSlash => {
                        if q == 0.0 {
                            return self.err("ZeroDivisionError", "float floor division by zero");
                        }
                        Value::Float((p / q).floor())
                    }
                    Percent => {
                        if q == 0.0 {
                            return self.err("ZeroDivisionError", "float modulo");
                        }
                        Value::Float(mod_f64(p, q))
                    }
                    DoubleStar => Value::Float(p.powf(q)),
                    _ => {
                        return self.err(
                            "TypeError",
                            format!(
                                "unsupported operand type(s) for {}: '{}' and '{}'",
                                op.as_str(),
                                a.type_name(),
                                b.type_name()
                            ),
                        )
                    }
                }
            }
        };
        Ok(r)
    }

    fn try_dunder_binop(
        &mut self,
        op: Op,
        a: &Value,
        b: &Value,
        reflected: bool,
    ) -> EResult<Option<Value>> {
        let name = match op {
            Op::Plus => "__add__",
            Op::Minus => "__sub__",
            Op::Star => "__mul__",
            Op::Slash => "__truediv__",
            Op::DoubleSlash => "__floordiv__",
            Op::Percent => "__mod__",
            Op::DoubleStar => "__pow__",
            Op::Amp => "__and__",
            Op::Pipe => "__or__",
            Op::Caret => "__xor__",
            Op::Shl => "__lshift__",
            Op::Shr => "__rshift__",
            Op::At => "__matmul__",
            _ => return Ok(None),
        };
        let name = if reflected {
            match name {
                "__add__" => "__radd__",
                "__sub__" => "__rsub__",
                "__mul__" => "__rmul__",
                "__truediv__" => "__rtruediv__",
                "__floordiv__" => "__rfloordiv__",
                "__mod__" => "__rmod__",
                "__pow__" => "__rpow__",
                "__and__" => "__rand__",
                "__or__" => "__ror__",
                "__xor__" => "__rxor__",
                "__lshift__" => "__rlshift__",
                "__rshift__" => "__rrshift__",
                "__matmul__" => "__rmatmul__",
                _ => name,
            }
        } else {
            name
        };
        let target = if reflected { b } else { a };
        let other = if reflected { a } else { b };
        if let Value::Instance(i) = target {
            if let Some(m) = self.find_class_attr(&i.class, name) {
                let r = self.call_value(&m, vec![a.clone(), b.clone()], Vec::new())?;
                // NotImplemented 以 None 形态返回时继续尝试
                if !r.is_none() {
                    return Ok(Some(r));
                }
            }
        }
        let _ = other;
        Ok(None)
    }

    pub fn unop(&mut self, op: UnOp, v: Value) -> EResult<Value> {
        match op {
            UnOp::Not => {
                let t = self.truthy(&v)?;
                Ok(Value::Bool(!t))
            }
            UnOp::Neg => match &v {
                Value::Int(i) => Ok(i
                    .checked_neg()
                    .map(Value::Int)
                    .unwrap_or(Value::Float(-(*i as f64)))),
                Value::Bool(b) => Ok(Value::Int(-(*b as i64))),
                Value::Float(f) => Ok(Value::Float(-f)),
                Value::Instance(i) => match self.find_class_attr(&i.class, "__neg__") {
                    Some(m) => self.call_value(&m, vec![v.clone()], Vec::new()),
                    None => self.err(
                        "TypeError",
                        format!("bad operand type for unary -: '{}'", v.type_name()),
                    ),
                },
                other => self.err(
                    "TypeError",
                    format!("bad operand type for unary -: '{}'", other.type_name()),
                ),
            },
            UnOp::Pos => match &v {
                Value::Int(_) | Value::Float(_) => Ok(v.clone()),
                Value::Bool(b) => Ok(Value::Int(*b as i64)),
                Value::Instance(i) => match self.find_class_attr(&i.class, "__pos__") {
                    Some(m) => self.call_value(&m, vec![v.clone()], Vec::new()),
                    None => self.err(
                        "TypeError",
                        format!("bad operand type for unary +: '{}'", v.type_name()),
                    ),
                },
                other => self.err(
                    "TypeError",
                    format!("bad operand type for unary +: '{}'", other.type_name()),
                ),
            },
            UnOp::Invert => match &v {
                Value::Int(i) => Ok(Value::Int(!i)),
                Value::Bool(b) => Ok(Value::Int(!(*b as i64))),
                Value::Instance(i) => match self.find_class_attr(&i.class, "__invert__") {
                    Some(m) => self.call_value(&m, vec![v.clone()], Vec::new()),
                    None => self.err(
                        "TypeError",
                        format!("bad operand type for unary ~: '{}'", v.type_name()),
                    ),
                },
                other => self.err(
                    "TypeError",
                    format!("bad operand type for unary ~: '{}'", other.type_name()),
                ),
            },
        }
    }

    /// 旧式 `%` 字符串格式化。
    fn percent_format(&mut self, fmt: &str, args: &Value) -> EResult<Value> {
        let mut list_args: Vec<Value> = match args {
            Value::Tuple(t) => t.as_ref().clone(),
            other => vec![other.clone()],
        };
        let map_args: Option<HashMap<String, Value>> = match args {
            Value::Dict(d) => {
                let mut m = HashMap::new();
                for (k, v) in d.borrow().entries.iter() {
                    if let Value::Str(s) = k {
                        m.insert(s.to_string(), v.clone());
                    }
                }
                Some(m)
            }
            _ => None,
        };
        let mut out = String::new();
        let mut chars = fmt.chars().peekable();
        let mut arg_i = 0usize;
        while let Some(c) = chars.next() {
            if c != '%' {
                out.push(c);
                continue;
            }
            let mut spec = String::new();
            while let Some(&c2) = chars.peek() {
                if c2.is_ascii_alphabetic() || c2 == '%' {
                    break;
                }
                spec.push(c2);
                chars.next();
            }
            let conv = match chars.next() {
                Some(c2) => c2,
                None => return self.err("ValueError", "incomplete format"),
            };
            if conv == '%' {
                out.push('%');
                continue;
            }
            let use_map = spec.contains('(');
            let val: Value = if use_map {
                let m = match &map_args {
                    Some(m) => m,
                    None => return self.err("TypeError", "format requires a mapping"),
                };
                let key = spec
                    .split('(')
                    .nth(1)
                    .and_then(|s| s.split(')').next())
                    .unwrap_or("")
                    .to_string();
                match m.get(&key) {
                    Some(v) => v.clone(),
                    None => return self.err("KeyError", format!("'{}'", key)),
                }
            } else {
                if arg_i >= list_args.len() {
                    return self.err("TypeError", "not enough arguments for format string");
                }
                let v = list_args[arg_i].clone();
                arg_i += 1;
                v
            };
            let spec = spec.replace(['(', ')'], "");
            let spec = if use_map { String::new() } else { spec };
            let text = match conv {
                's' => {
                    let t = self.value_str(&val)?;
                    if spec.is_empty() {
                        t
                    } else {
                        self.format_with_spec(&Value::str_from(t), &spec)?
                    }
                }
                'r' => {
                    let t = self.value_repr(&val)?;
                    if spec.is_empty() {
                        t
                    } else {
                        self.format_with_spec(&Value::str_from(t), &spec)?
                    }
                }
                'd' | 'i' => match self.as_index(&val) {
                    Ok(i) => self.format_with_spec(&Value::Int(i), &spec)?,
                    Err(_) => match &val {
                        Value::Float(f) => self.format_with_spec(&Value::Float(*f), &spec)?,
                        _ => self.format_with_spec(&val, &spec)?,
                    },
                },
                'f' | 'F' | 'e' | 'E' | 'g' | 'G' | '%' => {
                    let f = match val.as_number() {
                        Some(n) => n.as_f64(),
                        None => {
                            return self.err(
                                "TypeError",
                                format!("must be real number, not {}", val.type_name()),
                            )
                        }
                    };
                    self.format_with_spec(&Value::Float(f), &spec)?
                }
                'x' | 'X' | 'o' => {
                    let i = self.as_index(&val)?;
                    self.format_with_spec(&Value::Int(i), &format!("{}{}", spec, conv))?
                }
                'c' => match val {
                    Value::Str(s) => s.to_string(),
                    other => {
                        let i = self.as_index(&other)?;
                        self.format_with_spec(&Value::Int(i), "c")?
                    }
                },
                other => {
                    return self.err(
                        "ValueError",
                        format!("unsupported format character '{}'", other),
                    )
                }
            };
            out.push_str(&text);
        }
        if arg_i < list_args.len() && map_args.is_none() {
            return self.err(
                "TypeError",
                "not all arguments converted during string formatting",
            );
        }
        list_args.clear();
        Ok(Value::str_from(out))
    }
}

fn opt_int(v: Option<i64>) -> Value {
    match v {
        Some(i) => Value::Int(i),
        None => Value::None,
    }
}

fn opt_int_repr(v: Option<i64>) -> String {
    match v {
        Some(i) => i.to_string(),
        None => "None".to_string(),
    }
}

impl SetData {
    /// 合并去重（用于 set 的 `|=`）。
    pub fn extend_distinct(&mut self, items: Vec<Value>) {
        for v in items {
            self.insert(v);
        }
    }
}
