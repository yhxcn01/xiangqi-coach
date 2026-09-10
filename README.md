# 象棋思考教练

引擎是大脑，AI 是嘴 —— 每步棋都给你讲明白。灵感来自公众号文章《我用 Rust 写了个象棋教学软件，被群友直接封神》。

**在线版**：https://yhxcn01.github.io/xiangqi-coach/ （手机/平板/电脑通用，纯静态免费托管）

## 功能

- **人机对弈**：自研 α-β 搜索引擎（吃子静态搜索）执黑
- **AI 讲解当前局面**：对方意图 / 为什么推荐那步 / 哪些是坑，支持真流式输出
- **AI 讲解这枚棋**：任意棋子（含对方棋子——讲它的威胁与你的应对）
- **走子即时评分**：每步显示「✓ 与引擎最佳一致」或「亏约 X 分（最佳 Y）」
- **复盘分析**：终局后逐手标出亏损着法
- **全局态势上下文**：文本棋盘图、对局轨迹、子力对比、过河兵力、受威胁子力注入 AI 提示词
- **本地续局**：刷新不丢局；PWA 支持添加到主屏幕
- **AI 降级**：未配 Key 时自动降级为引擎基础提示，宁可不讲也不乱讲

## AI 配置

推荐智谱 **GLM-4.7-Flash**（免费）：

| Key 类型 | 接口地址 | 模型 |
|---|---|---|
| 普通 API Key（含免费） | `https://open.bigmodel.cn/api/paas/v4` | `glm-4.7-flash` |
| GLM Coding Plan 订阅 Key | `https://open.bigmodel.cn/api/coding/paas/v4` | `glm-5.3-flash` |

也兼容任何 OpenAI 兼容接口（DeepSeek 等）。Key 只存在访客自己设备的浏览器里。

## 架构

```
crates/core    规则 / α-β 引擎 / 讲解文案（纯计算，双端共用）
crates/server  axum Web 服务（本地开发版，cargo run -p xiangqi-coach）
crates/wasm    C ABI 导出 → docs/xqwasm.wasm（线上版，引擎在浏览器运行）
docs/          纯静态前端（GitHub Pages 部署根目录）
```

## 与原文章的差异说明

原文使用 pikafish（皮卡鱼）专业引擎。本项目为保证纯静态部署（引擎编译进 WASM、无需服务器），采用自研 α-β 搜索引擎（release 深度 4 层 + 吃子静态搜索 8 层），棋力低于 pikafish 但结论同样是引擎确定性计算、AI 不参与计算，符合「引擎是大脑，AI 是嘴」的核心理念。在此之上额外实现了原文没有的：走子即时评分、复盘分析、对方棋子讲解、全局态势上下文、PWA。

## 本地开发

- 线上版预览：`cd docs && python -m http.server 8123` → http://127.0.0.1:8123/ （不要用 file:// 直接打开）
- 服务端版：`npm run dev`（即 `cargo run -p xiangqi-coach`）→ http://localhost:7100/
- 改引擎后重建 WASM：`cargo build -p xqwasm --target wasm32-unknown-unknown --release && cp target/wasm32-unknown-unknown/release/xqwasm.wasm docs/`

部署见 [DEPLOY.md](DEPLOY.md)。
