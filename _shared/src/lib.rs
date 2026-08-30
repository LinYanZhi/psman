//! # color — ToolBox 共享颜色与格式化库
//!
//! 统一管理 ANSI 颜色、CJK 等宽宽度计算、对齐填充等功能，让使用方
//! 专注于业务逻辑，无需重复处理终端渲染细节。
//!
//! ## 快速入门
//!
//! ```ignore
//! use color::*;
//!
//! // 1. 终端启动时启用 ANSI（仅 Windows 需要）
//! enable_ansi();
//!
//! // 2. 颜色快捷函数
//! println!("{} 文件 {}", green("✓"), bright_cyan("readme.md"));
//!
//! // 3. 自定义样式组合
//! let warn = YELLOW.bold().underline().paint("警告");
//! println!("{}: 磁盘空间不足", warn);
//!
//! // 4. 等宽宽度 & 对齐
//! println!("{}", pad_left("你好", 10));   // "你好      "
//! println!("{}", pad_right("你好", 10));  // "      你好"
//!
//! // 5. 格式化行（多列表格）
//! let row = format_row(&[
//!     cell(green("名称"), 10, Alignment::Left),
//!     cell(bright_cyan("版本"), 8, Alignment::Right),
//! ], "  ");
//! println!("{}", row);
//! ```

pub mod align;
pub mod ansi;
pub mod format;
pub mod style;
pub mod width;

// ── 核心 API 提升到 crate 根级别 ───────────────────

pub use align::{cell, format_row, pad_center, pad_left, pad_right, truncate, Alignment, Cell};
pub use ansi::{enable_ansi, strip_ansi};
pub use format::format_size;
pub use style::{
    black, blue, bold, bold_blue, bold_bright_cyan, bold_cyan, bold_green, bold_magenta, bold_red,
    bold_yellow, bright_black, bright_blue, bright_cyan, bright_green, bright_magenta, bright_red,
    bright_yellow, cyan, gray, green, magenta, red, white, yellow, BLACK, BLUE, BOLD, BOLD_BLUE,
    BOLD_BRIGHT_CYAN, BOLD_CYAN, BOLD_GREEN, BOLD_MAGENTA, BOLD_RED, BOLD_YELLOW, BRIGHT_BLACK,
    BRIGHT_BLUE, BRIGHT_CYAN, BRIGHT_GREEN, BRIGHT_MAGENTA, BRIGHT_RED, BRIGHT_WHITE,
    BRIGHT_YELLOW, CYAN, GRAY, GREEN, MAGENTA, RED, Style, WHITE, YELLOW,
};
pub use width::DisplayWidth;
