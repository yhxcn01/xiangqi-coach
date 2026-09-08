#!/usr/bin/env bash
# 象棋思考教练 — 一键部署到 GitHub Pages（free-for.dev 免费托管）
#
# 前置：准备一个 yhxcn01 的 GitHub Token（classic，勾选 repo 权限）：
#   GitHub → Settings → Developer settings → Personal access tokens →
#   Tokens (classic) → Generate new token (classic) → 勾选 repo → 生成并复制
#
# 用法：
#   ./deploy.sh ghp_你的token
#
# 说明：token 只在本次运行中用于「创建仓库 + 推送 + 开启 Pages」，
# 不会被写入任何文件或 git 凭据存储；部署完成后可在 GitHub 上吊销。

set -euo pipefail

TOKEN="${1:-}"
if [ -z "$TOKEN" ]; then
  echo "用法: ./deploy.sh <github_token>"
  exit 1
fi

OWNER="yhxcn01"
REPO="xiangqi-coach"
API="https://api.github.com"
AUTH="Authorization: token $TOKEN"

command -v git >/dev/null || { echo "需要 git"; exit 1; }
command -v curl >/dev/null || { echo "需要 curl"; exit 1; }
git rev-parse --is-inside-work-tree >/dev/null 2>&1 || { echo "请在项目目录运行"; exit 1; }

echo "==> 1/3 创建仓库 $OWNER/$REPO（已存在则跳过）"
HTTP=$(curl -s -o /tmp/xq_repo.json -w "%{http_code}" -X POST "$API/user/repos" \
  -H "$AUTH" -H "Accept: application/vnd.github+json" \
  -d "{\"name\":\"$REPO\",\"description\":\"象棋思考教练 - Rust 引擎 + WASM，手机/平板可玩\",\"private\":false}" || true)
if [ "$HTTP" = "201" ]; then
  echo "    仓库已创建"
elif grep -q "already exists" /tmp/xq_repo.json 2>/dev/null; then
  echo "    仓库已存在，继续"
elif [ "$HTTP" = "401" ] || [ "$HTTP" = "403" ]; then
  echo "    token 无效或权限不足（需要 repo 权限）："; cat /tmp/xq_repo.json; exit 1
else
  echo "    创建返回 $HTTP："; cat /tmp/xq_repo.json; exit 1
fi

echo "==> 2/3 推送代码（token 仅本次使用，不写入凭据存储）"
git add -A
git diff --cached --quiet || git commit -q -m "部署前同步"
GIT_TERMINAL_PROMPT=0 git \
  -c credential.helper= \
  -c credential.helper='!f() { echo "username=yhxcn01"; echo "password='$TOKEN'"; }; f' \
  push -u "https://github.com/$OWNER/$REPO.git" main:main
echo "    推送完成"

echo "==> 3/3 开启 GitHub Pages（main 分支根目录，已开启则跳过）"
HTTP=$(curl -s -o /tmp/xq_pages.json -w "%{http_code}" -X POST "$API/repos/$OWNER/$REPO/pages" \
  -H "$AUTH" -H "Accept: application/vnd.github+json" \
  -d '{"source":{"branch":"main","path":"/"}}' || true)
if [ "$HTTP" = "201" ]; then
  echo "    Pages 已开启"
elif [ "$HTTP" = "409" ]; then
  echo "    Pages 已开启过，继续"
else
  echo "    Pages 返回 $HTTP："; cat /tmp/xq_pages.json
  echo "    可手动开启：仓库 Settings → Pages → Branch: main → Save"
fi

echo
echo "✅ 部署指令已全部下发。首次构建约 1~3 分钟，之后用手机/平板访问："
echo "   https://$OWNER.github.io/$REPO/"
