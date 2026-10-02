# 04: 泄漏检测：DNS + WebRTC

**What to build:** 两个泄漏检测页。DNS 泄漏页：对随机 token 子域走系统 resolver 发 UDP 53 查询，回读解析器出口列表（带国旗）并给出三态判定。WebRTC 泄漏页：手写 STUN Binding 探测公网 UDP 地址（IPv4 与 IPv6 都要采集），与 HTTP 出口对照判定泄漏。

**Blocked by:** 02（首页与连通测量——复用其 geoip 客户端与出口探测）

**Status:** resolved

> 历史验收：本票正文与 Comments 记录当次实现和检查。当前行为入口、改写前提交引用的可用性见 [规格顶部的历史边界](../spec.md)。

- [x] DNS：真随机 24 位 token（生成逻辑有测试，不得是固定值或弱随机）
- [x] DNS：按系统 resolver 配置对 `<token>-<n>` 子域发 UDP 53 A 查询（快速 5 轮/深度 8 轮），随后轮询回读接口取解析器列表
- [x] DNS 三态判定：解析器含中国大陆且出口非中国大陆 → 泄漏；列表为空 → 已加密未暴露；否则干净（判定逻辑单元测试）
- [x] DNS 解析器列表逐项带国旗与归属地（geoip）
- [x] WebRTC：手写 RFC 5389 STUN Binding，探测 3 个 STUN 服务器，采集 IPv4 + IPv6 候选，按清单过滤私网段，类型标注（公网 STUN/中继/本地）
- [x] STUN 编解码为纯函数并有测试（构造合法响应字节解出映射地址；畸形输入不 panic）
- [x] WebRTC 泄漏判定：任一公网 UDP 地址 ≠ HTTP 出口 IP → 可能泄漏（单元测试）
- [x] 页面附「代理模式 UDP 不通、TUN 模式才准」的提示文案


## Answer

实现已合入 `integration/linklens`（合并提交 `39fdcb5`；合入前响应校验修复 `d9710fd`）。

- DNS 使用系统 resolver，OS CSPRNG 生成 24 位 token；提供快速 5 轮、深度 8 轮触发与回读、归属地和国旗、三态判定。回读 HTTP 错误与缺少解析器字段的 JSON 作为失败处理；归属信息缺失时明确说明无法判定。
- WebRTC 向三个 STUN 服务采集 IPv4/IPv6 映射地址，过滤私网并去重后与 HTTP 出口对照；HTTP 出口不可用时显示失败，保留代理模式 UDP 不通、TUN 模式才准的提示。STUN 编解码校验消息长度、类型、魔数与事务 ID，畸形消息不产生候选。
- 验证：合入后 `cargo test -q`：192 通过、7 个需公网的测试默认跳过；`cargo clippy --all-targets -- -D warnings`、`cargo fmt --check`、`git diff --check` 全通过。STUN 声明长度与实际字节数不一致、DNS 缺字段回读均完成失败复现后修复。
- 在票分支修复后额外执行 `cargo test --lib probe_leak -- --ignored --nocapture`：DNS 与 WebRTC 两项真实网络冒烟通过，12.45 秒结束。冒烟验证状态机完整收尾，允许环境导致的明确失败态，不能据此保证任何网络均可取得 IPv6 或公网 UDP 候选。
- 窄终端下顶部两卡的高度裁剪，由 [票 06](06-polish-and-review.md) 的全局多宽度验收统一修复。
