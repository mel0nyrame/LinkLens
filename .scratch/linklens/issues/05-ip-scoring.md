# 05: IP 评分页

**What to build:** 输入任意合法 IPv4/IPv6 进行深度体检：深度聚合接口全字段卡片（AI 判词、多源定位对比、位置/ASN/公司历史轨迹、机房邻居、反查域名含 pending 轮询）、v2 增强区块（C 段热度、BGP 拓扑、12 家 DNSBL、Radar 人机比、同 ASN 公司），以及按 TikTok/社媒/AI 三场景计算的 0-10 场景评分（含地区硬门槛）。

**Blocked by:** 03（AI 出口检测——复用其 net.coffee 客户端模式与卡片页框架）

**Status:** ready-for-agent

- [ ] 输入合法 IPv4/IPv6 触发深度查询；非法输入有明确报错（且区分 geoip 502 与 lookup 400 两种失败）
- [ ] 深度字段卡片：ai_verdict 中文判词与置信度、多源 geo（按 g1>g7>g3>g2 优先取坐标）、原生/广播 IP 徽章（注册国 vs 归属国推导，有单元测试）、历史轨迹、机房邻居、反查域名
- [ ] 反查域名 pending 时每 1.5s 轮询（≤10 次）自动补全；同 ASN 公司 pending 轮询（≤12 次）
- [ ] v2 区块：C 段热度趋势、BGP 拓扑（ origins/upstreams 占比）、DNSBL 12 家结果（IPv6 不支持时隐藏对应区块）、Radar 人机比、同 ASN 公司列表
- [ ] 三场景评分公式逐字采用研究报告 §3.3（base 与全部加减项、地区 block→0 分/partial→封顶 5 分、档位文案与「risky 上限 9」），以报告真实样例为 fixture 逐项断言
- [ ] 冷查询耗时 5-45s 期间有阶段性状态指示，UI 不冻结
