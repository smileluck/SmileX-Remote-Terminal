#!/usr/bin/env bash
# 在本机（macOS）通过 Docker 构建 Linux 安装包（deb / AppImage）
#
# 用法：pnpm build:linux
# 产物：dist-linux/bundle/{deb,appimage}/
#
# 注意：构建基于「当前 git 已提交状态」（容器内 git clone 本仓库），
#       未提交的改动不会进入安装包；首次构建需下载镜像与依赖，耗时较长，
#       之后靠命名卷缓存 cargo registry 与 target，增量构建明显加快。
set -euo pipefail
cd "$(dirname "$0")/.."

IMAGE="smilex-linux-builder"

if ! command -v docker >/dev/null 2>&1; then
  echo "错误：未检测到 Docker。请先安装并启动 Docker Desktop。" >&2
  exit 1
fi

docker build -q -t "$IMAGE" -f scripts/Dockerfile.linux-builder . >/dev/null

mkdir -p dist-linux
docker run --rm \
  -v "$PWD":/src:ro \
  -v "$PWD/dist-linux":/out \
  -v smilex-cargo-registry:/usr/local/cargo/registry \
  -v smilex-linux-target:/build/app/target \
  "$IMAGE"
