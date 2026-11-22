#!/usr/bin/env bash
# 공통 경로. 다른 스크립트가 source 한다.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UPSTREAM="$ROOT/upstream"
AGENT_DIR="$UPSTREAM/agent-client"
BIN="$UPSTREAM/target/release/agent-client"
CONFIGS="$ROOT/configs"
INSTANCES="$ROOT/instances"

export PATH="$HOME/.cargo/bin:$PATH"

die() { echo "error: $*" >&2; exit 1; }

need_instance_name() {
  [ $# -ge 1 ] && [ -n "$1" ] || die "인스턴스 이름이 필요합니다. 예: $0 neon"
  [ -f "$CONFIGS/$1.toml" ] || die "configs/$1.toml 이 없습니다. config/example.toml 을 복사해 만드세요."
}

# instances/<name>/ 을 만든다. agent-client 는 cwd 의 data/config.toml 만 읽으므로
# 인스턴스마다 작업 폴더를 두고, 공용 데이터는 upstream 쪽으로 심볼릭 링크한다.
prepare_instance() {
  local name="$1" dir="$INSTANCES/$1"
  mkdir -p "$dir/data/cache" "$dir/logs"
  local entry
  for entry in "$AGENT_DIR"/data/*; do
    local base; base="$(basename "$entry")"
    case "$base" in config.toml|config.toml.example|cache|google.json) continue ;; esac
    ln -sfn "$entry" "$dir/data/$base"
  done
  cp "$CONFIGS/$name.toml" "$dir/data/config.toml"
  echo "$dir"
}
