//! IP 评分查询编排：主资料45s、失败等待15s后重试一次，增强区块并行流式更新。
use std::time::Duration;

use crate::app::Page;
use crate::detect::scene::{QueryKind, QueryTarget, validate_query_input};
use crate::net::{
    geoip::GeoIp,
    http,
    ip_score::{
        self, ApiError, AsnCompanies, Bgp, Dnsbl, Heat, Lookup, PollDecision, Radar, Related,
    },
    trace,
};
use crate::state::SharedState;
use crate::state_score::{ScorePhase, ScoreState, Section};

const DEEP_TIMEOUT: Duration = Duration::from_secs(45);
const POLL_INTERVAL: Duration = Duration::from_millis(1500);

/// 首次进入自动查同源 trace 的当前出口；以后仅消费用户明确提交的 IP。
pub fn spawn_for_page_if_needed(shared: SharedState) {
    let request = {
        let mut state = shared.lock();
        if state.app.page != Page::IpScore {
            return;
        }
        let score = &mut state.score;
        let target = if let Some(target) = score.request.take() {
            Some(target)
        } else if !score.started {
            None
        } else {
            return;
        };
        let generation = score.generation.wrapping_add(1);
        let recent = std::mem::take(&mut score.recent);
        let input = std::mem::take(&mut score.input);
        let editing = score.editing;
        *score = ScoreState {
            started: true,
            generation,
            recent,
            input,
            editing,
            target: target.clone(),
            phase: if target.is_some() {
                ScorePhase::Lookup
            } else {
                ScorePhase::DetectingExit
            },
            ..ScoreState::default()
        };
        (generation, target)
    };
    shared.notify();
    tokio::spawn(async move {
        let client = http::client();
        let (generation, target) = request;
        let target = match target {
            Some(t) => t,
            None => {
                let result = trace::fetch_trace(&client, "ip.net.coffee")
                    .await
                    .and_then(|t| t.ip)
                    .and_then(|ip| validate_query_input(&ip).ok());
                let Some(t) = result else {
                    update(&shared, generation, |s| {
                        s.phase = ScorePhase::Failed(ApiError::new(
                            "当前出口识别失败，请按 / 手动输入 IP".into(),
                        ))
                    });
                    return;
                };
                t
            }
        };
        if !update(&shared, generation, |s| {
            s.target = Some(target.clone());
            if !s.editing {
                s.input = target.ip.clone();
            }
            s.recent.retain(|ip| ip != &target.ip);
            s.recent.insert(0, target.ip.clone());
            s.recent.truncate(20);
            s.phase = ScorePhase::Lookup;
            s.lookup = Section::Pending {
                attempt: 1,
                limit: 2,
            };
            s.geo = Section::Pending {
                attempt: 1,
                limit: 1,
            };
        }) {
            return;
        }
        run_query(&client, &shared, generation, &target).await;
    });
}

/// 所有写回都验证查询代号，切换目标后旧请求的迟到结果不会覆盖新结果。
fn update(shared: &SharedState, generation: u64, change: impl FnOnce(&mut ScoreState)) -> bool {
    {
        let mut state = shared.lock();
        if state.score.generation != generation {
            return false;
        }
        change(&mut state.score);
    }
    shared.notify();
    true
}

async fn lookup_with_retry(
    client: &reqwest::Client,
    shared: &SharedState,
    generation: u64,
    ip: &str,
) -> Result<Lookup, ApiError> {
    let path = format!("/api/ip/lookup/{ip}");
    let mut last = None;
    for attempt in 1..=2 {
        let result = ip_score::fetch(client, &path, "深度查询", DEEP_TIMEOUT).await;
        match result {
            Ok(data) => return Ok(data),
            Err(err) if !err.retryable() => return Err(err),
            Err(err) => last = Some(err),
        }
        if attempt == 1 {
            if !update(shared, generation, |s| s.phase = ScorePhase::RetryWait) {
                return Err(ApiError::new("查询已被替换".into()));
            }
            tokio::time::sleep(Duration::from_secs(15)).await;
            if !update(shared, generation, |s| {
                s.phase = ScorePhase::Lookup;
                s.lookup = Section::Pending {
                    attempt: 2,
                    limit: 2,
                };
            }) {
                return Err(ApiError::new("查询已被替换".into()));
            }
        }
    }
    if !update(shared, generation, |s| s.phase = ScorePhase::Fallback) {
        return Err(ApiError::new("查询已被替换".into()));
    }
    ip_score::fetch(
        client,
        &format!("/api/ipv2/lookup/{ip}"),
        "v2 兜底",
        Duration::from_secs(20),
    )
    .await
    .map_err(|_| last.expect("两次失败后必有错误"))
}

async fn run_query(
    client: &reqwest::Client,
    shared: &SharedState,
    generation: u64,
    target: &QueryTarget,
) {
    let ip = &target.ip;
    let (lookup, ()) = tokio::join!(lookup_with_retry(client, shared, generation, ip), async {
        let result = ip_score::fetch::<GeoIp>(
            client,
            &format!("/api/geoip/{ip}"),
            "归属地",
            http::PUBLIC_TIMEOUT,
        )
        .await;
        update(shared, generation, |s| s.geo = result.into());
    });
    let data = match lookup {
        Ok(d) => d,
        Err(error) => {
            update(shared, generation, |s| {
                s.lookup = Section::Failed(error.clone());
                s.phase = ScorePhase::Failed(error);
            });
            return;
        }
    };
    // 上游接口报告遇到 bogon 仅显示非公网提示，避免100分缓存记录暗示公网可用。
    if data.is_bogon {
        update(shared, generation, |s| {
            s.lookup = Section::Ready(Box::new(data));
            s.related = Section::Unsupported;
            s.heat = Section::Unsupported;
            s.bgp = Section::Unsupported;
            s.dnsbl = Section::Unsupported;
            s.radar = Section::Unsupported;
            s.companies = Section::Unsupported;
            s.phase = ScorePhase::Done;
        });
        return;
    }
    let asn = data.risk.asn;
    let pending = data.related_domains_pending;
    if !update(shared, generation, |s| {
        s.phase = ScorePhase::Enhancing;
        s.related = if pending {
            Section::Pending {
                attempt: 0,
                limit: 10,
            }
        } else {
            Section::Ready(Related {
                pending: false,
                related_domains: data.related_domains.clone(),
            })
        };
        s.bgp = Section::Pending {
            attempt: 1,
            limit: 1,
        };
        s.heat = if target.kind == QueryKind::V4 {
            Section::Pending {
                attempt: 1,
                limit: 1,
            }
        } else {
            Section::Unsupported
        };
        s.dnsbl = if target.kind == QueryKind::V4 {
            Section::Pending {
                attempt: 1,
                limit: 1,
            }
        } else {
            Section::Unsupported
        };
        s.radar = if asn.is_some() {
            Section::Pending {
                attempt: 1,
                limit: 1,
            }
        } else {
            Section::Unsupported
        };
        s.companies = if asn.is_some() {
            Section::Pending {
                attempt: 1,
                limit: 12,
            }
        } else {
            Section::Unsupported
        };
        s.lookup = Section::Ready(Box::new(data));
    }) {
        return;
    }
    tokio::join!(
        async {
            let result = ip_score::fetch::<Bgp>(
                client,
                &format!("/api/ipv2/bgp/{ip}"),
                "BGP",
                Duration::from_secs(22),
            )
            .await;
            update(shared, generation, |s| s.bgp = result.into());
        },
        async {
            if target.kind != QueryKind::V4 {
                return;
            }
            let result = ip_score::fetch::<Heat>(
                client,
                &format!("/api/ipv2/heat/{ip}"),
                "C 段热度",
                Duration::from_secs(15),
            )
            .await;
            update(shared, generation, |s| s.heat = result.into());
        },
        async {
            if target.kind != QueryKind::V4 {
                return;
            }
            let result = ip_score::fetch::<Dnsbl>(
                client,
                &format!("/api/ipv2/dnsbl/{ip}"),
                "DNSBL",
                Duration::from_secs(15),
            )
            .await;
            update(shared, generation, |s| s.dnsbl = result.into());
        },
        async {
            if let Some(asn) = asn {
                let result = ip_score::fetch::<Radar>(
                    client,
                    &format!("/api/ipv2/radar/{asn}"),
                    "Radar",
                    Duration::from_secs(14),
                )
                .await;
                update(shared, generation, |s| s.radar = result.into());
            }
        },
        async {
            if pending {
                poll_related(client, shared, generation, ip).await;
            }
        },
        async {
            if let Some(asn) = asn {
                poll_companies(client, shared, generation, asn).await;
            }
        },
    );
    update(shared, generation, |s| s.phase = ScorePhase::Done);
}

async fn poll_related(client: &reqwest::Client, shared: &SharedState, generation: u64, ip: &str) {
    for attempt in 1..=10 {
        tokio::time::sleep(POLL_INTERVAL).await;
        if !update(shared, generation, |s| {
            s.related = Section::Pending { attempt, limit: 10 }
        }) {
            return;
        }
        match ip_score::fetch::<Related>(
            client,
            &format!("/api/ip/related/{ip}"),
            "反查域名",
            Duration::from_secs(15),
        )
        .await
        {
            Ok(data) => match ip_score::poll_decision(data.pending, attempt, 10) {
                PollDecision::Done => {
                    update(shared, generation, |s| s.related = Section::Ready(data));
                    return;
                }
                PollDecision::TimedOut => {
                    update(shared, generation, |s| {
                        s.related =
                            Section::Failed(ApiError::new("反查域名扫描超时，请按 r 重试".into()))
                    });
                    return;
                }
                PollDecision::Again => {}
            },
            Err(error) if attempt == 10 => {
                update(shared, generation, |s| s.related = Section::Failed(error));
            }
            Err(_) => {}
        }
    }
}

async fn poll_companies(client: &reqwest::Client, shared: &SharedState, generation: u64, asn: u32) {
    for attempt in 1..=12 {
        if attempt > 1 {
            tokio::time::sleep(POLL_INTERVAL).await;
        }
        if !update(shared, generation, |s| {
            s.companies = Section::Pending { attempt, limit: 12 }
        }) {
            return;
        }
        match ip_score::fetch::<AsnCompanies>(
            client,
            &format!("/api/ipv2/asncos/{asn}"),
            "同 ASN 公司",
            Duration::from_secs(15),
        )
        .await
        {
            Ok(data) => match ip_score::poll_decision(data.pending, attempt, 12) {
                PollDecision::Done => {
                    update(shared, generation, |s| s.companies = Section::Ready(data));
                    return;
                }
                PollDecision::TimedOut => {
                    update(shared, generation, |s| {
                        s.companies = Section::Failed(ApiError::new(
                            "同 ASN 公司聚合超时，请按 r 重试".into(),
                        ))
                    });
                    return;
                }
                PollDecision::Again => {}
            },
            Err(error) => {
                update(shared, generation, |s| s.companies = Section::Failed(error));
                return;
            }
        }
    }
}

impl<T> From<Result<T, ApiError>> for Section<T> {
    fn from(value: Result<T, ApiError>) -> Self {
        match value {
            Ok(data) => Self::Ready(data),
            Err(error) => Self::Failed(error),
        }
    }
}
