/// 将字节数格式化为人类可读的字符串。
///
/// 自适应选择 B / KB / MB / GB / TB 单位，保留一位小数。
///
/// # 示例
/// ```ignore
/// use color::format_size;
/// assert_eq!(format_size(0), "0B");
/// assert_eq!(format_size(1023), "1023B");
/// assert_eq!(format_size(1536), "1.5KB");
/// assert_eq!(format_size(1_048_576), "1.0MB");
/// assert_eq!(format_size(1_073_741_824), "1.0GB");
/// ```
pub fn format_size(size: u64) -> String {
    if size < 1024 {
        return format!("{}B", size);
    }

    let units = ["KB", "MB", "GB", "TB"];
    let mut value = size as f64;

    for &unit in &units {
        value /= 1024.0;
        if value < 1024.0 {
            return if value < 10.0 {
                format!("{:.1}{}", value, unit)
            } else {
                format!("{:.0}{}", value, unit)
            };
        }
    }

    // TB 及以上
    format!("{:.1}TB", value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bytes() {
        assert_eq!(format_size(0), "0B");
        assert_eq!(format_size(512), "512B");
        assert_eq!(format_size(1023), "1023B");
    }

    #[test]
    fn test_kb() {
        assert_eq!(format_size(1024), "1.0KB");
        assert_eq!(format_size(1536), "1.5KB");
        assert_eq!(format_size(10_240), "10KB");
        assert_eq!(format_size(102_400), "100KB");
    }

    #[test]
    fn test_mb() {
        assert_eq!(format_size(1_048_576), "1.0MB");
        assert_eq!(format_size(1_572_864), "1.5MB");
    }

    #[test]
    fn test_gb() {
        assert_eq!(format_size(1_073_741_824), "1.0GB");
    }

    #[test]
    fn test_tb() {
        assert_eq!(format_size(1_099_511_627_776), "1.0TB");
    }
}
