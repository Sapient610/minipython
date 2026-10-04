//! 词法分析器：把源码切分为 token 流。
//!
//! 特点：
//! * 缩进敏感（INDENT / DEDENT），Tab 按 8 列对齐；
//! * 括号内换行、反斜杠续行不产生 NEWLINE；
//! * 支持注释、三引号字符串、raw 字符串、f-string、数字下划线分隔；
//! * 每个 token 带行号列号，便于报错。

use crate::error::LexError;
use std::fmt;

/// f-string 的组成片段。
#[derive(Clone, Debug, PartialEq)]
pub enum FPart {
    /// 字面文本（`{{` / `}}` 会在此处还原为 `{` / `}`）。
    Lit(String),
    /// `{表达式!转换:格式}`，表达式源码在此保存，由 parser 二次解析。
    Expr {
        src: String,
        conv: Option<char>,
        spec: String,
    },
}

/// Python 关键字。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kw {
    False,
    None,
    True,
    And,
    As,
    Assert,
    Async,
    Await,
    Break,
    Class,
    Continue,
    Def,
    Del,
    Elif,
    Else,
    Except,
    Finally,
    For,
    From,
    Global,
    If,
    Import,
    In,
    Is,
    Lambda,
    Nonlocal,
    Not,
    Or,
    Pass,
    Raise,
    Return,
    Try,
    While,
    With,
    Yield,
}

impl Kw {
    pub fn keyword(s: &str) -> Option<Kw> {
        Some(match s {
            "False" => Kw::False,
            "None" => Kw::None,
            "True" => Kw::True,
            "and" => Kw::And,
            "as" => Kw::As,
            "assert" => Kw::Assert,
            "async" => Kw::Async,
            "await" => Kw::Await,
            "break" => Kw::Break,
            "class" => Kw::Class,
            "continue" => Kw::Continue,
            "def" => Kw::Def,
            "del" => Kw::Del,
            "elif" => Kw::Elif,
            "else" => Kw::Else,
            "except" => Kw::Except,
            "finally" => Kw::Finally,
            "for" => Kw::For,
            "from" => Kw::From,
            "global" => Kw::Global,
            "if" => Kw::If,
            "import" => Kw::Import,
            "in" => Kw::In,
            "is" => Kw::Is,
            "lambda" => Kw::Lambda,
            "nonlocal" => Kw::Nonlocal,
            "not" => Kw::Not,
            "or" => Kw::Or,
            "pass" => Kw::Pass,
            "raise" => Kw::Raise,
            "return" => Kw::Return,
            "try" => Kw::Try,
            "while" => Kw::While,
            "with" => Kw::With,
            "yield" => Kw::Yield,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Kw::False => "False",
            Kw::None => "None",
            Kw::True => "True",
            Kw::And => "and",
            Kw::As => "as",
            Kw::Assert => "assert",
            Kw::Async => "async",
            Kw::Await => "await",
            Kw::Break => "break",
            Kw::Class => "class",
            Kw::Continue => "continue",
            Kw::Def => "def",
            Kw::Del => "del",
            Kw::Elif => "elif",
            Kw::Else => "else",
            Kw::Except => "except",
            Kw::Finally => "finally",
            Kw::For => "for",
            Kw::From => "from",
            Kw::Global => "global",
            Kw::If => "if",
            Kw::Import => "import",
            Kw::In => "in",
            Kw::Is => "is",
            Kw::Lambda => "lambda",
            Kw::Nonlocal => "nonlocal",
            Kw::Not => "not",
            Kw::Or => "or",
            Kw::Pass => "pass",
            Kw::Raise => "raise",
            Kw::Return => "return",
            Kw::Try => "try",
            Kw::While => "while",
            Kw::With => "with",
            Kw::Yield => "yield",
        }
    }
}

/// 运算符 / 分隔符。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Plus,
    Minus,
    Star,
    DoubleStar,
    Slash,
    DoubleSlash,
    Percent,
    At,
    Amp,
    Pipe,
    Caret,
    Tilde,
    Shl,
    Shr,
    Lt,
    Gt,
    Le,
    Ge,
    EqEq,
    NotEq,
    Assign,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    DoubleSlashEq,
    PercentEq,
    AmpEq,
    PipeEq,
    CaretEq,
    ShlEq,
    ShrEq,
    DoubleStarEq,
    AtEq,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Colon,
    Dot,
    Semi,
    Arrow,
    Walrus,
    Ellipsis,
}

impl Op {
    pub fn as_str(self) -> &'static str {
        use Op::*;
        match self {
            Plus => "+",
            Minus => "-",
            Star => "*",
            DoubleStar => "**",
            Slash => "/",
            DoubleSlash => "//",
            Percent => "%",
            At => "@",
            Amp => "&",
            Pipe => "|",
            Caret => "^",
            Tilde => "~",
            Shl => "<<",
            Shr => ">>",
            Lt => "<",
            Gt => ">",
            Le => "<=",
            Ge => ">=",
            EqEq => "==",
            NotEq => "!=",
            Assign => "=",
            PlusEq => "+=",
            MinusEq => "-=",
            StarEq => "*=",
            SlashEq => "/=",
            DoubleSlashEq => "//=",
            PercentEq => "%=",
            AmpEq => "&=",
            PipeEq => "|=",
            CaretEq => "^=",
            ShlEq => "<<=",
            ShrEq => ">>=",
            DoubleStarEq => "**=",
            AtEq => "@=",
            LParen => "(",
            RParen => ")",
            LBracket => "[",
            RBracket => "]",
            LBrace => "{",
            RBrace => "}",
            Comma => ",",
            Colon => ":",
            Dot => ".",
            Semi => ";",
            Arrow => "->",
            Walrus => ":=",
            Ellipsis => "...",
        }
    }
}

impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    Name(String),
    Int(i64),
    Float(f64),
    Str(String),
    FStr(Vec<FPart>),
    Kw(Kw),
    Op(Op),
    Newline,
    Indent,
    Dedent,
    Eof,
}

impl TokenKind {
    /// 用于报错时展示。
    pub fn describe(&self) -> String {
        match self {
            TokenKind::Name(n) => format!("'{}'", n),
            TokenKind::Int(i) => format!("{}", i),
            TokenKind::Float(x) => format!("{}", x),
            TokenKind::Str(_) => "字符串字面量".to_string(),
            TokenKind::FStr(_) => "f-string".to_string(),
            TokenKind::Kw(k) => format!("'{}'", k.as_str()),
            TokenKind::Op(o) => format!("'{}'", o.as_str()),
            TokenKind::Newline => "换行".to_string(),
            TokenKind::Indent => "缩进".to_string(),
            TokenKind::Dedent => "减少缩进".to_string(),
            TokenKind::Eof => "文件结尾".to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub line: u32,
    pub col: u32,
}

impl Token {
    pub fn new(kind: TokenKind, line: u32, col: u32) -> Self {
        Token { kind, line, col }
    }
}

pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: u32,
    col: u32,
    indents: Vec<usize>,
    /// 括号 / 方括号 / 花括号的嵌套层数
    depth: usize,
    tokens: Vec<Token>,
    at_line_start: bool,
}

impl Lexer {
    pub fn new(src: &str) -> Self {
        Lexer {
            chars: src.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
            indents: vec![0],
            depth: 0,
            tokens: Vec::new(),
            at_line_start: true,
        }
    }

    /// 词法分析入口。
    pub fn tokenize(src: &str) -> Result<Vec<Token>, LexError> {
        let mut lx = Lexer::new(src);
        lx.run()?;
        Ok(lx.tokens)
    }

    fn peek(&self, n: usize) -> Option<char> {
        self.chars.get(self.pos + n).copied()
    }

    fn cur(&self) -> Option<char> {
        self.peek(0)
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied();
        if c.is_some() {
            self.pos += 1;
            if c == Some('\n') {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
        }
        c
    }

    fn err<T>(&self, msg: impl Into<String>) -> Result<T, LexError> {
        Err(LexError {
            msg: msg.into(),
            line: self.line,
            col: self.col,
        })
    }

    fn push(&mut self, kind: TokenKind, line: u32, col: u32) {
        self.tokens.push(Token::new(kind, line, col));
    }

    fn run(&mut self) -> Result<(), LexError> {
        loop {
            if self.at_line_start
                && self.depth == 0
                && self.cur().is_some()
                && self.handle_indent()?
            {
                continue; // 空行，继续下一行
            }
            let (line, col) = (self.line, self.col);
            let c = match self.cur() {
                Some(c) => c,
                None => break,
            };
            match c {
                ' ' | '\t' | '\x0c' => {
                    self.bump();
                }
                '\r' => {
                    self.bump();
                }
                '\n' => {
                    self.bump();
                    if self.depth == 0 {
                        self.at_line_start = true;
                        self.emit_newline(line, col);
                    }
                }
                '#' => {
                    while let Some(c) = self.cur() {
                        if c == '\n' {
                            break;
                        }
                        self.bump();
                    }
                }
                '\\' => {
                    if self.peek(1) == Some('\n') {
                        self.bump();
                        self.bump();
                    } else if self.peek(1) == Some('\r') && self.peek(2) == Some('\n') {
                        self.bump();
                        self.bump();
                        self.bump();
                    } else {
                        return self.err("意外的字符 '\\'");
                    }
                }
                c if c.is_ascii_digit() => self.lex_number()?,
                '.' if self.peek(1).is_some_and(|c| c.is_ascii_digit()) => self.lex_number()?,
                '"' | '\'' => {
                    let s = self.lex_string()?;
                    self.push(TokenKind::Str(s), line, col);
                }
                c if c == '_' || c.is_alphabetic() => self.lex_name_or_string()?,
                _ => self.lex_op()?,
            }
        }

        // 文件结束：补 NEWLINE，逐层退回到 0 缩进，最后 EOF
        let (line, col) = (self.line, self.col);
        if !self.tokens.is_empty() {
            let need_nl = !matches!(
                self.tokens.last().map(|t| &t.kind),
                Some(TokenKind::Newline) | Some(TokenKind::Indent) | Some(TokenKind::Dedent) | None
            );
            if need_nl {
                self.push(TokenKind::Newline, line, col);
            }
        }
        while self.indents.len() > 1 {
            self.indents.pop();
            self.push(TokenKind::Dedent, line, col);
        }
        self.push(TokenKind::Eof, line, col);
        Ok(())
    }

    fn emit_newline(&mut self, line: u32, col: u32) {
        if self.tokens.is_empty() {
            return;
        }
        match self.tokens.last().map(|t| &t.kind) {
            Some(TokenKind::Newline) | Some(TokenKind::Indent) | Some(TokenKind::Dedent) | None => {
            }
            _ => self.push(TokenKind::Newline, line, col),
        }
    }

    /// 处理行首缩进。返回 true 表示这是一行空行/注释行，已被整行跳过。
    fn handle_indent(&mut self) -> Result<bool, LexError> {
        let mut width = 0usize;
        loop {
            match self.cur() {
                Some(' ') => {
                    width += 1;
                    self.bump();
                }
                Some('\t') => {
                    width = (width / 8 + 1) * 8;
                    self.bump();
                }
                Some('\x0c') => {
                    width = 0;
                    self.bump();
                }
                _ => break,
            }
        }
        // 空行 / 只有注释的行：整行跳过，不产生 token
        if matches!(self.cur(), None | Some('\n') | Some('\r')) || self.cur() == Some('#') {
            while let Some(c) = self.cur() {
                if c == '\n' {
                    break;
                }
                self.bump();
            }
            if self.cur() == Some('\n') {
                self.bump();
            }
            return Ok(true);
        }
        self.at_line_start = false;
        let cur_indent = *self.indents.last().unwrap();
        let (line, col) = (self.line, self.col);
        if width > cur_indent {
            self.indents.push(width);
            self.push(TokenKind::Indent, line, col);
        } else if width < cur_indent {
            while width < *self.indents.last().unwrap() {
                self.indents.pop();
                self.push(TokenKind::Dedent, line, col);
            }
            if width != *self.indents.last().unwrap() {
                return Err(LexError {
                    msg: "缩进不一致（unindent does not match any outer indentation level）"
                        .to_string(),
                    line,
                    col,
                });
            }
        }
        Ok(false)
    }

    fn lex_number(&mut self) -> Result<(), LexError> {
        let (line, col) = (self.line, self.col);
        let start = self.pos;
        let mut is_float = false;
        if self.cur() == Some('0')
            && matches!(
                self.peek(1),
                Some('x') | Some('X') | Some('o') | Some('O') | Some('b') | Some('B')
            )
        {
            let radix_char = self.peek(1).unwrap();
            self.bump();
            self.bump();
            let radix = match radix_char {
                'x' | 'X' => 16,
                'o' | 'O' => 8,
                _ => 2,
            };
            let digits_start = self.pos;
            while let Some(c) = self.cur() {
                if c == '_' || c.is_digit(radix) {
                    self.bump();
                } else {
                    break;
                }
            }
            let text: String = self.chars[digits_start..self.pos]
                .iter()
                .filter(|c| **c != '_')
                .collect();
            if text.is_empty() {
                return self.err("无效的数字字面量");
            }
            match i64::from_str_radix(&text, radix) {
                Ok(v) => {
                    self.push(TokenKind::Int(v), line, col);
                    return Ok(());
                }
                Err(_) => return self.err("整数常量过大（本实现使用 64 位整数）"),
            }
        }
        // 十进制整数 / 浮点
        while let Some(c) = self.cur() {
            if c.is_ascii_digit() || c == '_' {
                self.bump();
            } else if c == '.' && !is_float && self.peek(1) != Some('.') {
                is_float = true;
                self.bump();
            } else if (c == 'e' || c == 'E')
                && self
                    .peek(1)
                    .is_some_and(|n| n.is_ascii_digit() || n == '+' || n == '-')
            {
                is_float = true;
                self.bump();
                if matches!(self.cur(), Some('+') | Some('-')) {
                    self.bump();
                }
            } else {
                break;
            }
        }
        // 紧跟字母说明是无效的数字（如 1abc）
        if self.cur().is_some_and(|c| c.is_alphabetic() || c == '_') {
            return self.err("无效的数字字面量");
        }
        let text: String = self.chars[start..self.pos]
            .iter()
            .filter(|c| **c != '_')
            .collect();
        if is_float {
            match text.parse::<f64>() {
                Ok(v) => self.push(TokenKind::Float(v), line, col),
                Err(_) => return self.err("无效的浮点字面量"),
            }
        } else {
            match text.parse::<i64>() {
                Ok(v) => self.push(TokenKind::Int(v), line, col),
                Err(_) => return self.err("整数常量过大（本实现使用 64 位整数）"),
            }
        }
        Ok(())
    }

    /// 标识符、关键字，以及带前缀的字符串（r/b/f/u）。
    fn lex_name_or_string(&mut self) -> Result<(), LexError> {
        let (line, col) = (self.line, self.col);
        let start = self.pos;
        while let Some(c) = self.cur() {
            if c == '_' || c.is_alphanumeric() {
                self.bump();
            } else {
                break;
            }
        }
        let word: String = self.chars[start..self.pos].iter().collect();
        // 字符串前缀？
        let lower = word.to_ascii_lowercase();
        let is_prefix = word.len() <= 2
            && lower.chars().all(|c| "rbfu".contains(c))
            && self.cur().is_some_and(|c| c == '"' || c == '\'');
        if is_prefix {
            let raw = lower.contains('r');
            let is_f = lower.contains('f');
            if lower.contains('b') {
                return Err(LexError {
                    msg: "暂不支持 bytes 字面量".to_string(),
                    line,
                    col,
                });
            }
            if is_f {
                let parts = self.lex_fstring()?;
                self.push(TokenKind::FStr(parts), line, col);
            } else {
                let s = self.lex_string_body(raw)?;
                self.push(TokenKind::Str(s), line, col);
            }
            return Ok(());
        }
        match Kw::keyword(&word) {
            Some(k) => {
                if matches!(k, Kw::Async | Kw::Await) {
                    return Err(LexError {
                        msg: "暂不支持 async / await".to_string(),
                        line,
                        col,
                    });
                }
                self.push(TokenKind::Kw(k), line, col);
            }
            None => self.push(TokenKind::Name(word), line, col),
        }
        Ok(())
    }

    fn lex_string(&mut self) -> Result<String, LexError> {
        self.lex_string_body(false)
    }

    /// 读取字符串主体（调用前当前字符是引号）。
    fn lex_string_body(&mut self, raw: bool) -> Result<String, LexError> {
        let quote = self.cur().unwrap();
        let triple = self.peek(1) == Some(quote) && self.peek(2) == Some(quote);
        if triple {
            self.bump();
            self.bump();
            self.bump();
        } else {
            self.bump();
        }
        let mut out = String::new();
        loop {
            let c = match self.cur() {
                Some(c) => c,
                None => return self.err("字符串没有结束引号"),
            };
            if c == quote {
                if triple {
                    if self.peek(1) == Some(quote) && self.peek(2) == Some(quote) {
                        self.bump();
                        self.bump();
                        self.bump();
                        return Ok(out);
                    }
                    out.push(c);
                    self.bump();
                } else {
                    self.bump();
                    return Ok(out);
                }
            } else if c == '\\' && !raw {
                self.bump();
                self.lex_escape(&mut out)?;
            } else if c == '\n' && !triple {
                return self.err("行尾的字符串没有闭合");
            } else {
                out.push(c);
                self.bump();
            }
        }
    }

    fn lex_escape(&mut self, out: &mut String) -> Result<(), LexError> {
        let c = match self.bump() {
            Some(c) => c,
            None => return self.err("字符串没有结束引号"),
        };
        match c {
            '\n' => {}
            '\\' => out.push('\\'),
            '\'' => out.push('\''),
            '"' => out.push('"'),
            'a' => out.push('\x07'),
            'b' => out.push('\x08'),
            'f' => out.push('\x0c'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            'v' => out.push('\x0b'),
            '0'..='7' => {
                let mut v = c.to_digit(8).unwrap();
                for _ in 0..2 {
                    match self.cur().and_then(|c| c.to_digit(8)) {
                        Some(d) => {
                            v = v * 8 + d;
                            self.bump();
                        }
                        None => break,
                    }
                }
                out.push(char::from_u32(v).unwrap_or('\u{fffd}'));
            }
            'x' => {
                let mut v = 0u32;
                for _ in 0..2 {
                    match self.cur().and_then(|c| c.to_digit(16)) {
                        Some(d) => {
                            v = v * 16 + d;
                            self.bump();
                        }
                        None => return self.err("无效的 \\x 转义"),
                    }
                }
                out.push(char::from_u32(v).unwrap_or('\u{fffd}'));
            }
            'u' | 'U' => {
                let n = if c == 'u' { 4 } else { 8 };
                let mut v = 0u32;
                for _ in 0..n {
                    match self.cur().and_then(|c| c.to_digit(16)) {
                        Some(d) => {
                            v = v * 16 + d;
                            self.bump();
                        }
                        None => return self.err("无效的 unicode 转义"),
                    }
                }
                match char::from_u32(v) {
                    Some(ch) => out.push(ch),
                    None => return self.err("无效的 unicode 码点"),
                }
            }
            other => {
                out.push('\\');
                out.push(other);
            }
        }
        Ok(())
    }

    /// 读取 f-string，返回组成片段。表达式部分只做原文抽取，交给 parser。
    fn lex_fstring(&mut self) -> Result<Vec<FPart>, LexError> {
        let quote = self.cur().unwrap();
        let triple = self.peek(1) == Some(quote) && self.peek(2) == Some(quote);
        if triple {
            self.bump();
            self.bump();
            self.bump();
        } else {
            self.bump();
        }
        let mut parts: Vec<FPart> = Vec::new();
        let mut lit = String::new();
        loop {
            let c = match self.cur() {
                Some(c) => c,
                None => return self.err("f-string 没有结束引号"),
            };
            if c == quote {
                if triple {
                    if self.peek(1) == Some(quote) && self.peek(2) == Some(quote) {
                        self.bump();
                        self.bump();
                        self.bump();
                        break;
                    }
                    lit.push(c);
                    self.bump();
                } else {
                    self.bump();
                    break;
                }
            } else if c == '\\' {
                self.bump();
                self.lex_escape(&mut lit)?;
            } else if c == '{' {
                if self.peek(1) == Some('{') {
                    self.bump();
                    self.bump();
                    lit.push('{');
                    continue;
                }
                self.bump();
                if !lit.is_empty() {
                    parts.push(FPart::Lit(std::mem::take(&mut lit)));
                }
                // 抽取表达式源码（记录顶层 ':' 的位置，它后面是格式说明符）
                let mut depth = 0usize;
                let mut src = String::new();
                let mut spec_pos: Option<usize> = None;
                loop {
                    let c = match self.cur() {
                        Some(c) => c,
                        None => return self.err("f-string 中的表达式没有闭合"),
                    };
                    match c {
                        '{' | '(' | '[' => {
                            depth += 1;
                            src.push(c);
                            self.bump();
                        }
                        '}' if depth == 0 => {
                            break;
                        }
                        ')' | ']' => {
                            depth = depth.saturating_sub(1);
                            src.push(c);
                            self.bump();
                        }
                        '}' => {
                            depth -= 1;
                            src.push(c);
                            self.bump();
                        }
                        ':' if depth == 0 && spec_pos.is_none() => {
                            spec_pos = Some(src.chars().count());
                            src.push(c);
                            self.bump();
                        }
                        '!' if depth == 0
                            && spec_pos.is_none()
                            && matches!(self.peek(1), Some('r') | Some('s') | Some('a'))
                            && (self.peek(2) == Some(':') || self.peek(2) == Some('}')) =>
                        {
                            break;
                        }
                        '\'' | '"' => {
                            // 表达式里的字符串字面量整体跳过
                            let q = c;
                            src.push(c);
                            self.bump();
                            while let Some(ic) = self.cur() {
                                if ic == '\\' {
                                    src.push(ic);
                                    self.bump();
                                    if let Some(n) = self.cur() {
                                        src.push(n);
                                        self.bump();
                                    }
                                } else if ic == q {
                                    src.push(ic);
                                    self.bump();
                                    break;
                                } else {
                                    src.push(ic);
                                    self.bump();
                                }
                            }
                        }
                        _ => {
                            src.push(c);
                            self.bump();
                        }
                    }
                }
                let (src, mut spec) = match spec_pos {
                    Some(p) => {
                        let chars: Vec<char> = src.chars().collect();
                        (
                            chars[..p].iter().collect::<String>(),
                            chars[p + 1..].iter().collect::<String>(),
                        )
                    }
                    None => (src, String::new()),
                };
                let mut conv = None;
                if self.cur() == Some('!') {
                    self.bump();
                    conv = self.bump();
                }
                if self.cur() == Some(':') {
                    self.bump();
                    // 格式说明符：允许嵌套 {}，如 f"{x:>{w}}"
                    let mut depth = 0usize;
                    let mut s = String::new();
                    while let Some(c) = self.cur() {
                        if c == '{' {
                            depth += 1;
                        } else if c == '}' {
                            if depth == 0 {
                                break;
                            }
                            depth -= 1;
                        }
                        s.push(c);
                        self.bump();
                    }
                    spec = s;
                }
                if self.cur() != Some('}') {
                    return self.err("f-string 中的表达式没有闭合");
                }
                self.bump();
                if src.trim().is_empty() {
                    return self.err("f-string 中的表达式为空");
                }
                parts.push(FPart::Expr {
                    src: src.trim().to_string(),
                    conv,
                    spec,
                });
            } else if c == '}' {
                if self.peek(1) == Some('}') {
                    self.bump();
                    self.bump();
                    lit.push('}');
                    continue;
                }
                return self.err("f-string 中出现了单个 '}'");
            } else {
                lit.push(c);
                self.bump();
            }
        }
        if !lit.is_empty() {
            parts.push(FPart::Lit(lit));
        }
        Ok(parts)
    }

    fn lex_op(&mut self) -> Result<(), LexError> {
        use Op::*;
        let (line, col) = (self.line, self.col);
        let c0 = self.cur().unwrap();
        let c1 = self.peek(1);
        let c2 = self.peek(2);
        // 三字符
        let three = match (c0, c1, c2) {
            ('*', Some('*'), Some('=')) => Some(DoubleStarEq),
            ('/', Some('/'), Some('=')) => Some(DoubleSlashEq),
            ('<', Some('<'), Some('=')) => Some(ShlEq),
            ('>', Some('>'), Some('=')) => Some(ShrEq),
            ('.', Some('.'), Some('.')) => Some(Ellipsis),
            _ => None,
        };
        if let Some(op) = three {
            for _ in 0..3 {
                self.bump();
            }
            self.push(TokenKind::Op(op), line, col);
            return Ok(());
        }
        let two = match (c0, c1) {
            ('*', Some('*')) => Some(DoubleStar),
            ('/', Some('/')) => Some(DoubleSlash),
            ('<', Some('<')) => Some(Shl),
            ('>', Some('>')) => Some(Shr),
            ('<', Some('=')) => Some(Le),
            ('>', Some('=')) => Some(Ge),
            ('=', Some('=')) => Some(EqEq),
            ('!', Some('=')) => Some(NotEq),
            ('+', Some('=')) => Some(PlusEq),
            ('-', Some('=')) => Some(MinusEq),
            ('*', Some('=')) => Some(StarEq),
            ('/', Some('=')) => Some(SlashEq),
            ('%', Some('=')) => Some(PercentEq),
            ('&', Some('=')) => Some(AmpEq),
            ('|', Some('=')) => Some(PipeEq),
            ('^', Some('=')) => Some(CaretEq),
            ('@', Some('=')) => Some(AtEq),
            ('-', Some('>')) => Some(Arrow),
            (':', Some('=')) => Some(Walrus),
            _ => None,
        };
        if let Some(op) = two {
            self.bump();
            self.bump();
            self.push(TokenKind::Op(op), line, col);
            return Ok(());
        }
        let one = match c0 {
            '+' => Some(Plus),
            '-' => Some(Minus),
            '*' => Some(Star),
            '/' => Some(Slash),
            '%' => Some(Percent),
            '@' => Some(At),
            '&' => Some(Amp),
            '|' => Some(Pipe),
            '^' => Some(Caret),
            '~' => Some(Tilde),
            '<' => Some(Lt),
            '>' => Some(Gt),
            '=' => Some(Assign),
            '(' => Some(LParen),
            ')' => Some(RParen),
            '[' => Some(LBracket),
            ']' => Some(RBracket),
            '{' => Some(LBrace),
            '}' => Some(RBrace),
            ',' => Some(Comma),
            ':' => Some(Colon),
            '.' => Some(Dot),
            ';' => Some(Semi),
            _ => None,
        };
        match one {
            Some(op) => {
                self.bump();
                match op {
                    LParen | LBracket | LBrace => self.depth += 1,
                    RParen | RBracket | RBrace => {
                        self.depth = self.depth.saturating_sub(1);
                    }
                    _ => {}
                }
                self.push(TokenKind::Op(op), line, col);
                Ok(())
            }
            None => self.err(format!("无法识别的字符 '{}'", c0)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<TokenKind> {
        Lexer::tokenize(src)
            .unwrap()
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    #[test]
    fn simple_numbers_and_names() {
        let k = kinds("a = 1 + 2.5\n");
        assert_eq!(
            k,
            vec![
                TokenKind::Name("a".into()),
                TokenKind::Op(Op::Assign),
                TokenKind::Int(1),
                TokenKind::Op(Op::Plus),
                TokenKind::Float(2.5),
                TokenKind::Newline,
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn indentation_tokens() {
        let k = kinds("if x:\n    y = 1\nz = 2\n");
        assert_eq!(
            k,
            vec![
                TokenKind::Kw(Kw::If),
                TokenKind::Name("x".into()),
                TokenKind::Op(Op::Colon),
                TokenKind::Newline,
                TokenKind::Indent,
                TokenKind::Name("y".into()),
                TokenKind::Op(Op::Assign),
                TokenKind::Int(1),
                TokenKind::Newline,
                TokenKind::Dedent,
                TokenKind::Name("z".into()),
                TokenKind::Op(Op::Assign),
                TokenKind::Int(2),
                TokenKind::Newline,
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn blank_lines_and_comments_are_skipped() {
        let k = kinds("# hi\n\n   \nx = 1  # trailing\n");
        assert_eq!(
            k,
            vec![
                TokenKind::Name("x".into()),
                TokenKind::Op(Op::Assign),
                TokenKind::Int(1),
                TokenKind::Newline,
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn implicit_line_continuation() {
        let k = kinds("x = [1,\n     2]\n");
        assert!(!k.contains(&TokenKind::Indent));
        assert_eq!(k.iter().filter(|t| **t == TokenKind::Newline).count(), 1);
    }

    #[test]
    fn strings_and_escapes() {
        let k = kinds("s = 'a\\nb' + \"c\"\n");
        assert!(k.contains(&TokenKind::Str("a\nb".into())));
        assert!(k.contains(&TokenKind::Str("c".into())));
    }

    #[test]
    fn radix_and_underscore_numbers() {
        assert_eq!(kinds("0xff\n")[0], TokenKind::Int(255));
        assert_eq!(kinds("0b1010\n")[0], TokenKind::Int(10));
        assert_eq!(kinds("1_000_000\n")[0], TokenKind::Int(1_000_000));
    }

    #[test]
    fn fstring_parts() {
        let k = kinds("f'{a}-{b!r:>3}'\n");
        match &k[0] {
            TokenKind::FStr(parts) => {
                assert_eq!(parts.len(), 3);
                assert_eq!(
                    parts[0],
                    FPart::Expr {
                        src: "a".into(),
                        conv: None,
                        spec: "".into()
                    }
                );
                assert_eq!(parts[1], FPart::Lit("-".into()));
                assert_eq!(
                    parts[2],
                    FPart::Expr {
                        src: "b".into(),
                        conv: Some('r'),
                        spec: ">3".into()
                    }
                );
            }
            other => panic!("期望 f-string，得到 {:?}", other),
        }
    }

    #[test]
    fn bad_dedent_is_an_error() {
        let e = Lexer::tokenize("if x:\n        y = 1\n    z = 2\n");
        assert!(e.is_err());
    }
}
