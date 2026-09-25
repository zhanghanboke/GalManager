# 界面截图

本目录存放 README 引用的界面截图（窗口尺寸 1360×860，浅色主题下亦可正常显示）。

| 文件名 | 内容 |
| --- | --- |
| `library.png` | 游戏库封面墙（侧边栏分类 / 标签、搜索与筛选工具栏） |
| `detail.png` | 游戏详情页 · 概览（引擎识别、路径信息、游玩概况） |
| `saves.png` | 游戏详情页 · 存档（路径自动探测、多槽位备份与还原） |
| `stats.png` | 数据统计 · 年度游玩报告 |
| `settings.png` | 设置页（转区启动、存档备份、数据管理） |
| `notes.png` | 游戏详情页 · 攻略笔记（内嵌 Markdown） |

## 如何重新生成

截图由 `scripts/mock.html` 配合无头浏览器渲染产出。该页面注入了一份 Tauri IPC Mock，
可在普通浏览器中加载完整前端并填充演示数据，无需启动真实窗口：

```bash
# 1. 启动 Vite 开发服务器
pnpm dev

# 2. 渲染各个界面（以 Edge 为例）
msedge --headless=new --disable-gpu --hide-scrollbars \
  --force-device-scale-factor=1 --virtual-time-budget=9000 \
  --window-size=1360,860 --screenshot=screenshots/library.png \
  "http://localhost:1420/scripts/mock.html"

# 其余界面通过 URL 参数与路由 hash 指定：
#   detail.png   ->  /scripts/mock.html#/game/1
#   saves.png    ->  /scripts/mock.html?tab=saves#/game/1
#   notes.png    ->  /scripts/mock.html?tab=notes#/game/1
#   stats.png    ->  /scripts/mock.html#/stats
#   settings.png ->  /scripts/mock.html#/settings
```

> `mock.html` 仅用于截图，不参与 `vite build` 的生产构建产物。
