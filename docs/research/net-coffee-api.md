# net.coffee 接口报告

> 调查日期：2026-10-01（浏览器实测 + JS 静态逆向 + curl 复放三重验证）
> 响应样例保留接口字段与结构；实测网络的地址、定位及会话标识已替换为示例值。示例地址用于说明格式，不代表这些保留地址具有对应的上游评分或地理属性。

> 使用边界：本文的上游事实对应调查日期及所记录的脚本来源；端点耗时观测与页面请求时限分别阅读。各功能的 LinkLens 采用值和实现入口见 §3 对应章节，面向用户的实际行为见 [README](../../README.md)。契约变更、响应异常或现有来源无法解释问题时，补查上游并更新对应章节及来源。

---

## 1. 站点概览与技术栈

- **域名**：`https://ip.net.coffee/`（主站 net.coffee）。页面路由：
  `/`（IP查询+分流）、`/claude/`、`/ip/` + `/ip/{ip}`（IP评分）、`/gpt/`、`/link/`（连通）、`/dns/`、`/webrtc/`、`/cloudflare/`、`/ping/`（全球Ping）、`/status/`、`/whois/`、`/news/`。
- **前端**：无框架的**原生 JS 多页站点**。每个页面一个独立 JS（未打包、未压缩混淆，可直接读）：
  `/home-page.js`、`/claude/claude-page.js`、`/ip/ip-page.js` + `/ip/ip-page-v2.js`（v2 只做 DOM 增强）、`/gpt/gpt-page.js`、`/link/link-page.js`、`/ping/ping-page.js`、`/cloudflare/cloudflare-page.js`；`/dns/`、`/webrtc/`、`/status/` 为内联 `<script>`。
- **后端**：站点自有 API 全部在 `ip.net.coffee/api/*`，前面套 **Cloudflare**（`server: cloudflare`，含 CDN 缓存）。自有 API 响应带 `access-control-allow-origin: *`。
- **WAF/反爬**：API 的 GET/POST 用普通 curl（任意 UA、无 Cookie）即可调用，未见 Bot 挑战。但大流量探测页面（如 speed.cloudflare.com 测速）浏览器内才方便。
- **统计**：Google Analytics `G-C8X91YXEH5` + Cloudflare Insights（TUI 无需实现）。

### 关键架构结论（对 TUI 最重要）
站点大部分"检测"其实是**前端纯逻辑 + 第三方 no-cors 资源探测**；自有后端只提供 4 类数据服务：
1. **geoip 缓存**（`/api/geoip`、`/api/geoip-batch`，按 /24 归并缓存）
2. **IP 风险评分聚合**（`/api/iprisk`、`/api/ip/lookup`、`/api/ipv2/*`）—— 字段形状与 **ipapi.is** 高度一致（`is_datacenter/isResidential/abuser_score/asn_kind/rpki_status`），响应里 `src: "g1"` 及 `geo_sources[{src: g1/g2/g3/g7}]` 表明后端聚合了多个上游 geo 库
3. **DNS 泄漏回读**（`/api/dns/result/{token}`，配合自有权威 DNS `*.d.ip.net.coffee`）
4. **全球拨测**（`/api/ping/*`、`/api/ip/portscan`、`/api/ip/pingcheck`，20 个自有节点）

---

## 2. 站点自有后端 API（全量）

### 2.1 GET `/api/geoip/{ip}` — 简单归属地（首页/各页通用）
- 参数：IPv4 或 IPv6，路径段。可带 `%3A` 编码的 IPv6。
- 缓存：`cache-control: public, max-age=3600`，`cdn-cache-control: max-age=86400`；服务端按 **/24 子网归并**（JS 内 `normalizeIp()` 把 IPv4 末段替换成 `.1` 再去重缓存）。非法 IP → `502`（Cloudflare 原样透传 `error code: 502`）。
- 响应样例（8.8.8.8）：
```json
{"country": "United States", "region": "", "city": "", "isp": "Google", "country_code": "us"}
```
- 字段：`country/region/city/isp`（拼接成归属地文案的顺序就是 JS 里的 `[country, region, city, isp].join(' ')`）、`country_code` 为小写 ISO-2。
- **curl 复放：✅ 可直接调用**。

### 2.2 GET `/api/geoip-batch?ips=a,b,c` — 批量归属地
- `ips` 逗号分隔。响应是 `{ip: geoObj}` 映射（IPv6 key 原样）。
```json
{"8.8.8.8": {"country": "United States", "region": "", "city": "", "isp": "Google", "country_code": "us"},
 "1.1.1.1": {"country": "Australia", "region": "Queensland", "city": "South Brisbane", "isp": "Cloudflare, Inc.", "country_code": "au"},
 "2606:4700:4700::1111": {"country": "United States", "region": "California", "city": "San Francisco", "isp": "Cloudflare, Inc.", "country_code": "us"}}
```
- 前端用 fetch 拦截器把 20ms 窗口内的多个 `/api/geoip` 合并成一次 batch，失败自动回退单查。TUI 可直接用 batch。
- **curl 复放：✅**。

### 2.3 GET `/api/iprisk/{ip}` — IP 风险/评分（Claude/GPT 页核心）
- 45s 内返回；服务端按 **/24（IPv6 为 /48）CIDR 聚合缓存**（响应头 `x-cache-cidr: 192.0.2.0/24`、`cache-control: public, max-age=600`）。注意：请求 `.179` 可能返回同段 `.12` 的数据（`ip` 字段是段代表 IP）。
- 样例（请求 203.0.113.179，返回段代表 203.0.113.12）：
```json
{"ip": "203.0.113.12", "cidr": "203.0.113.0/24", "is_datacenter": true, "isResidential": false, "isBroadcast": null,
 "is_vpn": false, "is_proxy": false, "is_tor": false, "is_crawler": false, "is_abuser": false, "is_mobile": false,
 "company_type": "hosting", "company_name": "Clean Pipe Networks LLC", "abuser_score": "0 (Very Low)",
 "datacenter_name": "Clean Pipe Networks LLC", "asn": 979, "asOrganization": "NetLab Global",
 "country": "United States", "countryCode": "us", "region": "California", "city": "Los Angeles",
 "timezone": "America/Los_Angeles", "src": "g1", "rdns": "", "rpki_status": "valid", "trust_score": 85}
```
- 字段含义：
  - `trust_score` 0-100 评分（**后端按 CIDR 计算，缓存 365 天**）。前端分档：≥95 极度纯净 / ≥80 纯净 / ≥50 良好 / ≥25 中性 / <25 可疑。
  - `is_datacenter/isResidential` 判定"机房/家宽"；`company_type ∈ isp|hosting|business|education`；
  - `abuser_score` 文本 `"0 (Very Low)"`；`asn_kind ∈ residential|hosting|cdn|mobile`；`rpki_status`；`src` = 主 geo 源代号（g1 为主库）。
  - **上游特征与 ipapi.is 完全一致**（含 `isBroadcast`、`company_type` 枚举），站点做了中转缓存。
- **curl 复放：✅**（无 Cookie/UA 要求）。

### 2.4 GET `/api/ip/lookup/{ip}` — IP 评分页主接口（深度聚合）
- `/ip/{ip}` 页面加载即调（45s 超时、`cache: no-store`；失败 15s 后自动重试一次）；`/cloudflare/` 页也用它做风险卡。
- 非法 IP → HTTP 400； bogon 返回 `is_bogon: true`。
- 完整响应样例（8.8.8.8，`related_domains` 截断至 4 条，原始 19 条）：
```json
{"ip": "8.8.8.8", "cidr": "8.8.8.0/24", "is_bogon": false, "is_datacenter": true, "isResidential": false,
 "is_vpn": false, "is_proxy": false, "is_tor": false, "is_crawler": false, "is_abuser": false, "is_mobile": false,
 "company_type": "hosting", "company_name": "Google LLC", "abuser_score": "", "datacenter_name": "",
 "asn": 15169, "asOrganization": "Google LLC", "country": "United States", "countryCode": "us", "region": "", "city": "",
 "src": "g1", "trust_score": 63, "rdns": "dns.google",
 "range": {"first": "8.8.8.0", "last": "8.8.8.255", "count": 256, "prefix": 24},
 "asn_tbps": "100+Tbps", "asn_ipv4_count": 4078592, "asn_kind": "hosting", "asn_allocated": "2000-03-30",
 "reddit_blocked": false,
 "ai_verdict": {"label": "公共 DNS 任播", "confidence": 100, "reasoning": "已识别为 Google Public DNS：全球任播部署，地理/威胁数据无法归因到单一节点，已屏蔽误导性标签。"},
 "geo_sources": [
   {"src": "g1", "country": "United States", "country_code": "us", "region": "", "city": "", "lat": null, "lon": null},
   {"src": "g2", "country": "United States", "country_code": "us", "region": "", "city": "", "lat": 37.751, "lon": -97.822, "accuracy_km": 1000},
   {"src": "g3", "country": "United States", "country_code": "us", "region": "California", "city": "Mountain View", "lat": 37.422, "lon": -122.085},
   {"src": "g7", "country": "United States", "country_code": "us", "region": "Virginia", "city": "Ashburn", "lat": 39.03, "lon": -77.5}],
 "intelligence": {"threats": [], "abuser_level": "safe", "abuser_score_raw": "", "rep_threat": null},
 "vpn_trace": null,
 "related_domains": [{"domain": "dns.google", "via": "reverse DNS"}, {"domain": "nginx.dev.funpresp.com.br", "via": "rapiddns reverse-IP"}],
 "location_history": [], "asname": "GOOGLE", "is_public_service": true,
 "public_service": {"service": "Google Public DNS", "operator": "Google", "service_type": "anycast_dns", "note": "全球任播部署", "ip": "8.8.8.8"},
 "asn_history": [], "company_history": [], "dc_neighbors": [],
 "registered_country_code": "us", "registered_country": "United States", "rpki_status": "valid", "isp": "Google"}
```
- 字段要点：
  - `geo_sources`：多源定位（g1/g2/g3/g7），地图页按 `g1 > g7 > g3 > g2` 优先取 `lat/lon` 画点。
  - `location_history`/`asn_history`：历史观测（元素含 `seen_at`(unix s)、`country/region/city/country_code`；ASN 史 `asn/asn_org`）。
  - `intelligence`：`abuser_score_raw`(0-1 小数)、`abuser_level ∈ safe|elevated|high|very_high`、`rep_threat`(httpBL 风险值)、`httpbl_threat`。
  - `ai_verdict`：服务端预生成的中文判词（置信度 confidence 0-100）。
  - `registered_country_*`：IP 注册国 vs `countryCode` 归属国 → 推导"原生 IP / 广播 IP"。
- 首次查询冷数据耗时 ~5-6s（`related_domains` 是后端异步扫完再给，见 2.5）。
- **curl 复放：✅**。

### 2.5 GET `/api/ip/related/{ip}` — 反向 DNS 邻居轮询
- `/api/ip/lookup` 返回里若 `related_domains` 还在扫描，前端每 1.5s 轮询本接口（≤10 次）。响应：
```json
{"ip": "8.8.8.8", "related_domains": [{"domain": "dns.google", "via": "reverse DNS"}, {"domain": "x.com", "via": "rapiddns reverse-IP"}]}
```
- 扫描中（未完成）返回 `{"pending": true, ...}`；`via ∈ reverse DNS | rapiddns reverse-IP`（上游可见：Reverse DNS + RapidDNS）。
- **curl 复放：✅**。

### 2.6 GET `/api/ipv2/*` — IP 评分页 v2 区块（5 个子端点）
全部 `✅ curl` 可直接调，IPv4/IPv6 均支持（除 heat/dnsbl 标注 IPv6 不支持，前端直接隐藏）：

| 端点 | 用途 | 响应要点 |
|---|---|---|
| `/api/ipv2/lookup/{ip}` | v2 兜底数据源 | 与 `/api/ip/lookup` 同 schema（前段缺数据时备用） |
| `/api/ipv2/heat/{ip}` | C 段热度趋势（近14天） | `{"supported":true,"base":"45.12.33","idx":6,"mode":"冷门","vals":[13,...],"days":["2026-09-16",...],"chg":3,"peak":{"i":1,"v":15,"day":"2026-09-17"}}`；mode ∈ 热门/活跃/正常/冷门 |
| `/api/ipv2/bgp/{ip}` | BGP 路由拓扑 | `{"prefix":"8.8.8.0/24","paths":374,"origins":[{asn,name,share,tier1}],"upstreams":[{asn,name,share,tier1}],"second":[{asn,name,share,tier1,via}],"snapshots":[{taken_at,upstreams,second}]}`（share 为路径占比 %；snapshots 为历史快照） |
| `/api/ipv2/dnsbl/{ip}` | DNSBL 黑名单（12 家并行） | `{"ip":"...","supported":true,"results":[{"engine":"Spamhaus ZEN","zone":"zen.spamhaus.org","category":"Spam","listed":false,"codes":[],"ms":104,"status":"nxdomain"}]}`；category ∈ Spam/Botnet/Malware/Reputation/Proxy-Tor；status ∈ ok/nxdomain/timeout/rcode2；Spamhaus 127.0.0.10/11 被 JS 视为 PBL"纯净" |
| `/api/ipv2/radar/{asn}` | Cloudflare Radar 人机流量（按 ASN，28d） | `{"asn":15169,"ok":true,"human":2.66,"bot":97.34,"range":"28d","cached":true,"stale":false}` |
| `/api/ipv2/asncos/{asn}` | 同 ASN 公司列表 | `{"asn":15169,"companies":[{"name":"Google LLC","type":"hosting","n":1467}],"total_profiles":1897}`；可能返回 `pending:true` 需 1.5s 轮询（≤12 次） |

- **curl 复放：✅**（bgp 实测 5.8s，冷查询慢）。

### 2.7 GET `/api/dns/result/{token}` — DNS 泄漏回读
- `token` 为前端随机串 `[a-z0-9]{24}`（两次 `Math.random().toString(36).slice(2)` 拼接）。响应：
```json
{"token": "sampledns0123456789ab", "dns_servers": ["192.0.2.1", "192.0.2.2", "192.0.2.3", "192.0.2.4",
 "192.0.2.5", "192.0.2.6", "192.0.2.7", "192.0.2.8", "192.0.2.9", "192.0.2.10",
 "192.0.2.11", "192.0.2.12", "192.0.2.13", "192.0.2.14", "192.0.2.15"]}
```
- 未知 token 也返回 200 + 空 `dns_servers`（可放心轮询）。
- **TUI 实现关键**：token 无需注册/签名，**但必须先用真实 DNS 查询 `<token>-<n>.d.ip.net.coffee`（n=1..N）触发解析**，服务器端权威 DNS 才会把"来查的 resolver IP"挂到该 token 上。原生 TUI 直接对随机子域名做 UDP 53 的 A 查询即可（**比浏览器更简单，不需要 pixel.gif**）；随后轮询本接口。泄漏判定见 §3.6。
- **curl 复放：✅**（回读部分；触发部分 TUI 用 DNS 客户端实现）。

### 2.8 全球拨测三件套

**GET `/api/ping/global?host={host}&node=n01&node=n02...`** — 同步版多节点 ping（IP 评分页小卡片用，8 节点：n01,n02,n03,n04,n09,n11,n13,n15）
- 8s 超时；实测冷查询 11.7s。响应：
```json
{"cached": false, "results": {"n03": 1, "n01": 150, "n02": 1}, "timeouts": []}
```
- `results[nodeId]` 为整数 ms；不在 results 也不在 timeouts = 超时。

**GET `/api/ping/start?host={host}&user_ip={ip}&node=n01&...&force=1`** — 异步版（/ping/ 页用，最多 20 节点）
- `user_ip` 来自浏览器先请求 `1.1.1.1/cdn-cgi/trace` 的 `ip=`；`force=1` 跳过缓存。
- 响应：`{"request_id": "sampleping01", "cached": false}`；host 为 IP 时前端把 URL 改写为 `/ping/{host}`。
- 轮询 **GET `/api/ping/result/{request_id}`**（每 1.5s 直到非 pending）：
```json
{"n09": [[["OK", 0.0017399787902832, "1.1.1.1"], ["OK", 0.00189709663391113], ["OK", 0.00213003158569336], ["OK", 0.00445699691772461]]],
 "n01": [[["OK", 0.0389, "1.1.1.1"], ["OK", 0.0385], ["OK", 0.037899999999999996]]]}
```
- 每节点为"轮次数组"：`[[sample,...]]`，sample = `["OK", 延迟秒, 目标]`（**秒为单位，前端 ×1000 取 ms**）；失败项形如 `["TIMEOUT",...]`/`["ERROR",...]`。页面取最小值展示。
- 节点表（20 个，`NODES`）：`n01` 中国上海、`n02` 中国香港、`n03` 日本东京、`n04` 新加坡、`n05` 越南胡志明、`n06` 印尼雅加达、`n07` 印度孟买、`n08` 以色列特拉维夫、`n09` 美国洛杉矶、`n10` 美国亚特兰大、`n11` 加拿大温哥华、`n12` 巴西圣保罗、`n13` 德国法兰克福、`n14` 荷兰阿姆斯特丹、`n15` 法国巴黎、`n16` 瑞典斯德哥尔摩、`n17` 瑞士苏黎世、`n18` 西班牙马德里、`n19` 俄罗斯莫斯科、`n20` 土耳其伊斯坦布尔。
- **curl 复放：✅**（start→result 轮询模型对 TUI 天然友好）。

**GET `/api/ip/portscan/{ip}[?force=1|?probe=0]`** — 全球多节点 TCP 端口扫描（评分页）
- `probe=0` 只读缓存不触发扫描；`force=1` 强制重扫。**限流：24h 内 >20 次返回 HTTP 429**：
  - 冷却中：`{"error": "cooldown", "remaining_s": 37}`（429）
  - 配额尽：`{"error": "rate_limited", "limit": 20}`（429）
- 成功响应（8.8.8.8）：
```json
{"ok": true, "ip": "8.8.8.8", "ports": {"443": "open", "80": "closed", "8443": "closed", "22": "closed", "8080": "closed", "25": "closed"},
 "age_s": 83310, "scanned_at": 1790703840, "cooldown_sec": 60}
```
- 扫描端口固定 6 个：22/25/80/443/8080/8443。
**GET `/api/ip/pingcheck/{ip}`** — 判断 IP 是否可 ping（19 节点投票）：
```json
{"ok": true, "ip": "1.1.1.1", "verdict": "reachable", "reachable": true, "ok_nodes": 19, "timeout_nodes": 0, "pending_nodes": 0,
 "total_nodes": 19, "ok_ratio": 1.0, "timeout_ratio": 0.0, "ok_threshold": 0.4, "timeout_threshold": 0.6,
 "src": "", "scan_all_closed": false, "marked_dead": false}
```
- `verdict ∈ reachable|unreachable|inconclusive`；`marked_dead=true` 表示已从"同机房列表"剔除。
- **curl 复放：✅**。

### 2.9 POST `/api/session` — 分享/分析上报（fire-and-forget）
```json
// POST https://ip.net.coffee/api/session   Content-Type: application/json
{"claude_ip": "198.51.100.32", "cf_ip": "198.51.100.32", "cn_ip": "192.0.2.216"}
// 响应 {"ok": true, "new": true}
```
- Claude/GPT 页加载完成后静默上报。TUI 可忽略或采用。**curl 复放：✅**。

### 2.10 GET `/claude/status.json`、`/gpt/status.json` — 服务状态
- 静态 JSON（gpt 页带 `?t=时间戳` 防缓存）。schema：
```json
{"overall": "全部正常", "overall_indicator": "none",
 "components": [{"name": "claude.ai", "status": "operational", "status_cn": "正常运行"}],
 "incidents": [{"name": "...", "name_cn": "...", "status": "resolved", "impact": "major", "created_at": "2026-09-29T14:21:37.188Z", "updated_at": "..."}],
 "fetched_at": "2026-09-30T16:49:32+00:00"}
```
- `overall_indicator ∈ none|minor|major|critical|maintenance`；映射：none=全部服务正常，minor=轻微故障，major=重大故障，critical=严重故障，maintenance=维护中。claude 组件含 claude.ai / Claude Console / Claude API / Claude Code；gpt 组件含 Codex CLI / ChatGPT 对话 / ChatGPT 登录。数据疑似镜像 Anthropic/OpenAI 官方 status API 的中文转写。
- **curl 复放：✅**。

### 2.11 GET `/api/captcha/config`、POST `/api/captcha/verify` — Cloudflare 页人机验证
- config：`{"turnstile": {"managed": "0x4AAAAAAE2X1UT7jnAphqRG", "non-interactive": "0x4AAAAAAE2X1gCtY8fcK272"}, "recaptcha": {"sitekey": ""}}`
- verify（浏览器拿 Turnstile token 后服务端 siteverify 代理）：
```json
// POST /api/captcha/verify  {"provider":"turnstile","mode":"non-interactive|managed","token":"<turnstile token>"}
{"ok": true, "provider": "turnstile", "mode": "non-interactive", "success": false, "elapsed_ms": 28,
 "hostname": null, "challenge_ts": null, "error_codes": ["invalid-input-response"], "score": null, "action": null, "cdata": null}
```
- **TUI 无法实现**（Turnstile 需要真实浏览器环境跑 JS 挑战），只能丢弃该子功能或提示跳转网页。
- **curl 复放：✅ 接口本身可调（但拿到 success=true 需浏览器 Turnstile token）**。

### 2.12 静态资源
- `/ip/land110.txt`：世界地图 SVG path 数据（59KB，地图底图）。`/favicons/*` 站点图标。**curl ✅**。

---

## 3. 各功能页：UI 流程 → 接口 → 判定逻辑

### 3.1 首页 `/`（IP 查询 + 分流测试）
**A. 我的 IP**：并发请求两个国内源 →
- GET `https://2026.ip138.com/`（8s 超时）：HTML `<title>您的IP地址是：192.0.2.160</title>`，正则 `(\d{1,3}\.){3}\d{1,3}` + `来自：([^<\n]+)`
- GET `https://my.ip.cn/`（8s 超时）：纯文本 `ip：192.0.2.216 归属地：中国 示例省份 示例城市  示例运营商`，正则同上 + `归属地：(.+)`
- 两者 IP 不同则显示两张卡（主 + "备用出口"）。归属地与 `/api/geoip/{ip}` 的 country_code 交叉校验（内置约 50 国中文前缀表 `CC_TO_CN_PREFIX`），不一致时以 `CC_TO_CN_NAME` + geoip 的 region/city/isp 重拼中文归属地，来源标记"GeoIP（地区库纠正）"。
- 再调 `/api/iprisk/{ip}` 显示"家庭宽带/机房IP/商业专线/专用(Education)"标签。

**B. 网络连通性（6 目标）**：`fetch(url, {mode:'no-cors', cache:'no-store', signal: 2.5s})` 计时 = HTTP 完成时间。
1 次隐藏预热 + **12 轮测量取中位数**，轮间隔 ≥80ms。目标：
| 名称 | URL |
|---|---|
| 字节跳动（国内） | `https://perfops.byte-test.com/500b-bench.jpg` |
| 淘宝（国内） | `https://www.taobao.com/favicon.ico`（302→gw.alicdn.com） |
| 微信（国内） | `https://res.wx.qq.com/a/wx_fed/assets/res/NTI4MWU5.ico` |
| GitHub（国际） | `https://github.com/generate_204`（实测返回 404 也能计时） |
| Cloudflare（国际） | `https://1.1.1.1/cdn-cgi/trace` |
| YouTube（国际） | `https://www.youtube.com/generate_204`（204） |

分档：<100ms 绿 / <400ms 浅绿 / 其余黄 / 失败"超时"。

**C. 网站分流测试（37 站）**——出口 IP 探测的 4 种手法（TUI 全部可用原生 TCP 实现）：
1. `cftrace`（33 站）：GET `https://{domain}/cdn-cgi/trace`（cors 模式，5s 超时），解析 `ip=`、`loc=`。域名清单：`www.cloudflare-cn.com`、`www.qualcomm.cn`、`gateway.discord.gg`、`x.com`、`medium.com`、`signal.org`、`anthropic.com`、`claude.ai`、`chatgpt.com`、`openai.com`、`sora.com`、`grok.com`、`pixpix.com`、`www.perplexity.ai`、`midjourney.com`、`mistral.ai`、`coinbase.com`、`www.okx.com`、`www.binance.info`、`crypto.com`、`zoom.us`、`1password.com`、`wise.com`、`poe.com`、`notion.so`、`shopify.com`、`godaddy.com`、`producthunt.com`、`www.cloudflare.com`、`cdnjs.cloudflare.com`、`registry.npmjs.org`、`kali.download`、`unpkg.com`、`nodejs.org`、`gitlab.com`、`crunchyroll.com`
2. `netease`（网易）：**HEAD** `https://necaptcha.nosdn.127.net/ab7f4275c1744aa28e0a8f3a1c58c532.png` → 响应头 **`cdn-user-ip`** 即出口 IP（curl 已验证返回 `cdn-user-ip: 192.0.2.216`）
3. `bytedance`（字节）：**HEAD** `https://perfops.byte-test.com/500b-bench.jpg` → 头 **`x-request-ip`** 或 **`x-response-cinfo`**（curl 验证：`X-Request-Ip: 10.0.0.216`（内网大 NAT）、`X-Response-Cinfo: 198.51.100.176`）
4. `ip138`（备用分支）
- 每个拿到的 IP → `/api/geoip/{ip}` 补国旗/归属地；并发 12、失败重试 2 次（间隔 2s/4s）；汇总去重生成"分流出口 IP 汇总"。
- **TUI 建议**：`cftrace` 直接 GET；HEAD 头探测用原生 HTTP 即可；无需浏览器。

### 3.2 Claude AI IP 检测 `/claude/`
**流程**（全并行）：
1. 三个 IP 卡并行：
   - Cloudflare IP：GET `https://1.1.1.1/cdn-cgi/trace`（5s）→ `ip=`
   - 国内 IP：ip138 → my.ip.cn（各 5s）
   - **Claude 出口 IP：GET `https://claude.ai/cdn-cgi/trace`（8s，cors 模式）→ `ip=` + `loc=`**（这就是"Claude 看到的你的 IP"）
2. Claude IP → 并行 GET `/api/iprisk/{claude_ip}`（10s）+ `/api/geoip/{claude_ip}`（5s）。IPv6 拿不到 geo 时用 trace 的 `loc` 兜底（内置 14 国映射表）。
3. **信任分**：`trust_score` 来自 `/api/iprisk`；**受限地区强制 0 分**——`CLAUDE_RESTRICTED_CC = {CN 中国大陆, HK 香港, MO 澳门, RU 俄罗斯, KP 朝鲜, IR 伊朗, SY 叙利亚, CU 古巴, BY 白俄罗斯, VE 委内瑞拉}`，显示"不可访问"+红色警告"不建议尝试登录 Claude"。
4. **可用性探测**（`detectClaudeAvail`）：`fetch(url, {mode:'no-cors', signal:6s})` 计时：
   - `https://claude.ai/cdn-cgi/trace`、`https://www.anthropic.com/cdn-cgi/trace`
   - ok → <250ms 正常 / <500ms 良好 / 其余较慢；异常 → 不可访问。受限地区直接覆盖为"不可访问"（不显示时延）。
   - 另拉 `/claude/status.json` 显示"Claude服务状态"行。
5. **DNS 泄漏检测**（内嵌）：token=随机24位 → 循环 2 次 `new Image().src = https://{token}-{i}.d.ip.net.coffee/pixel.gif?_={ts}`（每轮 2s 超时）→ 等 1.5s → GET `/api/dns/result/{token}`（3s，最多 2 次）→ 对每个 resolver IP 查 `/api/geoip`。判定：**任一 DNS 出口 country_code==cn 且 Claude 出口非中国 → "可能泄露"（中国DNS）**；`dns_servers` 为空 → "DNS 加密或未暴露出口"。
6. **WebRTC UDP 泄漏**（内嵌）：RTCPeerConnection + STUN `stun:stun.l.google.com:19302`、`stun:stun.cloudflare.com:3478`（5s 收集 ICE candidate，正则抽 IPv4/IPv6，过滤私网 192.168./10./172./198.18./198.19./100.64./127./0.）。判定：**任一公网 UDP IP ≠ Claude 出口 IP → 可能泄露**。
7. 设备信息（时区/语言/WebGL/Canvas 指纹）纯前端；POST `/api/session` 上报；localStorage 存最近 6 条 Claude IP 历史（同 IP 24h 内不重复记录）。
- **TUI 建议**：无浏览器无法跑 WebRTC，其余全部可实现（trace + 2 个 API + DNS 探测用 UDP 53 直查随机子域）。

#### LinkLens 请求边界

上面的秒数是上游页面设置。LinkLens 的通用公网客户端采用 8 秒，[iprisk 请求](../../src/net/iprisk.rs) 显式覆盖为 10 秒；geoip 使用通用 8 秒，而上游 AI 页为 5 秒。可用性采用原生 TCP/TLS 测量，单次总边界为 8 秒，不能把端点调查的返回耗时当成超时设置。编排与平台 profile 见 [probe_ai.rs](../../src/probe_ai.rs)，默认值见 [http.rs](../../src/net/http.rs)，用户可见差异见 [README](../../README.md)。GPT 共用此边界；独立 DNS 页采用值见 §3.6。

### 3.3 IP 评分 `/ip/` 与 `/ip/{ip}`

#### 上游请求与轮询

- 当前 IP 自动检测：GET **同源** `https://ip.net.coffee/cdn-cgi/trace`（`/cdn-cgi/trace` 相对路径）取 `ip=`；搜索仅接受合法 IPv4/IPv6，跳转 `/ip/{ip}`（IPv6 URL encode）。
- 主数据：GET `/api/ip/lookup/{ip}`，页面请求时限 45 秒；400 不重试，其余失败等待 15 秒后重试一次。主接口以 `related_domains_pending` 表示反查未完成，轮询 `/api/ip/related/{ip}` 的响应使用 `pending`。
- 主接口返回后请求 v2 增强（字段见 §2.6）：heat/DNSBL/同 ASN 公司各 15 秒，BGP 22 秒，Radar 14 秒。以上为页面设置，端点调查中一次返回的耗时不能替代请求边界。
- 端口扫描、可 Ping、全球延迟见 §2.8；Shodan/AbuseIPDB/VirusTotal/bgp.tools/ipinfo/Spamhaus/ipdata/ip2location/Scamalytics 为外部搜索深链。
- Bogon（非公网地址）主页面提前显示 `bogon_reason`/`bogon_rfc`，v2 也停止增强，接口高信任分不能解释为可用公网出口。上游评分页还隐藏公共服务 IP 的 v2 增强卡。

#### 场景评分算法

来源为 2026-10-01 核验的 [IP 评分主脚本](https://ip.net.coffee/ip/ip-page.js) 的 `sceneScores()`。按以下顺序处理：

1. **分类与 base**：`base = floor(trust_score / 10 + 0.5)`。机房包括 `is_datacenter`、`company_type=hosting`、`asn_kind=hosting/cdn`。归属国与注册国均非空时，相同为原生、不同为广播；任一为空时两者均不成立。
2. **属性加减**：原生 +0.5；非机房、非爬虫且非公共服务 +0.5；商业网络 -0.5；广播 -1；机房在 AI 场景 -2、其他场景 -3。
3. **风险加减**：proxy/vpn/tor 命中任一，AI -2、其他 -3；历史滥用 AI -0.5、其他 -1；爬虫 -0.5。滥用原始值优先取 `intelligence.abuser_score_raw`，缺失时取 `abuser_score`，按浮点前缀解析：>0.05 扣 2、>0.025 扣 1、>0.01 扣 0.5，其余扣 0；不能解析时才按 `abuser_level` 的 high/very_high/veryhigh 扣 2、elevated 扣 1。蜜罐优先取非空 `rep_threat`，否则 `httpbl_threat`；>25 扣 2、>0 扣 1，其余扣 0。
4. **最终取整与风险上限**：对加减后的值再次 `floor(x + 0.5)`，限制在 0–10；历史滥用、正数滥用扣分、正数蜜罐扣分、爬虫或代理/VPN/Tor 任一成立时，`risky` 上限为 9。机房、商业网络、广播属性本身不触发上限。
5. **地区门槛与档位**：block 归 0 分「地区不可用」，partial 封顶 5 分「部分可用」。地区表如下；其余按 ≥10「极佳」、≥8「推荐」、≥5「可用」、<5「不推荐」展示。上游 AI 场景 5–7 分的短文案为「可以尝试」，副提示为「GPT和Gemini可用，Claude不建议使用」。

| 场景 | block | partial |
| --- | --- | --- |
| TikTok | CN、HK、IN、IR、AF、KP、JO、SO、SN、KG、UZ | 无 |
| 社媒 | CN、IR、KP、TM | RU、MM |
| AI | CN、RU、BY、IR、KP、CU、SY、AF | HK、MO、VE、MM |

#### Radar 人机比

来源为同日核验的 [v2 脚本](https://ip.net.coffee/ip/ip-page-v2.js)。读取 `/api/ipv2/radar/{asn}` 的 `human`；无数据时按公共服务、爬虫、移动网络、机房、商业网络、其余网络的顺序估算，分别为 2/6/93/18/65/88%。实际值与估算值都按 Tor 乘 0.4，否则代理/VPN 乘 0.6；爬虫取 `min(h, h×0.3+2)`，历史滥用再减 5，最后限制在 1–99%。界面区分统计值与估算值。

#### LinkLens 采用值与验证入口

- 场景数值、取整、风险上限、地区门槛及 Radar 算法采用上述规则；AI 5–7 分短文案采用「可用」，保留上述副提示。纯函数与属性、阈值、取整、风险封顶和地区测试见 [scene.rs](../../src/detect/scene.rs)，解析见 [ip_score.rs](../../src/net/ip_score.rs)。
- 主查询与 v2 请求时限按本节采用；请求编排与重试入口见 [probe_score.rs](../../src/probe_score.rs)。同 ASN 公司总请求数最多 12 次，上游为首轮加 12 次重试；反查最多 10 次，两类 pending 请求间隔均为 1.5 秒。
- Bogon 停止增强并显示非公网依据；合法公共服务 IP 展示完整增强资料。页面入口见 [评分页](../../src/ui/pages/ip_score.rs)。
- curl 可复放接口请求，场景评分与 Radar 折减属于前端算法；网络冒烟和纯函数边界验证分别记录。

来源快照：主脚本 SHA-256 为 `d74fb71b553e1aabe3167cfcd3a3dd3c4eb3b6e773f171d65dd35ffdc07db15a`，v2 脚本为 `63e53102af14a256a74930d1a6dacc20137d07ee52f11efb4464d2996e7d76d8`。

### 3.4 GPT 检测 `/gpt/`
与 Claude 页**完全同构**，仅 3 处不同：
1. 出口 IP：GET `https://chatgpt.com/cdn-cgi/trace`（8s）
2. 受限表同名 `OPENAI_RESTRICTED_CC`：**与 Claude 完全相同的 10 个地区**（注释称按 OpenAI 官方支持清单校准；TW/KR/SG/JP/UA 等常见代理出口不在受限表）
3. 可用性探测目标：`https://chatgpt.com/cdn-cgi/trace` + **`https://api.openai.com/`**（api.openai.com 是 Codex/API 链路提示）；服务状态取 `/gpt/status.json?t={ts}`。
- 其余（iprisk/geoip/DNS/WebRTC/session 上报）与 Claude 页一字不差。
- **结论：GPT/Claude "检测"没有调用任何 Anthropic/OpenAI 后端做验证**，只是"trace 拿出口 IP + 自有风险库打分 + 地区硬表 + no-cors 连通性"，TUI 完全可实现。

### 3.5 网络连通 `/link/`（47 目标）
- 手法同首页 ping：`no-cors fetch` + 2.5s 超时，1 次预热 + **8 轮取中位数**，轮间隔 ≥90ms + 0-140ms 抖动，**并发池 9**。分组：cn 12 站 / jp 6 站 / us 17 站 / 全球 12 站。
- 目标 URL 全录（节选特殊的，其余为 `https://{host}/favicon.ico`）：
  - 特殊：抖音 `https://lf3-static.bytednsdoc.com/obj/eden-cn/favicon.ico`；微信 `res.wx.qq.com/.../NTI4MWU5.ico`；小红书 `https://fe-static.xhscdn.com/favicon.ico`；微博 `https://tva1.sinaimg.cn/favicon.ico`；Google/YouTube/GitHub `generate_204`；Cloudflare `https://1.1.1.1/cdn-cgi/trace`；**Claude `https://api.anthropic.com/favicon.ico`**；ChatGPT `https://chatgpt.com/cdn-cgi/trace`；AI Studio `https://generativelanguage.googleapis.com/favicon.ico`；Bing `www.bing.com/favicon.ico`；Zoom `st1.zoom.us/favicon.ico`；Facebook/Instagram `static.xx.fbcdn.net|static.cdninstagram.com/rsrc.php/yb/r/hLRJ1GG_y0J.ico`；X `abs.twimg.com/favicons/twitter.3.ico`；LinkedIn `static.licdn.com/favicon.ico`；Twitch `static.twitchcdn.net/assets/favicon-32-e29e246c157142c94346.png`；Netflix `assets.nflxext.com/us/ffe/siteui/common/icons/nficon2016.ico`；TikTok `www.tiktok.com/favicon.ico`；Spotify `open.spotify.com/favicon.ico`；npm `https://registry.npmjs.org/`；Yandex `yastatic.net/favicon.ico`；MercadoLibre `http2.mlstatic.com/favicon.ico`。
  - 普通 host（拼 `/favicon.ico`）：`www.deepseek.com, www.bilibili.com, www.jd.com, www.qq.com, weibo.com, www.baidu.com, www.163.com, www.taobao.com, www.mi.com, www.sony.jp, www.nintendo.co.jp, www.yahoo.co.jp, line.me, www.apple.com, www.amazon.com, store.steampowered.com, www.oracle.com, www.reddit.com, www.twitch.tv, www.netflix.com, static.takealot.com, www.pixpix.com, www.naver.com, www.noon.com, www.wikipedia.org, www.bbc.com, mistral.ai`。
- 小组汇总 = 组内中位数平均 + 可达数。
- **TUI 建议**：原生 HTTP HEAD/GET + 计时即可，无需浏览器。❌站点无后端参与。

### 3.6 DNS 泄漏 `/dns/`（独立页，实测全流程）
1. GET `https://1.1.1.1/cdn-cgi/trace` 拿本机出口 IP → `/api/geoip/{ip}` 拿国家。
2. 选"快速测试"= 5 轮 / "深度测试" = 8 轮；每轮 `new Image().src = https://{token}-{i}.d.ip.net.coffee/pixel.gif?_={ts}`（3s 超时），轮间隔 600ms。
3. 等 2s → GET `/api/dns/result/{token}`（5s，最多 3 次轮询）。
4. 每个 `dns_servers[]` IP → `/api/geoip/{ip}`。
5. **判定**：`results` 为空 → "未检测到 DNS 解析器出口 IP，你的 DNS 可能已加密 (DoH/DoT)"；存在 `countryCode=='cn'` 的 resolver 且用户出口非 cn → "⚠️ 检测到 DNS 泄露！暴露了中国大陆的 DNS 服务器"；否则"完美！未检测到 DNS 泄露"。
- 实测样例：token `sampledns0123456789ab`，5 轮后回读 15 个 resolver IP（含 多个解析器与转发节点，具体网络地址已脱敏 —— 说明本地 DNS 走了 CF/公共 DNS 的多级转发）。
- 域名模式：`<token>-<n>.d.ip.net.coffee`，`*.d.ip.net.coffee` 是泛解析（权威 NS 为站点自建，pixel.gif 由 nginx 返回 42B gif）。**TUI**：直接对 `<token>-1..5.d.ip.net.coffee` 发 UDP 53 A 查询（系统 resolver 或指定 resolver），等 2-3s 后 curl 回读。✅接口已验证。
- 补充：`/dns/` 页面顶部注释还提到备用方案 "Query o-o.myaddr.l.google.com TXT via multiple DoH providers"（实际代码未启用，可作 TUI 的跨验证手段）。

#### LinkLens 采用值与验证入口

LinkLens 通过系统 resolver 对随机子域发起原生 DNS 查询；快速 5 轮、深度 8 轮，等待 2 秒后回读，最多 3 次轮询、轮询间隔 2 秒。结果回读显式采用 5 秒请求时限，覆盖通用客户端 8 秒默认值；resolver 查询仍采用通用 8 秒边界。此处针对独立 DNS 页，上游 AI 页内嵌 DNS 的 3 秒回读属于另一流程。

token、回读解析、轮数与等待常量及其纯函数测试见 [dnsleak.rs](../../src/net/dnsleak.rs)；编排与默认忽略的真实网络冒烟见 [probe_leak.rs](../../src/probe_leak.rs)，三态判定及测试见 [leak.rs](../../src/detect/leak.rs)。结果为空与 HTTP/解析失败分别处理，不能把失败解释为已加密；真实网络结果受运行环境影响。

### 3.7 WebRTC `/webrtc/`（独立页，❌ 纯浏览器能力）
- STUN 列表（**共 3 个**）：`stun:stun.l.google.com:19302`、`stun:stun.cloudflare.com:3478`、`stun:stun1.l.google.com:19302`（无 TURN）。
- RTCPeerConnection + createDataChannel + createOffer/setLocalDescription，收集 ICE candidate 6s；正则抽 IPv4（过滤 `0./127./10./172./192.168./198.18./198.19./100.64.`）与 IPv6（过滤 `::1`、`fe80`）；类型：含 `srflx` → "公网 (STUN)"、`relay` → "中继 (TURN)"、否则"本地"。
- 每个 IP → `/api/geoip/{ip}`；**判定：多个不同公网 STUN IP → 可能泄露；0 个 → "WebRTC 已禁用或未暴露"**（并提示代理模式 UDP 不通、TUN 模式才准）。
- 参照系：加载时即请求 1.1.1.1 trace + `/api/geoip` 显示"当前 TCP 出口"。
- **TUI**：Rust 无浏览器 WebRTC，可用 `webrtc-rs` 自建 PeerConnection 走 STUN binding request 实现（等价于向 3 个 STUN 服务器发 Binding Request 读 XOR-MAPPED-ADDRESS），或标注"需自行实现 STUN 探测"。

### 3.8 Cloudflare `/cloudflare/`（附加页）
- **trace 组**：`https://{domain}/cdn-cgi/trace`，`timedTrace` = 预热 1 次 + 3 次取最小 ms；对比表 8 目标：`ip.net.coffee`(免费)、`www.cloudflare.com`、`openai.com`、`gateway.discord.gg`(企业)、`1.1.1.1`、`speed.cloudflare.com`、`cdnjs.cloudflare.com`、`workers.cloudflare.com`；解析 `ip/colo/loc/warp/http/tls`。
- **测速（2026-09 v2，浏览器直连 Cloudflare）**：下载 GET `https://speed.cloudflare.com/__down?bytes={N}`，N = 1M 预热 → 10M/25M/64M/200M（累计≥10s 停）；上传 POST `https://speed.cloudflare.com/__up`（body 为 Blob），1M 预热 → 5M/10M/34M（≥8s 停）；RTT 取响应头 `server-timing: rtt=`（ms→/1000 显示），colo 取响应头 `cf-meta-colo`。速率算法：每级去掉前 15% 慢启动样本求平均，各级取最大。
- **人机验证**：`/api/captcha/config` 拿 sitekey → 加载 `https://challenges.cloudflare.com/turnstile/v0/api.js`（managed + non-interactive 两模式）+ reCAPTCHA v3 备用（`https://www.google.com/recaptcha/api.js?render={key}`）→ token POST `/api/captcha/verify` 服务端 siteverify。
- 风险卡：`/api/geoip/{ip}` + `/api/ip/lookup/{ip}`；WARP 开启 risk+10；历史记录 localStorage。
- **TUI**：trace/测速（直接 HTTP 流式读）✅；Turnstile ❌（需浏览器）。

### 3.9 全球 Ping `/ping/`（附加页）
- 输入 host（自动 `1.1.1.1/cdn-cgi/trace` 预填）→ GET `/api/ping/start?host=&user_ip=&node=n01..n20&force=1` → 每 1.5s 轮询 `/api/ping/result/{request_id}` → 每节点取样本最小值（样本为秒，×1000）。分档 <100/<200/<350 ms。
- **curl ✅**。

---

## 4. 端点汇总表

| # | Method | URL | 用途 | TUI 可用性 |
|---|---|---|---|---|
| 1 | GET | `https://ip.net.coffee/api/geoip/{ip}` | 归属地缓存 | ✅ curl 直调 |
| 2 | GET | `https://ip.net.coffee/api/geoip-batch?ips=a,b` | 批量归属地 | ✅ |
| 3 | GET | `https://ip.net.coffee/api/iprisk/{ip}` | 风险分/家宽机房（/24 缓存） | ✅ |
| 4 | GET | `https://ip.net.coffee/api/ip/lookup/{ip}` | 深度聚合（评分页主数据） | ✅ |
| 5 | GET | `https://ip.net.coffee/api/ip/related/{ip}` | 反向DNS邻居轮询 | ✅ |
| 6 | GET | `https://ip.net.coffee/api/ipv2/lookup/{ip}` | v2 兜底数据 | ✅ |
| 7 | GET | `https://ip.net.coffee/api/ipv2/heat/{ip}` | C段热度（IPv4） | ✅ |
| 8 | GET | `https://ip.net.coffee/api/ipv2/bgp/{ip}` | BGP 拓扑 | ✅（慢 ~6s） |
| 9 | GET | `https://ip.net.coffee/api/ipv2/dnsbl/{ip}` | 12 家 DNSBL（IPv4） | ✅ |
| 10 | GET | `https://ip.net.coffee/api/ipv2/radar/{asn}` | CF Radar 人机比 | ✅ |
| 11 | GET | `https://ip.net.coffee/api/ipv2/asncos/{asn}` | 同 ASN 公司（可能 pending 轮询） | ✅ |
| 12 | GET | `https://ip.net.coffee/api/dns/result/{token}` | DNS 泄漏回读 | ✅（需先 DNS 触发） |
| — | (DNS) | `https://{token}-{n}.d.ip.net.coffee/pixel.gif` | DNS 触发（泛解析） | ⚠️ TUI 直接发 UDP53 A 查询替代 |
| 13 | GET | `https://ip.net.coffee/api/ping/global?host=&node=` | 同步 8 节点 ping | ✅（冷查询 ~12s） |
| 14 | GET | `https://ip.net.coffee/api/ping/start?host=&user_ip=&node=&force=1` | 异步拨测启动 | ✅ |
| 15 | GET | `https://ip.net.coffee/api/ping/result/{request_id}` | 拨测结果轮询 | ✅ |
| 16 | GET | `https://ip.net.coffee/api/ip/portscan/{ip}?probe=0|force=1` | 端口扫描（429 限流：24h 20 次/冷却 60s） | ✅ |
| 17 | GET | `https://ip.net.coffee/api/ip/pingcheck/{ip}` | 可 ping 投票 | ✅ |
| 18 | POST | `https://ip.net.coffee/api/session` | 会话上报 | ✅（可忽略） |
| 19 | GET | `https://ip.net.coffee/claude/status.json` `/gpt/status.json` | 服务状态 | ✅ |
| 20 | GET | `https://ip.net.coffee/api/captcha/config` | Turnstile sitekey | ✅（TUI 用不上） |
| 21 | POST | `https://ip.net.coffee/api/captcha/verify` | Turnstile 校验代理 | ⚠️ 需浏览器 token |
| 22 | GET | `https://ip.net.coffee/cdn-cgi/trace` | 本站出口 IP（CF 标准 trace） | ✅ |
| 23 | GET | `https://1.1.1.1/cdn-cgi/trace`（及其他 40+ 域名 `/cdn-cgi/trace`） | 各站点出口 IP | ✅ |
| 24 | GET/HEAD | `https://2026.ip138.com/`、`https://my.ip.cn/` | 国内直连出口 IP | ✅（curl 已验证响应格式） |
| 25 | HEAD | `https://necaptcha.nosdn.127.net/ab7f4275c1744aa28e0a8f3a1c58c532.png` | 网易 CDN 头 `cdn-user-ip` | ✅ |
| 26 | HEAD | `https://perfops.byte-test.com/500b-bench.jpg` | 字节 CDN 头 `x-request-ip`/`x-response-cinfo` | ✅ |
| 27 | GET | `https://{rand}.dns-detect.alicdn.com/api/detect/DescribeDNSLookup?cb=cb` | 阿里 JSONP：localIp+LDNS+运营商（首页未用但接口存活，curl 验证） | ✅ |
| 28 | GET/POST | `https://speed.cloudflare.com/__down?bytes=N`、`/__up` | CF 测速 | ✅（TUI 流式读） |
| 29 | GET | `https://api.ip.sb/geoip/{ip}`、`https://ipwho.is/{ip}` | geoip 兜底（站点后端挂时） | ✅ |
| 30 | STUN | `stun:stun.l.google.com:19302`、`stun:stun.cloudflare.com:3478`、`stun:stun1.l.google.com:19302` | WebRTC | ❌ 需自实现 STUN 客户端 |
| 31 | GET | `https://registry.npmjs.org/` 等 47 个连通目标 | no-cors 计时 | ✅ |
| 32 | (JS) | `https://challenges.cloudflare.com/turnstile/...`、`google.com/recaptcha` | 人机验证 | ❌ |

---

## 5. curl 可用性验证结论

所有 `ip.net.coffee/api/*` 与静态 JSON 用 `curl -A "Mozilla/5.0"`（甚至无 UA）直接 200：
- `geoip`、`geoip-batch`、`iprisk`、`ip/lookup`、`ip/related`、`ipv2/*`（6 个）、`dns/result`、`ping/global`、`ping/start`+`result`、`ip/portscan`、`ip/pingcheck`、`session`(POST)、`claude|gpt/status.json`、`captcha/config`、`captcha/verify`(POST)、`ip/land110.txt` —— **全部 ✅**。
- 通用响应头：`access-control-allow-origin: *`、`server: cloudflare`；geoip 缓存 1h、iprisk 10min/按 CIDR；无 Cookie/CSRF 要求；POST `/api/session`、`/api/captcha/verify` 直接成功。
- 第三方：`2026.ip138.com`、`my.ip.cn`、`necaptcha.nosdn.127.net`(cdn-user-ip)、`perfops.byte-test.com`(x-request-ip)、`*.d.ip.net.coffee`(泛解析+泛证书)、`{rand}.dns-detect.alicdn.com`(JSONP) 均 ✅；`1.1.1.1/cdn-cgi/trace` 终端直调 ✅。
- 注意：`/api/geoip/非法输入` → 502；`/api/ip/lookup/非法输入` → 400（text/html 错误页）；`iprisk` 的响应 `ip` 可能是 /24 段代表 IP 而非请求 IP。

## 6. 未解之谜 / 限制

1. **`/status/`（服务状态总览页）未逆向**（内联脚本 43KB，预计是 claude/gpt status.json 的聚合展示）；**`/whois/`** 仅确认存在独立 `whois-page.js`，未逐行分析（TUI 若需要可再查）。
2. **`/api/ping/start` 与 `/api/ping/global` 的后端节点探测方式未知**（推测是节点服务器发 ICMP/TCP，前端只见 JSON）；`user_ip` 参数为服务端供参考，是否参与路由未验证。
3. **DNS 泄漏的权威 DNS 侧实现不可见**：只知道 `<token>-<n>.d.ip.net.coffee` 触发 + `/api/dns/result` 回读，resolver 去重窗口/TTL 未知（实测 token 结果至少几分钟内可重复读取；空 token 返回 200 空数组）。
4. **Cloudflare Turnstile 流程无法在 TUI 实现**（需浏览器执行挑战 JS）；`/api/captcha/verify` 的 success=true 分支未实测（无有效 token）。
5. **portscan 的实际扫描节点/端口列表**：响应只见 6 端口（22/25/80/443/8080/8443），"全球多节点"节点数与扫描协议未暴露。
6. 首页连通性对淘宝/微信实测多次超时（ERR_ABORTED = 浏览器 no-cors 图片加载被中断属正常现象，curl 实现时应以"连接+TLS 握手完成时间"为准，不要等 body 下载）。
7. `api.anthropic.com/favicon.ico`（/link/ 页 Claude 项）未单独验证可达性；部分目标（mistral.ai、x.com）在测试环境出现 CONNECTION_CLOSED，属本地网络问题。
8. `/api/iprisk` 上游确认为 ipapi.is 风格字段（未拿到站点自述）；`src` 代号 g1/g2/g3/g7 的完整对应表（推测多 geo 库聚合：IP-API/IPinfo/MaxMind/IP2Location 类）未证实。
