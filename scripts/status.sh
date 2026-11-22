#!/usr/bin/env bash
# 각 인스턴스의 상태 JSON(logs/farm/<이름>.json)을 한 줄씩 보여준다.
#   scripts/status.sh            # 전부
#   scripts/status.sh neon       # 하나
#   watch -n 5 scripts/status.sh # 5초마다 갱신
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
command -v jq >/dev/null || die "jq 가 없습니다. apt-get install jq"

shopt -s nullglob
files=()
if [ $# -ge 1 ]; then
  for n in "$@"; do files+=("$INSTANCES/$n"/logs/farm/*.json); done
else
  files=("$INSTANCES"/*/logs/farm/*.json)
fi
[ ${#files[@]} -gt 0 ] || { echo "상태 파일이 없습니다. 아직 한 턴도 돌지 않았거나 status_file 이 꺼져 있습니다."; exit 0; }

printf '%-14s %-22s %-7s %5s %7s %4s %4s %4s %5s  %s\n' 캐릭터 갱신 HP 층 골드 물약 음식 귀환 사망 판단
for f in "${files[@]}"; do
  jq -r '[
      .character, (.updated|sub("\\.[0-9]+";"")|sub("T";" ")|.[0:19]),
      "\(.hp)/\(.max_hp)", (.floor|tostring), (.gold|tostring),
      (.potions|tostring), (.food|tostring), (.return_scrolls|tostring), (.deaths|tostring), .thought
    ] | @tsv' "$f" |
  while IFS=$'\t' read -r c u hp fl g p fo rs d th; do
    session="automode-$(basename "$(dirname "$(dirname "$(dirname "$f")")")")"
    live="" ; tmux has-session -t "$session" 2>/dev/null || live=" (tmux 없음)"
    printf '%-14s %-22s %-7s %5s %7s %4s %4s %4s %5s  %s%s\n' "$c" "$u" "$hp" "$fl" "$g" "$p" "$fo" "$rs" "$d" "$th" "$live"
  done
done
