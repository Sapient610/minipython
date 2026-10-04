//! MiniPython：一个用 Rust 从零实现的 Python 子集解释器。
//!
//! 快速开始：
//! ```
//! use minipython::Session;
//!
//! let mut s = Session::new();
//! s.run_source("print('hi', 1 + 2)", "<test>").unwrap();
//! assert_eq!(s.output(), "hi 3\n");
//! ```
//!
//! 说明：`Session` 会在一个拥有大栈（64MB）的工作线程中运行解释器，
//! 因此递归调用可以比较深；如果直接使用 [`Interp`]，
//! 请注意所在线程的栈大小（可通过 [`Interp::stack_budget`] 调整预算）。

pub mod ast;
pub mod builtins;
pub mod env;
pub mod error;
pub mod interp;
pub mod lexer;
pub mod methods;
pub mod modules;
pub mod ops;
pub mod parser;
pub mod repl;
pub mod value;

pub use error::PyError;
pub use interp::{Interp, Signal};
pub use value::Value;

use std::io::Write;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// 工作线程的栈大小。
const WORKER_STACK: usize = 64 * 1024 * 1024;
/// 工作线程内允许解释器递归消耗的栈预算。
const WORKER_STACK_BUDGET: usize = 32 * 1024 * 1024;

enum Cmd {
    Source(String, String),
    File(String),
    Quit,
}

struct Reply {
    error: Option<(String, String, String)>, // (kind, message, 完整 traceback)
}

/// 把解释器输出写入共享缓冲区的 writer。
struct SharedWriter(Arc<Mutex<Vec<u8>>>);

impl Write for SharedWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// 一次解释器会话：自动捕获标准输出，便于测试与嵌入。
///
/// 会话内部使用独立的大栈线程，因此同名变量、导入的模块等状态在多次
/// `run_*` 调用之间保持。
pub struct Session {
    tx: Sender<Cmd>,
    rx: Receiver<Reply>,
    handle: Option<JoinHandle<()>>,
    buf: Arc<Mutex<Vec<u8>>>,
    last_traceback: String,
}

impl Default for Session {
    fn default() -> Self {
        Session::new()
    }
}

impl Session {
    pub fn new() -> Session {
        let buf = Arc::new(Mutex::new(Vec::new()));
        let (tx, cmd_rx) = channel::<Cmd>();
        let (reply_tx, rx) = channel::<Reply>();
        let worker_buf = buf.clone();
        let handle = std::thread::Builder::new()
            .stack_size(WORKER_STACK)
            .spawn(move || {
                let mut interp = Interp::new()
                    .with_writer(Box::new(SharedWriter(worker_buf)))
                    .with_stack_budget(WORKER_STACK_BUDGET);
                while let Ok(cmd) = cmd_rx.recv() {
                    let result = match cmd {
                        Cmd::Source(src, filename) => interp.run_source(&src, &filename),
                        Cmd::File(path) => interp.run_file(&path),
                        Cmd::Quit => break,
                    };
                    let error = match result {
                        Ok(()) => None,
                        Err(e) => Some((
                            e.kind.clone(),
                            e.message.clone(),
                            interp.format_traceback(&e),
                        )),
                    };
                    if reply_tx.send(Reply { error }).is_err() {
                        break;
                    }
                }
            })
            .expect("无法创建解释器工作线程");
        Session {
            tx,
            rx,
            handle: Some(handle),
            buf,
            last_traceback: String::new(),
        }
    }

    fn request(&mut self, cmd: Cmd) -> Result<(), PyError> {
        self.last_traceback.clear();
        if self.tx.send(cmd).is_err() {
            return Err(PyError::new("RuntimeError", "解释器工作线程已退出"));
        }
        match self.rx.recv() {
            Ok(reply) => match reply.error {
                None => Ok(()),
                Some((kind, message, tb)) => {
                    self.last_traceback = tb;
                    Err(PyError::new(kind, message))
                }
            },
            Err(_) => Err(PyError::new("RuntimeError", "解释器工作线程已退出")),
        }
    }

    /// 执行一段源码。
    pub fn run_source(&mut self, src: &str, filename: &str) -> Result<(), PyError> {
        self.request(Cmd::Source(src.to_string(), filename.to_string()))
    }

    /// 执行一个脚本文件。
    pub fn run_file(&mut self, path: &str) -> Result<(), PyError> {
        self.request(Cmd::File(path.to_string()))
    }

    /// 已产生的标准输出。
    pub fn output(&self) -> String {
        let guard = self.buf.lock().unwrap();
        String::from_utf8_lossy(&guard).to_string()
    }

    /// 清空已捕获的输出。
    pub fn clear_output(&self) {
        self.buf.lock().unwrap().clear();
    }

    /// 最近一次异常的完整 traceback。
    ///
    /// `Session` 跨线程返回的 [`PyError`] 只保留类型与消息，
    /// 完整的 traceback 由本方法提供。
    pub fn traceback(&self, e: &PyError) -> String {
        if self.last_traceback.is_empty() {
            e.summary()
        } else {
            self.last_traceback.clone()
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Quit);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// 便捷函数：执行源码并返回标准输出；出错时返回异常。
pub fn run_capture(src: &str) -> Result<String, PyError> {
    let mut s = Session::new();
    s.run_source(src, "<string>")?;
    Ok(s.output())
}

/// 便捷函数：执行源码，返回 (标准输出, 完整 traceback)。
pub fn run_or_error(src: &str) -> (String, Option<String>) {
    let mut s = Session::new();
    match s.run_source(src, "<string>") {
        Ok(()) => (s.output(), None),
        Err(e) => {
            let tb = s.traceback(&e);
            (s.output(), Some(tb))
        }
    }
}
