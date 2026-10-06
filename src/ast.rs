//! 抽象语法树（AST）。

use crate::lexer::Op;
use std::rc::Rc;

#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub line: u32,
}

impl Stmt {
    pub fn new(kind: StmtKind, line: u32) -> Self {
        Stmt { kind, line }
    }
}

/// 函数参数列表。
#[derive(Debug, Clone, Default)]
pub struct Params {
    /// 普通位置参数
    pub args: Vec<String>,
    /// 与 `args` 末尾对齐的默认值表达式
    pub defaults: Vec<Expr>,
    /// `*args`
    pub vararg: Option<String>,
    /// `*` 之后的关键字参数及其默认值
    pub kwonly: Vec<(String, Option<Expr>)>,
    /// `**kwargs`
    pub kwarg: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FuncDef {
    pub name: String,
    pub params: Params,
    pub body: Vec<Stmt>,
    pub decorators: Vec<Expr>,
    pub line: u32,
}

#[derive(Debug, Clone)]
pub struct ClassDef {
    pub name: String,
    pub bases: Vec<Expr>,
    pub body: Vec<Stmt>,
    pub decorators: Vec<Expr>,
    pub line: u32,
}

#[derive(Debug, Clone)]
pub struct ExceptHandler {
    /// `None` 表示裸 `except:`
    pub types: Option<Vec<Expr>>,
    pub name: Option<String>,
    pub body: Vec<Stmt>,
    pub line: u32,
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    Expr(Expr),
    Assign {
        targets: Vec<Expr>,
        value: Expr,
    },
    AugAssign {
        target: Expr,
        op: Op,
        value: Expr,
    },
    AnnAssign {
        target: Expr,
        annotation: Expr,
        value: Option<Expr>,
    },
    If {
        test: Expr,
        body: Vec<Stmt>,
        orelse: Vec<Stmt>,
    },
    While {
        test: Expr,
        body: Vec<Stmt>,
        orelse: Vec<Stmt>,
    },
    For {
        target: Expr,
        iter: Expr,
        body: Vec<Stmt>,
        orelse: Vec<Stmt>,
    },
    FuncDef(Rc<FuncDef>),
    ClassDef(Rc<ClassDef>),
    Return(Option<Expr>),
    Break,
    Continue,
    Pass,
    Raise {
        exc: Option<Expr>,
        cause: Option<Expr>,
    },
    Try {
        body: Vec<Stmt>,
        handlers: Vec<ExceptHandler>,
        orelse: Vec<Stmt>,
        finalbody: Vec<Stmt>,
    },
    Import {
        /// (点分模块名, 别名)
        names: Vec<(Vec<String>, Option<String>)>,
    },
    ImportFrom {
        module: Vec<String>,
        names: Vec<(String, Option<String>)>,
        level: usize,
    },
    Global(Vec<String>),
    Nonlocal(Vec<String>),
    Assert {
        test: Expr,
        msg: Option<Expr>,
    },
    Delete(Vec<Expr>),
    With {
        items: Vec<(Expr, Option<Expr>)>,
        body: Vec<Stmt>,
    },
}

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub line: u32,
}

impl Expr {
    pub fn new(kind: ExprKind, line: u32) -> Self {
        Expr { kind, line }
    }
}

#[derive(Debug, Clone)]
pub enum FStrPiece {
    Lit(String),
    Value {
        expr: Box<Expr>,
        conv: Option<char>,
        /// 格式说明符，可能包含嵌套的替换字段（如 `f"{x:>{w}}"`）
        spec: Vec<SpecPart>,
    },
}

#[derive(Debug, Clone)]
pub enum SpecPart {
    Lit(String),
    Field(Box<Expr>),
}

#[derive(Debug, Clone)]
pub enum Arg {
    Pos(Expr),
    Kw(String, Expr),
    Star(Expr),
    DoubleStar(Expr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Pos,
    Not,
    Invert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoolOpKind {
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    NotEq,
    Lt,
    LtE,
    Gt,
    GtE,
    Is,
    IsNot,
    In,
    NotIn,
}

#[derive(Debug, Clone)]
pub struct CompFor {
    pub target: Expr,
    pub iter: Expr,
    pub ifs: Vec<Expr>,
}

#[derive(Debug, Clone)]
pub struct LambdaDef {
    pub params: Params,
    pub body: Box<Expr>,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    None_,
    FStr(Vec<FStrPiece>),
    Name(String),
    /// 海象运算符 `(name := value)`；赋值发生在最近的非推导式作用域
    Named {
        name: String,
        value: Box<Expr>,
    },
    List(Vec<Expr>),
    Tuple(Vec<Expr>),
    Set(Vec<Expr>),
    /// 字典；键为 `None` 表示 `**other` 展开
    Dict(Vec<(Option<Expr>, Expr)>),
    Starred(Box<Expr>),
    BinOp {
        left: Box<Expr>,
        op: Op,
        right: Box<Expr>,
    },
    UnaryOp {
        op: UnOp,
        operand: Box<Expr>,
    },
    BoolOp {
        op: BoolOpKind,
        values: Vec<Expr>,
    },
    Compare {
        left: Box<Expr>,
        ops: Vec<CmpOp>,
        comparators: Vec<Expr>,
    },
    IfExp {
        test: Box<Expr>,
        body: Box<Expr>,
        orelse: Box<Expr>,
    },
    Lambda(Rc<LambdaDef>),
    Call {
        func: Box<Expr>,
        args: Vec<Arg>,
    },
    Attr {
        obj: Box<Expr>,
        name: String,
    },
    Index {
        obj: Box<Expr>,
        index: Box<Expr>,
    },
    Slice {
        lower: Option<Box<Expr>>,
        upper: Option<Box<Expr>>,
        step: Option<Box<Expr>>,
    },
    ListComp {
        elt: Box<Expr>,
        generators: Vec<CompFor>,
    },
    SetComp {
        elt: Box<Expr>,
        generators: Vec<CompFor>,
    },
    DictComp {
        key: Box<Expr>,
        value: Box<Expr>,
        generators: Vec<CompFor>,
    },
    GenExp {
        elt: Box<Expr>,
        generators: Vec<CompFor>,
    },
}
