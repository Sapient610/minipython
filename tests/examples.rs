//! 每个示例程序都必须与随附的期望输出（由 CPython 3.13 生成）完全一致。

use minipython::Session;
use std::path::{Path, PathBuf};

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples")
}

fn example_files() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(examples_dir())
        .expect("examples 目录必须存在")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|e| e == "py").unwrap_or(false))
        .collect();
    files.sort();
    files
}

#[test]
fn all_examples_match_expected_output() {
    let files = example_files();
    assert!(files.len() >= 8, "示例数量过少: {}", files.len());
    let mut failures = Vec::new();
    for py in &files {
        let stem = py.file_stem().unwrap().to_string_lossy().to_string();
        let expected_path = py.with_extension("out");
        if !expected_path.exists() {
            failures.push(format!("{} 缺少期望输出文件", stem));
            continue;
        }
        let expected = std::fs::read_to_string(&expected_path)
            .expect("读取期望输出失败")
            .replace("\r\n", "\n");
        let mut session = Session::new();
        let rel = format!("examples/{}.py", stem);
        match session.run_file(&rel) {
            Ok(()) => {}
            Err(e) => {
                failures.push(format!("{} 执行出错: {}", stem, session.traceback(&e)));
                continue;
            }
        }
        let actual = session.output();
        if actual != expected {
            failures.push(format!(
                "{} 输出不一致\n--- 期望 ---\n{}\n--- 实际 ---\n{}",
                stem, expected, actual
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn examples_are_deterministic() {
    // 同一个脚本跑两次结果必须一致（示例里使用了固定随机种子）
    for py in &example_files() {
        let stem = py.file_stem().unwrap().to_string_lossy().to_string();
        let rel = format!("examples/{}.py", stem);
        let mut a = Session::new();
        a.run_file(&rel).expect("第一次执行失败");
        let mut b = Session::new();
        b.run_file(&rel).expect("第二次执行失败");
        assert_eq!(a.output(), b.output(), "{} 两次运行结果不同", stem);
    }
}
