//! 命令行入口。

use minipython::interp::Interp;
use minipython::repl::run_repl;
use std::io::Write;

const VERSION: &str = "MiniPython 0.1.0";

fn usage() -> String {
    format!(
        "{version}
用法：
  minipython [选项] [脚本.py] [脚本参数...]

选项：
  -c CODE        执行一段代码
  -i             执行完脚本后进入交互模式
  -h, --help     显示帮助
  -V, --version  显示版本

示例：
  minipython examples/fizzbuzz.py
  minipython -c \"print(sum(range(101)))\"
  minipython            # 进入交互式 REPL
",
        version = VERSION
    )
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // 使用更大的栈，支持较深的递归调用
    let code = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || real_main(args))
        .expect("无法创建主线程")
        .join()
        .unwrap_or(1);
    std::process::exit(code);
}

fn real_main(args: Vec<String>) -> i32 {
    let mut code: Option<String> = None;
    let mut script: Option<String> = None;
    let mut script_args: Vec<String> = Vec::new();
    let mut interactive = false;
    let mut i = 1usize;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "-h" | "--help" => {
                print!("{}", usage());
                return 0;
            }
            "-V" | "--version" => {
                println!("{}", VERSION);
                return 0;
            }
            "-i" => {
                interactive = true;
                i += 1;
            }
            "-c" => {
                if i + 1 >= args.len() {
                    eprintln!("minipython: -c 需要一个参数");
                    return 2;
                }
                code = Some(args[i + 1].clone());
                script_args = args[i + 2..].to_vec();
                i = args.len();
            }
            "--" => {
                if i + 1 < args.len() {
                    script = Some(args[i + 1].clone());
                    script_args = args[i + 2..].to_vec();
                }
                i = args.len();
            }
            _ if a.starts_with('-') && a.len() > 1 => {
                eprintln!("minipython: 未知选项 '{}'", a);
                eprint!("{}", usage());
                return 2;
            }
            _ => {
                script = Some(a.clone());
                script_args = args[i + 1..].to_vec();
                i = args.len();
            }
        }
    }

    let mut interp = Interp::new();
    // 主线程有 64MB 栈，允许较深的递归
    interp.stack_budget = 48 * 1024 * 1024;
    match (&code, &script) {
        (Some(_), _) => {
            let mut argv = vec!["-c".to_string()];
            argv.extend(script_args.iter().cloned());
            interp.argv = argv;
            interp.search_path.insert(0, std::path::PathBuf::from("."));
        }
        (None, Some(s)) => {
            let mut argv = vec![s.clone()];
            argv.extend(script_args.iter().cloned());
            interp.argv = argv;
            if let Some(dir) = std::path::Path::new(s).parent() {
                let d = if dir.as_os_str().is_empty() {
                    std::path::PathBuf::from(".")
                } else {
                    dir.to_path_buf()
                };
                interp.search_path.insert(0, d);
            }
        }
        (None, None) => {
            interp.argv = vec!["".to_string()];
        }
    }

    let mut status = 0;
    if let Some(c) = &code {
        if let Err(e) = interp.run_source(c, "<string>") {
            status = report(&interp, &e);
        }
    } else if let Some(s) = &script {
        if let Err(e) = interp.run_file(s) {
            status = report(&interp, &e);
        }
    }

    if code.is_none() && script.is_none() || (interactive && status == 0) {
        if let Err(e) = run_repl(&mut interp) {
            eprintln!("minipython: {}", e);
            return 1;
        }
    }
    status
}

/// 打印异常（traceback）并返回进程退出码。
fn report(interp: &Interp, e: &minipython::PyError) -> i32 {
    if e.kind == "SystemExit" {
        return match &e.value {
            Some(minipython::Value::Int(n)) => (n & 0xff) as i32,
            Some(minipython::Value::Instance(inst)) => match inst.get("args") {
                Some(minipython::Value::Tuple(args)) => match args.first() {
                    Some(minipython::Value::Int(n)) => (n & 0xff) as i32,
                    Some(minipython::Value::Str(msg)) => {
                        let mut err = std::io::stderr();
                        let _ = writeln!(err, "{}", msg);
                        1
                    }
                    _ => 0,
                },
                _ => 0,
            },
            _ => 0,
        };
    }
    let text = interp.format_traceback(e);
    let mut err = std::io::stderr();
    let _ = writeln!(err, "{}", text);
    1
}
