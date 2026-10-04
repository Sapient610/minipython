//! 作用域 / 环境：实现 Python 的 LEGB 查找规则与 global / nonlocal 语义。

use crate::value::Value;
pub use crate::value::EnvRef;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    Module,
    Function,
    Class,
    Comprehension,
}

#[derive(Debug)]
pub struct Env {
    pub vars: RefCell<HashMap<String, Value>>,
    pub parent: Option<EnvRef>,
    pub kind: ScopeKind,
    /// 最外层（模块）作用域；模块自身为 None
    pub globals: Option<EnvRef>,
    /// 由 `global x` 声明的名字
    pub global_names: RefCell<HashSet<String>>,
    /// 由 `nonlocal x` 声明的名字
    pub nonlocal_names: RefCell<HashSet<String>>,
}

impl Env {
    pub fn new(parent: Option<EnvRef>, kind: ScopeKind) -> EnvRef {
        let globals = parent.as_ref().map(|p| match p.globals.clone() {
                Some(g) => g,
                None => p.clone(),
            });
        Rc::new(Env {
            vars: RefCell::new(HashMap::new()),
            parent,
            kind,
            globals,
            global_names: RefCell::new(HashSet::new()),
            nonlocal_names: RefCell::new(HashSet::new()),
        })
    }

    /// 新建模块（最外层）作用域。
    pub fn new_module() -> EnvRef {
        Env::new(None, ScopeKind::Module)
    }

    pub fn scope_kind(&self) -> ScopeKind {
        self.kind
    }

    /// 定义（或覆盖）当前作用域中的名字。
    pub fn define(&self, name: impl Into<String>, value: Value) {
        self.vars.borrow_mut().insert(name.into(), value);
    }

    /// 当前作用域中是否有该名字。
    pub fn has_local(&self, name: &str) -> bool {
        self.vars.borrow().contains_key(name)
    }

    pub fn get_local(&self, name: &str) -> Option<Value> {
        self.vars.borrow().get(name).cloned()
    }

    /// 沿作用域链查找（不查内置命名空间）。
    pub fn lookup(&self, name: &str) -> Option<Value> {
        if let Some(v) = self.vars.borrow().get(name) {
            return Some(v.clone());
        }
        match &self.parent {
            Some(p) => p.lookup(name),
            None => None,
        }
    }

    /// 赋值：遵循 `global` / `nonlocal` 声明。
    pub fn assign(&self, name: &str, value: Value) {
        if self.global_names.borrow().contains(name) {
            match &self.globals {
                Some(g) => {
                    g.vars.borrow_mut().insert(name.to_string(), value);
                }
                None => {
                    self.vars.borrow_mut().insert(name.to_string(), value);
                }
            }
            return;
        }
        if self.nonlocal_names.borrow().contains(name) {
            if let Some(env) = self.find_nonlocal_scope(name) {
                env.vars.borrow_mut().insert(name.to_string(), value);
                return;
            }
        }
        self.vars.borrow_mut().insert(name.to_string(), value);
    }

    /// 找到 `nonlocal name` 应该绑定的外层函数作用域。
    fn find_nonlocal_scope(&self, name: &str) -> Option<EnvRef> {
        let mut cur = self.parent.clone();
        while let Some(env) = cur {
            if env.scope_kind() != ScopeKind::Module
                && env.scope_kind() != ScopeKind::Class
                && env.has_local(name)
            {
                return Some(env);
            }
            cur = env.parent.clone();
        }
        // 没找到绑定，退回到最近的外层函数作用域
        let mut cur = self.parent.clone();
        while let Some(env) = cur {
            if env.scope_kind() == ScopeKind::Function {
                return Some(env);
            }
            cur = env.parent.clone();
        }
        None
    }

    pub fn delete(&self, name: &str) -> bool {
        self.vars.borrow_mut().remove(name).is_some()
    }

    /// 收集所有可见的名字（用于 REPL 补全 / dir()）。
    pub fn visible_names(&self) -> Vec<String> {
        let mut out: Vec<String> = self.vars.borrow().keys().cloned().collect();
        if let Some(p) = &self.parent {
            for n in p.visible_names() {
                if !out.contains(&n) {
                    out.push(n);
                }
            }
        }
        out.sort();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_walks_parent_chain() {
        let m = Env::new_module();
        m.define("x", Value::Int(1));
        let f = Env::new(Some(m.clone()), ScopeKind::Function);
        assert_eq!(f.lookup("x"), Some(Value::Int(1)));
        f.define("x", Value::Int(2));
        assert_eq!(f.lookup("x"), Some(Value::Int(2)));
        assert_eq!(m.lookup("x"), Some(Value::Int(1)));
    }

    #[test]
    fn global_declaration_writes_to_module() {
        let m = Env::new_module();
        m.define("x", Value::Int(1));
        let f = Env::new(Some(m.clone()), ScopeKind::Function);
        f.global_names.borrow_mut().insert("x".to_string());
        f.assign("x", Value::Int(9));
        assert_eq!(m.lookup("x"), Some(Value::Int(9)));
        assert!(!f.has_local("x"));
    }

    #[test]
    fn nonlocal_declaration_writes_to_enclosing_function() {
        let m = Env::new_module();
        let outer = Env::new(Some(m.clone()), ScopeKind::Function);
        outer.define("n", Value::Int(1));
        let inner = Env::new(Some(outer.clone()), ScopeKind::Function);
        inner.nonlocal_names.borrow_mut().insert("n".to_string());
        inner.assign("n", Value::Int(5));
        assert_eq!(outer.lookup("n"), Some(Value::Int(5)));
    }

    #[test]
    fn closures_see_updated_enclosing_variables() {
        let m = Env::new_module();
        let outer = Env::new(Some(m.clone()), ScopeKind::Function);
        let inner = Env::new(Some(outer.clone()), ScopeKind::Function);
        outer.define("n", Value::Int(1));
        assert_eq!(inner.lookup("n"), Some(Value::Int(1)));
        outer.assign("n", Value::Int(2));
        assert_eq!(inner.lookup("n"), Some(Value::Int(2)));
    }
}
