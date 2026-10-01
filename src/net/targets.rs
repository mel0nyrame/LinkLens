//! 网络连通页的目标清单与分组。
//!
//! 权威依据：net.coffee 接口报告 §3.5 与活站 `link-page.js` 的 `TARGETS` 表（逐项转录）。
//! 分组 cn/jp/us/全球；连通手法与首页 6 目标小卡共用 `net::latency` 计时工具
//! （预热 1 次 + 8 轮取中位数）。
//! 注：活站清单现为 48 项目标（cn 12 / jp 4 / us 20 / 全球 12），
//! 较报告统计的「47」多一项（活站近期增删所致），以活站为准。

/// 连通页分组。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LinkGroup {
    /// 组名（中文）。
    pub name: &'static str,
    /// 组旗帜 emoji（全球组用地球）。
    pub flag: &'static str,
}

/// 四组的索引顺序，用于目标归组。
pub const GROUPS: [LinkGroup; 4] = [
    LinkGroup {
        name: "中国",
        flag: "🇨🇳",
    },
    LinkGroup {
        name: "日本",
        flag: "🇯🇵",
    },
    LinkGroup {
        name: "美国",
        flag: "🇺🇸",
    },
    LinkGroup {
        name: "全球",
        flag: "🌐",
    },
];

/// 分组序号，用于目标归组。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GroupId {
    Cn,
    Jp,
    Us,
    Global,
}

impl GroupId {
    /// 在 `GROUPS` 中的下标。
    pub fn index(self) -> usize {
        match self {
            GroupId::Cn => 0,
            GroupId::Jp => 1,
            GroupId::Us => 2,
            GroupId::Global => 3,
        }
    }
}

/// 一个连通测量目标。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LinkTarget {
    pub group: GroupId,
    pub name: &'static str,
    pub url: &'static str,
}

/// 全部目标（活站 `link-page.js` `TARGETS`，2026-10-01 抓取；URL 已按其规则展开）。
pub const TARGETS: &[LinkTarget] = &[
    // ---- 中国（12）----
    LinkTarget {
        group: GroupId::Cn,
        name: "DeepSeek",
        url: "https://www.deepseek.com/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Cn,
        name: "抖音",
        url: "https://lf3-static.bytednsdoc.com/obj/eden-cn/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Cn,
        name: "哔哩哔哩",
        url: "https://www.bilibili.com/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Cn,
        name: "京东",
        url: "https://www.jd.com/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Cn,
        name: "腾讯QQ",
        url: "https://www.qq.com/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Cn,
        name: "微信",
        url: "https://res.wx.qq.com/a/wx_fed/assets/res/NTI4MWU5.ico",
    },
    LinkTarget {
        group: GroupId::Cn,
        name: "小红书",
        url: "https://fe-static.xhscdn.com/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Cn,
        name: "新浪微博",
        url: "https://tva1.sinaimg.cn/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Cn,
        name: "百度",
        url: "https://www.baidu.com/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Cn,
        name: "网易",
        url: "https://www.163.com/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Cn,
        name: "淘宝",
        url: "https://www.taobao.com/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Cn,
        name: "小米",
        url: "https://www.mi.com/favicon.ico",
    },
    // ---- 日本（4）----
    LinkTarget {
        group: GroupId::Jp,
        name: "Sony",
        url: "https://www.sony.jp/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Jp,
        name: "任天堂",
        url: "https://www.nintendo.co.jp/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Jp,
        name: "Yahoo! JP",
        url: "https://www.yahoo.co.jp/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Jp,
        name: "LINE",
        url: "https://line.me/favicon.ico",
    },
    // ---- 美国（20）----
    LinkTarget {
        group: GroupId::Us,
        name: "Apple",
        url: "https://www.apple.com/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Us,
        name: "Google",
        url: "https://www.google.com/generate_204",
    },
    LinkTarget {
        group: GroupId::Us,
        name: "YouTube",
        url: "https://www.youtube.com/generate_204",
    },
    LinkTarget {
        group: GroupId::Us,
        name: "GitHub",
        url: "https://github.com/generate_204",
    },
    LinkTarget {
        group: GroupId::Us,
        name: "Cloudflare",
        url: "https://1.1.1.1/cdn-cgi/trace",
    },
    LinkTarget {
        group: GroupId::Us,
        name: "Claude",
        url: "https://api.anthropic.com/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Us,
        name: "ChatGPT",
        url: "https://chatgpt.com/cdn-cgi/trace",
    },
    LinkTarget {
        group: GroupId::Us,
        name: "AI Studio",
        url: "https://generativelanguage.googleapis.com/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Us,
        name: "Amazon",
        url: "https://www.amazon.com/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Us,
        name: "Bing",
        url: "https://www.bing.com/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Us,
        name: "Steam",
        url: "https://store.steampowered.com/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Us,
        name: "Oracle",
        url: "https://www.oracle.com/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Us,
        name: "Zoom",
        url: "https://st1.zoom.us/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Us,
        name: "Facebook",
        url: "https://static.xx.fbcdn.net/rsrc.php/yb/r/hLRJ1GG_y0J.ico",
    },
    LinkTarget {
        group: GroupId::Us,
        name: "Instagram",
        url: "https://static.cdninstagram.com/rsrc.php/yb/r/hLRJ1GG_y0J.ico",
    },
    LinkTarget {
        group: GroupId::Us,
        name: "X",
        url: "https://abs.twimg.com/favicons/twitter.3.ico",
    },
    LinkTarget {
        group: GroupId::Us,
        name: "Reddit",
        url: "https://www.reddit.com/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Us,
        name: "LinkedIn",
        url: "https://static.licdn.com/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Us,
        name: "Twitch",
        url: "https://static.twitchcdn.net/assets/favicon-32-e29e246c157142c94346.png",
    },
    LinkTarget {
        group: GroupId::Us,
        name: "Netflix",
        url: "https://assets.nflxext.com/us/ffe/siteui/common/icons/nficon2016.ico",
    },
    // ---- 全球（12）----
    LinkTarget {
        group: GroupId::Global,
        name: "TikTok",
        url: "https://www.tiktok.com/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Global,
        name: "Spotify",
        url: "https://open.spotify.com/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Global,
        name: "npm",
        url: "https://registry.npmjs.org/",
    },
    LinkTarget {
        group: GroupId::Global,
        name: "Takealot",
        url: "https://static.takealot.com/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Global,
        name: "PixPix",
        url: "https://www.pixpix.com/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Global,
        name: "Naver",
        url: "https://www.naver.com/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Global,
        name: "Noon",
        url: "https://www.noon.com/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Global,
        name: "Wikipedia",
        url: "https://www.wikipedia.org/static/favicon/wikipedia.ico",
    },
    LinkTarget {
        group: GroupId::Global,
        name: "BBC",
        url: "https://www.bbc.com/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Global,
        name: "Mistral AI",
        url: "https://mistral.ai/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Global,
        name: "Yandex",
        url: "https://yastatic.net/favicon.ico",
    },
    LinkTarget {
        group: GroupId::Global,
        name: "MercadoLibre",
        url: "https://http2.mlstatic.com/favicon.ico",
    },
];

#[cfg(test)]
mod tests {
    use super::{GROUPS, GroupId, TARGETS};

    #[test]
    fn four_groups_in_cn_jp_us_global_order() {
        let names: Vec<&str> = GROUPS.iter().map(|g| g.name).collect();
        assert_eq!(names, ["中国", "日本", "美国", "全球"]);
        assert_eq!(GROUPS.iter().map(|g| g.flag).collect::<Vec<_>>().len(), 4);
    }

    #[test]
    fn target_counts_match_live_site() {
        // 活站 2026-10-01：cn 12 / jp 4 / us 20 / 全球 12 = 48
        assert_eq!(TARGETS.len(), 48);
        assert_eq!(
            TARGETS.iter().filter(|t| t.group == GroupId::Cn).count(),
            12
        );
        assert_eq!(TARGETS.iter().filter(|t| t.group == GroupId::Jp).count(), 4);
        assert_eq!(
            TARGETS.iter().filter(|t| t.group == GroupId::Us).count(),
            20
        );
        assert_eq!(
            TARGETS
                .iter()
                .filter(|t| t.group == GroupId::Global)
                .count(),
            12
        );
    }

    #[test]
    fn target_names_are_unique() {
        for (i, a) in TARGETS.iter().enumerate() {
            for b in &TARGETS[i + 1..] {
                assert_ne!(a.name, b.name, "目标名重复：{}", a.name);
            }
        }
    }

    #[test]
    fn all_urls_are_https_with_a_host() {
        for target in TARGETS {
            let parsed =
                url::Url::parse(target.url).unwrap_or_else(|e| panic!("{}: {e}", target.name));
            assert_eq!(parsed.scheme(), "https", "{} 应为 https", target.name);
            assert!(
                !parsed.host_str().unwrap_or_default().is_empty(),
                "{} 缺主机名",
                target.name
            );
        }
    }

    #[test]
    fn homepage_six_targets_subset_consistency() {
        // 首页 6 目标与连通页重叠的目标 URL 应一致（同一计时口径）
        let github = TARGETS
            .iter()
            .find(|t| t.name == "GitHub")
            .expect("GitHub 在清单中");
        assert_eq!(github.url, "https://github.com/generate_204");
        let cloudflare = TARGETS
            .iter()
            .find(|t| t.name == "Cloudflare")
            .expect("Cloudflare 在清单中");
        assert_eq!(cloudflare.url, "https://1.1.1.1/cdn-cgi/trace");
    }
}
