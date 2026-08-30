//! 命令表 — 交互 REPL 与单次执行共用同一套解析/分发
//!
//! 裸输入智能解析（无命令名前缀）：
//!   - 模式只含数字/通配符且至少含一个数字 → 端口占用查询（3306、*3306、513*、3.06）
//!   - 其余字符串 → 进程名（exe 名称）模糊搜索（python、py*、*ava）
//! 通配符：`*` = 0 或多个任意字符；`.` = 1 个任意字符

use std::collections::HashMap;

use color::{format_size, pad_left, Alignment, DisplayWidth};

use crate::log::{c, status_text, CLR_ACCENT, CLR_DIM, CLR_ERR, CLR_HEAD, CLR_OK, CLR_WARN};
use crate::table;
use psman::{net, proc};

pub enum Flow {
    Continue,
    Quit,
}

// ── 分发入口 ─────────────────────────────────────────

/// 解析并执行一条命令。args[0] 为命令名（或裸输入模式的第一个 token）。
pub fn dispatch(args: &[&str]) -> Result<Flow, String> {
    if args.is_empty() {
        return Ok(Flow::Continue);
    }
    // 以 `!` 开头的命令（!kill <pid...> 等）
    if let Some(rest) = args[0].strip_prefix('!') {
        return cmd_bang(rest, &args[1..]);
    }
    match args[0] {
        "exit" | "quit" | "q" => Ok(Flow::Quit),
        _ => {
            // 无命令名前缀：整行（保留空格）作为搜索模式
            let pattern = args.join(" ");
            if is_port_pattern(&pattern) {
                cmd_port_query(&pattern)
            } else {
                cmd_proc_query(&pattern)
            }
        }
    }
}

// ── ! 命令 ──────────────────────────────────────────

fn cmd_bang(cmd: &str, rest: &[&str]) -> Result<Flow, String> {
    match cmd {
        "" => {
            println!("{} ! 命令", c("==>", CLR_HEAD));
            println!("  {} <pid...> 结束一个或多个进程", c("!kill", CLR_ACCENT));
            Ok(Flow::Continue)
        }
        "kill" => cmd_kill(rest),
        other => Err(format!("未知命令: !{other}")),
    }
}

fn cmd_kill(pids: &[&str]) -> Result<Flow, String> {
    if pids.is_empty() {
        println!("用法: {} <pid...>", c("!kill", CLR_ACCENT));
        return Ok(Flow::Continue);
    }
    let mut ids: Vec<u32> = Vec::new();
    for p in pids {
        match p.parse::<u32>() {
            Ok(v) => ids.push(v),
            Err(_) => return Err(format!("无效的 PID: {p}")),
        }
    }
    let snap = proc::Snapshot::new();
    let mut ok = 0;
    for pid in ids {
        let name = snap
            .get(pid)
            .map(proc::display_name)
            .unwrap_or_else(|| "未知进程".into());
        if proc::kill_pid(pid) {
            println!("{} {} ({}) 已终止", c("✓", CLR_OK), pid, name);
            ok += 1;
        } else {
            println!(
                "{} {} ({}) 终止失败（无权限或已退出）",
                c("✗", CLR_ERR),
                pid,
                name
            );
        }
    }
    if ok > 0 {
        println!("{} 共终止 {} 个进程", c("done", CLR_OK), ok);
    }
    Ok(Flow::Continue)
}

// ── 裸输入分类与查询 ─────────────────────────────────

/// 是否为端口模式：只含数字与通配符，且至少含一个数字
fn is_port_pattern(s: &str) -> bool {
    let mut has_digit = false;
    for ch in s.chars() {
        match ch {
            '0'..='9' => has_digit = true,
            '*' | '.' => {}
            _ => return false,
        }
    }
    has_digit
}

/// 通配符匹配：`*` = 0 或多个任意字符；`.` = 1 个任意字符。
/// 按 char 匹配（而非字节），`.` 不会劈开多字节字符（如中文进程名）。
fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut mark) = (usize::MAX, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '.' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = pi;
            mark = ti;
            pi += 1;
        } else if star != usize::MAX {
            pi = star + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// 进程名匹配：模式完整匹配 exe 名（含扩展名）或其去扩展名形式。
/// 这样 `py*` 命中 python.exe，`*ava` 也能命中 java.exe。
fn name_matches(pattern: &str, name: &str) -> bool {
    let lower = name.to_lowercase();
    if wildcard_match(pattern, &lower) {
        return true;
    }
    if let Some(stem) = std::path::Path::new(name).file_stem() {
        let stem = stem.to_string_lossy().to_lowercase();
        if !stem.is_empty() && stem != lower && wildcard_match(pattern, &stem) {
            return true;
        }
    }
    false
}

/// 端口占用查询：端口号字符串匹配模式
fn cmd_port_query(pattern: &str) -> Result<Flow, String> {
    let snap = proc::Snapshot::new();
    let names = snap.pid_to_name();
    let pat = pattern.to_lowercase();

    let mut rows: Vec<net::PortRow> = net::all_rows()
        .into_iter()
        .filter(|r| wildcard_match(&pat, &r.port.to_string()))
        .collect();
    if rows.is_empty() {
        println!(
            "{} 没有匹配 {} 的端口占用",
            c("!", CLR_WARN),
            c(pattern, CLR_DIM)
        );
        return Ok(Flow::Continue);
    }
    rows.sort_by_key(|r| r.port);

    let rendered: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            vec![
                r.port.to_string(),
                r.proto.to_string(),
                tcp_state_text(r.state).into(),
                r.pid.to_string(),
                names.get(&r.pid).cloned().unwrap_or_else(|| "-".into()),
            ]
        })
        .collect();
    table::table(
        &["端口", "协议", "状态", "PID", "进程"],
        &rendered,
        &[
            Alignment::Right, // 端口
            Alignment::Left,  // 协议
            Alignment::Left,  // 状态
            Alignment::Right, // PID
            Alignment::Left,  // 进程
        ],
    );
    Ok(Flow::Continue)
}

/// MIB_TCP_STATE → 中文（UDP 无状态显示 -）
fn tcp_state_text(s: u8) -> &'static str {
    match s {
        1 => "关闭",
        2 => "监听",
        3 => "SYN_SENT",
        4 => "SYN_RCVD",
        5 => "已建立",
        6 => "FIN_WAIT1",
        7 => "FIN_WAIT2",
        8 => "等待关闭",
        9 => "CLOSING",
        10 => "LAST_ACK",
        11 => "等待关闭",
        12 => "删除TCB",
        _ => "-",
    }
}

/// 进程命中的完整信息（详情布局与表格共用）
struct ProcHit {
    pid: u32,
    name: String,
    path: String,
    cmd: String,
    mem: u64,
    threads: usize,
    status: String,
    ports: String,
    start: u64,
}

/// 进程名模糊搜索：exe 名称（大小写不敏感）匹配模式
fn cmd_proc_query(pattern: &str) -> Result<Flow, String> {
    let snap = proc::Snapshot::new();
    let pat = pattern.to_lowercase();
    let thread_counts = proc::thread_count_map();

    // pid → 占用的端口列表（排序、去重、带协议）
    let mut port_map: HashMap<u32, Vec<(u16, String)>> = HashMap::new();
    for r in net::all_rows() {
        port_map.entry(r.pid).or_default().push((r.port, r.proto.to_string()));
    }
    let port_text = |pid: u32| -> String {
        let mut v = port_map.get(&pid).cloned().unwrap_or_default();
        v.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        v.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
        v.iter()
            .map(|(p, proto)| {
                let pn = if proto.starts_with("TCP") { "tcp" } else { "udp" };
                format!("{}/{}", p, pn)
            })
            .collect::<Vec<_>>()
            .join(" ")
    };

    let mut hits: Vec<ProcHit> = Vec::new();
    for (_, pr) in snap.sys.processes() {
        let name = proc::display_name(pr);
        if !name_matches(&pat, &name) {
            continue;
        }
        let pid = pr.pid().as_u32();
        let path = proc::exe_path(pid).unwrap_or_default();
        let cmd = proc::cmdline_of(pid).unwrap_or_default();
        hits.push(ProcHit {
            pid,
            name,
            path,
            cmd,
            mem: pr.memory(),
            threads: thread_counts.get(&pid).copied().unwrap_or(0),
            status: status_text(&pr.status()),
            ports: port_text(pid),
            start: pr.start_time(),
        });
    }
    if hits.is_empty() {
        println!(
            "{} 没有名称匹配 {} 的进程",
            c("!", CLR_WARN),
            c(pattern, CLR_DIM)
        );
        return Ok(Flow::Continue);
    }
    hits.sort_by(|a, b| a.name.cmp(&b.name));

    // 智能布局：命中 ≤3 个（详情模式，含路径/启动命令）或端口列表超长 → 左右布局
    let use_detail = hits.len() <= 3 || hits.iter().any(|h| h.ports.display_width() > 40);
    if use_detail {
        for (i, h) in hits.iter().enumerate() {
            if i > 0 {
                println!();
            }
            render_field("PID", &h.pid.to_string());
            render_field("名称", &h.name);
            render_field("路径", if h.path.is_empty() { "-" } else { &h.path });
            render_field("命令", if h.cmd.is_empty() { "-" } else { &h.cmd });
            render_field("内存", &format_size(h.mem));
            render_field("线程", &h.threads.to_string());
            render_field("状态", &h.status);
            render_field("端口", if h.ports.is_empty() { "-" } else { &h.ports });
            render_field("启动时间", &crate::log::fmt_unix(h.start));
        }
        return Ok(Flow::Continue);
    }

    let rendered: Vec<Vec<String>> = hits
        .iter()
        .map(|h| {
            vec![
                h.pid.to_string(),
                h.name.clone(),
                format_size(h.mem),
                h.threads.to_string(),
                h.status.clone(),
                if h.ports.is_empty() { "-".into() } else { h.ports.clone() },
                crate::log::fmt_unix(h.start),
            ]
        })
        .collect();
    table::table(
        &["PID", "名称", "内存", "线程", "状态", "端口", "启动时间"],
        &rendered,
        &[
            Alignment::Right, // PID
            Alignment::Left,  // 名称
            Alignment::Right, // 内存
            Alignment::Right, // 线程
            Alignment::Left,  // 状态
            Alignment::Left,  // 端口
            Alignment::Left,  // 启动时间
        ],
    );
    Ok(Flow::Continue)
}

// ── 左右布局（端口列表过长时的详情展示） ──────────────

/// 字段名显示列宽；值折行宽度 = 控制台实际宽度 - 字段名列宽（下限 40）
const FIELD_W: usize = 10;

/// 渲染单个字段：`字段名  值`；值超长时按空格折行并对齐缩进
fn render_field(label: &str, value: &str) {
    let line_w = crate::console::console_width()
        .saturating_sub(FIELD_W + 2)
        .max(40);
    let prefix = pad_left(c(label, CLR_ACCENT), FIELD_W);
    if value.display_width() <= line_w {
        println!("{}{}", prefix, value);
        return;
    }
    let mut first = true;
    let mut cur = String::new();
    let mut cur_w = 0usize;
    for word in value.split_whitespace() {
        let w = word.display_width() + 1;
        if cur_w + w > line_w && !cur.is_empty() {
            if first {
                println!("{}{}", prefix, cur);
                first = false;
            } else {
                println!("{}{}", " ".repeat(FIELD_W), cur);
            }
            cur.clear();
            cur_w = 0;
        }
        cur.push_str(word);
        cur.push(' ');
        cur_w += w;
    }
    if !cur.trim().is_empty() {
        if first {
            println!("{}{}", prefix, cur);
        } else {
            println!("{}{}", " ".repeat(FIELD_W), cur);
        }
    }
}

// ── 单次执行入口（psman <cmd> ...） ───────────────────

pub fn run_one(args: &[String]) -> i32 {
    if args.is_empty() {
        println!("psman v{} — 进程/端口/线程管理", env!("CARGO_PKG_VERSION"));
        return 0;
    }
    match args[0].as_str() {
        "-h" | "--help" => {
            println!("psman v{} — 进程/端口/线程管理", env!("CARGO_PKG_VERSION"));
            println!("  裸输入数字/通配符查端口，字符串查进程；exit 退出");
            return 0;
        }
        "-V" | "--version" => {
            println!("psman v{}", env!("CARGO_PKG_VERSION"));
            return 0;
        }
        _ => {}
    }
    let tokens: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    match dispatch(&tokens) {
        Ok(Flow::Quit) | Ok(Flow::Continue) => 0,
        Err(e) => {
            eprintln!("{}", c(&format!("错误: {e}"), CLR_ERR));
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_basic() {
        assert!(wildcard_match("3306", "3306"));
        assert!(wildcard_match("*3306", "3306"));
        assert!(wildcard_match("*3306", "13306"));
        assert!(!wildcard_match("*3306", "33061"));
        assert!(wildcard_match("513*", "51380"));
        assert!(wildcard_match("513*", "513"));
        assert!(!wildcard_match("513*", "4120"));
        assert!(wildcard_match("33*6", "3306"));
        assert!(wildcard_match("3.06", "3306"));
        assert!(!wildcard_match("3.06", "306"));
        assert!(!wildcard_match("3.06", "3166"));
        assert!(wildcard_match("py*", "python.exe"));
        assert!(wildcard_match("*", "anything"));
    }

    #[test]
    fn name_matches_variants() {
        assert!(name_matches("python", "python.exe"));
        assert!(name_matches("py*", "python.exe"));
        assert!(name_matches("*ava", "java.exe"));
        assert!(name_matches("chrome*", "chrome.exe"));
        assert!(name_matches("*.exe", "chrome.exe"));
        assert!(!name_matches("java", "python.exe"));
        // 多字节字符：`.` 匹配一个完整字符而非半个
        assert!(wildcard_match("微信.", "微信开发者工具"));
        assert!(name_matches("微*", "微信.exe"));
    }

    #[test]
    fn port_classify() {
        assert!(is_port_pattern("3306"));
        assert!(is_port_pattern("*3306"));
        assert!(is_port_pattern("513*"));
        assert!(is_port_pattern("3.06"));
        assert!(!is_port_pattern("python"));
        assert!(!is_port_pattern("py*"));
        assert!(!is_port_pattern("*ava"));
        assert!(!is_port_pattern("*"));
    }

    #[test]
    fn bang_commands() {
        // ! 单独 → 提示 ! 命令列表（不报错）
        assert!(dispatch(&["!"]).is_ok());
        // !kill 无参数 → 提示用法（不杀任何进程）
        assert!(dispatch(&["!kill"]).is_ok());
        // !kill 非数字 PID → 报错
        assert!(dispatch(&["!kill", "abc"]).is_err());
        // !kill 带空格参数 → 报错
        assert!(dispatch(&["!kill", "10 20"]).is_err());
        // 未知 ! 命令 → 报错
        assert!(dispatch(&["!foo"]).is_err());
        // exit 仍是退出
        assert!(matches!(dispatch(&["exit"]), Ok(Flow::Quit)));
    }

    // 临时验证：自实现 exe_path/cmdline_of 是否与系统层（Win32_Process）一致
    #[test]
    #[ignore]
    fn debug_read_cmd() {
        for pid in [7796u32, 29888] {
            println!(
                "PID {pid}: exe={:?} cmd=[{:?}]",
                psman::proc::exe_path(pid),
                psman::proc::cmdline_of(pid)
            );
        }
    }
}
