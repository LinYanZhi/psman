//! 日志辅助 — 颜色常量、时间戳
//! 部分颜色常量/工具函数待后续命令挂接时启用。
#![allow(dead_code)]

use color::Style;

pub const CLR_HEAD: u8 = 96;   // 标题/提示符
pub const CLR_OK: u8 = 92;     // 成功
pub const CLR_ERR: u8 = 91;    // 错误
pub const CLR_WARN: u8 = 93;   // 警告
pub const CLR_ACCENT: u8 = 95; // 强调（进程名等）
pub const CLR_DIM: u8 = 90;    // 次要信息
pub const CLR_PID: u8 = 94;    // 数字/PID

/// ANSI 着色
pub fn c(text: &str, clr: u8) -> String {
    Style::new(clr).paint(text)
}

/// Unix 秒 → FILETIME（1601-01-01 起的 100ns 数）
fn unix_to_filetime(secs: u64) -> i64 {
    (secs as i64 + 11_644_473_600) * 10_000_000
}

/// 将 Unix 秒格式化为本地时区时间字符串（FileTimeToLocalFileTime 自动应用时区/DST）
#[allow(dead_code)]
pub fn fmt_unix(secs: u64) -> String {
    use windows::Win32::Foundation::{FILETIME, BOOL};
    use windows::Win32::Storage::FileSystem::{FileTimeToLocalFileTime, FileTimeToSystemTime};
    use windows::Win32::System::SystemInformation::SYSTEMTIME;

    unsafe {
        let ft = FILETIME {
            dwLowDateTime: unix_to_filetime(secs) as u32,
            dwHighDateTime: (unix_to_filetime(secs) >> 32) as u32,
        };
        let mut local = FILETIME::default();
        if FileTimeToLocalFileTime(&ft, &mut local).is_err() {
            return "-".into();
        }
        let mut st = SYSTEMTIME::default();
        if !FileTimeToSystemTime(&local, &mut st).as_bool() {
            return "-".into();
        }
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond
        )
    }
}

/// 进程状态（sysinfo ProcessStatus）的中文显示
#[allow(dead_code)]
pub fn status_text(s: &sysinfo::ProcessStatus) -> String {
    use sysinfo::ProcessStatus;
    match s {
        ProcessStatus::Run => "运行中".into(),
        ProcessStatus::Sleep => "休眠".into(),
        ProcessStatus::Stop => "已停止".into(),
        ProcessStatus::Zombie => "僵尸".into(),
        ProcessStatus::Idle => "空闲".into(),
        ProcessStatus::Dead => "已结束".into(),
        other => format!("{other:?}"),
    }
}
