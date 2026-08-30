use crate::width::DisplayWidth;

/// 对齐方式
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alignment {
    Left,
    Right,
    Center,
}

/// 将文本左对齐填充到指定显示宽度。
///
/// 如果文本宽度已 >= `width`，原样返回。
///
/// 示例（doc test 因 Windows 路径问题标记 ignore）：
/// ```ignore
/// use color::pad_left;
/// assert_eq!(pad_left("你好", 8), "你好    ");
/// assert_eq!(pad_left("hi", 6), "hi    ");
/// ```
pub fn pad_left(text: impl AsRef<str>, width: usize) -> String {
    pad_impl(text.as_ref(), width, Alignment::Left)
}

/// 将文本右对齐填充到指定显示宽度。
///
/// 示例（doc test 因 Windows 路径问题标记 ignore）：
/// ```ignore
/// use color::pad_right;
/// assert_eq!(pad_right("你好", 8), "    你好");
/// ```
pub fn pad_right(text: impl AsRef<str>, width: usize) -> String {
    pad_impl(text.as_ref(), width, Alignment::Right)
}

/// 将文本居中对齐填充到指定显示宽度。
///
/// 示例（doc test 因 Windows 路径问题标记 ignore）：
/// ```ignore
/// use color::pad_center;
/// assert_eq!(pad_center("你好", 8), "  你好  ");
/// ```
pub fn pad_center(text: impl AsRef<str>, width: usize) -> String {
    pad_impl(text.as_ref(), width, Alignment::Center)
}

fn pad_impl(text: &str, width: usize, align: Alignment) -> String {
    let dw = text.display_width();
    // debug 模式下：内容超出列宽直接 panic，第一时间暴露对齐 bug
    // release 模式下：跳过检查，原样返回（优雅降级）
    debug_assert!(
        dw <= width,
        "内容宽度 ({}) 超出目标宽度 ({}): '{}'",
        dw, width, crate::ansi::strip_ansi(text),
    );
    if dw >= width {
        return text.to_string();
    }
    let pad = width - dw;
    match align {
        Alignment::Left => {
            let mut s = String::with_capacity(text.len() + pad);
            s.push_str(text);
            for _ in 0..pad { s.push(' '); }
            s
        }
        Alignment::Right => {
            let mut s = String::with_capacity(text.len() + pad);
            for _ in 0..pad { s.push(' '); }
            s.push_str(text);
            s
        }
        Alignment::Center => {
            let left = pad / 2;
            let right = pad - left;
            let mut s = String::with_capacity(text.len() + pad);
            for _ in 0..left { s.push(' '); }
            s.push_str(text);
            for _ in 0..right { s.push(' '); }
            s
        }
    }
}

/// 截断文本到最大显示宽度，超出部分替换为 `...`。
///
/// 返回的字符串显示宽度不会超过 `max_width`。
///
/// 示例（doc test 因 Windows 路径问题标记 ignore）：
/// ```ignore
/// use color::truncate;
/// assert_eq!(truncate("hello world", 8), "hello...");
/// assert_eq!(truncate("你好世界", 6), "你...");
/// assert_eq!(truncate("短文本", 10), "短文本");
/// ```
pub fn truncate(text: impl AsRef<str>, max_width: usize) -> String {
    let text = text.as_ref();
    if text.display_width() <= max_width {
        return text.to_string();
    }

    let suf = "...";
    let suf_w = suf.display_width();
    let available = max_width.saturating_sub(suf_w);

    let mut result = String::new();
    let mut w = 0usize;

    for c in text.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if w + cw > available {
            result.push_str(suf);
            return result;
        }
        result.push(c);
        w += cw;
    }

    // 理论上不应到这里，但为安全兜底
    result
}

// ── 表格/行输出 ──────────────────────────────────────

/// 格式化单元格：已着色文本 + 目标宽度 + 对齐方式。
///
/// 通常与 [`format_row`] 配合使用。
///
/// 用法示例（doc test 因 Windows 路径问题标记 ignore）：
/// ```ignore
/// use color::{cell, format_row, Alignment};
/// use color::red;
///
/// let cells = vec![
///     cell(red("名称"), 10, Alignment::Left),
///     cell("值".into(), 8, Alignment::Left),
/// ];
/// let row = format_row(&cells, "  ");
/// assert!(row.len() > 20);
/// ```
pub struct Cell {
    text: String,
    width: usize,
    align: Alignment,
}

/// 创建一个格式化单元格。
pub fn cell(text: String, width: usize, align: Alignment) -> Cell {
    Cell { text, width, align }
}

impl Cell {
    /// 渲染此单元格（填充至目标宽度）。
    pub fn render(&self) -> String {
        pad_impl(&self.text, self.width, self.align)
    }
}

/// 将多个单元格渲染为一行，用 `separator` 分隔。
///
/// 用法示例（doc test 因 Windows 路径问题标记 ignore）：
/// ```ignore
/// use color::format_row;
/// let row = format_row(&[], ", ");
/// assert_eq!(row, "");
/// ```
pub fn format_row(cells: &[Cell], separator: &str) -> String {
    let mut result = String::new();
    for (i, cell) in cells.iter().enumerate() {
        if i > 0 {
            result.push_str(separator);
        }
        result.push_str(&cell.render());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pad_left() {
        assert_eq!(pad_left("你好", 6), "你好  ");
        assert_eq!(pad_left("abc", 5), "abc  ");
        // 内容超出时 debug 模式 panic，release 模式原样返回
        if cfg!(debug_assertions) {
            let result = std::panic::catch_unwind(|| pad_left("过长文本", 2));
            assert!(result.is_err(), "debug 模式下内容超出应 panic");
        } else {
            assert_eq!(pad_left("过长文本", 2), "过长文本");
        }
    }

    #[test]
    fn test_pad_right() {
        assert_eq!(pad_right("你好", 6), "  你好");
    }

    #[test]
    fn test_pad_center() {
        assert_eq!(pad_center("你好", 6), " 你好 ");
        assert_eq!(pad_center("a", 4), " a  ");
    }

    #[test]
    fn test_truncate_short() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_cjk() {
        // max_width=6 → available=3 → "你" (2宽) 可放 → "你..." (5宽)
        let r = truncate("你好世界", 6);
        assert_eq!(r, "你...");
        assert!(r.display_width() <= 6);
    }

    #[test]
    fn test_format_row() {
        let cells = vec![
            cell("A".into(), 4, Alignment::Left),
            cell("B".into(), 4, Alignment::Right),
        ];
        // "A   " + "  " + "   B" = "A        B"
        assert_eq!(format_row(&cells, "  "), "A        B");
    }

    #[test]
    fn test_format_row_with_color() {
        let cells = vec![
            cell(crate::red("名称"), 8, Alignment::Left),
            cell("值".into(), 6, Alignment::Left),
        ];
        let row = format_row(&cells, " ");
        assert_eq!(row.display_width(), 15); // 8 + 1空格 + 6
    }

    #[test]
    fn test_overflow_detected_in_debug() {
        // debug 模式下，列宽不足会 panic
        if cfg!(debug_assertions) {
            let r = std::panic::catch_unwind(|| {
                pad_left("内容太长超出边界", 4)
            });
            assert!(r.is_err(), "debug_assert! 应在内容超出时 panic");
        }
    }
}
