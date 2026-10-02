# Issue tracker: GitHub

LinkLens 的新需求、规格和实现票据记录在 [GitHub Issues](https://github.com/mel0nyrame/LinkLens/issues)。通过 `gh` 操作，仓库为 `mel0nyrame/LinkLens`；先核对 `git remote -v`，多工作树或仓库外调用时显式传 `--repo mel0nyrame/LinkLens`。

## 发布与读取

技能要求 “publish to the issue tracker” 时创建 GitHub issue；要求 “fetch the relevant ticket” 时读取对应 issue 的正文、标签、负责人和评论。一个规格对应一个 issue，实施票据分别建 issue，用编号或 URL 关联；GitHub open/closed 表达生命周期，分诊角色使用 [标签映射](triage-labels.md)。

多行正文先保存到临时 UTF-8 文件，写入时传 `--body-file`，保留实际换行。以下 `<number>`、`<title>` 与 `<body-file>` 均为操作时的实际值：

```sh
# 新建规格或实施票据
gh issue create --repo mel0nyrame/LinkLens --title '<title>' --body-file '<body-file>'
# 读取完整任务上下文
gh issue view <number> --repo mel0nyrame/LinkLens --json number,title,body,state,labels,assignees,comments,url
# 查看候选，再读取选中票据及评论
gh issue list --repo mel0nyrame/LinkLens --state open --limit 100 --json number,title,labels,assignees,url
# 更新正文、追加验收说明、调整分诊标签
gh issue edit <number> --repo mel0nyrame/LinkLens --body-file '<body-file>'
gh issue comment <number> --repo mel0nyrame/LinkLens --body-file '<body-file>'
gh issue edit <number> --repo mel0nyrame/LinkLens --add-label '<label>' --remove-label '<old-label>'
# 验收通过并记录证据后关闭
gh issue close <number> --repo mel0nyrame/LinkLens --reason completed
```

列表达到 limit 时继续分批检索；只有覆盖所需范围后才作无票据或无阻塞结论。标签名称以映射文件为准；缺少所需标签时，在获准执行分诊的范围内创建该标签后再应用。

## Pull requests as a triage surface

**PRs as a request surface: no.**

PR 用于审查和合入实现，需求队列从 Issues 读取。GitHub 的 issue 与 PR 共用编号空间；遇到裸编号先确认对象类型，再执行对应操作。

## Wayfinding operations

`/wayfinder` 的 map 与 child ticket 使用 GitHub Issues：

- **Map**：单个 issue，标签 `wayfinder:map`，正文保存 Notes / Decisions-so-far / Fog。
- **Child ticket**：一个问题一个 issue，标签 `wayfinder:<type>`（research/prototype/grilling/task），正文链接 map；使用 `gh issue edit <child> --parent <map>` 建立原生子票关系。原生关系不可用时，在 map 正文按顺序维护子票任务列表，并在子票顶部写 `Part of #<map>`。
- **Blocking**：用 `gh issue edit <child> --add-blocked-by <blocker>` 记录原生依赖；用 `gh issue view <child> --json blockedBy` 读取并核对阻塞票状态。原生依赖不可用时，子票顶部写 `Blocked by: #<n>, #<n>`；所有阻塞票关闭才可领取。
- **Frontier**：读取 map 的子票（`gh issue view <map> --json subIssues` 或正文任务列表），按 map 顺序选取 open、无未关闭阻塞票且无负责人的票据。
- **Claim**：领取前重读状态与负责人，再用 `gh issue edit <number> --add-assignee @me` 登记。
- **Resolve**：验收满足后追加答案评论、关闭子票，再在 map 的 Decisions-so-far 中追加结论和子票链接；编辑 map 前重读正文，保留他人内容。

上述写操作均带 `--repo mel0nyrame/LinkLens`，map/type 标签沿技能约定按需要使用。

## 历史基线与验收证据

[本地七页规格](../../.scratch/linklens/spec.md) 及其票据保留为已完成的历史基线。切换配置不为这些 resolved 记录批量创建 GitHub issues；只有明确迁移要求才另行发布历史记录。新工作以 GitHub issue 为状态与讨论的唯一来源，本地草稿或捕获目录在 issue 中说明用途。

读取历史验收时确认其边界，再按任务阅读正文与相关 Comments；当前行为由 README、源码与测试核验。关闭新票据时，正文保留需求，评论记录验收证据及当前行为入口。记录 Git 审查范围前确认两端可解析；历史改写导致范围失效时，标明改写前引用及经核验映射或可访问的备份，来源不可得时明确限制。

终端验证要求见 [任务导航](navigation.md)。issue 保存验证入口、结果、证据位置和测量边界；运行产物放自忽略目录，使用示例地址与会话标识。可复用脚本受版本管理，外部脚本说明位置及可用性。关闭前确认所声称可复现的命令、脚本和引用能从交接环境访问；PR 合并本身不替代验收。
