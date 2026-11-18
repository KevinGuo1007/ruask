//! 等待模型首行时在 stderr 显示的一行提示（仅 stderr 是终端时）。
//! 120 ms 内就有回答时不显示，避免闪烁；结束时清除该行，不在终端里留下痕迹。

use std::io::{IsTerminal, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

pub struct Waiting {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Waiting {
    pub fn start(label: String) -> Option<Self> {
        if !std::io::stderr().is_terminal() {
            return None;
        }
        let dim = std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty());
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let handle = thread::spawn(move || {
            thread::park_timeout(Duration::from_millis(120));
            let mut drawn = false;
            let mut frame = 0;
            while !flag.load(Ordering::Acquire) {
                let text = if dim {
                    format!("\r\x1b[2m{} {label}\x1b[0m", FRAMES[frame % FRAMES.len()])
                } else {
                    format!("\r{} {label}", FRAMES[frame % FRAMES.len()])
                };
                // 诊断输出失败不影响查询本身。
                let _ = std::io::stderr().lock().write_all(text.as_bytes());
                drawn = true;
                frame += 1;
                thread::park_timeout(Duration::from_millis(80));
            }
            if drawn {
                let _ = std::io::stderr().lock().write_all(b"\r\x1b[K");
            }
        });
        Some(Self {
            stop,
            handle: Some(handle),
        })
    }
}

impl Drop for Waiting {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            handle.thread().unpark();
            let _ = handle.join();
        }
    }
}
