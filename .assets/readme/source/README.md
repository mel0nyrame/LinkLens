# LinkLens README 视觉源文件

面向需要在终端查看 IP 检测结果的用户。阅读顺序为产品介绍、真实截图、检测总览、首次运行、键盘操作、历史记录与数据来源。

## 视觉约定

- 浅色背景 `#f4faf8`，正文 `#193d38`，强调 `#216659`，辅助色 `#50716b`，淡薄荷色 `#e1efea`。
- 系统无衬线字体承载标题与说明，等宽字体承载命令、路径和按键。
- 统一 1200 单位画布、24 单位外框圆角和明确的内容分组；终端提示符、透镜、键帽和记录卡片分别对应产品、操作和历史。
- 真实结果使用仓库截图。生成插画仅表达概念，不承载检测数值。

## 发布图与源文件

| 发布图 | 修改入口 |
| --- | --- |
| `../hero.png` | `hero-layout.svg`、`hero-subject.png`、`hero-prompt.txt` |
| `../controls.png` | `controls-layout.svg`、`controls-subject.png`、`controls-prompt.txt` |
| `../history.png` | `history-layout.svg`、`history-subject.png`、`history-prompt.txt` |
| `../overview.svg` | 直接编辑 SVG |
| `../first-run.svg` | 直接编辑 SVG |
| `../data-sources.svg` | 直接编辑 SVG |

三个透明主体均由内置 ImageGen 生成。提示词文件保存实际使用的完整提示词；标题、命令和路径由 SVG 排版。

混合排版源通过相邻 PNG 的相对路径加载素材。修改后需在支持本地图片引用的 SVG 渲染器中检查，并按两倍尺寸导出对应 PNG。README 引用合成 PNG，不直接引用带栅格素材的源 SVG。

验收宽度为桌面 900 CSS 像素和手机 360 CSS 像素。完整功能、命令、按键与路径同时保留在 Markdown 中，图片失败或缩小时仍可阅读。
