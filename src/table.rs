//! 表格渲染 — 基于 color 库（CJK 等宽对齐），列宽按内容自适应
//! 基础设施模块：当前无命令挂接，待用户逐个需求启用。
#![allow(dead_code)]

use color::{cell, format_row, Alignment, Cell, DisplayWidth, Style};

/// 渲染表头 + 数据行。列宽 = max(表头宽, 该列内容最大宽) + 内边距。
/// `aligns` 指定每列对齐方式（数字列建议右对齐），缺省按左对齐。
pub fn table(headers: &[&str], rows: &[Vec<String>], aligns: &[Alignment]) {
    let mut widths: Vec<usize> = headers.iter().map(|h| h.display_width() + 2).collect();
    for row in rows {
        for (i, v) in row.iter().enumerate() {
            if i < widths.len() {
                widths[i] = widths[i].max(v.display_width() + 2);
            }
        }
    }

    let head: Vec<Cell> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let a = aligns.get(i).copied().unwrap_or(Alignment::Left);
            cell(Style::new(96).bold().paint(h), widths[i], a)
        })
        .collect();
    println!("{}", format_row(&head, "  "));

    for row in rows {
        let cells: Vec<Cell> = row
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let a = aligns.get(i).copied().unwrap_or(Alignment::Left);
                cell(v.clone(), widths[i], a)
            })
            .collect();
        println!("{}", format_row(&cells, "  "));
    }
}
