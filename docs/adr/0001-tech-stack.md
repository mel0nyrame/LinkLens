# 技术栈：ratatui + tokio + reqwest(rustls) + hickory + 手写 STUN

linklens 的 TUI 骨架选用 ratatui（终端 UI 事实标准）+ tokio + reqwest（rustls-tls，避免 openssl 依赖负担）+ serde_json。七个功能都依赖大量并行探测（47 目标连通计时、多出口并行 trace、多 resolver 回读），异步运行时天然契合「多探测并行 + 结果流式刷新」的交互。DNS 泄漏检测用 hickory-resolver 发 UDP 53 查询，且必须按系统 resolver 配置（/etc/resolv.conf）发询——否则测到的是库的出口而非本机真实 DNS 行为。WebRTC 泄漏检测手写 RFC 5389 STUN Binding Request（约百行，只读 XOR-MAPPED-ADDRESS），否决 webrtc-rs：只为一个功能引入整个 WebRTC 栈不值得；同样否决阻塞式 ureq + 线程池：并发编排手写比 async 更繁琐。
