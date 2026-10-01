# AGENTS.md

LinkLens 是 Rust 终端网络诊断工具。产品名写作 **LinkLens**，主命令为 `linklens`，短命令为 `llens`；两者共用应用逻辑。net.coffee 仅作为后端接口来源描述，实际 API 域名与协议值保持准确。

## 工作约定

- 永远使用中文回答；用户可见文案以中文为主，保留必要的协议和产品名称。
- 先检查 `git status` 和 `git worktree list`，确认当前分支及任务所属工作树，保留其他任务的修改和本地数据。
- 优先沿现有入口做最小改动。影响结果的假设须明确说明，并定义可执行的完成条件。
- 工程术语使用 [GLOSSARY.md](GLOSSARY.md)。涉及模块边界、依赖或持久化策略时，读取 `docs/adr/` 中对应决策；领域文档的消费约定见 [domain.md](docs/agents/domain.md)。

## 修改入口与边界

- `net/` 负责后端接口和原生网络 IO，`detect/` 负责纯解析与判定，`probe*.rs` 负责异步编排，`state*.rs` 负责状态，`ui/` 负责输入与渲染。沿现有职责扩展，保持判定逻辑可独立测试。
- 调整 API 字段、错误分类、判定公式、请求时限或探测节奏前，读取 [net.coffee 接口报告](docs/research/net-coffee-api.md) 对应章节。报告记录接口事实与来源，README 描述 LinkLens 的实际行为。
- 卡片、徽章、颜色和图标复用 `theme/` 的统一入口。页面正文保持可滚动，键盘与滚轮使用一致的边界；无页面行为的鼠标事件在布局和重绘前过滤。
- 共享状态锁内只做短暂读取或修改；网络请求和其他等待在锁外执行。重查须明确任务取消与迟到结果隔离。
- 历史目录固定为 `~/.config/linklens/datas/`，两个命令和所有工作树共用。修改目录、格式、保留规则或迁移逻辑前，读取 [用户级历史数据决策](docs/adr/0003-user-data-dir.md)。迁移保留源文件，避免覆盖有效记录。

## 验证与交付

- 非平凡修改先定义能捕获具体问题的验证，再执行。默认单元测试覆盖纯函数、状态变换和布局几何；真实 HTTP/DNS/STUN 与终端渲染使用独立冒烟或 PTY 验证。
- 修改 Rust 代码后运行 `cargo test`、`cargo clippy --all-targets -- -D warnings`、`cargo fmt --check`，交付前检查 diff。真实网络测试默认忽略，只在任务需要时显式执行。
- 修改命令入口时，构建并验证 `linklens`、`llens` 的等价行为；修改界面时验证实际终端输入、滚动和退出恢复。
- 新功能和验收要求写入本地 Markdown tracker；目录、状态和关闭约定见 [issue-tracker.md](docs/agents/issue-tracker.md)，需要分诊标签时读取 [triage-labels.md](docs/agents/triage-labels.md)。现有七页规格位于 [.scratch/linklens/spec.md](.scratch/linklens/spec.md)。
- 交付说明报告实际修改、实际检查及验证边界。提交说明使用 LinkLens 的功能语义；push 和后续历史改写分别需要用户明确授权。

## 发布维护

- 分支职责：`dev` 保存源码和开发历史，`main` 是独立的展示入口，保持一次初始化提交。修改首页或发布文件后同步两边；`main` 的允许文件与同步方法见 [发布维护](.github/RELEASING.md)。
- 修改 CI、安装脚本、发布版本或 Release 文案前，读取 [发布维护](.github/RELEASING.md)，按其验证和交付步骤执行。Release 文案由维护者逐版本编写，以 `.github/releases/vX.Y.Z.md` 为唯一来源，打标签前完成。

## 库文档

涉及库、SDK、API 或 CLI 的具体用法时，通过 Context7 先 `resolve-library-id`，再按单一概念 `query-docs`。纯业务逻辑、代码审查和一般编程概念不需要库文档查询。
