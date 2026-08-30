use color::*;

fn main() {
    enable_ansi();

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  {} — right_align: 1~100", bold_cyan("测试 1"));
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    test_right_align();

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  {} — 多行多列表格，每列不同颜色", bold_cyan("测试 2"));
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    test_table();

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  {} — 每个单词不同颜色", bold_cyan("测试 3"));
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    test_colored_sentence();

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  {} — 中英文混合对齐", bold_cyan("测试 4"));
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    test_mixed_cjk_table();
}

fn test_right_align() {
    // 分10行打印，每行10个数字，右对齐到宽度4
    for row in 0..10 {
        let mut line = String::new();
        line.push_str("  ");
        for col in 0..10 {
            let n = row * 10 + col + 1;
            let colored_num = match n % 3 {
                0 => bright_cyan(&format!("{}", n)),
                1 => green(&format!("{}", n)),
                _ => yellow(&format!("{}", n)),
            };
            line.push_str(&pad_right(&colored_num, 6));
        }
        println!("{}", line);
    }
}

fn test_table() {
    // 表头
    let header = format_row(&[
        cell(bold_cyan("名称"),   14, Alignment::Left),
        cell(bold_green("版本"),  10, Alignment::Left),
        cell(bold_yellow("大小"), 10, Alignment::Right),
        cell(bold_red("状态"),    8, Alignment::Left),
    ], "  ");
    println!("  {}", header);

    // 分隔线
    println!("  {}",
        gray(&format_row(&[
            cell("──".into(), 14, Alignment::Left),
            cell("──".into(), 10, Alignment::Left),
            cell("──".into(), 10, Alignment::Right),
            cell("──".into(), 8, Alignment::Left),
        ], "  "))
    );

    // 数据行
    let rows = vec![
        ("7-Zip",       "24.08",  "1.5MB",  "✓"),
        ("Firefox",     "136.0",  "85.2MB", "✓"),
        ("VSCode",      "1.98",   "128MB",  "✓"),
        ("Python",      "3.13",   "25.6MB", "✗"),
        ("Git",         "2.47",   "62.3MB", "✓"),
        ("Docker",      "27.0",   "340MB",  "✗"),
    ];

    for (name, ver, size, status) in &rows {
        let status_styled = match *status {
            "✓" => green(status),
            "✗" => red(status),
            _ => gray(status),
        };
        let line = format_row(&[
            cell(cyan(name),   14, Alignment::Left),
            cell(white(ver),   10, Alignment::Left),
            cell(yellow(size), 10, Alignment::Right),
            cell(status_styled, 8, Alignment::Left),
        ], "  ");
        println!("  {}", line);
    }
}

fn test_colored_sentence() {
    // 普通的句子
    println!("  {} {} {} {}!",
        bold_red("Error:"),
        yellow("file"),
        bright_cyan("not_found.rs"),
        gray("does not exist"),
    );

    println!("  {} {} {} {}",
        bold_green("Compiling"),
        bright_cyan("my-project"),
        gray("v"),
        yellow("1.0.0"),
    );

    println!("  {} {} {} {} {} {}",
        bold_green("✓"),
        gray("All"),
        bright_blue("42"),
        gray("tests passed in"),
        yellow("1.23s"),
        gray("(3 suites)"),
    );

    // 混合样式组合
    println!();
    println!("  {}",
        format!("{} 文件 {} {} {} {} {}",
            green("✔"),
            white("main.rs"),
            gray("|"),
            yellow("12KB"),
            gray("|"),
            bright_blue("0.3s"),
        )
    );
}

/// 测试 4：中英文混合对齐
///
/// 中文字符显示宽度为 2（半角），ascii 为 1（半角），
/// 本测试验证 `pad_left` / `pad_right` / `format_row`
/// 能否正确处理混合宽度对齐。
fn test_mixed_cjk_table() {
    // ── 场景：文件浏览列表 ──
    let cols = [
        bold_cyan("文件名"),
        bold_green("大小"),
        bold_yellow("修改日期"),
        bold_magenta("类型"),
    ];
    let col_widths = [34, 12, 22, 14];
    let aligns = [
        Alignment::Left,
        Alignment::Right,
        Alignment::Center,
        Alignment::Left,
    ];

    // 构造表头单元格
    let header_cells: Vec<Cell> = cols
        .iter()
        .zip(col_widths.iter())
        .zip(aligns.iter())
        .map(|((text, &w), &a)| cell(text.clone(), w, a))
        .collect();

    // 标尺行：数字标注每一列的结束位置，方便目测对齐
    let mut ruler = String::from("  ");
    for &w in &col_widths {
        for i in 1..=w {
            ruler.push(match i {
                1 => '1',
                _ if i == w => '|',
                _ => '·',
            });
        }
        ruler.push_str("  ");
    }
    println!("{}", gray(&ruler));

    println!("  {}", format_row(&header_cells, "  "));

    // 分隔线：用 `-` 作为基本字符保证宽度计算准确
    let sep_line = format_row(&[
        cell(gray(&"-".repeat(col_widths[0])), col_widths[0], aligns[0]),
        cell(gray(&"-".repeat(col_widths[1])), col_widths[1], aligns[1]),
        cell(gray(&"-".repeat(col_widths[2])), col_widths[2], aligns[2]),
        cell(gray(&"-".repeat(col_widths[3])), col_widths[3], aligns[3]),
    ], "  ");
    println!("  {}", sep_line);

    // 数据——注意每行中文字符宽度不同，测试对齐
    let files = [
        ("Cargo.toml",                         "1.2KB", "2026-06-10 14:22", "配置文件"),
        ("src/lib.rs",                         "8.5KB", "2026-06-12 09:15", "源代码文件"),
        ("src/main.rs",                        "3.1KB", "2026-06-11 20:30", "源代码文件"),
        ("文档说明.md",                        "2.4KB", "2026-06-09 11:00", "文本文档"),
        ("设计稿_2026年终版_final_v3.png",     "2.3MB", "2026-06-08 16:45", "图像文件"),
        ("学习资料/高等数学/微积分笔记.txt",   "128B",  "2026-05-20 08:00", "文本文档"),
        ("学习资料/Rust入门指南.pdf",          "4.7MB", "2026-06-01 13:30", "PDF文档"),
    ];

    for (name, size, date, kind) in &files {
        let row = format_row(&[
            cell(white(name),  col_widths[0], aligns[0]),
            cell(yellow(size), col_widths[1], aligns[1]),
            cell(cyan(date),   col_widths[2], aligns[2]),
            cell(gray(kind),   col_widths[3], aligns[3]),
        ], "  ");
        println!("  {}", row);
    }

    // 程序验证：检查每个单元格渲染后的显示宽度是否等于目标宽度
    let mut all_ok = true;
    for (idx, (name, size, date, kind)) in files.iter().enumerate() {
        let cells = [
            (name,  col_widths[0], aligns[0]),
            (size,  col_widths[1], aligns[1]),
            (date,  col_widths[2], aligns[2]),
            (kind,  col_widths[3], aligns[3]),
        ];
        for (ci, (text, tw, ta)) in cells.iter().enumerate() {
            let rendered = cell(text.to_string(), *tw, *ta).render();
            let actual = rendered.display_width();
            if actual != *tw {
                all_ok = false;
                eprintln!("  行{idx}列{ci}: text={text}, target={tw}, actual={actual}, rendered=<{rendered}>");
            }
        }
    }
    if all_ok {
        println!("  {}", green("✓ 所有列对齐正确 (每列宽度均符合目标)"));
    } else {
        println!("  {}", red("✗ 存在对齐错误，详情见上"));
    }

    // ── 场景 2：软件信息表格（中文软件名 + 英文版本号）──
    println!();
    let app_widths = [20, 16, 16, 14];
    let mut ruler2 = String::from("  ");
    for &w in &app_widths {
        for i in 1..=w {
            ruler2.push(match i { 1 => '1', _ if i == w => '|', _ => '·' });
        }
        ruler2.push_str("  ");
    }
    println!("{}", gray(&ruler2));
    let hdr = format_row(&[
        cell(bold_cyan("软件名称"),    app_widths[0], Alignment::Left),
        cell(bold_green("版本号"),     app_widths[1], Alignment::Left),
        cell(bold_yellow("许可证"),    app_widths[2], Alignment::Left),
        cell(bold_magenta("安装来源"), app_widths[3], Alignment::Left),
    ], "  ");
    println!("  {}", hdr);
    println!("  {}",
        gray(&format_row(&[
            cell("-".repeat(app_widths[0]), app_widths[0], Alignment::Left),
            cell("-".repeat(app_widths[1]), app_widths[1], Alignment::Left),
            cell("-".repeat(app_widths[2]), app_widths[2], Alignment::Left),
            cell("-".repeat(app_widths[3]), app_widths[3], Alignment::Left),
        ], "  "))
    );

    let apps = [
        ("7-Zip",          "24.08",     "LGPL",        "开源软件"),
        ("Microsoft Edge", "126.0.2592","专有软件",     "预装"),
        ("Visual Studio Code", "1.98.2","MIT",         "开源软件"),
        ("网易云音乐",     "3.0.12",    "专有软件",     "官方下载"),
        ("WPS Office",     "12.1.0",    "专有软件",     "官方下载"),
        ("PotPlayer",      "240531",    "专有软件",     "第三方"),
        ("火绒安全软件",   "6.0.0.5",   "专有软件",     "官方下载"),
        ("Python 3.13",    "3.13.1",    "PSF",         "开源软件"),
    ];

    for (name, ver, license, source) in &apps {
        let row = format_row(&[
            cell(white(name),    app_widths[0], Alignment::Left),
            cell(green(ver),     app_widths[1], Alignment::Left),
            cell(yellow(license),app_widths[2], Alignment::Left),
            cell(cyan(source),   app_widths[3], Alignment::Left),
        ], "  ");
        println!("  {}", row);
    }

    // 验证场景 2 对齐
    let mut ok2 = true;
    for (name, ver, license, source) in &apps {
        let cells = [
            (name.display_width(),  app_widths[0]),
            (ver.display_width(),   app_widths[1]),
            (license.display_width(),app_widths[2]),
            (source.display_width(), app_widths[3]),
        ];
        for (ci, (actual, target)) in cells.iter().enumerate() {
            if actual > target {
                ok2 = false;
                eprintln!("  软件表第{ci}列: 内容宽度 {actual} > 列宽 {target}");
            }
        }
    }
    if ok2 {
        println!("  {}", green("✓ 软件表列宽充足，无溢出"));
    }

    // ── 场景 3：对齐极限测试 —— 同一列内长短不一的内容 ──
    // 先算出实际需要的最大宽度，确保不会溢出
    println!();
    let pairs = [
        ("姓名",     "张三"),
        ("英文名",   "John Smith"),
        ("国籍",     "中华人民共和国"),
        ("住址",     "北京市海淀区中关村大街1号"),
        ("邮箱",     "zhangsan@example.com"),
        ("简介",     "Rust/Go/Python 全栈工程师"),
    ];
    let field_col_w: usize = 14;
    let max_val_w = pairs.iter().map(|(_, v)| v.display_width()).max().unwrap();
    let value_col_w = max_val_w + 2; // +2 留个余量

    let ruler3 = format!("  {}{}",
        gray(&format!("1{:·<width$}|", "", width = field_col_w.saturating_sub(2))),
        gray(&format!("1{:·<width$}|", "", width = value_col_w.saturating_sub(2))),
    );
    println!("{}", ruler3);

    let hdr2 = format_row(&[
        cell(bold_cyan("字段"),     field_col_w, Alignment::Left),
        cell(bold_yellow("值"),     value_col_w, Alignment::Left),
    ], "  ");
    println!("  {}", hdr2);
    println!("  {}",
        gray(&format_row(&[
            cell("-".repeat(field_col_w), field_col_w, Alignment::Left),
            cell("-".repeat(value_col_w), value_col_w, Alignment::Left),
        ], "  "))
    );

    for (k, v) in &pairs {
        let row = format_row(&[
            cell(cyan(k),    field_col_w, Alignment::Left),
            cell(white(v),   value_col_w, Alignment::Left),
        ], "  ");
        println!("  {}", row);
    }

    // 验证场景 3 对齐
    let mut ok3 = true;
    for (k, v) in &pairs {
        if k.display_width() > field_col_w {
            ok3 = false;
            eprintln!("  字段列: '{k}' 宽度 {} > 列宽 {field_col_w}", k.display_width());
        }
        if v.display_width() > value_col_w {
            ok3 = false;
            eprintln!("  值列: '{v}' 宽度 {} > 列宽 {value_col_w}", v.display_width());
        }
    }
    if ok3 {
        println!("  {}", green("✓ 字段表列宽充足，无溢出"));
    }
}
