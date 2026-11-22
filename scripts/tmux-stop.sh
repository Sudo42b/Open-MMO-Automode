#!/usr/bin/env bash
# tmux 세션을 끝낸다. 이름을 안 주면 automode-* 전부.
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

if [ $# -eq 0 ]; then
  mapfile -t names < <(tmux ls -F '#S' 2>/dev/null | sed -n 's/^automode-//p')
  [ ${#names[@]} -gt 0 ] || { echo "실행 중인 인스턴스가 없습니다."; exit 0; }
  set -- "${names[@]}"
fi
for name in "$@"; do
  if tmux kill-session -t "automode-$name" 2>/dev/null; then
    echo "[$name] 중지"
  else
    echo "[$name] 실행 중이 아님"
  fi
done
