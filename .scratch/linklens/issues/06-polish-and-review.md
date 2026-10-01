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

- 首页、Claude/GPT、DNS/WebRTC、连通页先按换行正文计算完整画布，再复用评分页视口复制；各页保存滚动位置，↑/↓、PgUp/PgDn、Home/End 可访问首尾。评分输入先消费按键。首页空出口、AI 缺少已结束的首页出口明确显示失败。
- 连通页按上游接口报告 8 轮、并发池 9；首页仍 12 轮。TCP/TLS 单次 8 秒总边界涵盖 DNS 与全部地址尝试，展示的延迟只计 TCP/TLS。AI 不等待首页 Notify；iprisk 显式采用 AI 页 10 秒请求边界，评分主查询 45 秒和增强请求 15/22/14 秒保持接口约定。geoip 维持规格公网 8 秒红线，未采用上游接口报告 AI 页较短的 5 秒；DNS 回读的专用阶段边界保持票 04 契约。
- trace 拒绝非法 IP；国内出口源、trace、geoip、iprisk、status 的 HTTP 错误响应不作为有效数据解析，CDN 头探测保留其原有状态码语义。
- 纯行为回归先红后绿：连通轮数、非法 trace IP（含合法 IPv6 对照）、滚动首尾与钳制、中文目标名的终端显示宽度。未新增渲染或真实收发单元测试；STUN 回环测试保留为默认 ignored 的手动冒烟。
- 实际执行 cargo test：222 通过、0 失败、8 ignored；cargo fmt --check、cargo clippy --all-targets -- -D warnings、git diff --check 均通过。显式执行 query_stun_parses_response_from_live_socket -- --ignored，通过。
- 实际 PTY 80/120/200 × 45：七页首尾、80 评分坏值/IPv4/IPv6/打码、DNS 快速检测与 q 退出。外部捕获 /tmp/linklens-terminal-validation-after-06/；三宽最终 quit_exited=true、status=0。脚本在 PTY EIO 后短期限轮询 waitpid，修正单次 WNOHANG 的退出竞态。追加 final-score-80 验证最终评分帮助行的 q 提示完整、退出 0。
- 人工读取窄屏卡片正文、首页分流 End、Claude 服务状态与历史 End、DNS 快速测试完成结论、STUN 候选、宽屏三列正文。以当前 TARGETS 清单逐名字核对三宽首/End 捕获，48/48 个目标均出现；未改现有目标清单。DNS 实际回读为空解析器列表，未将其当成长列表实测。字体检查限于 PTY ANSI 与文字解析，未声称实测用户 Nerd Font 的字形观感。
- README 已完整重读，说明七页用途、所有键位、最近 IP 重查、HTTP 代理环境变量与原生 TCP/TLS/DNS/STUN 链路差异、数据路径、字体要求、上游接口报告差异及公共服务 IP 展示。运行时数据继续自忽略；.data/.gitignore 退出 Git 跟踪并保留本地文件。Cargo 显式库名使源码导入与占位包名解耦，包名未改。
