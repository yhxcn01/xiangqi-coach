# 部署指南 — 上线到外网（手机/平板可玩）

本项目的 `web/` 目录是**纯静态站点**（棋盘 + WASM 引擎全部在浏览器里运行，无服务器），
按 [free-for.dev](https://free-for.dev/#/tools) 收录的免费托管 **GitHub Pages** 部署，费用为零。

## 方式一：一条命令（推荐）

1. **生成 Token**（1 分钟）：
   浏览器登录 `yhxcn01` 账号 → GitHub → 右上角头像 → **Settings** → 左下 **Developer settings**
   → **Personal access tokens** → **Tokens (classic)** → **Generate new token (classic)**
   → Note 随意填，Expiration 选 7 天即可 → 只勾 **repo** → 生成后复制 `ghp_` 开头的 token。

2. **部署**（本项目目录下，Git Bash）：

   ```bash
   ./deploy.sh ghp_粘贴你的token
   ```

   脚本自动完成：创建仓库 `yhxcn01/xiangqi-coach` → 推送代码 → 开启 Pages。
   token 不落盘、不进 git 凭据存储，部署完可在 GitHub 上吊销。

3. 等 1~3 分钟构建，手机/平板/电脑访问：

   **https://yhxcn01.github.io/xiangqi-coach/**

## 方式二：网页手动操作

1. 登录 `yhxcn01`，在 github.com/new 创建仓库，名字 `xiangqi-coach`，Public，**不要**勾选任何初始化选项。
2. 本项目目录下执行：

   ```bash
   git remote add origin https://github.com/yhxcn01/xiangqi-coach.git
   git push -u origin main        # 弹窗登录时选 yhxcn01 账号
   ```

3. 仓库页面 → **Settings** → 左侧 **Pages** → Branch 选 `main` / `(root)` → **Save**。
4. 等 1~3 分钟，访问 https://yhxcn01.github.io/xiangqi-coach/

## 部署后：开启 AI 讲解

页面右下 **⚙️ AI 设置** 里填入 API Key 即可。推荐智谱 **GLM-4.7-Flash**（免费模型）：
在 [open.bigmodel.cn](https://open.bigmodel.cn/) 注册 → API Keys → 创建并复制。
接口地址和模型保持默认即可。Key 只存在访客自己设备的浏览器里。

## 常见问题

- **首次打开 404**：Pages 首次构建要 1~3 分钟，稍后刷新。
- **棋盘空白**：确认网址是 `https://yhxcn01.github.io/xiangqi-coach/`（路径区分大小写）。
- **更新版本**：改完代码后重新构建 WASM 并提交推送即可：

  ```bash
  cargo build -p xqwasm --target wasm32-unknown-unknown --release
  cp target/wasm32-unknown-unknown/release/xqwasm.wasm web/
  git add -A && git commit -m "更新" && git push
  ```

- **本地预览**：项目目录下 `python -m http.server 8123`（在 `web/` 里运行），打开 http://127.0.0.1:8123/
  注意：不能用 `file://` 直接打开（Worker 与 fetch 限制）。
