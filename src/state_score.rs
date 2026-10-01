//! IP 评分页的输入和查询快照；探测编排在 probe_score，渲染在 ui/pages/ip_score。
use crate::detect::scene::{QueryTarget, validate_query_input};
use crate::net::geoip::GeoIp;
use crate::net::ip_score::{ApiError, AsnCompanies, Bgp, Dnsbl, Heat, Lookup, Radar, Related};
use crossterm::event::{KeyCode, KeyModifiers};

#[derive(Clone, Debug, Default)]
pub enum Section<T> {
    #[default]
    Waiting,
    Pending {
        attempt: u8,
        limit: u8,
    },
    Ready(T),
    Failed(ApiError),
    Unsupported,
}

#[derive(Clone, Debug, Default)]
pub enum ScorePhase {
    #[default]
    Idle,
    DetectingExit,
    Lookup,
    RetryWait,
    Enhancing,
    Fallback,
    Done,
    Failed(ApiError),
}

impl ScorePhase {
    pub fn label(&self) -> &str {
        match self {
            Self::Idle => "按 / 输入 IPv4 / IPv6，Enter 查询",
            Self::DetectingExit => "正在识别当前出口…",
            Self::Lookup => "正在聚合深度资料（冷数据可能需 5–45 秒）…",
            Self::RetryWait => "深度查询失败，15 秒后自动重试一次…",
            Self::Fallback => "正在读取 v2 兜底资料…",
            Self::Enhancing => "主资料就绪，正在读取增强区块与补全 pending 数据…",
            Self::Done => "查询完成（↑/↓ 滚动，PgUp/PgDn 翻页，Home/End 首尾）",
            Self::Failed(error) => &error.0,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ScoreState {
    pub started: bool,
    /// 每次新查询递增；迟到的前一次结果不得覆盖当前查询。
    pub generation: u64,
    pub target: Option<QueryTarget>,
    pub request: Option<QueryTarget>,
    pub input: String,
    pub editing: bool,
    pub input_error: Option<&'static str>,
    pub recent: Vec<String>,
    pub recent_index: usize,
    pub scroll: u16,
    pub phase: ScorePhase,
    pub lookup: Section<Box<Lookup>>,
    pub geo: Section<GeoIp>,
    pub related: Section<Related>,
    pub heat: Section<Heat>,
    pub bgp: Section<Bgp>,
    pub dnsbl: Section<Dnsbl>,
    pub radar: Section<Radar>,
    pub companies: Section<AsnCompanies>,
}

impl ScoreState {
    /// 返回 true 表示已消费，事件循环不再执行全局键位。
    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers, max_scroll: u16) -> bool {
        if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
            return false;
        }
        if self.editing {
            match code {
                KeyCode::Esc => {
                    self.editing = false;
                    self.input_error = None;
                }
                KeyCode::Enter => match validate_query_input(&self.input) {
                    Ok(target) => self.queue(target),
                    Err(err) => self.input_error = Some(err),
                },
                KeyCode::Char(c)
                    if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    if self.input.len() < 64 {
                        self.input.push(c);
                    }
                }
                KeyCode::Backspace => {
                    self.input.pop();
                }
                KeyCode::Delete => self.input.clear(),
                _ => {}
            }
            return true;
        }
        self.scroll = self.scroll.min(max_scroll);
        match code {
            KeyCode::Up => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::Down => self.scroll = self.scroll.saturating_add(1).min(max_scroll),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(10),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(10).min(max_scroll),
            KeyCode::Home => self.scroll = 0,
            KeyCode::End => self.scroll = max_scroll,
            KeyCode::Char('r') => {
                if let Some(target) = self.target.clone() {
                    self.queue(target);
                }
            }
            KeyCode::Char('[') => self.recent_index = self.recent_index.saturating_sub(1),
            KeyCode::Char(']') => {
                self.recent_index = (self.recent_index + 1).min(self.recent.len().saturating_sub(1))
            }
            KeyCode::Enter => {
                if let Some(ip) = self.recent.get(self.recent_index).cloned()
                    && let Ok(target) = validate_query_input(&ip)
                {
                    self.queue(target);
                }
            }
            KeyCode::Char('/') => {
                self.editing = true;
                self.input.clear();
                self.input_error = None;
            }
            _ => return false,
        }
        true
    }
    pub fn queue(&mut self, target: QueryTarget) {
        self.input = target.ip.clone();
        self.request = Some(target);
        self.input_error = None;
        self.editing = false;
        self.scroll = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn editing_consumes_page_digits_global_shortcuts_but_not_ctrl_c() {
        let mut s = ScoreState::default();
        assert!(s.handle_key(KeyCode::Char('/'), KeyModifiers::NONE, 0));
        for c in "2606:4700:4700::1111".chars() {
            assert!(s.handle_key(KeyCode::Char(c), KeyModifiers::NONE, 0));
        }
        assert!(s.editing);
        assert_eq!(s.input, "2606:4700:4700::1111");
        assert!(!s.handle_key(KeyCode::Char('c'), KeyModifiers::CONTROL, 0));
        assert!(s.handle_key(KeyCode::Enter, KeyModifiers::NONE, 0));
        assert_eq!(s.request.as_ref().unwrap().ip, "2606:4700:4700::1111");
        assert!(!s.editing);
        assert!(!s.handle_key(KeyCode::Char('i'), KeyModifiers::NONE, 0));
        s.handle_key(KeyCode::Char('/'), KeyModifiers::NONE, 0);
        for c in "rihlq".chars() {
            assert!(s.handle_key(KeyCode::Char(c), KeyModifiers::NONE, 0));
        }
        assert!(s.handle_key(KeyCode::Esc, KeyModifiers::NONE, 0));
        assert!(!s.editing);
    }
    #[test]
    fn invalid_input_never_queues_and_recent_ips_can_be_resubmitted() {
        let mut s = ScoreState {
            editing: true,
            input: "hostname".into(),
            recent: vec!["8.8.8.8".into(), "1.1.1.1".into()],
            ..ScoreState::default()
        };
        s.handle_key(KeyCode::Enter, KeyModifiers::NONE, 0);
        assert!(s.request.is_none());
        assert!(s.input_error.is_some());
        s.handle_key(KeyCode::Esc, KeyModifiers::NONE, 0);
        assert!(s.handle_key(KeyCode::Char(']'), KeyModifiers::NONE, 0));
        assert_eq!(s.recent_index, 1);
        s.handle_key(KeyCode::Enter, KeyModifiers::NONE, 0);
        assert_eq!(s.request.unwrap().ip, "1.1.1.1");
    }

    #[test]
    fn scrolling_clamps_on_resize_and_end_can_immediately_move_up() {
        let mut s = ScoreState::default();
        assert!(s.handle_key(KeyCode::End, KeyModifiers::NONE, 100));
        assert_eq!(s.scroll, 100);
        s.handle_key(KeyCode::Up, KeyModifiers::NONE, 100);
        assert_eq!(s.scroll, 99);
        s.handle_key(KeyCode::Up, KeyModifiers::NONE, 20);
        assert_eq!(s.scroll, 19);
        s.handle_key(KeyCode::Home, KeyModifiers::NONE, 20);
        s.handle_key(KeyCode::PageDown, KeyModifiers::NONE, 20);
        assert_eq!(s.scroll, 10);
    }
}
