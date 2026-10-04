//! 错误类型：词法错误、语法错误、运行时异常。
//!
//! 运行时异常（`PyError`）尽量模拟 CPython 的表现：
//! 异常类型名 + 消息 + 调用栈（traceback）。

use crate::value::Value;
use std::fmt;

/// 词法分析错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub msg: String,
    pub line: u32,
    pub col: u32,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}:{}: {}", self.line, self.col, self.msg)
    }
}

impl std::error::Error for LexError {}

/// 语法分析错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub msg: String,
    pub line: u32,
    pub col: u32,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}:{}: {}", self.line, self.col, self.msg)
    }
}

impl std::error::Error for ParseError {}

/// 调用栈中的一帧，用于打印 traceback。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceFrame {
    pub file: String,
    pub line: u32,
    pub func: String,
}

/// 运行时异常（Python 层面的异常）。
#[derive(Debug, Clone)]
pub struct PyError {
    /// 异常类型名，如 `TypeError`。
    pub kind: String,
    /// 异常消息。
    pub message: String,
    /// 抛出时携带的异常对象（用户自定义异常时会用到）。
    pub value: Option<Value>,
    /// 调用栈，从最外层到最内层。
    pub trace: Vec<TraceFrame>,
}

impl PyError {
    pub fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        PyError {
            kind: kind.into(),
            message: message.into(),
            value: None,
            trace: Vec::new(),
        }
    }

    pub fn with_value(mut self, v: Value) -> Self {
        self.value = Some(v);
        self
    }

    pub fn with_trace(mut self, trace: Vec<TraceFrame>) -> Self {
        self.trace = trace;
        self
    }

    /// 只有一行摘要，形如 `TypeError: ...`。
    pub fn summary(&self) -> String {
        if self.message.is_empty() {
            self.kind.clone()
        } else {
            format!("{}: {}", self.kind, self.message)
        }
    }

    /// 是否可以被 `except <name>` 捕获（按类名及其基类名判断）。
    pub fn matches_name(&self, name: &str) -> bool {
        if self.kind == name {
            return true;
        }
        exception_bases(&self.kind).contains(&name)
    }
}

impl fmt::Display for PyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.summary())
    }
}

impl std::error::Error for PyError {}

/// 内置异常体系（名称 -> 直接基类）。
pub const BUILTIN_EXCEPTIONS: &[(&str, &str)] = &[
    ("BaseException", ""),
    ("Exception", "BaseException"),
    ("SystemExit", "BaseException"),
    ("KeyboardInterrupt", "BaseException"),
    ("GeneratorExit", "BaseException"),
    ("StopIteration", "Exception"),
    ("StopAsyncIteration", "Exception"),
    ("ArithmeticError", "Exception"),
    ("ZeroDivisionError", "ArithmeticError"),
    ("OverflowError", "ArithmeticError"),
    ("FloatingPointError", "ArithmeticError"),
    ("AssertionError", "Exception"),
    ("AttributeError", "Exception"),
    ("BufferError", "Exception"),
    ("EOFError", "Exception"),
    ("ImportError", "Exception"),
    ("ModuleNotFoundError", "ImportError"),
    ("LookupError", "Exception"),
    ("IndexError", "LookupError"),
    ("KeyError", "LookupError"),
    ("MemoryError", "Exception"),
    ("NameError", "Exception"),
    ("UnboundLocalError", "NameError"),
    ("OSError", "Exception"),
    ("IOError", "OSError"),
    ("FileNotFoundError", "OSError"),
    ("FileExistsError", "OSError"),
    ("PermissionError", "OSError"),
    ("ReferenceError", "Exception"),
    ("RuntimeError", "Exception"),
    ("NotImplementedError", "RuntimeError"),
    ("RecursionError", "RuntimeError"),
    ("SyntaxError", "Exception"),
    ("IndentationError", "SyntaxError"),
    ("TabError", "IndentationError"),
    ("SystemError", "Exception"),
    ("TypeError", "Exception"),
    ("ValueError", "Exception"),
    ("UnicodeError", "ValueError"),
    ("UnicodeDecodeError", "UnicodeError"),
    ("UnicodeEncodeError", "UnicodeError"),
    ("Warning", "Exception"),
    ("DeprecationWarning", "Warning"),
    ("UserWarning", "Warning"),
];

/// 返回某个内置异常的全部基类名。
pub fn exception_bases(kind: &str) -> Vec<&'static str> {
    let mut out = Vec::new();
    let mut cur = kind.to_string();
    loop {
        match BUILTIN_EXCEPTIONS.iter().find(|(n, _)| *n == cur) {
            Some((_, base)) if !base.is_empty() => {
                if let Some(b) = BUILTIN_EXCEPTIONS.iter().find(|(n, _)| n == base) {
                    out.push(b.0);
                    cur = b.0.to_string();
                } else {
                    break;
                }
            }
            _ => break,
        }
    }
    out
}

/// 某个名字是否为内置异常类。
pub fn is_builtin_exception(name: &str) -> bool {
    BUILTIN_EXCEPTIONS.iter().any(|(n, _)| *n == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exception_hierarchy() {
        assert!(PyError::new("IndexError", "x").matches_name("LookupError"));
        assert!(PyError::new("IndexError", "x").matches_name("Exception"));
        assert!(PyError::new("IndexError", "x").matches_name("IndexError"));
        assert!(!PyError::new("IndexError", "x").matches_name("ValueError"));
        assert_eq!(
            exception_bases("ValueError"),
            vec!["Exception", "BaseException"]
        );
    }
}
