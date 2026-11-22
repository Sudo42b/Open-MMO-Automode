#!/usr/bin/env bash
# 인스턴스를 tmux 세션(automode-<이름>)에서 무인 실행한다. 죽으면 10초 뒤 다시 띄운다.
#   scripts/tmux-start.sh neon [other ...]
#   tmux attach -t automode-neon     # 들여다보기 (Ctrl-b d 로 나옴)
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
[ $# -ge 1 ] || die "인스턴스 이름을 하나 이상 적으세요."
command -v tmux >/dev/null || die "tmux 가 없습니다. apt-get install tmux"

mkdir -p "$ROOT/logs"
for name in "$@"; do
  need_instance_name "$name"
  session="automode-$name"
  if tmux has-session -t "$session" 2>/dev/null; then
    echo "[$name] 이미 실행 중: tmux attach -t $session"
    continue
  fi
  tmux new-session -d -s "$session" -c "$ROOT" \
    "while true; do \"$ROOT/scripts/run.sh\" \"$name\" 2>&1 | tee -a \"$ROOT/logs/$name.log\"; echo \"[$name] 종료됨, 10초 뒤 재시작 (Ctrl-C 로 중단)\"; sleep 10; done"
  echo "[$name] 시작: tmux attach -t $session"
done
