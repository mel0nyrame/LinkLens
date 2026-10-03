# AGENTS.md

LinkLens 是 Rust 终端网络诊断工具。产品名写作 **LinkLens**，主命令为 `linklens`，短命令为 `llens`；两者共用应用逻辑。net.coffee 仅作为后端接口来源描述，实际 API 域名与协议值保持准确。

## 工作约定

- 永远使用中文回答；用户可见文案以中文为主，保留必要的协议和产品名称。
- 先检查 `git status` 和 `git worktree list`，确认当前分支及任务所属工作树，保留其他任务的修改和本地数据。
- 优先沿现有入口做最小改动。影响结果的假设须明确说明，并定义可执行的完成条件。
- 工程术语使用 [GLOSSARY.md](GLOSSARY.md)。涉及模块边界、依赖或持久化策略时，读取 `docs/adr/` 中对应决策。

## 修改入口与边界

- `src/net/` 负责后端接口和原生网络 IO，`src/detect/` 负责纯解析与判定，`probe*.rs` 负责异步编排，`state*.rs` 负责状态，`src/app.rs` 分发输入，`src/ui/` 负责布局与渲染。沿现有职责扩展，保持判定逻辑可独立测试。修改重查、滚动、评分、历史或共享视觉规则，以及交接工作树、跨模块事务、复现终端验收时，先读 [任务导航](docs/agents/navigation.md) 对应条目。
- 调整 API 字段、错误分类、判定公式、请求时限或探测节奏前，读取 [net.coffee 接口报告](docs/research/net-coffee-api.md) 对应章节。报告记录接口事实与来源，README 描述 LinkLens 的实际行为。
- 卡片、徽章、颜色和图标复用 `theme/` 的统一入口。页面正文保持可滚动，键盘与滚轮使用一致的边界；无页面行为的鼠标事件在布局和重绘前过滤。
- 共享状态锁内只做短暂读取或修改；网络请求和其他等待在锁外执行。重查须明确任务取消与迟到结果隔离。
- 历史目录固定为 `~/.config/linklens/datas/`，两个命令和所有工作树共用。修改目录、格式、保留规则或迁移逻辑前，读取 [用户级历史数据决策](docs/adr/0003-user-data-dir.md)。迁移保留源文件，避免覆盖有效记录。

## 验证与交付

- 非平凡修改先定义能捕获具体问题的验证，再执行。默认单元测试覆盖纯函数、状态变换和布局几何；真实 HTTP/DNS/STUN 与终端渲染使用独立冒烟或 PTY 验证。
- 本地默认仅做无需安装环境的静态检查，包括文档、语法、格式和 diff；完成实现后，在已获授权的范围内推送任务分支，由 CI 执行测试、Clippy、构建与平台验收，并核对结果。主代理与子代理均不得为验证在本机新增 Docker 镜像、Rust 工具链或目标标准库、SDK、全局依赖及仓库外编译缓存；已有工具缺失时交给 CI，详见 [验证环境决策](docs/adr/0005-validation-environment.md)。真实网络测试默认忽略，只在任务需要时于 CI 显式执行。
- 修改命令入口时，由 CI 构建并验证 `linklens`、`llens` 的等价行为；修改界面时，由 CI 验证实际终端输入、滚动和退出恢复。缺少所需 CI 入口时补齐后再验收，静态检查通过不代表运行验收通过。
- 新功能和验收要求写入 GitHub Issues，操作约定见下方 Agent skills。`.scratch/` 是只读历史归档，不新增、修改或迁移其中内容；追溯七页初始需求时读取 [历史规格](.scratch/linklens/spec.md)。当前决策写入 `docs/adr/`，当前行为与修改入口见 README、源码及任务导航。
- 交付说明报告实际修改、实际检查及验证边界。提交说明使用 LinkLens 的功能语义；push 和后续历史改写分别需要用户明确授权。

## Agent skills

### Issue tracker

新需求、规格与实施票据使用 GitHub Issues；建票、领取、关闭或读取历史验收时，见 [tracker 约定](docs/agents/issue-tracker.md)。

### Triage labels

分诊时使用五个默认角色及 [标签映射](docs/agents/triage-labels.md)。

### Domain docs

单上下文：根目录 `GLOSSARY.md` 与 `docs/adr/`；读取领域文档时，见 [消费约定](docs/agents/domain.md)。

## 发布维护

- 发布卫生：调查记录与 fixture 使用文档示例地址、示例定位及会话标识；公开截图检查地址与反向 DNS，提交邮箱使用 GitHub noreply。保留正常接口域名与公共服务地址，验证脱敏不改变测试覆盖。

- 分支职责：`dev` 保存源码和开发历史，`main` 是独立的展示入口，保持一次初始化提交。修改首页或发布文件后同步两边；`main` 的允许文件与同步方法见 [发布维护](.github/RELEASING.md)。
- 修改 CI、安装脚本、发布版本或 Release 文案前，读取 [发布维护](.github/RELEASING.md)，按其验证和交付步骤执行。Release 文案由维护者逐版本编写，以 `.github/releases/vX.Y.Z.md` 为唯一来源，打标签前完成。

## 库文档

涉及库、SDK、API 或 CLI 的具体用法时，通过 Context7 先 `resolve-library-id`，再按单一概念 `query-docs`。纯业务逻辑、代码审查和一般编程概念不需要库文档查询。
