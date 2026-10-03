# AGENTS.md

LinkLens 是交互式终端 IP 检测工具。产品名为 **LinkLens**，命令为 `linklens` 和 `llens`。net.coffee 仅作为部分后端接口来源描述。

## 展示维护

- 永远使用中文回答。维护项目介绍、浅色 README 视觉、安装入口、MIT 协议和发布配置。
- `main` 只保存展示与发布文件，保持一次无父提交的初始化提交。允许的文件及更新步骤见 [发布维护](.github/RELEASING.md)。更新已有提交并推送历史前获得用户授权。
- README 面向使用者，产品表述使用 IP 检测。安装优先使用预编译 Release，命令、路径和操作同时保留为可复制的文字。
- 修改图片前读取 [.assets/readme/source/README.md](.assets/readme/source/README.md)，从可编辑源生成产物。公开截图和日志检查完整地址、反向 DNS 及个人标识；示例使用文档保留地址。
- 修改安装脚本、CI、版本标签或 Release 文案前读取 [发布维护](.github/RELEASING.md)。Release 文案按版本手写，打标签前完成对应 `.github/releases/vX.Y.Z.md`。
- 本地使用已有工具做文档、语法、格式和 diff 等静态检查；推送后由 CI 执行安装与发布工具测试。本机不为验收新增 Docker 镜像、Rust 工具链或目标标准库、SDK、全局依赖及仓库外编译缓存；主代理与子代理均遵守此约定。报告实际验证及边界，提交邮箱使用 GitHub noreply。
