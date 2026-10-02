# Issue tracker: Local Markdown

Issues and specs for this repo live as markdown files in `.scratch/`.

## Conventions

- One feature per directory: `.scratch/<feature-slug>/`
- The spec is `.scratch/<feature-slug>/spec.md`
- Implementation issues are one file per ticket at `.scratch/<feature-slug>/issues/<NN>-<slug>.md`, numbered from `01`, never a single combined tickets file
- Triage state is recorded as a `Status:` line near the top of each issue file (see `triage-labels.md` for the role strings)
- Comments and conversation history append to the bottom of the file under a `## Comments` heading

## When a skill says "publish to the issue tracker"

Create a new file under `.scratch/<feature-slug>/` (creating the directory if needed).

## When a skill says "fetch the relevant ticket"

Read the file at the referenced path. The user will normally pass the path or the issue number directly.

## 历史基线

读取已 resolved 的规格或票据时，先确认其历史边界，再按任务阅读正文及相关 Comments。当前行为由 README、源码与测试核验；新增需求另建票，已完成验收正文保留为历史证据。

关闭规格或票据时，在顶部标明历史基线及当前行为入口。记录 Git 审查范围前确认两端提交均可解析；历史改写后，将失效范围标为改写前引用。只有经核验的映射或可访问的备份才能作为复查来源；来源不可得时明确记录这一限制。

## 验收证据

终端场景与捕获要求见 [任务导航](navigation.md)。票据保存验证入口、结果、证据位置和测量边界；仅有临时目录不足以复现。运行产物放自忽略目录，使用示例地址与会话标识；可复用脚本受版本管理，若脚本尚未入库，明确其外部位置及可用性。关闭前确认所声称可复现的命令、脚本和引用能从交接环境访问。

## Wayfinding operations

Used by `/wayfinder`. The **map** is a file with one **child** file per ticket.

- **Map**: `.scratch/<effort>/map.md` (the Notes / Decisions-so-far / Fog body).
- **Child ticket**: `.scratch/<effort>/issues/NN-<slug>.md`, numbered from `01`, with the question in the body. A `Type:` line records the ticket type (`research`/`prototype`/`grilling`/`task`); a `Status:` line records `claimed`/`resolved`.
- **Blocking**: a `Blocked by: NN, NN` line near the top. A ticket is unblocked when every file it lists is `resolved`.
- **Frontier**: scan `.scratch/<effort>/issues/` for files that are open, unblocked, and unclaimed; first by number wins.
- **Claim**: set `Status: claimed` and save before any work.
- **Resolve**: append the answer under an `## Answer` heading, set `Status: resolved`, then append a context pointer (gist + link) to the map's Decisions-so-far in `map.md`.
