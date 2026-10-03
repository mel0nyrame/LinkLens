# LinkLens 发布维护

## 展示入口

`main` 保持一次无父提交的初始化提交，仅包含 `README.md`、`AGENTS.md`、`LICENSE`、`.gitignore`、`install.sh`、`install.ps1`、`.github/` 与 `.assets/`。`.assets/` 保存 README 使用的视觉及其编辑源文件。

更新展示内容时，在本分支修改这些文件，完成检查后 amend 初始化提交，保持 `git rev-list --count main` 为 `1`。改写已公开提交须获得用户授权，使用 `--force-with-lease` 推送，GitHub 默认分支保持 `main`。

## 检查

文档检查入口为 [check_docs.py](scripts/check_docs.py)，检查本地 Markdown 链接与冲突标记，不依赖外网。本地仅使用已有工具做文档、语法、格式和 diff 等静态检查；推送后由 CI 对安装与发布工具变更执行 [工具测试](scripts/test_release_tools.py)，Windows 安装入口另由 [PowerShell 测试](scripts/test_install.ps1) 验证。

本机不为验收新增 Docker 镜像、Rust 工具链或目标标准库、SDK、全局依赖及仓库外编译缓存。[CI workflow](workflows/ci.yml) 根据变更路径分流；有 Cargo 工程且代码发生变化时才运行 Rust 测试、格式与 Clippy。分流入口为 [ci_changes.py](scripts/ci_changes.py)。

## 版本发布

发布说明由维护者针对版本手写，唯一来源是 `.github/releases/vX.Y.Z.md`。文案说明实际功能、安装方式、构建平台及已知限制，产品表述使用 LinkLens / IP 检测。每个版本都应完成文案再创建 `vX.Y.Z` 标签，文案、标签与 Cargo 版本须匹配。

[Release workflow](workflows/release.yml) 在发布标签的版本提交上运行，要求该提交含完整 Cargo 工程；展示入口不作为程序构建源。校验版本和文案后，由 [build_release.py](scripts/build_release.py) 注入官方版本与目标身份，测试并构建 Windows x64、macOS Intel / Apple Silicon、Linux x64 / ARM64。全部构建成功才上传压缩包与 `SHA256SUMS`，公开使用手写文案的 Release。

包内包含 `linklens`、`llens`、README 与 MIT 协议。Linux 使用 musl，Windows 静态链接 C 运行库，macOS 和 Windows 产物目前未进行平台代码签名。安装入口会选择系统与架构，校验 SHA-256，下载失败或校验失败时停止安装。

发布完成条件：五个构建、publish 及 `verify-published` 成功。公开后的独立 `verify-published` 作业实际下载五个平台附件，核对附件列表与 SHA-256，并在 Linux runner 用安装脚本安装已发布包、验证两个命令的版本和官方身份。该作业失败时只重试验收，不重新上传公开附件。已公开的版本标签保持不可变。

Windows 安装脚本以 UTF-8 with BOM 保存，兼容系统自带 PowerShell 5.1 的中文解析；更改脚本后必须同时通过 PowerShell 5.1 和 7 的安装验证。下载使用 basic parsing，校验清单先保存为文件再读取，避免二进制响应内容类型造成解析差异。
