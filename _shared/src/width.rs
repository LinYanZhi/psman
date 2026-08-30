use crate::ansi::strip_ansi;

/// 可计算显示宽度的类型（CJK 等宽字符感知）。
///
/// 显示宽度 ≠ 字符长度。中文、日文、韩文字符占 2 列，emoji 等
/// 宽字符也按规范计算。ANSI 转义码不计入宽度。
///
/// # 示例
/// ```ignore
/// use color::DisplayWidth;
///
/// assert_eq!("hello".display_width(), 5);
/// assert_eq!("你好".display_width(), 4);
/// assert_eq!("\x1b[31m你好\x1b[0m".display_width(), 4);  // ANSI 码不计宽
/// ```
pub trait DisplayWidth {
    fn display_width(&self) -> usize;
}

impl DisplayWidth for str {
    fn display_width(&self) -> usize {
        let clean = strip_ansi(self);
        unicode_width::UnicodeWidthStr::width(clean.as_ref())
    }
}

impl DisplayWidth for String {
    fn display_width(&self) -> usize {
        self.as_str().display_width()
    }
}

impl<T: DisplayWidth + ?Sized> DisplayWidth for &T {
    fn display_width(&self) -> usize {
        T::display_width(self)
    }
}
