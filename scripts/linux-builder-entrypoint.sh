#!/usr/bin/env bash
# 容器内构建入口（由 scripts/build-linux-docker.sh 触发，勿直接运行）
#
# 挂载约定：
#   /src   —— 仓库（只读）；用 git clone 复制到 /build/app，保证构建基于已提交状态且
#             不污染宿主机的 node_modules / target
#   /out   —— 产物输出目录（对应宿主机 dist-linux/）
#   命名卷 smilex-linux-target / smilex-cargo-registry —— 增量编译与依赖缓存
set -euo pipefail

# AppImage 打包工具在容器内无 FUSE，用解包运行模式
export APPIMAGE_EXTRACT_AND_RUN=1
git config --global --add safe.directory /src

rm -rf /build/app
git clone --quiet /src /build/app
cd /build/app

pnpm install --frozen-lockfile
pnpm tauri build

rm -rf /out/bundle
cp -a target/release/bundle /out/bundle
echo "==> Linux 产物已输出到 dist-linux/bundle/"
