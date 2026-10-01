# 04: 泄漏检测：DNS + WebRTC

**What to build:** 两个泄漏检测页。DNS 泄漏页：对随机 token 子域走系统 resolver 发 UDP 53 查询，回读解析器出口列表（带国旗）并给出三态判定。WebRTC 泄漏页：手写 STUN Binding 探测公网 UDP 地址（IPv4 与 IPv6 都要采集），与 HTTP 出口对照判定泄漏。

**Blocked by:** 02（首页与连通测量——复用其 geoip 客户端与出口探测）

**Status:** ready-for-agent

- [x] DNS：真随机 24 位 token（生成逻辑有测试，不得是固定值或弱随机）
- [x] DNS：按系统 resolver 配置对 `<token>-<n>` 子域发 UDP 53 A 查询（快速 5 轮/深度 8 轮），随后轮询回读接口取解析器列表
- [x] DNS 三态判定：解析器含中国大陆且出口非中国大陆 → 泄漏；列表为空 → 已加密未暴露；否则干净（判定逻辑单元测试）
- [x] DNS 解析器列表逐项带国旗与归属地（geoip）
- [x] WebRTC：手写 RFC 5389 STUN Binding，探测 3 个 STUN 服务器，采集 IPv4 + IPv6 候选，按清单过滤私网段，类型标注（公网 STUN/中继/本地）
- [x] STUN 编解码为纯函数并有测试（构造合法响应字节解出映射地址；畸形输入不 panic）
- [x] WebRTC 泄漏判定：任一公网 UDP 地址 ≠ HTTP 出口 IP → 可能泄漏（单元测试）
- [x] 页面附「代理模式 UDP 不通、TUN 模式才准」的提示文案
