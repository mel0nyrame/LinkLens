//! 检测历史持久化：同 IP 24 小时内去重、最新在前、上限 6 条（上游接口报告 localStorage 语义）。
//!
//! 数据落仓库根 `.data/` 目录（ADR-0002：目录内含内容为 `*` 的 `.gitignore`
//! 自忽略，数据跟着仓库走、路径与项目名解耦）。去重判定与序列化是纯函数
//!（测试接缝），磁盘读写是薄 IO 壳。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 同 IP 去重窗口：24 小时。
pub const DEDUP_WINDOW_MS: u64 = 24 * 60 * 60 * 1000;

/// 历史条目上限（上游接口报告 localStorage 保留最近 6 条）。
pub const MAX_ENTRIES: usize = 6;

/// 一条 AI 出口检测历史。
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// 出口 IP。
    pub ip: String,
    /// 风险库原始信任分（风险库缺失时为 0，配合 `restricted` 解读）。
    pub trust_score: u8,
    /// 是否命中受限地区。
    pub restricted: bool,
    /// 记录时间（unix 毫秒）。
    pub recorded_at_ms: u64,
}

/// 记录一条检测历史，返回新列表（最新在前）：
/// 同 IP 距最近一次记录不足 24 小时不重复记录；重录时移除旧同 IP 条目；截断到上限。
pub fn record(
    existing: &[HistoryEntry],
    ip: &str,
    trust_score: u8,
    restricted: bool,
    now_ms: u64,
) -> Vec<HistoryEntry> {
    if let Some(latest) = existing.iter().find(|e| e.ip == ip) {
        if now_ms.saturating_sub(latest.recorded_at_ms) < DEDUP_WINDOW_MS {
            return existing.to_vec();
        }
    }
    let mut entries: Vec<HistoryEntry> = existing
        .iter()
        .filter(|e| e.ip != ip)
        .cloned()
        .collect();
    entries.insert(
        0,
        HistoryEntry {
            ip: ip.to_string(),
            trust_score,
            restricted,
            recorded_at_ms: now_ms,
        },
    );
    entries.truncate(MAX_ENTRIES);
    entries
}

/// 记录时间的展示文本：unix 毫秒 → `MM-DD HH:MM`（中国标准时间 UTC+8）。
pub fn format_recorded_at(ms: u64) -> String {
    let total_secs = ms / 1000 + 8 * 3600;
    let days = (total_secs / 86_400) as i64;
    let secs_of_day = total_secs % 86_400;
    let (month, day) = civil_month_day(days);
    format!(
        "{:02}-{:02} {:02}:{:02}",
        month,
        day,
        secs_of_day / 3600,
        (secs_of_day % 3600) / 60
    )
}

/// 天数（自 unix 纪元）转公历月日（Howard Hinnant 的 civil_from_days 算法，年份不参与展示）。
fn civil_month_day(days: i64) -> (u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (month, day)
}

/// 序列化为 JSON 文本（落盘前的纯变换）。
pub fn entries_to_json(entries: &[HistoryEntry]) -> String {
    serde_json::to_string(entries).unwrap_or_else(|_| "[]".to_string())
}

/// 从 JSON 文本反序列化；损坏文件按空历史处理（重启后不因坏文件崩溃）。
pub fn entries_from_json(text: &str) -> Vec<HistoryEntry> {
    serde_json::from_str(text).unwrap_or_default()
}

/// 当前 unix 毫秒（IO/编排侧取时；纯函数一律显式传入时间）。
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

/// 数据目录：从当前工作目录向上找 `.git`（仓库根），数据落在 `<仓库根>/.data/`；
/// 找不到时退回 `<cwd>/.data/`。
pub fn data_dir() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut dir: Option<PathBuf> = None;
    for ancestor in cwd.ancestors() {
        if ancestor.join(".git").exists() {
            dir = Some(ancestor.to_path_buf());
            break;
        }
    }
    dir.unwrap_or(cwd).join(".data")
}

/// 按平台标签给出历史文件路径：`<数据目录>/{tag}-history.json`。
pub fn history_path(tag: &str) -> PathBuf {
    data_dir().join(format!("{tag}-history.json"))
}

/// 确保数据目录存在且自忽略（缺 `.gitignore` 时补写内容为 `*` 的自忽略文件）。
pub fn ensure_data_dir(dir: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let gitignore = dir.join(".gitignore");
    if !gitignore.exists() {
        std::fs::write(&gitignore, "*\n")?;
    }
    Ok(dir.to_path_buf())
}

/// 读取历史；文件缺失或损坏按空历史处理。
pub fn load(path: &Path) -> Vec<HistoryEntry> {
    std::fs::read_to_string(path)
        .map(|text| entries_from_json(&text))
        .unwrap_or_default()
}

/// 写入历史（自动建目录与自忽略文件）。
pub fn save(path: &Path, entries: &[HistoryEntry]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        ensure_data_dir(dir)?;
    }
    std::fs::write(path, entries_to_json(entries))
}

#[cfg(test)]
mod tests {
    use super::{DEDUP_WINDOW_MS, HistoryEntry, MAX_ENTRIES, entries_from_json, entries_to_json, format_recorded_at, record};

    fn entry(ip: &str, at_ms: u64) -> HistoryEntry {
        HistoryEntry {
            ip: ip.to_string(),
            trust_score: 85,
            restricted: false,
            recorded_at_ms: at_ms,
        }
    }

    #[test]
    fn first_record_becomes_newest() {
        let entries = record(&[], "1.2.3.4", 85, false, 1_000);
        assert_eq!(entries, vec![entry("1.2.3.4", 1_000)]);
    }

    #[test]
    fn same_ip_within_24h_is_deduplicated() {
        let existing = record(&[], "1.2.3.4", 85, false, 1_000);
        let again = record(&existing, "1.2.3.4", 40, true, 1_000 + DEDUP_WINDOW_MS - 1);
        assert_eq!(again, existing, "窗口内重复检测不改写历史");
    }

    #[test]
    fn same_ip_beyond_24h_rerecords_and_drops_old() {
        let existing = record(&[], "1.2.3.4", 85, false, 1_000);
        let again = record(&existing, "1.2.3.4", 40, true, 1_000 + DEDUP_WINDOW_MS);
        assert_eq!(
            again,
            vec![HistoryEntry {
                ip: "1.2.3.4".into(),
                trust_score: 40,
                restricted: true,
                recorded_at_ms: 1_000 + DEDUP_WINDOW_MS,
            }]
        );
    }

    #[test]
    fn different_ips_always_record() {
        let existing = record(&[], "1.2.3.4", 85, false, 1_000);
        let entries = record(&existing, "5.6.7.8", 60, false, 1_100);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].ip, "5.6.7.8");
        assert_eq!(entries[1].ip, "1.2.3.4");
    }

    #[test]
    fn history_is_capped_at_max_entries() {
        let mut entries: Vec<HistoryEntry> = Vec::new();
        for i in 0..(MAX_ENTRIES + 2) as u64 {
            entries = record(&entries, &format!("10.0.0.{i}"), 50, false, i * 1_000);
        }
        assert_eq!(entries.len(), MAX_ENTRIES);
        assert_eq!(entries[0].ip, format!("10.0.0.{}", MAX_ENTRIES + 1));
    }

    #[test]
    fn entries_round_trip_through_json() {
        let entries = vec![entry("1.2.3.4", 1_000), entry("2606:4700::1111", 2_000)];
        let text = entries_to_json(&entries);
        assert_eq!(entries_from_json(&text), entries);
    }

    #[test]
    fn corrupted_json_yields_empty_history() {
        assert!(entries_from_json("not json").is_empty());
        assert!(entries_from_json("").is_empty());
    }

    #[test]
    fn recorded_at_formats_as_cn_local_time() {
        // 期望值经 date 命令独立核实（unix 秒 → UTC+8）
        assert_eq!(format_recorded_at(0), "01-01 08:00");
        assert_eq!(format_recorded_at(1_700_000_000_000), "11-15 06:13");
        assert_eq!(format_recorded_at(1_790_812_800_000), "10-01 08:00");
    }
}
