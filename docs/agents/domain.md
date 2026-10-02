# 领域文档

工程技能探索代码库时，按本页约定读取领域文档。

## 探索前读取

- 根目录没有术语地图时，读取 **`GLOSSARY.md`**。
- 若根目录存在 **`GLOSSARY-MAP.md`**，则按其指针读取与任务相关的上下文术语表；每个上下文有一份 `GLOSSARY.md`。
- 在 **`docs/adr/`** 中读取与待修改领域相关的架构决策。多上下文仓库还需检查 `src/<context>/docs/adr/` 中的上下文级决策。

缺少上述文件时，**直接继续任务**，无需提示缺失或提前建议创建。只有术语或决策得到明确结论后，才由 `/domain-modeling` 技能按需建立文档；`/grill-with-docs` 和 `/improve-codebase-architecture` 可调用该技能。

## 目录结构

单上下文仓库（适用于大多数项目，以下为结构示例）：

```
/
├── GLOSSARY.md
├── docs/adr/
│   ├── 0001-event-sourced-orders.md
│   └── 0002-postgres-for-write-model.md
└── src/
```

多上下文仓库（根目录存在 `GLOSSARY-MAP.md`，以下为结构示例）：

```
/
├── GLOSSARY-MAP.md
├── docs/adr/                          ← 系统级决策
└── src/
    ├── ordering/
    │   ├── GLOSSARY.md
    │   └── docs/adr/                  ← 上下文级决策
    └── billing/
        ├── GLOSSARY.md
        └── docs/adr/
```

## 沿用术语表

在任务票据标题、重构建议、诊断假设或测试名称中提及领域概念时，使用 `GLOSSARY.md` 定义的术语，并遵循其中的避免用词约定。

所需概念尚未出现在术语表时，先确认它是否属于项目已有概念；若确有术语缺口，记录下来供 `/domain-modeling` 处理。

## 明示决策冲突

输出与现有架构决策冲突时，明确指出冲突及重新讨论的理由。例如：

> _与 ADR-0007（订单事件溯源）冲突，但值得重新讨论，因为……_
