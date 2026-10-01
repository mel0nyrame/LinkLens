# 06: 收尾打磨与审查

**What to build:** 全局质量关：多宽度终端实机适配、超时红线核对、文案术语过闸、用户文档、双轴代码审查。完成后整个 v1 交付。

**Blocked by:** 02（首页与连通测量）、03（AI 出口检测）、04（泄漏检测）、05（IP 评分页）

**Status:** claimed

- [x] 80/120/200 列三档终端宽度实机过检，卡片网格降级表现正确
- [x] 超时红线核对：公网探测 8s、各接口超时与net.coffee 接口报告一致（旧实现曾误写 5s）
- [x] 全部用户文案过 GLOSSARY 检查：无 Avoid 词（如「评分查询」）、「泄漏/泄露」统一「泄漏」
- [x] README：用法说明 + 与上游接口报告行为差异（不上报统计数据、无浏览器指纹维度）
- [ ] 对全部改动做双轴 code-review（Standards + Spec 对照规格），问题修复或记录
- [x] cargo test 全绿、clippy 无警告

## Comments

2026-10-01：非审查收尾完成，保留 claimed，双轴审查由整合分支执行。

- 首页、Claude/GPT、DNS/WebRTC、连通页先按换行正文计算完整画布，再复用评分页视口复制；各页保存滚动位置，↑/↓、PgUp/PgDn、Home/End 可访问首尾。评分输入先消费按键。首页空出口、AI 本轮缺失出口明确显示失败。
- 连通页按上游接口报告 8 轮、并发池 9；首页仍 12 轮。TCP/TLS 单次 8 秒总边界涵盖 DNS 与全部地址尝试，展示的延迟只计 TCP/TLS。AI 不等待首页 Notify；iprisk 显式采用 AI 页 10 秒请求边界，评分主查询 45 秒和增强请求 15/22/14 秒保持接口约定。geoip 维持规格公网 8 秒红线，未采用上游接口报告 AI 页较短的 5 秒；DNS 回读应为专用 5 秒请求边界；初次收尾遗漏请求 override，双轴审查后补齐。
- trace 拒绝非法 IP；国内出口源、trace、geoip、iprisk、status 的 HTTP 错误响应不作为有效数据解析，CDN 头探测保留其原有状态码语义。
- 纯行为回归先红后绿：连通轮数、非法 trace IP（含合法 IPv6 对照）、滚动首尾与钳制、中文目标名的终端显示宽度。未新增渲染或真实收发单元测试；STUN 回环测试保留为默认 ignored 的手动冒烟。
- 实际执行 cargo test：222 通过、0 失败、8 ignored；cargo fmt --check、cargo clippy --all-targets -- -D warnings、git diff --check 均通过。显式执行 query_stun_parses_response_from_live_socket -- --ignored，通过。
- 实际 PTY 80/120/200 × 45：七页首尾、80 评分坏值/IPv4/IPv6/打码、DNS 快速检测与 q 退出。外部捕获 /tmp/linklens-terminal-validation-after-06/；三宽最终 quit_exited=true、status=0。脚本在 PTY EIO 后短期限轮询 waitpid，修正单次 WNOHANG 的退出竞态。追加 final-score-80 验证最终评分帮助行的 q 提示完整、退出 0。
- 人工读取窄屏卡片正文、首页分流 End、Claude 服务状态与历史 End、DNS 快速测试完成结论、STUN 候选、宽屏三列正文。以当前 TARGETS 清单逐名字核对三宽首/End 捕获，48/48 个目标均出现；未改现有目标清单。DNS 实际回读为空解析器列表，未将其当成长列表实测。字体检查限于 PTY ANSI 与文字解析，未声称实测用户 Nerd Font 的字形观感。
- README 已完整重读，说明七页用途、所有键位、最近 IP 重查、HTTP 代理环境变量与原生 TCP/TLS/DNS/STUN 链路差异、数据路径、字体要求、上游接口报告差异及公共服务 IP 展示。运行时数据继续自忽略；.data/.gitignore 退出 Git 跟踪并保留本地文件。Cargo 显式库名使源码导入与占位包名解耦，包名未改。

2026-10-01：修复双轴审查的 7 项发现，仍保留 claimed，审查验收项由整合分支复核后勾选。

- Spec：AI 每轮并行重采国内双源、Cloudflare 与平台出口，复用已有采集流程；本轮快照保存在 AI 页结果，不更新首页状态。启动与刷新在同一状态锁内同时设 started/Pending，连续重查忽略在途请求。移除旧的等待首页注释。
- Spec：历史纯接缝接收可选信任分，缺分不新增记录、不写成 0，也不占用 24 小时去重窗口；有效 0 分仍可记录。现有 JSON schema 保持不变；旧文件无法区分真实 0 分与先前缺失误记的 0 分，未推测迁移旧数据。
- Spec：评分中文归属地复用 cc::chinese_location；DNS 结果回读请求显式覆盖为 5 秒，纠正初次收尾遗漏 override 的记录。
- Standards：分流 IP 校验使用标准库 IpAddr 真实解析，保留国内源原文容忍规则；评分先消费输入焦点，再复用 ui::scroll::key_scroll；两页信任分颜色收口到共用 UI helper，detect 不依赖 UI。
- 纯行为红绿证据：非法 IPv6 `:`、缺分历史、刷新后 Pending 均先出现预期失败再修复通过；含合法 IPv6/IPv4 对照、未知检测后有效 95 分记录、有效 0 分、刷新防重入回归。未新增默认真实 IO 或渲染单测。
- 实际执行 cargo test：224 通过、0 失败、8 ignored；cargo fmt --check、cargo clippy --all-targets -- -D warnings、cargo build、git diff --check 均通过。Context7 核对 reqwest 请求级 timeout 覆盖客户端默认值，以及 tokio::join! 的并行轮询语义。
- 针对性实际 PTY 80×45：Claude 初次及连续 rr 刷新均有两个本轮参照出口 Pending 槽，随后完成。国内出口从 192.0.2.160/iP138.com 更新为 192.0.2.216/IP.cn，Claude 出口从 IPv4 更新为 IPv6；评分中文归属地显示“美国 Google”。q 正常退出，status=0。
- 评分追加 PTY 专测：End 后 Up、Home、PgDn/PgUp、Down/Up 全部通过，q 退出 0。首次逐字符全帧比较受 pyte 宽字占位留白影响，追加专测按行忽略排版留白并人工读首尾正文确认；证据在 /tmp/linklens-terminal-validation-review-fixes/ 及其 score-scroll-80/，外部脚本未进入仓库。README 已重读并同步本轮刷新、有效历史与 DNS 专用边界。
