#!/usr/bin/env bash
# 인스턴스 하나를 전경에서 실행한다. 첫 실행(구글 로그인 코드 입력)은 이걸로 한다.
#   scripts/run.sh <이름>
# 무인 실행은 tmux-start.sh 또는 systemd 유닛이 이 스크립트를 반복 호출한다.
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
need_instance_name "$@"
[ -x "$BIN" ] || die "빌드된 바이너리가 없습니다. scripts/build.sh 를 먼저 실행하세요."

name="$1"
dir="$(prepare_instance "$name")"
cd "$dir"
export RUST_LOG="${RUST_LOG:-info}"
echo "[$name] cwd=$dir config=configs/$name.toml"
exec "$BIN"
