# 仓库级历史目录（已替代）

Status: superseded
Superseded by: [0003-user-data-dir.md](0003-user-data-dir.md)

历史记录等运行数据放仓库根的 `.data/` 目录，目录内放一个内容为 `*` 的 `.gitignore`，使整个目录（含该 .gitignore 自身）永不被 git 跟踪——沿用 `.agents/` 已有的自忽略先例，不用 XDG 配置目录：数据跟着仓库走，存储路径与项目名解耦。该早期方案将目录与当时的占位包名解耦；固定品牌和用户级目录由 ADR-0003 定义。
