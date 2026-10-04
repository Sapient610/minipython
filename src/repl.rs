//! 交互式 REPL。

use crate::ast::StmtKind;
use crate::interp::Interp;
use crate::parser::Parser;
use std::io::{BufRead, Write};

/// 判断语法错误是否意味着「语句还没写完」（需要继续输入）。
fn is_incomplete(msg: &str) -> bool {
    msg.contains("期望一个缩进代码块")
        || msg.contains("文件结尾")
        || msg.contains("没有结束引号")
        || msg.contains("没有闭合")
        || msg.contains("表达式为空")
        // 表达式/括号在行尾被截断
        || msg.contains("但遇到 换行")
}

/// 当前缓冲区是否以「明显的续行符号」结尾（例如 `x = 1 +`）。
fn ends_with_continuation(src: &str) -> bool {
    let last = match src.lines().last() {
        Some(l) => l.trim_end(),
        None => return false,
    };
    if last.ends_with([
        '+', '-', '*', '/', '%', '&', '|', '^', '=', '<', '>', '!', ',', '\\',
    ]) || last.ends_with(['(', '[', '{'])
    {
        return true;
    }
    let words = ["and", "or", "not", "in", "is", "lambda", "return"];
    words.iter().any(|w| {
        last.strip_suffix(w)
            .map(|rest| {
                rest.is_empty() || !rest.ends_with(|c: char| c.is_alphanumeric() || c == '_')
            })
            .unwrap_or(false)
    })
}

pub fn run_repl(interp: &mut Interp) -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    println!("MiniPython 0.1.0 （输入 exit() 或 Ctrl-D 退出）");
    let mut buffer = String::new();
    loop {
        let prompt = if buffer.is_empty() { ">>> " } else { "... " };
        print!("{}", prompt);
        std::io::stdout().flush()?;
        let line = match lines.next() {
            Some(l) => l?,
            None => {
                println!();
                break;
            }
        };
        if buffer.is_empty() {
            if line.trim().is_empty() {
                continue;
            }
            buffer.push_str(&line);
        } else if ends_with_continuation(&buffer) {
            // `x = 1 +` 这类续行：用空格拼成一行
            buffer.push(' ');
            buffer.push_str(&line);
        } else {
            buffer.push('\n');
            buffer.push_str(&line);
        }
        // 仍在缩进块内部：继续等待（与 CPython 一样，用空行结束代码块）
        if buffer.contains('\n') {
            let last = buffer.lines().last().unwrap_or("");
            if last.starts_with(' ') || last.starts_with('\t') {
                continue;
            }
        }
        match Parser::parse_source(&buffer) {
            Ok(stmts) => {
                buffer.clear();
                if stmts.is_empty() {
                    continue;
                }
                let echo = matches!(stmts.last().map(|s| &s.kind), Some(StmtKind::Expr(_)));
                match interp.exec_repl_stmts(&stmts, echo) {
                    Ok(Some(v)) => {
                        let text = match interp.value_repr(&v) {
                            Ok(t) => t,
                            Err(sig) => format!("<无法显示: {:?}>", sig),
                        };
                        println!("{}", text);
                    }
                    Ok(None) => {}
                    Err(e) => {
                        if e.kind == "SystemExit" {
                            break;
                        }
                        println!("{}", interp.format_traceback(&e));
                    }
                }
            }
            Err(e) => {
                // 代码块或明显的续行：继续等待输入
                if is_incomplete(&e.msg) || ends_with_continuation(&buffer) {
                    continue;
                }
                buffer.clear();
                println!("  File \"<stdin>\", line {}", e.line);
                println!("SyntaxError: {}", e.msg);
            }
        }
    }
    Ok(())
}
