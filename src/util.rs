use chrono::{DateTime, Local, TimeZone};

/// 将 Unix 时间戳格式化为本地时间
/// 例如: 1730108404 -> "2024-10-28 17:40:04"
pub fn format_timestamp_local(timestamp: i64) -> String {
    match Local.timestamp_opt(timestamp, 0) {
        chrono::LocalResult::Single(dt) => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
        _ => timestamp.to_string(),
    }
}

/// 将 ISO 8601 时间字符串格式化为本地时间
/// 例如: "2025-10-28T09:40:07Z" -> "2025-10-28 17:40:07"
pub fn format_iso8601_datetime(iso_string: &str) -> String {
    match DateTime::parse_from_rfc3339(iso_string) {
        Ok(dt) => {
            // 转换为本地时区
            let local_dt = dt.with_timezone(&Local);
            local_dt.format("%Y-%m-%d %H:%M:%S").to_string()
        }
        Err(_) => iso_string.to_string(), // 解析失败时返回原字符串
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_timestamp_local() {
        // 使用一个已知的时间戳进行测试
        let timestamp = 1730108404; // 2024-10-28 09:40:04 UTC
        let formatted = format_timestamp_local(timestamp);
        // 因为本地时区可能不同，我们只检查格式
        assert!(formatted.contains("-"));
        assert!(formatted.contains(":"));
    }

    #[test]
    fn test_format_iso8601_datetime() {
        let iso_string = "2025-10-28T09:40:07Z";
        let formatted = format_iso8601_datetime(iso_string);
        // 应该包含日期和时间分隔符
        assert!(formatted.contains("-"));
        assert!(formatted.contains(":"));
        assert!(formatted.contains("2025"));
    }

    #[test]
    fn test_format_iso8601_datetime_invalid() {
        let invalid_string = "not a date";
        let formatted = format_iso8601_datetime(invalid_string);
        // 解析失败应该返回原字符串
        assert_eq!(formatted, invalid_string);
    }
}
