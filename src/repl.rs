//! REPL 交互循环 — 命令分发 + 方向键历史记录 + 行内编辑
//!
//! 输入不依赖 Rust `std::io::stdin()`：GUI 子系统进程（`windows_subsystem="windows"`）
//! 在双击 / IDE 终端启动时标准输入句柄无效，`stdin()` 会拿到读立即返回 0 字节（EOF）
//! 的句柄，`read_line` 立刻"结束"，导致主循环误判控制台关闭而无限重启 REPL（提示符刷屏）。
//! 因此这里用 `ReadConsoleInputW` 从 CONIN$ 逐键读取，任何启动方式下都可靠；
//! 同时自行实现行编辑（退格、插入）与 ↑/↓ 历史翻阅。
//! 控制台被关闭（点击 X / FreeConsole）时句柄失效 → 返回 Err → 退出循环。

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use windows::Win32::System::Console::{
    GetConsoleMode, GetStdHandle, ReadConsoleInputW, SetConsoleMode, CONSOLE_MODE,
    ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, EVENT_TYPE, INPUT_RECORD, STD_INPUT_HANDLE,
};

use crate::cmd::{self, Flow};
use crate::log::{c, CLR_DIM, CLR_ERR, CLR_HEAD};

/// REPL 是否活跃。控制台被关闭时置 false。
pub static REPL_ACTIVE: AtomicBool = AtomicBool::new(false);

static HISTORY: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn push_history(line: String) {
    if let Ok(mut h) = HISTORY.lock() {
        // 连续重复输入不重复记录
        if h.last().map(|l| l == &line).unwrap_or(false) {
            return;
        }
        h.push(line);
        // 历史上限，防止常驻进程内存无限增长
        if h.len() > 500 {
            h.remove(0);
        }
    }
}

fn history_len() -> usize {
    HISTORY.lock().map(|h| h.len()).unwrap_or(0)
}

fn history_get(i: usize) -> Option<String> {
    HISTORY.lock().ok()?.get(i).cloned()
}

// ── 虚拟键码 ──
const VK_BACK: u16 = 0x08;
const VK_RETURN: u16 = 0x0D;
const VK_ESCAPE: u16 = 0x1B;
const VK_UP: u16 = 0x26;
const VK_DOWN: u16 = 0x28;

/// 单个按键事件（仅按下时有效的事件会被处理）
struct KeyEvent {
    vk: u16,
    ch: u16,
}

/// 读一个按键事件；控制台句柄失效（已关闭）时返回 Err。
fn next_key(h: windows::Win32::Foundation::HANDLE) -> Result<KeyEvent, String> {
    unsafe {
        let mut recs = [INPUT_RECORD::default(); 1];
        let mut read: u32 = 0;
        loop {
            ReadConsoleInputW(h, recs.as_mut_ptr(), 1, &mut read)
                .map_err(|_| "控制台输入读取失败（控制台可能已关闭）".to_string())?;
            if read == 0 {
                return Err("控制台输入流结束".to_string());
            }
            let rec = &recs[0];
            if rec.EventType == KEY_EVENT {
                let kev = rec.Event.KeyEvent();
                if kev.bKeyDown.as_bool() {
                    return Ok(KeyEvent {
                        vk: kev.wVirtualKeyCode,
                        ch: kev.uChar.UnicodeChar as u16,
                    });
                }
            }
            // 其他事件（鼠标/窗口尺寸变化/松键）忽略，继续读
        }
    }
}

/// 行编辑器状态（内容按 UTF-16 码元存储，正确支持中文/emoji 代理对）
struct LineEditor {
    buf: Vec<u16>,
    /// 当前浏览的历史条目下标；None = 正在输入草稿
    hist_idx: Option<usize>,
    /// 切换到历史前暂存的草稿
    draft: String,
    prompt: String,
}

impl LineEditor {
    fn new(prompt: &str) -> Self {
        Self {
            buf: Vec::new(),
            hist_idx: None,
            draft: String::new(),
            prompt: prompt.to_string(),
        }
    }

    fn text(&self) -> String {
        String::from_utf16_lossy(&self.buf)
    }

    /// 重绘当前行：光标回到行首、清行、重打印提示符与内容
    fn redraw(&self) {
        print!("{}\x1b[K{}", self.prompt, self.text());
        let _ = std::io::stdout().flush();
    }

    /// 删除光标前一个字符（UTF-16 码元成对删除，代理对不劈开）
    fn backspace(&mut self) -> bool {
        if self.buf.pop().is_none() {
            return false;
        }
        // 若删完剩高代理项开头，把另一半也删掉
        if let Some(&last) = self.buf.last() {
            if (0xD800..0xDC00).contains(&last) {
                self.buf.pop();
            }
        }
        true
    }

    fn insert(&mut self, ch: u16) {
        self.buf.push(ch);
    }

    fn set_text(&mut self, s: &str) {
        self.buf = s.encode_utf16().collect();
    }

    /// ↑：向上翻历史
    fn history_prev(&mut self) {
        let len = history_len();
        if len == 0 {
            return;
        }
        match self.hist_idx {
            None => {
                self.draft = self.text();
                self.hist_idx = Some(len - 1);
            }
            Some(0) => return,
            Some(i) => self.hist_idx = Some(i - 1),
        }
        let entry = history_get(self.hist_idx.unwrap()).unwrap_or_default();
        self.set_text(&entry);
    }

    /// ↓：向下翻历史；越过最新一条后恢复草稿
    fn history_next(&mut self) {
        let Some(i) = self.hist_idx else { return };
        let len = history_len();
        if i + 1 >= len {
            self.hist_idx = None;
            let draft = std::mem::take(&mut self.draft);
            self.set_text(&draft);
        } else {
            self.hist_idx = Some(i + 1);
            let entry = history_get(i + 1).unwrap_or_default();
            self.set_text(&entry);
        }
    }
}

/// 从控制台读一行（阻塞直到按回车），支持退格编辑与 ↑/↓ 历史翻阅。
/// 控制台句柄失效（已关闭）时返回 Err。空行返回空字符串。
fn read_line_console(prompt: &str) -> Result<String, String> {
    unsafe {
        let h = GetStdHandle(STD_INPUT_HANDLE)
            .map_err(|_| "无法获取控制台输入句柄".to_string())?;
        // 关闭行输入/回显（自行编辑渲染）；保留 PROCESSED_INPUT 使 Ctrl+C 维持系统默认行为
        let mut old_mode = CONSOLE_MODE(0);
        GetConsoleMode(h, &mut old_mode)
            .map_err(|_| "无法获取控制台输入模式".to_string())?;
        let raw = CONSOLE_MODE(old_mode.0 & !(ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT));
        if SetConsoleMode(h, raw).is_err() {
            return Err("无法设置控制台输入模式".to_string());
        }

        let result = read_line_loop(h, prompt);

        // 无论成功失败都恢复原模式
        let _ = SetConsoleMode(h, old_mode);
        result
    }
}

fn read_line_loop(
    h: windows::Win32::Foundation::HANDLE,
    prompt: &str,
) -> Result<String, String> {
    let mut ed = LineEditor::new(prompt);
    loop {
        match next_key(h) {
            Err(e) => return Err(e),
            Ok(key) => match key.vk {
                VK_RETURN => {
                    println!();
                    let _ = std::io::stdout().flush();
                    return Ok(ed.text());
                }
                VK_BACK => {
                    if ed.backspace() {
                        ed.redraw();
                    }
                }
                VK_UP => {
                    ed.history_prev();
                    ed.redraw();
                }
                VK_DOWN => {
                    ed.history_next();
                    ed.redraw();
                }
                VK_ESCAPE => {
                    // Esc 清空当前输入
                    if !ed.buf.is_empty() {
                        ed.buf.clear();
                        ed.redraw();
                    }
                }
                _ => {
                    // 可打印字符（含中文等多字节 UTF-16 码元）逐个插入，
                    // 代理对前后码元按序插入，text() 组合还原
                    if key.ch >= 0x20 {
                        ed.insert(key.ch);
                        ed.redraw();
                    }
                }
            },
        }
    }
}

/// 主 REPL 循环。返回 true = 用户输入 exit 主动退出；false = 控制台被关闭。
pub fn run() -> bool {
    REPL_ACTIVE.store(true, Ordering::Relaxed);
    let prompt = format!("{} ", c("psman>", CLR_HEAD));
    loop {
        if !REPL_ACTIVE.load(Ordering::Relaxed) {
            return false;
        }

        let line = match read_line_console(&prompt) {
            Ok(l) => l,
            Err(_) => {
                // 控制台句柄失效（点击 X 关闭 / FreeConsole）→ 退出循环
                REPL_ACTIVE.store(false, Ordering::Relaxed);
                return false;
            }
        };

        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        push_history(line.to_string());

        let tokens: Vec<&str> = line.split_whitespace().collect();
        match cmd::dispatch(&tokens) {
            Ok(Flow::Quit) => {
                println!("{}", c("bye！", CLR_DIM));
                REPL_ACTIVE.store(false, Ordering::Relaxed);
                return true;
            }
            Ok(Flow::Continue) => {}
            Err(e) => eprintln!("{}", c(&format!("错误: {e}"), CLR_ERR)),
        }
        // 命令执行完毕（含出错）后与下一个提示符之间空一行
        println!();
    }
}
