# 任务导航

本页保存跨文件的修改入口与复用边界；函数签名、字段和完整测试清单以源码为准。按任务读取对应行，再沿调用关系展开。

## 修改入口

| 任务 | 入口与相关验证 |
| --- | --- |
| 按键、鼠标、重查、退出 | [app.rs](../../src/app.rs) 分发事件，再进入对应 `probe*.rs` / `state*.rs`；全局快捷键和刷新测试也在该文件。鼠标过滤发生在布局与重绘前。 |
| 评分输入、取消与迟到结果 | [state_score.rs](../../src/state_score.rs) 先消费编辑按键，再调用共享滚动；[probe_score.rs](../../src/probe_score.rs) 按查询代号隔离迟到写回。输入及滚动边界测试在状态模块。 |
| 页面布局与滚动 | [layout.rs](../../src/ui/layout.rs) 测量卡片几何，[scroll.rs](../../src/ui/scroll.rs) 统一键盘、滚轮和视口复制，[shell.rs](../../src/ui/shell.rs) 分配正文区域；几何与滚动纯函数测试在前两者。 |
| Claude/GPT 检测与刷新 | [probe_ai.rs](../../src/probe_ai.rs) 以 profile 区分平台，共用编排；[ai.rs](../../src/ui/pages/ai.rs) 共用渲染。每轮三个出口使用同轮快照；在途重查与平台差异测试见 [state_ai.rs](../../src/state_ai.rs) 和编排模块。 |
| 场景评分与 Radar 人机比 | [scene.rs](../../src/detect/scene.rs) 保存纯判定与边界测试，[ip_score.rs](../../src/net/ip_score.rs) 负责解析，[probe_score.rs](../../src/probe_score.rs) 负责请求与轮询；改规则先读接口报告 §3.3。 |
| DNS/STUN 泄漏检测 | [probe_leak.rs](../../src/probe_leak.rs) 编排，[dnsleak.rs](../../src/net/dnsleak.rs) / [stun.rs](../../src/net/stun.rs) 处理 IO 与解析，[leak.rs](../../src/detect/leak.rs) 判定；测试就近放置，真实网络冒烟为 ignored。 |
| 历史持久化与双命令 | [history.rs](../../src/history.rs) 保存路径、格式和去重接缝；[main.rs](../../src/main.rs) / [llens.rs](../../src/bin/llens.rs) 共用应用逻辑。数据约束见 [ADR-0003](../adr/0003-user-data-dir.md)，缺分及去重测试在历史模块。 |
| 自更新与发布来源 | 两个命令经 [main.rs](../../src/main.rs) / [llens.rs](../../src/bin/llens.rs) 进入共享应用；更新编排与构建身份集中在 [src/update.rs](../../src/update.rs)，安装事务位于 [src/update_install.rs](../../src/update_install.rs)，提示与下载交互状态位于 [src/state_update.rs](../../src/state_update.rs)，纯判定与归档校验位于 [src/detect/update.rs](../../src/detect/update.rs)，网络 IO 位于 [src/net/update.rs](../../src/net/update.rs)。官方构建身份由 [.github/scripts/build_release.py](../../.github/scripts/build_release.py) 注入，普通源码构建缺省不具有该身份；长期边界见 [ADR-0004](../adr/0004-self-update.md)，验收范围见 [规格票](https://github.com/mel0nyrame/LinkLens/issues/2)。 |
| 卡片、徽章、图标与信任分颜色 | [widget.rs](../../src/theme/widget.rs) / [icon.rs](../../src/theme/icon.rs) 提供主题助手；[pages/mod.rs](../../src/ui/pages/mod.rs) 的 `trust_tier_color` 供 AI 与评分页复用。 |

评分页有固定输入栏，正文高度与滚动上限由 [ip_score.rs](../../src/ui/pages/ip_score.rs) 扣除输入栏后计算；其他页面的上限由 [pages/mod.rs](../../src/ui/pages/mod.rs) 计算。修改两类页面时，分别核对首尾、窗口缩放和输入焦点，滚动步长与边界复用 `scroll.rs`。

定位入口时，先搜索文件名、符号或文档标题，再读取相关函数、章节与 diff。每批输出应完整可见；发生截断时缩小范围补读，直到本次改动的调用方、复用入口与验证均已覆盖。

## 工作树交接

交接前核对目标工作树及完整分支名，提供：工作目录、任务分支、基分支或审查基点、当前规格/票据的 GitHub URL（历史记录使用本地路径）、适用技能文件路径。以接收工作树实际存在的路径和可解析的 Git 引用为完成条件；分支名取查询结果，创建工具返回的目录用于后续操作。

`.agents/` 是本机技能资产，被 Git 忽略；新工作树不会带上主 checkout 中的技能。需要本地技能时，从会话提供的技能位置或已确认的主 checkout 定位，交接其可读路径。仓库文档保留定位方法，机器专属绝对路径放在当次交接信息中。

## 终端验证与证据

自更新的受版本管理 PTY 入口是 [test_update_pty.py](../../.github/scripts/test_update_pty.py)，通过 Rust `cfg(test)` 场景验证真实事件循环、输入优先、失败、取消和终端恢复；使用临时安装目录，不启动真实网络检测或读写用户历史。真实网络冒烟可用 `cargo test --lib live_ -- --ignored`，避免误启动需要 PTY 脚本配置的更新 fixture。其他页面的历史本地票据中的 `/tmp` 目录只描述当次捕获；复现前核对脚本是否可得，脚本不可得时建立本次验证入口，并说明与历史场景的差别。

界面验收结果写入 GitHub issue，记录需要包含二进制来源、可执行脚本或完整命令、终端尺寸、输入序列、场景结果与退出状态。验证 80/120/200 列时，覆盖首尾滚动、鼠标移动后滚轮、输入焦点、连续重查及退出恢复；网络结果和字体观感按实际验证范围分别说明。证据保存方式见 [tracker 约定](issue-tracker.md)。

复用 PTY 脚本时核对三项边界：输入写入不会阻塞事件采集；中文宽字符的布局占位不会被误判为正文变化；读到 PTY EIO 后仍收集子进程退出状态。真实网络场景须显式执行，并区分网络失败态与交互失败。
