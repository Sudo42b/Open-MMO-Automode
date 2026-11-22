#!/usr/bin/env bash
# agent-client 릴리스 빌드. 결과: upstream/target/release/agent-client
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

grep -q '^mod farm;' "$AGENT_DIR/src/main.rs" || die "오버레이가 적용되지 않았습니다. scripts/apply-overlay.sh 를 먼저 실행하세요."
cd "$UPSTREAM"
cargo build --release -p agent-client
ls -l "$BIN"
