/// 文本样式：前景色 + 属性（加粗、暗淡、斜体、下划线等）
#[derive(Clone, Copy, Debug)]
pub struct Style {
    code: u8,
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
}

impl Style {
    /// 创建一个纯色样式（无额外属性）
    #[must_use]
    pub const fn new(code: u8) -> Self {
        Self { code, bold: false, dim: false, italic: false, underline: false }
    }

    /// 加粗
    #[must_use]
    pub const fn bold(mut self) -> Self { self.bold = true; self }

    /// 暗淡（降低亮度）
    #[must_use]
    pub const fn dim(mut self) -> Self { self.dim = true; self }

    /// 斜体
    #[must_use]
    pub const fn italic(mut self) -> Self { self.italic = true; self }

    /// 下划线
    #[must_use]
    pub const fn underline(mut self) -> Self { self.underline = true; self }

    /// 用本样式包裹文本，返回含 ANSI 转义码的字符串。
    ///
    /// ```ignore
    /// use color::RED;
    /// assert_eq!(RED.paint("错误"), "\x1b[31m错误\x1b[0m");
    /// ```
    pub fn paint(self, text: impl AsRef<str>) -> String {
        let text = text.as_ref();
        if text.is_empty() {
            return String::new();
        }
        let mut codes = self.code.to_string();
        if self.bold      { codes.push_str(";1"); }
        if self.dim       { codes.push_str(";2"); }
        if self.italic    { codes.push_str(";3"); }
        if self.underline { codes.push_str(";4"); }
        format!("\x1b[{}m{}\x1b[0m", codes, text)
    }
}

// ── 16 标准色 ──────────────────────────────────────────

pub const BLACK:   Style = Style::new(30);
pub const RED:     Style = Style::new(31);
pub const GREEN:   Style = Style::new(32);
pub const YELLOW:  Style = Style::new(33);
pub const BLUE:    Style = Style::new(34);
pub const MAGENTA: Style = Style::new(35);
pub const CYAN:    Style = Style::new(36);
pub const WHITE:   Style = Style::new(37);

// ── 明亮色（90-97）───────────────────────────────────

pub const GRAY:          Style = Style::new(90);
pub const BRIGHT_BLACK:  Style = Style::new(90);
pub const BRIGHT_RED:    Style = Style::new(91);
pub const BRIGHT_GREEN:  Style = Style::new(92);
pub const BRIGHT_YELLOW: Style = Style::new(93);
pub const BRIGHT_BLUE:   Style = Style::new(94);
pub const BRIGHT_MAGENTA:Style = Style::new(95);
pub const BRIGHT_CYAN:   Style = Style::new(96);
pub const BRIGHT_WHITE:  Style = Style::new(97);

// ── 常用组合 ──────────────────────────────────────────

pub const BOLD:           Style = WHITE.bold();
pub const BOLD_RED:      Style = RED.bold();
pub const BOLD_GREEN:    Style = GREEN.bold();
pub const BOLD_YELLOW:   Style = YELLOW.bold();
pub const BOLD_BLUE:     Style = BLUE.bold();
pub const BOLD_CYAN:     Style = CYAN.bold();
pub const BOLD_MAGENTA:  Style = MAGENTA.bold();
pub const BOLD_BRIGHT_CYAN: Style = BRIGHT_CYAN.bold();

// ── 快捷函数 ──────────────────────────────────────────

macro_rules! make_paint_fn {
    ($($name:ident => $style:expr),+ $(,)?) => {
        $(pub fn $name(text: impl AsRef<str>) -> String {
            $style.paint(text)
        })+
    };
}

make_paint_fn! {
    black   => BLACK,
    red     => RED,
    green   => GREEN,
    yellow  => YELLOW,
    blue    => BLUE,
    magenta => MAGENTA,
    cyan    => CYAN,
    white   => WHITE,
    gray    => GRAY,
    bright_black  => BRIGHT_BLACK,
    bright_red    => BRIGHT_RED,
    bright_green  => BRIGHT_GREEN,
    bright_yellow => BRIGHT_YELLOW,
    bright_blue   => BRIGHT_BLUE,
    bright_magenta => BRIGHT_MAGENTA,
    bright_cyan   => BRIGHT_CYAN,
    bold          => BOLD,
    bold_red      => BOLD_RED,
    bold_green    => BOLD_GREEN,
    bold_yellow   => BOLD_YELLOW,
    bold_blue     => BOLD_BLUE,
    bold_magenta  => BOLD_MAGENTA,
    bold_cyan     => BOLD_CYAN,
    bold_bright_cyan => BOLD_BRIGHT_CYAN,
}
