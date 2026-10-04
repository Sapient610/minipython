//! 语法分析器：递归下降 + 优先级爬升，产出 AST。

use crate::ast::*;
use crate::error::ParseError;
use crate::lexer::{FPart, Kw, Lexer, Op, Token, TokenKind};
use std::rc::Rc;

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

type PResult<T> = Result<T, ParseError>;

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
    }

    /// 解析一段源码为语句列表（模块）。
    pub fn parse_source(src: &str) -> PResult<Vec<Stmt>> {
        let tokens = Lexer::tokenize(src).map_err(|e| ParseError {
            msg: e.msg,
            line: e.line,
            col: e.col,
        })?;
        let mut p = Parser::new(tokens);
        p.parse_module()
    }

    /// 解析单个表达式（用于 f-string 内部、REPL 等）。
    pub fn parse_expression_string(src: &str, line: u32) -> PResult<Expr> {
        let tokens = Lexer::tokenize(src).map_err(|e| ParseError {
            msg: e.msg,
            line: e.line,
            col: e.col,
        })?;
        let mut p = Parser::new(tokens);
        let e = p.parse_expr_list(true, line)?;
        match p.peek() {
            TokenKind::Newline | TokenKind::Eof => Ok(e),
            other => p.err(format!("表达式中多余的内容: {}", other.describe())),
        }
    }

    // ---------- 基础工具 ----------

    fn peek(&self) -> &TokenKind {
        &self.tokens[self.pos.min(self.tokens.len() - 1)].kind
    }

    fn peek_at(&self, n: usize) -> &TokenKind {
        &self.tokens[(self.pos + n).min(self.tokens.len() - 1)].kind
    }

    fn line(&self) -> u32 {
        self.tokens[self.pos.min(self.tokens.len() - 1)].line
    }

    fn col(&self) -> u32 {
        self.tokens[self.pos.min(self.tokens.len() - 1)].col
    }

    fn advance(&mut self) -> Token {
        let t = self.tokens[self.pos.min(self.tokens.len() - 1)].clone();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        t
    }

    fn err<T>(&self, msg: impl Into<String>) -> PResult<T> {
        Err(ParseError {
            msg: msg.into(),
            line: self.line(),
            col: self.col(),
        })
    }

    fn at(&self, op: Op) -> bool {
        matches!(self.peek(), TokenKind::Op(o) if *o == op)
    }

    fn at_kw(&self, kw: Kw) -> bool {
        matches!(self.peek(), TokenKind::Kw(k) if *k == kw)
    }

    fn eat(&mut self, op: Op) -> bool {
        if self.at(op) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn eat_kw(&mut self, kw: Kw) -> bool {
        if self.at_kw(kw) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, op: Op) -> PResult<()> {
        if self.eat(op) {
            Ok(())
        } else {
            self.err(format!(
                "期望 '{}'，但遇到 {}",
                op.as_str(),
                self.peek().describe()
            ))
        }
    }

    fn expect_kw(&mut self, kw: Kw) -> PResult<()> {
        if self.eat_kw(kw) {
            Ok(())
        } else {
            self.err(format!(
                "期望 '{}'，但遇到 {}",
                kw.as_str(),
                self.peek().describe()
            ))
        }
    }

    fn expect_name(&mut self) -> PResult<String> {
        match self.peek().clone() {
            TokenKind::Name(n) => {
                self.advance();
                Ok(n)
            }
            other => self.err(format!("期望标识符，但遇到 {}", other.describe())),
        }
    }

    fn expect_newline(&mut self) -> PResult<()> {
        match self.peek() {
            TokenKind::Newline => {
                self.advance();
                Ok(())
            }
            TokenKind::Eof => Ok(()),
            other => self.err(format!(
                "语句末尾多余的内容: {}（同一行多条语句请用 ';' 分隔）",
                other.describe()
            )),
        }
    }

    // ---------- 模块与语句 ----------

    pub fn parse_module(&mut self) -> PResult<Vec<Stmt>> {
        let mut stmts = Vec::new();
        loop {
            while matches!(self.peek(), TokenKind::Newline) {
                self.advance();
            }
            if matches!(self.peek(), TokenKind::Eof) {
                break;
            }
            let mut s = self.parse_statement()?;
            stmts.append(&mut s);
        }
        Ok(stmts)
    }

    /// 解析一条逻辑行（可能被 `;` 分成多条语句）。
    fn parse_statement(&mut self) -> PResult<Vec<Stmt>> {
        if matches!(
            self.peek(),
            TokenKind::Kw(Kw::If | Kw::While | Kw::For | Kw::Try | Kw::With | Kw::Def | Kw::Class)
        ) {
            return Ok(vec![self.parse_compound()?]);
        }
        if self.at(Op::At) {
            return Ok(vec![self.parse_decorated()?]);
        }
        self.parse_simple_line()
    }

    fn parse_simple_line(&mut self) -> PResult<Vec<Stmt>> {
        let mut out = Vec::new();
        loop {
            out.push(self.parse_simple_statement()?);
            if self.eat(Op::Semi) {
                if matches!(self.peek(), TokenKind::Newline | TokenKind::Eof) {
                    break;
                }
                continue;
            }
            break;
        }
        self.expect_newline()?;
        Ok(out)
    }

    fn parse_simple_statement(&mut self) -> PResult<Stmt> {
        let line = self.line();
        if let TokenKind::Kw(kw) = self.peek() {
            match kw {
                Kw::Return => {
                    self.advance();
                    let value = if matches!(self.peek(), TokenKind::Newline | TokenKind::Eof)
                        || self.at(Op::Semi)
                    {
                        None
                    } else {
                        Some(self.parse_expr_list(true, line)?)
                    };
                    return Ok(Stmt::new(StmtKind::Return(value), line));
                }
                Kw::Pass => {
                    self.advance();
                    return Ok(Stmt::new(StmtKind::Pass, line));
                }
                Kw::Break => {
                    self.advance();
                    return Ok(Stmt::new(StmtKind::Break, line));
                }
                Kw::Continue => {
                    self.advance();
                    return Ok(Stmt::new(StmtKind::Continue, line));
                }
                Kw::Raise => {
                    self.advance();
                    if matches!(self.peek(), TokenKind::Newline | TokenKind::Eof)
                        || self.at(Op::Semi)
                    {
                        return Ok(Stmt::new(
                            StmtKind::Raise {
                                exc: None,
                                cause: None,
                            },
                            line,
                        ));
                    }
                    let exc = self.parse_expr(line)?;
                    let cause = if self.eat_kw(Kw::From) {
                        Some(self.parse_expr(line)?)
                    } else {
                        None
                    };
                    return Ok(Stmt::new(
                        StmtKind::Raise {
                            exc: Some(exc),
                            cause,
                        },
                        line,
                    ));
                }
                Kw::Global => {
                    self.advance();
                    let mut names = vec![self.expect_name()?];
                    while self.eat(Op::Comma) {
                        names.push(self.expect_name()?);
                    }
                    return Ok(Stmt::new(StmtKind::Global(names), line));
                }
                Kw::Nonlocal => {
                    self.advance();
                    let mut names = vec![self.expect_name()?];
                    while self.eat(Op::Comma) {
                        names.push(self.expect_name()?);
                    }
                    return Ok(Stmt::new(StmtKind::Nonlocal(names), line));
                }
                Kw::Assert => {
                    self.advance();
                    let test = self.parse_expr(line)?;
                    let msg = if self.eat(Op::Comma) {
                        Some(self.parse_expr(line)?)
                    } else {
                        None
                    };
                    return Ok(Stmt::new(StmtKind::Assert { test, msg }, line));
                }
                Kw::Del => {
                    self.advance();
                    let mut targets = vec![self.parse_expr(line)?];
                    while self.eat(Op::Comma) {
                        targets.push(self.parse_expr(line)?);
                    }
                    return Ok(Stmt::new(StmtKind::Delete(targets), line));
                }
                Kw::Import => {
                    self.advance();
                    return self.parse_import(line);
                }
                Kw::From => {
                    self.advance();
                    return self.parse_import_from(line);
                }
                Kw::Yield => {
                    return self.err("暂不支持 yield / 生成器函数");
                }
                _ => {}
            }
        }
        // 表达式语句 / 赋值 / 注解赋值 / 增量赋值
        let first = self.parse_expr_list(true, line)?;
        if self.at(Op::Colon) {
            // 注解赋值 x: T = v
            self.advance();
            let annotation = self.parse_expr(line)?;
            let value = if self.eat(Op::Assign) {
                Some(self.parse_expr_list(true, line)?)
            } else {
                None
            };
            match first.kind {
                ExprKind::Name(_) | ExprKind::Attr { .. } | ExprKind::Index { .. } => {}
                _ => return self.err("注解赋值的目标必须是变量、属性或下标"),
            }
            return Ok(Stmt::new(
                StmtKind::AnnAssign {
                    target: first,
                    annotation,
                    value,
                },
                line,
            ));
        }
        if let TokenKind::Op(op) = self.peek() {
            let op = *op;
            if let Some(base) = aug_base(op) {
                self.advance();
                let value = self.parse_expr_list(true, line)?;
                return Ok(Stmt::new(
                    StmtKind::AugAssign {
                        target: first,
                        op: base,
                        value,
                    },
                    line,
                ));
            }
        }
        if self.at(Op::Assign) {
            let mut targets = vec![first];
            let value;
            loop {
                self.expect(Op::Assign)?;
                let next = self.parse_expr_list(true, line)?;
                if self.at(Op::Assign) {
                    targets.push(next);
                } else {
                    value = next;
                    break;
                }
            }
            return Ok(Stmt::new(StmtKind::Assign { targets, value }, line));
        }
        Ok(Stmt::new(StmtKind::Expr(first), line))
    }

    fn parse_import(&mut self, line: u32) -> PResult<Stmt> {
        let mut names = Vec::new();
        loop {
            let mut path = vec![self.expect_name()?];
            while self.at(Op::Dot) {
                self.advance();
                path.push(self.expect_name()?);
            }
            let alias = if self.eat_kw(Kw::As) {
                Some(self.expect_name()?)
            } else {
                None
            };
            names.push((path, alias));
            if !self.eat(Op::Comma) {
                break;
            }
        }
        Ok(Stmt::new(StmtKind::Import { names }, line))
    }

    fn parse_import_from(&mut self, line: u32) -> PResult<Stmt> {
        let mut level = 0usize;
        while self.at(Op::Dot) || self.at(Op::Ellipsis) {
            if self.at(Op::Ellipsis) {
                level += 3;
            } else {
                level += 1;
            }
            self.advance();
        }
        let mut module = Vec::new();
        if !self.at_kw(Kw::Import) {
            module.push(self.expect_name()?);
            while self.at(Op::Dot) {
                self.advance();
                module.push(self.expect_name()?);
            }
        }
        self.expect_kw(Kw::Import)?;
        let mut names = Vec::new();
        if self.eat(Op::Star) {
            names.push(("*".to_string(), None));
        } else {
            loop {
                let name = self.expect_name()?;
                let alias = if self.eat_kw(Kw::As) {
                    Some(self.expect_name()?)
                } else {
                    None
                };
                names.push((name, alias));
                if !self.eat(Op::Comma) {
                    break;
                }
            }
        }
        Ok(Stmt::new(
            StmtKind::ImportFrom {
                module,
                names,
                level,
            },
            line,
        ))
    }

    fn parse_compound(&mut self) -> PResult<Stmt> {
        if self.at_kw(Kw::If) {
            return self.parse_if();
        }
        if self.at_kw(Kw::While) {
            return self.parse_while();
        }
        if self.at_kw(Kw::For) {
            return self.parse_for();
        }
        if self.at_kw(Kw::Try) {
            return self.parse_try();
        }
        if self.at_kw(Kw::With) {
            return self.parse_with();
        }
        if self.at_kw(Kw::Def) {
            return self.parse_funcdef(Vec::new());
        }
        if self.at_kw(Kw::Class) {
            return self.parse_classdef(Vec::new());
        }
        self.err(format!("无法解析的语句: {}", self.peek().describe()))
    }

    fn parse_decorated(&mut self) -> PResult<Stmt> {
        let mut decorators = Vec::new();
        while self.at(Op::At) {
            self.advance();
            let e = self.parse_expr(self.line())?;
            decorators.push(e);
            self.expect_newline()?;
        }
        if self.at_kw(Kw::Def) {
            self.parse_funcdef(decorators)
        } else if self.at_kw(Kw::Class) {
            self.parse_classdef(decorators)
        } else {
            self.err("装饰器只能用于函数或类定义")
        }
    }

    fn parse_if(&mut self) -> PResult<Stmt> {
        let line = self.line();
        self.expect_kw(Kw::If)?;
        let test = self.parse_expr(line)?;
        self.expect(Op::Colon)?;
        let body = self.parse_block()?;
        let mut orelse = Vec::new();
        if self.at_kw(Kw::Elif) {
            let nested = self.parse_elif(line)?;
            orelse.push(nested);
        } else if self.at_kw(Kw::Else) {
            self.advance();
            self.expect(Op::Colon)?;
            orelse = self.parse_block()?;
        }
        Ok(Stmt::new(StmtKind::If { test, body, orelse }, line))
    }

    fn parse_elif(&mut self, line: u32) -> PResult<Stmt> {
        self.expect_kw(Kw::Elif)?;
        let test = self.parse_expr(line)?;
        self.expect(Op::Colon)?;
        let body = self.parse_block()?;
        let mut orelse = Vec::new();
        if self.at_kw(Kw::Elif) {
            let nested = self.parse_elif(line)?;
            orelse.push(nested);
        } else if self.at_kw(Kw::Else) {
            self.advance();
            self.expect(Op::Colon)?;
            orelse = self.parse_block()?;
        }
        Ok(Stmt::new(StmtKind::If { test, body, orelse }, line))
    }

    fn parse_while(&mut self) -> PResult<Stmt> {
        let line = self.line();
        self.expect_kw(Kw::While)?;
        let test = self.parse_expr(line)?;
        self.expect(Op::Colon)?;
        let body = self.parse_block()?;
        let orelse = if self.at_kw(Kw::Else) {
            self.advance();
            self.expect(Op::Colon)?;
            self.parse_block()?
        } else {
            Vec::new()
        };
        Ok(Stmt::new(StmtKind::While { test, body, orelse }, line))
    }

    fn parse_for(&mut self) -> PResult<Stmt> {
        let line = self.line();
        self.expect_kw(Kw::For)?;
        let target = self.parse_target_list()?;
        self.expect_kw(Kw::In)?;
        let iter = self.parse_expr_list(false, line)?;
        self.expect(Op::Colon)?;
        let body = self.parse_block()?;
        let orelse = if self.at_kw(Kw::Else) {
            self.advance();
            self.expect(Op::Colon)?;
            self.parse_block()?
        } else {
            Vec::new()
        };
        Ok(Stmt::new(
            StmtKind::For {
                target,
                iter,
                body,
                orelse,
            },
            line,
        ))
    }

    fn parse_try(&mut self) -> PResult<Stmt> {
        let line = self.line();
        self.expect_kw(Kw::Try)?;
        self.expect(Op::Colon)?;
        let body = self.parse_block()?;
        let mut handlers = Vec::new();
        while self.at_kw(Kw::Except) {
            let hline = self.line();
            self.advance();
            let types = if self.at(Op::Colon) {
                None
            } else {
                let mut ts = vec![self.parse_expr(hline)?];
                while self.eat(Op::Comma) {
                    ts.push(self.parse_expr(hline)?);
                }
                Some(ts)
            };
            let name = if self.eat_kw(Kw::As) {
                Some(self.expect_name()?)
            } else {
                None
            };
            self.expect(Op::Colon)?;
            let hbody = self.parse_block()?;
            handlers.push(ExceptHandler {
                types,
                name,
                body: hbody,
                line: hline,
            });
        }
        let orelse = if self.at_kw(Kw::Else) {
            self.advance();
            self.expect(Op::Colon)?;
            self.parse_block()?
        } else {
            Vec::new()
        };
        let finalbody = if self.at_kw(Kw::Finally) {
            self.advance();
            self.expect(Op::Colon)?;
            self.parse_block()?
        } else {
            Vec::new()
        };
        if handlers.is_empty() && finalbody.is_empty() {
            return self.err("try 语句必须至少有 except 或 finally 子句");
        }
        Ok(Stmt::new(
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            },
            line,
        ))
    }

    fn parse_with(&mut self) -> PResult<Stmt> {
        let line = self.line();
        self.expect_kw(Kw::With)?;
        let mut items = Vec::new();
        loop {
            let ctx = self.parse_expr(line)?;
            let target = if self.eat_kw(Kw::As) {
                Some(self.parse_target_list()?)
            } else {
                None
            };
            items.push((ctx, target));
            if !self.eat(Op::Comma) {
                break;
            }
        }
        self.expect(Op::Colon)?;
        let body = self.parse_block()?;
        Ok(Stmt::new(StmtKind::With { items, body }, line))
    }

    fn parse_funcdef(&mut self, decorators: Vec<Expr>) -> PResult<Stmt> {
        let line = self.line();
        self.expect_kw(Kw::Def)?;
        let name = self.expect_name()?;
        self.expect(Op::LParen)?;
        let params = self.parse_params()?;
        self.expect(Op::RParen)?;
        if self.eat(Op::Arrow) {
            let _ = self.parse_expr(line)?; // 返回值注解暂不参与运行
        }
        self.expect(Op::Colon)?;
        let body = self.parse_block()?;
        Ok(Stmt::new(
            StmtKind::FuncDef(Rc::new(FuncDef {
                name,
                params,
                body,
                decorators,
                line,
            })),
            line,
        ))
    }

    fn parse_classdef(&mut self, decorators: Vec<Expr>) -> PResult<Stmt> {
        let line = self.line();
        self.expect_kw(Kw::Class)?;
        let name = self.expect_name()?;
        let mut bases = Vec::new();
        if self.eat(Op::LParen) {
            if !self.at(Op::RParen) {
                loop {
                    if let TokenKind::Name(n) = self.peek().clone() {
                        if matches!(self.peek_at(1), TokenKind::Op(Op::Assign)) {
                            return self.err(format!(
                                "暂不支持类定义中的关键字参数（如 {}=...，元类等）",
                                n
                            ));
                        }
                    }
                    if self.at(Op::Star) || self.at(Op::DoubleStar) {
                        return self.err("暂不支持类定义中的 * / ** 参数");
                    }
                    bases.push(self.parse_expr(line)?);
                    if !self.eat(Op::Comma) {
                        break;
                    }
                    if self.at(Op::RParen) {
                        break;
                    }
                }
            }
            self.expect(Op::RParen)?;
        }
        self.expect(Op::Colon)?;
        let body = self.parse_block()?;
        Ok(Stmt::new(
            StmtKind::ClassDef(Rc::new(ClassDef {
                name,
                bases,
                body,
                decorators,
                line,
            })),
            line,
        ))
    }

    /// 解析 `:` 之后的缩进块。
    fn parse_block(&mut self) -> PResult<Vec<Stmt>> {
        // 允许 `if x: pass` 这种同一行的简单语句
        if !matches!(self.peek(), TokenKind::Newline) {
            return self.parse_simple_line();
        }
        self.expect_newline()?;
        while matches!(self.peek(), TokenKind::Newline) {
            self.advance();
        }
        if !matches!(self.peek(), TokenKind::Indent) {
            return self.err("期望一个缩进代码块");
        }
        self.advance();
        let mut stmts = Vec::new();
        loop {
            while matches!(self.peek(), TokenKind::Newline) {
                self.advance();
            }
            match self.peek() {
                TokenKind::Dedent => {
                    self.advance();
                    break;
                }
                TokenKind::Eof => break,
                _ => {
                    let mut s = self.parse_statement()?;
                    stmts.append(&mut s);
                }
            }
        }
        Ok(stmts)
    }

    // ---------- 参数与目标 ----------

    fn parse_params(&mut self) -> PResult<Params> {
        self.parse_params_until(Op::RParen)
    }

    /// 解析参数列表，直到遇到 `term`（函数定义是 `)`，lambda 是 `:`）。
    fn parse_params_until(&mut self, term: Op) -> PResult<Params> {
        let mut params = Params::default();
        let mut defaults_pending: Vec<(usize, Expr)> = Vec::new();
        let mut seen_vararg = false;
        let mut seen_kwarg = false;
        let line = self.line();
        while !self.at(term) {
            if self.at(Op::Slash) {
                // 位置专用参数标记：忽略（本实现中位置参数本来也可以按关键字传递）
                self.advance();
                if !self.eat(Op::Comma) {
                    break;
                }
                continue;
            }
            if self.at(Op::Star) {
                self.advance();
                if self.at(Op::Comma) || self.at(term) {
                    // 裸 * ：后面全是关键字参数
                    seen_vararg = true;
                    if !self.eat(Op::Comma) {
                        break;
                    }
                    continue;
                }
                let n = self.expect_name()?;
                if term == Op::RParen && self.eat(Op::Colon) {
                    let _ = self.parse_expr(line)?;
                }
                params.vararg = Some(n);
                seen_vararg = true;
                if !self.eat(Op::Comma) {
                    break;
                }
                continue;
            }
            if self.at(Op::DoubleStar) {
                self.advance();
                let n = self.expect_name()?;
                if term == Op::RParen && self.eat(Op::Colon) {
                    let _ = self.parse_expr(line)?;
                }
                params.kwarg = Some(n);
                seen_kwarg = true;
                if !self.eat(Op::Comma) {
                    break;
                }
                continue;
            }
            let name = self.expect_name()?;
            if term == Op::RParen && self.eat(Op::Colon) {
                let _ = self.parse_expr(line)?; // 参数注解（lambda 没有注解语法）
            }
            let default = if self.eat(Op::Assign) {
                Some(self.parse_expr(line)?)
            } else {
                None
            };
            if seen_vararg || seen_kwarg {
                params.kwonly.push((name, default));
            } else {
                let idx = params.args.len();
                params.args.push(name);
                match default {
                    Some(d) => defaults_pending.push((idx, d)),
                    None => {
                        if !defaults_pending.is_empty() {
                            return self.err("没有默认值的参数不能跟在有默认值的参数之后");
                        }
                    }
                }
            }
            if !self.eat(Op::Comma) {
                break;
            }
        }
        if !defaults_pending.is_empty() {
            let first = defaults_pending[0].0;
            if first + defaults_pending.len() != params.args.len() {
                return self.err("参数默认值的排列不正确");
            }
            params.defaults = defaults_pending.into_iter().map(|(_, d)| d).collect();
        }
        Ok(params)
    }

    /// 解析赋值目标（`for x in ...`、`with ... as x` 等）。
    fn parse_target_list(&mut self) -> PResult<Expr> {
        let line = self.line();
        let first = self.parse_target_atom()?;
        if !self.at(Op::Comma) {
            return Ok(first);
        }
        let mut items = vec![first];
        while self.eat(Op::Comma) {
            if self.at_kw(Kw::In) || self.at(Op::Colon) || self.at(Op::Assign) {
                break;
            }
            items.push(self.parse_target_atom()?);
        }
        Ok(Expr::new(ExprKind::Tuple(items), line))
    }

    fn parse_target_atom(&mut self) -> PResult<Expr> {
        let line = self.line();
        if self.at(Op::Star) {
            self.advance();
            let inner = self.parse_target_atom()?;
            return Ok(Expr::new(ExprKind::Starred(Box::new(inner)), line));
        }
        if self.at(Op::LParen) || self.at(Op::LBracket) {
            let close = if self.at(Op::LParen) {
                Op::RParen
            } else {
                Op::RBracket
            };
            let is_list = self.at(Op::LBracket);
            self.advance();
            let mut items = Vec::new();
            while !self.at(close) {
                items.push(self.parse_target_atom()?);
                if !self.eat(Op::Comma) {
                    break;
                }
            }
            self.expect(close)?;
            return Ok(Expr::new(
                if is_list {
                    ExprKind::List(items)
                } else {
                    ExprKind::Tuple(items)
                },
                line,
            ));
        }
        let mut e = match self.peek().clone() {
            TokenKind::Name(n) => {
                self.advance();
                Expr::new(ExprKind::Name(n), line)
            }
            other => return self.err(format!("非法的赋值目标: {}", other.describe())),
        };
        // 允许 a.b 和 a[i]
        loop {
            if self.at(Op::Dot) {
                self.advance();
                let name = self.expect_name()?;
                e = Expr::new(
                    ExprKind::Attr {
                        obj: Box::new(e),
                        name,
                    },
                    line,
                );
            } else if self.at(Op::LBracket) {
                let idx = self.parse_subscript()?;
                e = Expr::new(
                    ExprKind::Index {
                        obj: Box::new(e),
                        index: Box::new(idx),
                    },
                    line,
                );
            } else {
                break;
            }
        }
        Ok(e)
    }

    // ---------- 表达式 ----------

    /// 逗号分隔的表达式列表；多于一个元素时生成 Tuple。
    pub fn parse_expr_list(&mut self, allow_star: bool, line: u32) -> PResult<Expr> {
        let first = if allow_star && self.at(Op::Star) {
            let l = self.line();
            self.advance();
            Expr::new(ExprKind::Starred(Box::new(self.parse_expr(l)?)), l)
        } else {
            self.parse_expr(line)?
        };
        if !self.at(Op::Comma) {
            return Ok(first);
        }
        let mut items = vec![first];
        while self.eat(Op::Comma) {
            if self.starts_statement_end() {
                break;
            }
            if allow_star && self.at(Op::Star) {
                let l = self.line();
                self.advance();
                items.push(Expr::new(
                    ExprKind::Starred(Box::new(self.parse_expr(l)?)),
                    l,
                ));
            } else {
                items.push(self.parse_expr(self.line())?);
            }
        }
        Ok(Expr::new(ExprKind::Tuple(items), line))
    }

    fn starts_statement_end(&self) -> bool {
        matches!(
            self.peek(),
            TokenKind::Newline
                | TokenKind::Eof
                | TokenKind::Op(Op::Assign)
                | TokenKind::Op(Op::RParen)
                | TokenKind::Op(Op::RBracket)
                | TokenKind::Op(Op::RBrace)
                | TokenKind::Op(Op::Colon)
                | TokenKind::Op(Op::Semi)
                | TokenKind::Kw(Kw::In)
        )
    }

    pub fn parse_expr(&mut self, line: u32) -> PResult<Expr> {
        if self.at_kw(Kw::Lambda) {
            return self.parse_lambda(line);
        }
        let e = self.parse_ternary(line)?;
        Ok(e)
    }

    fn parse_lambda(&mut self, line: u32) -> PResult<Expr> {
        self.expect_kw(Kw::Lambda)?;
        let params = if self.at(Op::Colon) {
            Params::default()
        } else {
            self.parse_params_until(Op::Colon)?
        };
        self.expect(Op::Colon)?;
        let body = self.parse_expr(line)?;
        Ok(Expr::new(
            ExprKind::Lambda(Rc::new(LambdaDef {
                params,
                body: Box::new(body),
            })),
            line,
        ))
    }

    fn parse_ternary(&mut self, line: u32) -> PResult<Expr> {
        let body = self.parse_or(line)?;
        if self.at_kw(Kw::If) {
            self.advance();
            let test = self.parse_or(line)?;
            self.expect_kw(Kw::Else)?;
            let orelse = self.parse_expr(line)?;
            return Ok(Expr::new(
                ExprKind::IfExp {
                    test: Box::new(test),
                    body: Box::new(body),
                    orelse: Box::new(orelse),
                },
                line,
            ));
        }
        Ok(body)
    }

    fn parse_or(&mut self, line: u32) -> PResult<Expr> {
        let first = self.parse_and(line)?;
        if !self.at_kw(Kw::Or) {
            return Ok(first);
        }
        let mut values = vec![first];
        while self.eat_kw(Kw::Or) {
            values.push(self.parse_and(line)?);
        }
        Ok(Expr::new(
            ExprKind::BoolOp {
                op: BoolOpKind::Or,
                values,
            },
            line,
        ))
    }

    fn parse_and(&mut self, line: u32) -> PResult<Expr> {
        let first = self.parse_not(line)?;
        if !self.at_kw(Kw::And) {
            return Ok(first);
        }
        let mut values = vec![first];
        while self.eat_kw(Kw::And) {
            values.push(self.parse_not(line)?);
        }
        Ok(Expr::new(
            ExprKind::BoolOp {
                op: BoolOpKind::And,
                values,
            },
            line,
        ))
    }

    fn parse_not(&mut self, line: u32) -> PResult<Expr> {
        if self.at_kw(Kw::Not) {
            self.advance();
            let operand = self.parse_not(line)?;
            return Ok(Expr::new(
                ExprKind::UnaryOp {
                    op: UnOp::Not,
                    operand: Box::new(operand),
                },
                line,
            ));
        }
        self.parse_comparison(line)
    }

    fn parse_comparison(&mut self, line: u32) -> PResult<Expr> {
        let left = self.parse_bit_or(line)?;
        let mut ops = Vec::new();
        let mut comparators = Vec::new();
        loop {
            let op = match self.peek() {
                TokenKind::Op(Op::EqEq) => Some(CmpOp::Eq),
                TokenKind::Op(Op::NotEq) => Some(CmpOp::NotEq),
                TokenKind::Op(Op::Lt) => Some(CmpOp::Lt),
                TokenKind::Op(Op::Le) => Some(CmpOp::LtE),
                TokenKind::Op(Op::Gt) => Some(CmpOp::Gt),
                TokenKind::Op(Op::Ge) => Some(CmpOp::GtE),
                TokenKind::Kw(Kw::In) => Some(CmpOp::In),
                TokenKind::Kw(Kw::Is) => {
                    if matches!(self.peek_at(1), TokenKind::Kw(Kw::Not)) {
                        Some(CmpOp::IsNot)
                    } else {
                        Some(CmpOp::Is)
                    }
                }
                TokenKind::Kw(Kw::Not) => {
                    if matches!(self.peek_at(1), TokenKind::Kw(Kw::In)) {
                        Some(CmpOp::NotIn)
                    } else {
                        None
                    }
                }
                _ => None,
            };
            let op = match op {
                Some(o) => o,
                None => break,
            };
            match op {
                CmpOp::IsNot => {
                    self.advance();
                    self.advance();
                }
                CmpOp::NotIn => {
                    self.advance();
                    self.advance();
                }
                _ => {
                    self.advance();
                }
            }
            ops.push(op);
            comparators.push(self.parse_bit_or(line)?);
        }
        if ops.is_empty() {
            return Ok(left);
        }
        Ok(Expr::new(
            ExprKind::Compare {
                left: Box::new(left),
                ops,
                comparators,
            },
            line,
        ))
    }

    fn parse_bit_or(&mut self, line: u32) -> PResult<Expr> {
        let mut left = self.parse_bit_xor(line)?;
        while self.at(Op::Pipe) {
            self.advance();
            let right = self.parse_bit_xor(line)?;
            left = Expr::new(
                ExprKind::BinOp {
                    left: Box::new(left),
                    op: Op::Pipe,
                    right: Box::new(right),
                },
                line,
            );
        }
        Ok(left)
    }

    fn parse_bit_xor(&mut self, line: u32) -> PResult<Expr> {
        let mut left = self.parse_bit_and(line)?;
        while self.at(Op::Caret) {
            self.advance();
            let right = self.parse_bit_and(line)?;
            left = Expr::new(
                ExprKind::BinOp {
                    left: Box::new(left),
                    op: Op::Caret,
                    right: Box::new(right),
                },
                line,
            );
        }
        Ok(left)
    }

    fn parse_bit_and(&mut self, line: u32) -> PResult<Expr> {
        let mut left = self.parse_shift(line)?;
        while self.at(Op::Amp) {
            self.advance();
            let right = self.parse_shift(line)?;
            left = Expr::new(
                ExprKind::BinOp {
                    left: Box::new(left),
                    op: Op::Amp,
                    right: Box::new(right),
                },
                line,
            );
        }
        Ok(left)
    }

    fn parse_shift(&mut self, line: u32) -> PResult<Expr> {
        let mut left = self.parse_arith(line)?;
        loop {
            let op = match self.peek() {
                TokenKind::Op(Op::Shl) => Op::Shl,
                TokenKind::Op(Op::Shr) => Op::Shr,
                _ => break,
            };
            self.advance();
            let right = self.parse_arith(line)?;
            left = Expr::new(
                ExprKind::BinOp {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                },
                line,
            );
        }
        Ok(left)
    }

    fn parse_arith(&mut self, line: u32) -> PResult<Expr> {
        let mut left = self.parse_term(line)?;
        loop {
            let op = match self.peek() {
                TokenKind::Op(Op::Plus) => Op::Plus,
                TokenKind::Op(Op::Minus) => Op::Minus,
                _ => break,
            };
            self.advance();
            let right = self.parse_term(line)?;
            left = Expr::new(
                ExprKind::BinOp {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                },
                line,
            );
        }
        Ok(left)
    }

    fn parse_term(&mut self, line: u32) -> PResult<Expr> {
        let mut left = self.parse_factor(line)?;
        loop {
            let op = match self.peek() {
                TokenKind::Op(Op::Star) => Op::Star,
                TokenKind::Op(Op::Slash) => Op::Slash,
                TokenKind::Op(Op::DoubleSlash) => Op::DoubleSlash,
                TokenKind::Op(Op::Percent) => Op::Percent,
                TokenKind::Op(Op::At) => Op::At,
                _ => break,
            };
            self.advance();
            let right = self.parse_factor(line)?;
            left = Expr::new(
                ExprKind::BinOp {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                },
                line,
            );
        }
        Ok(left)
    }

    fn parse_factor(&mut self, line: u32) -> PResult<Expr> {
        let op = match self.peek() {
            TokenKind::Op(Op::Minus) => Some(UnOp::Neg),
            TokenKind::Op(Op::Plus) => Some(UnOp::Pos),
            TokenKind::Op(Op::Tilde) => Some(UnOp::Invert),
            _ => None,
        };
        if let Some(op) = op {
            self.advance();
            let operand = self.parse_factor(line)?;
            return Ok(Expr::new(
                ExprKind::UnaryOp {
                    op,
                    operand: Box::new(operand),
                },
                line,
            ));
        }
        self.parse_power(line)
    }

    fn parse_power(&mut self, line: u32) -> PResult<Expr> {
        let base = self.parse_postfix(line)?;
        if self.at(Op::DoubleStar) {
            self.advance();
            let exp = self.parse_factor(line)?;
            return Ok(Expr::new(
                ExprKind::BinOp {
                    left: Box::new(base),
                    op: Op::DoubleStar,
                    right: Box::new(exp),
                },
                line,
            ));
        }
        Ok(base)
    }

    fn parse_postfix(&mut self, line: u32) -> PResult<Expr> {
        let mut e = self.parse_atom(line)?;
        loop {
            if self.at(Op::Dot) {
                self.advance();
                let name = self.expect_name()?;
                e = Expr::new(
                    ExprKind::Attr {
                        obj: Box::new(e),
                        name,
                    },
                    line,
                );
            } else if self.at(Op::LParen) {
                let args = self.parse_call_args()?;
                e = Expr::new(
                    ExprKind::Call {
                        func: Box::new(e),
                        args,
                    },
                    line,
                );
            } else if self.at(Op::LBracket) {
                let idx = self.parse_subscript()?;
                e = Expr::new(
                    ExprKind::Index {
                        obj: Box::new(e),
                        index: Box::new(idx),
                    },
                    line,
                );
            } else {
                break;
            }
        }
        Ok(e)
    }

    fn parse_call_args(&mut self) -> PResult<Vec<Arg>> {
        let line = self.line();
        self.expect(Op::LParen)?;
        let mut args = Vec::new();
        let mut seen_kw = false;
        while !self.at(Op::RParen) {
            if self.at(Op::Star) {
                self.advance();
                args.push(Arg::Star(self.parse_expr(line)?));
            } else if self.at(Op::DoubleStar) {
                self.advance();
                args.push(Arg::DoubleStar(self.parse_expr(line)?));
            } else if let TokenKind::Name(n) = self.peek().clone() {
                if matches!(self.peek_at(1), TokenKind::Op(Op::Assign)) {
                    self.advance();
                    self.advance();
                    seen_kw = true;
                    args.push(Arg::Kw(n, self.parse_expr(line)?));
                } else {
                    if seen_kw {
                        return self.err("位置参数不能跟在关键字参数之后");
                    }
                    let e = self.parse_expr(line)?;
                    // 生成器表达式作为唯一实参：f(x for x in y)
                    if self.at_kw(Kw::For) {
                        let generators = self.parse_comp_clauses()?;
                        args.push(Arg::Pos(Expr::new(
                            ExprKind::GenExp {
                                elt: Box::new(e),
                                generators,
                            },
                            line,
                        )));
                        break;
                    }
                    args.push(Arg::Pos(e));
                }
            } else {
                if seen_kw {
                    return self.err("位置参数不能跟在关键字参数之后");
                }
                let e = self.parse_expr(line)?;
                // 生成器表达式作为唯一实参：f(x for x in y)
                if self.at_kw(Kw::For) {
                    let generators = self.parse_comp_clauses()?;
                    args.push(Arg::Pos(Expr::new(
                        ExprKind::GenExp {
                            elt: Box::new(e),
                            generators,
                        },
                        line,
                    )));
                    break;
                }
                args.push(Arg::Pos(e));
            }
            if !self.eat(Op::Comma) {
                break;
            }
        }
        self.expect(Op::RParen)?;
        Ok(args)
    }

    fn parse_subscript(&mut self) -> PResult<Expr> {
        let line = self.line();
        self.expect(Op::LBracket)?;
        let mut items = Vec::new();
        loop {
            items.push(self.parse_subscript_item(line)?);
            if !self.eat(Op::Comma) {
                break;
            }
            if self.at(Op::RBracket) {
                break;
            }
        }
        self.expect(Op::RBracket)?;
        if items.len() == 1 {
            Ok(items.pop().unwrap())
        } else {
            Ok(Expr::new(ExprKind::Tuple(items), line))
        }
    }

    fn parse_subscript_item(&mut self, line: u32) -> PResult<Expr> {
        let lower = if self.at(Op::Colon) {
            None
        } else {
            Some(Box::new(self.parse_expr(line)?))
        };
        if self.at(Op::Colon) {
            self.advance();
            let upper = if self.at(Op::Colon) || self.at(Op::Comma) || self.at(Op::RBracket) {
                None
            } else {
                Some(Box::new(self.parse_expr(line)?))
            };
            let step = if self.eat(Op::Colon) {
                if self.at(Op::Comma) || self.at(Op::RBracket) {
                    None
                } else {
                    Some(Box::new(self.parse_expr(line)?))
                }
            } else {
                None
            };
            return Ok(Expr::new(ExprKind::Slice { lower, upper, step }, line));
        }
        match lower {
            Some(e) => Ok(*e),
            None => self.err("无效的下标"),
        }
    }

    fn parse_comp_clauses(&mut self) -> PResult<Vec<CompFor>> {
        let line = self.line();
        let mut out = Vec::new();
        loop {
            self.expect_kw(Kw::For)?;
            let target = self.parse_target_list()?;
            self.expect_kw(Kw::In)?;
            let iter = self.parse_or(line)?;
            let mut ifs = Vec::new();
            while self.at_kw(Kw::If) {
                self.advance();
                ifs.push(self.parse_or(line)?);
            }
            out.push(CompFor { target, iter, ifs });
            if !self.at_kw(Kw::For) {
                break;
            }
        }
        Ok(out)
    }

    fn parse_atom(&mut self, _line: u32) -> PResult<Expr> {
        let tline = self.line();
        match self.peek().clone() {
            TokenKind::Int(v) => {
                self.advance();
                Ok(Expr::new(ExprKind::Int(v), tline))
            }
            TokenKind::Float(v) => {
                self.advance();
                Ok(Expr::new(ExprKind::Float(v), tline))
            }
            TokenKind::Str(s) => {
                self.advance();
                let mut joined = s;
                // 相邻字符串字面量自动拼接
                while let TokenKind::Str(next) = self.peek().clone() {
                    self.advance();
                    joined.push_str(&next);
                }
                Ok(Expr::new(ExprKind::Str(joined), tline))
            }
            TokenKind::FStr(parts) => {
                self.advance();
                let mut pieces = Vec::new();
                for p in parts {
                    match p {
                        FPart::Lit(s) => {
                            if let Some(FStrPiece::Lit(prev)) = pieces.last_mut() {
                                prev.push_str(&s);
                            } else {
                                pieces.push(FStrPiece::Lit(s));
                            }
                        }
                        FPart::Expr { src, conv, spec } => {
                            let e = Parser::parse_expression_string(&src, tline)?;
                            let mut spec_parts: Vec<SpecPart> = Vec::new();
                            let mut lit = String::new();
                            let mut chars = spec.chars().peekable();
                            while let Some(c) = chars.next() {
                                if c == '{' {
                                    let mut inner = String::new();
                                    let mut depth = 1;
                                    for c2 in chars.by_ref() {
                                        if c2 == '{' {
                                            depth += 1;
                                        } else if c2 == '}' {
                                            depth -= 1;
                                            if depth == 0 {
                                                break;
                                            }
                                        }
                                        inner.push(c2);
                                    }
                                    if !lit.is_empty() {
                                        spec_parts.push(SpecPart::Lit(std::mem::take(&mut lit)));
                                    }
                                    let se = Parser::parse_expression_string(&inner, tline)?;
                                    spec_parts.push(SpecPart::Field(Box::new(se)));
                                } else {
                                    lit.push(c);
                                }
                            }
                            if !lit.is_empty() {
                                spec_parts.push(SpecPart::Lit(lit));
                            }
                            pieces.push(FStrPiece::Value {
                                expr: Box::new(e),
                                conv,
                                spec: spec_parts,
                            });
                        }
                    }
                }
                Ok(Expr::new(ExprKind::FStr(pieces), tline))
            }
            TokenKind::Name(n) => {
                self.advance();
                Ok(Expr::new(ExprKind::Name(n), tline))
            }
            TokenKind::Kw(Kw::True) => {
                self.advance();
                Ok(Expr::new(ExprKind::Bool(true), tline))
            }
            TokenKind::Kw(Kw::False) => {
                self.advance();
                Ok(Expr::new(ExprKind::Bool(false), tline))
            }
            TokenKind::Kw(Kw::None) => {
                self.advance();
                Ok(Expr::new(ExprKind::None_, tline))
            }
            TokenKind::Kw(Kw::Lambda) => self.parse_lambda(tline),
            TokenKind::Kw(Kw::Not) => {
                self.advance();
                let operand = self.parse_not(tline)?;
                Ok(Expr::new(
                    ExprKind::UnaryOp {
                        op: UnOp::Not,
                        operand: Box::new(operand),
                    },
                    tline,
                ))
            }
            TokenKind::Kw(Kw::Yield) => self.err("暂不支持 yield / 生成器"),
            TokenKind::Op(Op::Ellipsis) => self.err("暂不支持 `...`（Ellipsis）"),
            TokenKind::Op(Op::LParen) => self.parse_paren(tline),
            TokenKind::Op(Op::LBracket) => self.parse_list(tline),
            TokenKind::Op(Op::LBrace) => self.parse_brace(tline),
            other => self.err(format!("无法解析的表达式: {}", other.describe())),
        }
    }

    fn parse_paren(&mut self, line: u32) -> PResult<Expr> {
        self.expect(Op::LParen)?;
        if self.eat(Op::RParen) {
            return Ok(Expr::new(ExprKind::Tuple(Vec::new()), line));
        }
        // 生成器表达式
        if self.at(Op::Star) {
            let first = self.parse_star_or_expr(line)?;
            return self.finish_tuple(line, first, Op::RParen, true);
        }
        let first = self.parse_expr(line)?;
        if self.at_kw(Kw::For) {
            let generators = self.parse_comp_clauses()?;
            self.expect(Op::RParen)?;
            return Ok(Expr::new(
                ExprKind::GenExp {
                    elt: Box::new(first),
                    generators,
                },
                line,
            ));
        }
        self.finish_tuple(line, first, Op::RParen, true)
    }

    fn finish_tuple(&mut self, line: u32, first: Expr, close: Op, _paren: bool) -> PResult<Expr> {
        if !self.at(Op::Comma) {
            self.expect(close)?;
            return Ok(first);
        }
        let mut items = vec![first];
        while self.eat(Op::Comma) {
            if self.at(close) {
                break;
            }
            items.push(self.parse_star_or_expr(line)?);
        }
        self.expect(close)?;
        Ok(Expr::new(ExprKind::Tuple(items), line))
    }

    fn parse_star_or_expr(&mut self, line: u32) -> PResult<Expr> {
        if self.at(Op::Star) {
            self.advance();
            let inner = self.parse_expr(line)?;
            return Ok(Expr::new(ExprKind::Starred(Box::new(inner)), line));
        }
        self.parse_expr(line)
    }

    fn parse_list(&mut self, line: u32) -> PResult<Expr> {
        self.expect(Op::LBracket)?;
        if self.eat(Op::RBracket) {
            return Ok(Expr::new(ExprKind::List(Vec::new()), line));
        }
        let first = self.parse_star_or_expr(line)?;
        if self.at_kw(Kw::For) {
            let generators = self.parse_comp_clauses()?;
            self.expect(Op::RBracket)?;
            return Ok(Expr::new(
                ExprKind::ListComp {
                    elt: Box::new(first),
                    generators,
                },
                line,
            ));
        }
        let mut items = vec![first];
        while self.eat(Op::Comma) {
            if self.at(Op::RBracket) {
                break;
            }
            items.push(self.parse_star_or_expr(line)?);
        }
        self.expect(Op::RBracket)?;
        Ok(Expr::new(ExprKind::List(items), line))
    }

    fn parse_brace(&mut self, line: u32) -> PResult<Expr> {
        self.expect(Op::LBrace)?;
        if self.eat(Op::RBrace) {
            return Ok(Expr::new(ExprKind::Dict(Vec::new()), line));
        }
        // **mapping 展开
        if self.at(Op::DoubleStar) {
            let mut entries = Vec::new();
            let mut star = false;
            loop {
                if self.at(Op::DoubleStar) {
                    self.advance();
                    entries.push((None, self.parse_expr(line)?));
                    star = true;
                } else {
                    if star {
                        // 字典展开后面的普通元素
                    }
                    let k = self.parse_expr(line)?;
                    self.expect(Op::Colon)?;
                    let v = self.parse_expr(line)?;
                    entries.push((Some(k), v));
                }
                if !self.eat(Op::Comma) {
                    break;
                }
                if self.at(Op::RBrace) {
                    break;
                }
            }
            self.expect(Op::RBrace)?;
            return Ok(Expr::new(ExprKind::Dict(entries), line));
        }
        let first = self.parse_star_or_expr(line)?;
        if self.at(Op::Colon) {
            self.advance();
            let value = self.parse_expr(line)?;
            if self.at_kw(Kw::For) {
                let generators = self.parse_comp_clauses()?;
                self.expect(Op::RBrace)?;
                return Ok(Expr::new(
                    ExprKind::DictComp {
                        key: Box::new(first),
                        value: Box::new(value),
                        generators,
                    },
                    line,
                ));
            }
            let mut entries = vec![(Some(first), value)];
            while self.eat(Op::Comma) {
                if self.at(Op::RBrace) {
                    break;
                }
                if self.at(Op::DoubleStar) {
                    self.advance();
                    entries.push((None, self.parse_expr(line)?));
                    continue;
                }
                let k = self.parse_expr(line)?;
                if self.at(Op::Colon) {
                    self.advance();
                    let v = self.parse_expr(line)?;
                    entries.push((Some(k), v));
                } else {
                    // {**a, "b"} 不是合法字典
                    return self.err("字典字面量中的键缺少对应的值");
                }
            }
            self.expect(Op::RBrace)?;
            return Ok(Expr::new(ExprKind::Dict(entries), line));
        }
        // 集合
        if self.at_kw(Kw::For) {
            let generators = self.parse_comp_clauses()?;
            self.expect(Op::RBrace)?;
            return Ok(Expr::new(
                ExprKind::SetComp {
                    elt: Box::new(first),
                    generators,
                },
                line,
            ));
        }
        let mut items = vec![first];
        while self.eat(Op::Comma) {
            if self.at(Op::RBrace) {
                break;
            }
            items.push(self.parse_star_or_expr(line)?);
        }
        self.expect(Op::RBrace)?;
        Ok(Expr::new(ExprKind::Set(items), line))
    }
}

/// 增量赋值运算符对应的基础运算符。
fn aug_base(op: Op) -> Option<Op> {
    Some(match op {
        Op::PlusEq => Op::Plus,
        Op::MinusEq => Op::Minus,
        Op::StarEq => Op::Star,
        Op::SlashEq => Op::Slash,
        Op::DoubleSlashEq => Op::DoubleSlash,
        Op::PercentEq => Op::Percent,
        Op::AmpEq => Op::Amp,
        Op::PipeEq => Op::Pipe,
        Op::CaretEq => Op::Caret,
        Op::ShlEq => Op::Shl,
        Op::ShrEq => Op::Shr,
        Op::DoubleStarEq => Op::DoubleStar,
        Op::AtEq => Op::At,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Vec<Stmt> {
        Parser::parse_source(src).unwrap_or_else(|e| panic!("解析失败: {}\n源码:\n{}", e, src))
    }

    #[test]
    fn parse_assignment_and_expr() {
        let s = parse("x = 1\nprint(x + 2)\n");
        assert_eq!(s.len(), 2);
        match &s[0].kind {
            StmtKind::Assign { targets, value } => {
                assert_eq!(targets.len(), 1);
                assert!(matches!(targets[0].kind, ExprKind::Name(_)));
                assert!(matches!(value.kind, ExprKind::Int(1)));
            }
            other => panic!("期望赋值语句，得到 {:?}", other),
        }
    }

    #[test]
    fn parse_chained_assignment() {
        let s = parse("a = b = c = 0\n");
        match &s[0].kind {
            StmtKind::Assign { targets, .. } => assert_eq!(targets.len(), 3),
            other => panic!("期望赋值语句，得到 {:?}", other),
        }
    }

    #[test]
    fn precedence_and_right_assoc_power() {
        let s = parse("x = 2 ** 3 ** 2\n");
        match &s[0].kind {
            StmtKind::Assign { value, .. } => match &value.kind {
                ExprKind::BinOp { left, op, right } => {
                    assert_eq!(*op, Op::DoubleStar);
                    assert!(matches!(left.kind, ExprKind::Int(2)));
                    assert!(matches!(right.kind, ExprKind::BinOp { .. }));
                }
                other => panic!("期望幂运算，得到 {:?}", other),
            },
            other => panic!("期望赋值语句，得到 {:?}", other),
        }
    }

    #[test]
    fn parse_function_with_defaults_and_kwargs() {
        let s = parse("def f(a, b=2, *args, c=3, **kw):\n    return a\n");
        match &s[0].kind {
            StmtKind::FuncDef(f) => {
                assert_eq!(f.params.args, vec!["a", "b"]);
                assert_eq!(f.params.defaults.len(), 1);
                assert_eq!(f.params.vararg.as_deref(), Some("args"));
                assert_eq!(f.params.kwonly.len(), 1);
                assert_eq!(f.params.kwarg.as_deref(), Some("kw"));
            }
            other => panic!("期望函数定义，得到 {:?}", other),
        }
    }

    #[test]
    fn parse_class_and_inheritance() {
        let s = parse("class A(B, C):\n    def m(self):\n        pass\n");
        match &s[0].kind {
            StmtKind::ClassDef(c) => {
                assert_eq!(c.name, "A");
                assert_eq!(c.bases.len(), 2);
                assert_eq!(c.body.len(), 1);
            }
            other => panic!("期望类定义，得到 {:?}", other),
        }
    }

    #[test]
    fn parse_comprehensions() {
        parse("a = [x * 2 for x in range(10) if x % 2 == 0]\n");
        parse("b = {k: v for k, v in items}\n");
        parse("c = {x for x in xs}\n");
        parse("d = list(x for x in xs if x)\n");
    }

    #[test]
    fn parse_try_except_finally() {
        parse("try:\n    pass\nexcept ValueError as e:\n    raise\nfinally:\n    pass\n");
    }

    #[test]
    fn parse_slices() {
        parse("a[1:2]\na[:]\na[::-1]\na[1, 2]\n");
    }

    #[test]
    fn parse_fstring_expression() {
        let s = parse("print(f'{x + 1:>5}-{{literal}}')\n");
        match &s[0].kind {
            StmtKind::Expr(e) => match &e.kind {
                ExprKind::Call { args, .. } => match &args[0] {
                    Arg::Pos(inner) => match &inner.kind {
                        ExprKind::FStr(pieces) => {
                            assert_eq!(pieces.len(), 2);
                            assert!(
                                matches!(pieces[0], FStrPiece::Value { .. }),
                                "第一段应为表达式"
                            );
                        }
                        other => panic!("期望 f-string，得到 {:?}", other),
                    },
                    other => panic!("期望位置参数，得到 {:?}", other),
                },
                other => panic!("期望调用，得到 {:?}", other),
            },
            other => panic!("期望表达式语句，得到 {:?}", other),
        }
    }

    #[test]
    fn parse_multiline_structures() {
        parse("x = {\n    'a': 1,\n    'b': 2,\n}\n");
        parse("f(\n  1,\n  2,\n)\n");
    }

    #[test]
    fn syntax_errors_are_reported() {
        assert!(Parser::parse_source("def f(:\n").is_err());
        assert!(Parser::parse_source("if x\n    pass\n").is_err());
        assert!(Parser::parse_source("x = \n").is_err());
    }
}
