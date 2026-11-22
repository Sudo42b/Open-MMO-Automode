#!/usr/bin/env bash
# upstream/ (OpenMMO 서브모듈)에 farm 백엔드를 입힌다.
#   overlay/  : 새로 추가되는 파일 (그대로 복사)
#   patches/  : 기존 파일 수정 (git apply)
# 이미 적용돼 있으면 overlay 복사만 다시 하고 패치는 건너뛴다.
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

[ -f "$UPSTREAM/Cargo.toml" ] || die "upstream/ 이 비어 있습니다. git submodule update --init 을 먼저 실행하세요."

cp -r "$ROOT/overlay/." "$UPSTREAM/"
echo "overlay 복사 완료"

if grep -q '^mod farm;' "$AGENT_DIR/src/main.rs"; then
  echo "패치는 이미 적용되어 있습니다."
else
  git -C "$UPSTREAM" apply --check "$ROOT/patches/0001-farm-backend.patch"
  git -C "$UPSTREAM" apply "$ROOT/patches/0001-farm-backend.patch"
  echo "패치 적용 완료"
fi
