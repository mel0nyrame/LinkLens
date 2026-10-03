# LinkLens 发布维护

## 分支职责

`dev` 保存源码和开发历史；`main` 是无父提交的展示分支，总共只有一次初始化提交。两个分支均公开，发布标签指向 `dev`，所以源码和开发历史在 GitHub 可访问。

`main` 只允许 `README.md`、`AGENTS.md`、`LICENSE`、`.gitignore`、`install.sh`、`install.ps1`、`.github/` 和 `.assets/`。其中 `.assets/` 只同步 README 使用的视觉及编辑源文件。Rust 源码、Cargo 文件、测试、本地 tracker、接口报告和技能包留在 `dev`。

更新展示内容时，从经过验证的 `dev` 文件同步白名单到独立 `main` 工作树，检查文件列表后 amend 初始化提交，不合并 `dev`。更新已公开的 `main` 会改写其单次提交，须获得用户授权并用 `--force-with-lease` 推送。只推送指定的 `main`、`dev` 和本次版本标签，其他本地分支与备份引用保持本地。

## CI 的分流

分流入口是 [ci_changes.py](scripts/ci_changes.py)。只改 Markdown、`docs/`、`.assets/`、`LICENSE` 或 `.gitignore` 时运行本地链接与冲突标记检查。其他改动按代码处理，在 Linux、macOS、Windows 运行测试、格式、Clippy，以及安装和打包工具的验证。首次推送 `dev` 运行完整检查；没有 Cargo 文件的 `main` 运行文档检查。

文档检查入口是 [check_docs.py](scripts/check_docs.py)，只检查仓库内文件链接，不依赖外网请求。历史本地 tracker 和嵌入的技能包由各自 owner 维护，未纳入发布文档检查。

本地与 runner 的验证边界见 [验证环境决策](../docs/adr/0005-validation-environment.md)。本地仅使用已有工具做静态检查；测试、Clippy、构建和平台安装验收在推送后的 CI 执行。`update-acceptance` 沿用 Release 的五个目标和原生 runner，验证官方构建身份，并在四个 Unix 目标运行 PTY；平台环境仅在 runner 准备，不在开发机器安装。

## 打标签前

1. 在 `dev` 更新 Cargo 版本，确认 lockfile，并手写 `.github/releases/vX.Y.Z.md`。文案说明本版本的实际功能、安装方式、构建平台和已知限制；使用 LinkLens / IP 检测的产品表述。完成条件：文案无占位符、与版本功能一致，`check_release.py` 校验通过。
2. 本地运行文档、语法、格式及 diff 等静态检查，推送任务分支后由 CI 执行 `test_release_tools.py`、`cargo test --locked`、Clippy、双命令身份与相关 PTY 检查；安装脚本修改时由 CI 验证系统选择、校验失败和安装路径。完成条件：相关检查通过，合入后 CI 对 `dev` 的检查通过；无需为本地验证安装额外平台环境。
3. 必要时同步 `main` 的发布文件与展示内容，检查 `git rev-list --count main` 为 `1`，且树中只有白名单文件。完成条件：默认分支是 `main`，`dev` 已推送，本版本文件都在提交中。
4. 创建带注释的 `vX.Y.Z` 标签，指向已验证的 `dev` 提交，推送该标签。完成条件：[Release workflow](workflows/release.yml) 的五个构建及 publish 成功，Release 附件和安装脚本实际下载检查通过。

## 自动构建与发布

Release 校验标签与 Cargo 版本一致，要求对应文案文件。[build_release.py](scripts/build_release.py) 为五个平台执行测试与构建，并将 `LINKLENS_RELEASE_VERSION`（`vX.Y.Z` 标签）与 `LINKLENS_RELEASE_TARGET`（构建目标）传入 Cargo，供两个命令内嵌官方来源身份。普通源码构建不设置这两个变量。该身份用于来源与附件选择，不是签名认证；版本、目标或文案校验失败时不开始构建。打包 `linklens`、`llens`、README 和 MIT 协议。Linux 使用 musl，macOS 使用各架构的原生 runner，Windows 使用 x64 MSVC 并静态链接 C 运行库。SHA-256 汇总在 `SHA256SUMS`。

全部构建通过后才发布：先创建 draft，再上传附件，最后用文案文件公开 Release。公开后的独立 `verify-published` 作业实际下载五个平台附件，检查附件列表与 SHA-256，并在 Linux runner 用安装脚本安装已发布包、验证两个命令的版本和官方身份。该作业失败时只重试验收，不重新上传公开附件。流程不使用自动生成的发布说明；构建失败时修复 `dev` 并发布新版本，或对未公开的失败运行执行重试，保持已公开标签不可变。

安装入口是根目录 `install.sh`（macOS / Linux / Windows Git Bash）和 `install.ps1`（原生 Windows PowerShell）。包名、目标名称及校验文件是脚本与 workflow 的共同协议，修改时一起验证。GitHub 托管二进制未进行 Apple Developer 或 Windows 代码签名。

Windows 安装脚本以 UTF-8 with BOM 保存，兼容系统自带 PowerShell 5.1 的中文解析；更改脚本后必须同时通过 PowerShell 5.1 和 7 的安装验证。下载使用 basic parsing，校验清单先保存为文件再读取，避免二进制响应内容类型造成解析差异。
