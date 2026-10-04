//! 如果本机有 CPython，则把示例程序同时交给 CPython 与 MiniPython 执行并逐字节比较。
//!
//! 没有 python 可执行文件时自动跳过（不会让 CI 失败）。

use minipython::Session;
use std::path::{Path, PathBuf};
use std::process::Command;

fn python_command() -> Option<String> {
    for candidate in ["python", "python3", "py"] {
        if let Ok(out) = Command::new(candidate).arg("--version").output() {
            if out.status.success() {
                return Some(candidate.to_string());
            }
        }
    }
    None
}

fn examples() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("examples 目录必须存在")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|e| e == "py").unwrap_or(false))
        .collect();
    files.sort();
    files
}

#[test]
fn output_matches_cpython() {
    let python = match python_command() {
        Some(p) => p,
        None => {
            eprintln!("跳过：本机没有找到 CPython");
            return;
        }
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();
    for py in examples() {
        let stem = py.file_stem().unwrap().to_string_lossy().to_string();
        let rel = format!("examples/{}.py", stem);
        let out = Command::new(&python)
            .arg("-W")
            .arg("ignore")
            .arg(&rel)
            .current_dir(root)
            .env("PYTHONIOENCODING", "utf-8")
            .env("PYTHONUTF8", "1")
            .output()
            .expect("运行 CPython 失败");
        let expected = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");

        let mut session = Session::new();
        session.run_file(&rel).expect("MiniPython 执行失败");
        let actual = session.output().replace("\r\n", "\n");

        if expected != actual {
            failures.push(format!(
                "{} 与 CPython 输出不同\n--- CPython ---\n{}\n--- MiniPython ---\n{}",
                stem, expected, actual
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
