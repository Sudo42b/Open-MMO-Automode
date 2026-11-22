#!/usr/bin/env bash
# Ubuntu 24.04 (WSL2 포함) 한 번 설치: 빌드 도구, rustup, 서브모듈, 오버레이, 릴리스 빌드.
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

if command -v apt-get >/dev/null; then
  sudo apt-get update -q
  sudo apt-get install -y -q build-essential pkg-config libssl-dev git curl tmux jq
fi

# 배포판 cargo(1.75)는 Cargo.lock v4 를 읽지 못한다. rustup 의 stable 을 쓴다.
if ! command -v rustup >/dev/null; then
  curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable
  export PATH="$HOME/.cargo/bin:$PATH"
fi
rustup update stable >/dev/null
cargo --version

git -C "$ROOT" submodule update --init --depth 1 || git -C "$ROOT" submodule update --init
"$ROOT/scripts/apply-overlay.sh"
"$ROOT/scripts/build.sh"

mkdir -p "$CONFIGS" "$INSTANCES"
echo
echo "설치 끝. 다음 순서:"
echo "  cp config/example.toml configs/<이름>.toml   # 캐릭터·계획 편집"
echo "  scripts/run.sh <이름>                        # 첫 실행: 구글 기기 로그인 코드 입력"
echo "  scripts/tmux-start.sh <이름>                 # 이후 무인 실행"
