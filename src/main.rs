//! psman — 进程/端口/线程管理 CLI
//!
//! 三种使用方式：
//! 1. 单次执行：`psman <命令> [参数]`（如 `psman ports 9222`、`psman kill 1234`）
//! 2. 交互模式：直接运行 `psman`，进入 REPL，持续输入命令
//! 3. 双击启动：进入托盘常驻模式，托盘左键呼出/隐藏终端，右键菜单退出
//!
//! 模块划分：
//! - cmd.rs    命令表（解析/分发，交互与单次执行共用）
//! - proc.rs   进程数据层（sysinfo 枚举 + ToolHelp 线程 + TerminateProcess）
//! - net.rs    端口表（IP Helper 原生 API，不调 netstat）
//! - table.rs  表格渲染（CJK 等宽对齐）
//! - log.rs    颜色/时间戳
//! - console.rs 控制台生命周期（继承/分配/显隐/关闭处理）
//! - tray.rs   系统托盘（隐藏窗口 + 消息循环 + 看门狗）
//! - repl.rs   REPL 交互循环
//! - single.rs 单例控制（互斥体 + 通知旧实例重建托盘）

#![windows_subsystem = "windows"]

mod cmd;
mod console;
mod log;
mod repl;
mod single;
mod table;
mod tray;

use std::thread;
use std::time::Duration;

use crate::log::{c, CLR_HEAD, CLR_WARN};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // ── 模式 1：单次执行（带参数） ──
    if !args.is_empty() {
        // 继承父控制台（从 cmd/powershell 启动）；双击启动无父控制台则自行分配
        if !console::attach_parent() {
            console::alloc();
        }
        // 控制台就绪后再启用 ANSI 颜色，否则 VT 模式设置不到句柄上（转义码原样显示）
        color::enable_ansi();
        let code = cmd::run_one(&args);
        std::process::exit(code);
    }

    // ── 模式 2/3：交互 REPL + 托盘常驻（无参数） ──
    if !single::ensure_single_instance() {
        std::process::exit(0);
    }
    console::alloc();
    console::set_close_handler();
    // 控制台就绪后再启用 ANSI 颜色
    color::enable_ansi();

    tray::spawn_tray();
    tray::spawn_tray_watchdog();

    println!();
    println!("{} psman v{} — 进程/端口/线程管理", c("==>", CLR_HEAD), env!("CARGO_PKG_VERSION"));
    println!("  {} 裸输入数字/通配符查端口，字符串查进程；{} 退出", c("提示:", CLR_WARN), c("exit", CLR_HEAD));

    // 主循环：REPL 因 exit 退出 → 进程结束；
    // 因控制台被关闭（点击 X）退出 → 保持托盘常驻，等托盘重新打开控制台后重启 REPL
    loop {
        let quit = repl::run();
        if quit {
            console::hide_and_free();
            break;
        }
        while !console::console_visible() {
            thread::sleep(Duration::from_millis(200));
        }
    }
}
