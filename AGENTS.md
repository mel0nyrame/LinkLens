# AGENTS.md

linklens：Rust 终端网络诊断工具，使用 net.coffee 后端接口。接口逆向调查见 docs/research/net-coffee-api.md。

## Agent skills

### Issue tracker

Issues 以本地 Markdown 存放在 `.scratch/<feature>/`（每个 issue 一个文件）。See `docs/agents/issue-tracker.md`.

### Triage labels

默认五角色词汇，标签字符串与角色名相同（needs-triage / needs-info / ready-for-agent / ready-for-human / wontfix）。See `docs/agents/triage-labels.md`.

### Domain docs

单上下文布局：根 GLOSSARY.md + docs/adr/，按需惰性创建。See `docs/agents/domain.md`.
