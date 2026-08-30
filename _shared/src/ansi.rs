use std::borrow::Cow;

// ── ANSI 转义常量（用于 const/compile-time 场景如 clap help_template）──

/// # 用法
/// ```ignore
/// use color::ansi;
/// const HELP: &str = concat!(ansi::BOLD_CYAN, "标题", ansi::RESET, "\n");
/// ```
pub const ESC:  &str = "\x1b[";
pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";
pub const DIM: &str = "\x1b[2m";
pub const BOLD_CYAN: &str = "\x1b[1;36m";
pub const BOLD_YELLOW: &str = "\x1b[1;33m";
pub const BOLD_GREEN: &str = "\x1b[1;32m";
pub const BOLD_RED: &str = "\x1b[1;31m";
pub const CYAN: &str = "\x1b[36m";
pub const GREEN: &str = "\x1b[32m";
pub const YELLOW: &str = "\x1b[33m";
pub const RED: &str = "\x1b[31m";
pub const GRAY: &str = "\x1b[90m";
pub const BLUE: &str = "\x1b[34m";
pub const MAGENTA: &str = "\x1b[35m";
pub const DARK_GRAY: &str = "\x1b[2;90m";

/// 启用 Windows 终端 ANSI 转义序列支持。
///
/// 在 Windows 10 及以上版本中，需要启用 `ENABLE_VIRTUAL_TERMINAL_PROCESSING`
/// 标志才能使 `\x1b[...m` 颜色码生效。程序启动时调用一次即可。
///
/// # 示例
/// ```ignore
/// color::enable_ansi();
/// ```
pub fn enable_ansi() {
    #[cfg(windows)]
    {
        // SAFETY: 标准 Windows API 调用，只含整数/指针参数
        unsafe extern "system" {
            fn GetStdHandle(nStdHandle: u32) -> isize;
            fn GetConsoleMode(hConsoleHandle: isize, lpMode: *mut u32) -> i32;
            fn SetConsoleMode(hConsoleHandle: isize, dwMode: u32) -> i32;
        }

        const STD_OUTPUT_HANDLE: u32 = 0xFFFFFFF5u32;
        const STD_ERROR_HANDLE: u32 = 0xFFFFFFF4u32;
        const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;

        unsafe {
            for &handle_id in &[STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
                let h = GetStdHandle(handle_id);
                if h <= 0 { continue; }
                let mut mode: u32 = 0;
                if GetConsoleMode(h, &mut mode) != 0 && mode & ENABLE_VIRTUAL_TERMINAL_PROCESSING == 0 {
                    let _ = SetConsoleMode(h, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
                }
            }
        }
    }
}

/// 从字符串中剥离 ANSI 转义序列。
///
/// 只处理 `\x1b[...m` 形式的颜色/样式码，其他 ANSI 序列不受影响。
///
/// ```ignore
/// use color::strip_ansi;
/// assert_eq!(strip_ansi("\x1b[31m错误\x1b[0m"), "错误");
/// ```
pub fn strip_ansi(s: &str) -> Cow<'_, str> {
    if !s.contains('\x1b') {
        return Cow::Borrowed(s);
    }

    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // 跳过 ESC[ 后的 'm' 序列
            // 向前看是否为 CSI 序列（\x1b[）
            if let Some('[') = chars.next() {
                // 读参数直到 'm'
                for c in &mut chars {
                    if c == 'm' {
                        break;
                    }
                }
            } else {
                result.push(c);
            }
        } else {
            result.push(c);
        }
    }

    Cow::Owned(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{gray, green};

    #[test]
    fn test_strip_ansi_simple() {
        assert_eq!(strip_ansi("\x1b[31m红\x1b[0m"), "红");
    }

    #[test]
    fn test_strip_ansi_complex() {
        assert_eq!(strip_ansi("\x1b[1;31m粗体红\x1b[0m"), "粗体红");
    }

    #[test]
    fn test_strip_ansi_no_codes() {
        assert_eq!(strip_ansi("普通文本"), "普通文本");
    }

    #[test]
    fn test_strip_ansi_empty() {
        assert_eq!(strip_ansi(""), "");
    }

    #[test]
    fn test_strip_ansi_mixed() {
        let input = format!("{} 完成 {}", green("✓"), gray("1.2秒"));
        let cleaned = strip_ansi(&input);
        assert_eq!(cleaned, "✓ 完成 1.2秒");
    }
}
