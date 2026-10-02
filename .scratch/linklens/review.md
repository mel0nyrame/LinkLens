# v1 双轴代码审查

2026-10-01，Standards 与 Spec 两个独立审查轴覆盖票 01–06。

> 历史审查：提交引用的可用性及当前行为入口见 [规格顶部的历史边界](spec.md)。下文保留当时的发现、修复说明和验收结果，不代表对当前 HEAD 的重新审查。

当时固定基点为原始 main `218e8a7`；首次审查提交 `03c28aa`，修复后复核提交 `f111988`。首次范围为 `218e8a7...03c28aa`，复核范围为 `03c28aa...f111988`；这些范围已无法在当前仓库执行。首次发现中的行号属于 `03c28aa` 的源码，仅用于理解历史问题。规格见 [spec.md](spec.md)，接口事实见 [研究报告](../../docs/research/net-coffee-api.md)。

## Standards

### 首次审查

审查范围为 `218e8a7a153c828e15f0b824cb6b57d04ec53d25..03c28aa1de1779f6e9959ce51278f570083635a9` 的票 01–06 全部实现，已先读完整 commit list。未发现明确违反已记录规则的硬问题：层间依赖、统一主题/图标、占位项目名、`.data/` 路径与默认跳过真实收发测试符合约定；已有实际测试和终端验收记录。以下均为 Fowler baseline 的判断型发现，不升级为硬规则。

1. **[P2] possible Duplicated Code：重复地址校验已有行为偏差。** `src/net/split.rs:309–323` 与 `src/net/cn_source.rs:32–43` 重复四段/数字/255 校验；分流 IPv6 分支仅有 `all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.')`，因此 `is_valid_ip(":")`、`is_valid_ip("1:2:3")` 返回 true，`pick_bytedance_ip(Some(":"), None)` 返回无效出口，污染分流汇总。`src/net/trace.rs` 已直接用标准库真正解析地址。最小修复：至少将分流校验换为 `text.parse::<std::net::IpAddr>().is_ok()`，加非法 IPv6 与合法 IPv6 对照测试；国内源保留其必要的原文容忍规则，无需新增领域抽象。

2. **[P3] possible Duplicated Code：评分页重复统一滚动规则。** `src/state_score.rs:103–110` 和 `src/ui/scroll.rs:9–20` 均含 `KeyCode::PageDown => …saturating_add(10).min(max…)` 以及相同六个滚动键分支。以后调整翻页步长或边界需改两处。最小修复：评分页在消费输入焦点后调用已有 `key_scroll`，保留评分专属按键处理；将现有首尾/钳制回归测试继续运行。

3. **[P3] possible Repeated Switches：信任分颜色表重复。** `src/ui/pages/ai.rs:490–496` 与 `src/ui/pages/ip_score.rs:443–448` 都写同一 `TrustTier` 映射，例如 `TrustTier::Good => THEME_SUCCESS_SOFT`。调一档颜色时会要求同步修改两页。最小修复：把已有映射收口为两页共用的 UI helper；保持 `detect` 不依赖 UI，颜色仍取 `THEME_*`。

合计：硬违反 0 项；判断型发现 3 项，其中地址校验已存在可确定的无效 IPv6 接受问题。

### 修复复核

范围：`03c28aa1de1779f6e9959ce51278f570083635a9..f1119883ab47bd40fb1c7411358ef626660d578f`。已读取修复 commit list 与相关 diff；本次只读复核，不重复已完成的 224 通过、8 ignored、clippy/fmt 验证。

- **已修：原 P2 地址校验偏差。** `src/net/split.rs::is_valid_ip` 已改为标准库 `IpAddr` 解析；新增 `:`、`1:2:3`、`12345::`、`1::2::3` 非法输入，以及合法 IPv6/IPv4 映射 IPv6 对照。国内来源解析的容忍规则保持原状，未引入额外抽象。
- **已修：原 P3 重复滚动规则。** `src/state_score.rs::handle_key` 在输入焦点先行消费后调用既有 `ui::scroll::key_scroll`，移除六个重复滚动分支；评分专属按键和非滚动时的边界钳制仍保留。
- **已修：原 P3 重复信任分颜色映射。** `src/ui/pages/mod.rs::trust_tier_color` 成为两页共享的唯一映射；AI 与评分页均调用它，仍使用 `THEME_*`，`detect` 未反向依赖 UI。

**未修：0 项。新增：0 项。** 修复 diff 中未见明显标准回归：出口采集复用既有编排，AI 独立快照沿现有状态模式；历史缺分处理保留纯函数接缝；DNS 请求时限采用局部覆盖，文档同步说明。AI/历史/DNS 的规格行为由 Spec 轴另行复核。

## Spec

### 首次审查

审查范围：`218e8a7...03c28aa` 的票 01–06；完整 commit list、规格、六张票与权威研究报告均已读取。发现 4 项；未发现可证的额外范围扩张。

1. **[P2] AI 重查混合新旧网络出口。** 票 03 第 9 行要求「三出口 IP 卡并行探测」，spec 第 21 行要求并行展示三出口以核验真实出口。`src/probe_ai.rs:143` 每轮只重新探测平台出口；`src/ui/pages/ai.rs:38` 的国内与 Cloudflare 卡始终取应用启动时的 `home`。切换节点后按 `r`，平台卡更新，另两卡仍为旧 IP；首次启动失败也无法通过重查恢复。最小修复：每轮 AI 检测并行重测三出口，保存同一轮结果。

2. **[P2] 风险请求失败被记为真实 0 分，阻止当日补正。** spec 第 37 行要求历史可用于「跨天对比节点纯净度变化」，第 41 行要求「所有探测在失败/超时时给出明确的状态呈现」。`src/probe_ai.rs:170` 将风险缺失 `unwrap_or(0)` 并落盘；`src/ui/pages/ai.rs:462` 显示「0分」。同 IP 24h 去重还阻止稍后成功检测更正。最小修复：无有效分值时不写有效评分历史，或持久化明确未知状态并允许成功结果补全。外部复现先记缺失分值 0，再记 95，历史仍显示 0。

3. **[P3] 评分页的中文归属地仍显示英文国家。** spec 第 17 行要求「中文归属地与运营商」，第 42 行要求「全中文文案…贯穿所有页面」。`src/ui/pages/ip_score.rs:237` 调用只拼原始字段的 `g.geo_string()`；研究真实样例直接显示 `United States`，其他页面已有 `cc::chinese_location`。最小修复：该卡复用现有中文归属地转换。

4. **[P3] DNS 回读缺少约定的 5 秒边界。** 票 06 第 10 行要求「各接口超时与net.coffee 接口报告一致」；研究第 305 行明确回读「5s，最多 3 次轮询」。`src/net/dnsleak.rs:114–120` 没有请求超时覆盖，实际继承 `http::client()` 的 8s。回读不响应时三轮可额外等待 9 秒，票 06 关于专用边界已保持的记录不成立。最小修复：回读请求显式设 5s，并覆盖这一边界。

### 修复复核

复核范围：`03c28aa...f111988`；已读取修复 commit list、相关完整 diff、票 06 补充记录及两份针对性 PTY summary。

**已修：4 项全部关闭。**

1. AI 出口刷新：`probe_ai::run_page` 每轮并行采集国内/Cloudflare 与平台出口，三卡从 `AiOutcome.reference_egress` 读取同轮快照，首页缓存不再参与。`begin_probe` 在状态锁内同时设 started/Pending，连续重查不能启动重叠任务。PTY 证据确认重查进入两个参照卡 Pending 槽并完成。
2. 缺分历史：`history::record` 接收 `Option<u8>`，None 保留已有列表、不新增记录；编排不把缺失分值转换为 0，也不写盘。新增回归覆盖缺失后有效 95 分与有效 0 分。旧 schema 的已存在 0 分无法确定来源，未迁移的限制已如实记录，不影响本次新增误记缺陷的关闭。
3. 中文归属地：评分卡改用现有 `cc::chinese_location`；PTY 显示「美国 Google」。
4. DNS 回读：`fetch_dns_result` 显式设请求级 5 秒超时，已覆盖客户端默认 8 秒；票 06 更正了先前遗漏的记录。

**未修：0 项。新增：0 项。** 修复 diff 未发现可证的明显行为回归。滚动复用保持输入焦点先消费；追加评分 PTY 专测的首尾、上下与翻页断言全部通过，q 退出状态 0。本次仅针对原 4 项及修复 diff 复核，未扩大为新一轮全量审查。

验证：修复合入后的 `cargo test` 为 224 通过、0 失败、8 ignored；`cargo clippy --all-targets -- -D warnings`、`cargo fmt --check`、`git diff --check` 均通过。终端证据与测量边界见 [票 06](issues/06-polish-and-review.md)。用户 Nerd Font 字形观感、真实 DNS 长解析器列表未作为已完成的实测结论。

合计：Standards 3 项、Spec 4 项全部修复；两个轴均未遗留问题。各轴原最高严重性为 P2，已在 `f111988` 复核关闭。
