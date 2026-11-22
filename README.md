# Open-MMO-Automode

[OpenMMO](https://github.com/Julian-adv/OpenMMO) 캐릭터를 브라우저 없이, 사람이 붙어 있지 않아도 사냥시키는 무인 실행 모드다. OpenMMO 가 공식으로 배포하는 `agent-client`(사람 클라이언트와 같은 WebSocket 프로토콜로 접속하는 러스트 프로그램)에 **LLM 대신 규칙으로 판단하는 `farm` 백엔드**를 얹고, 구글 로그인·시간표·보급·로그 정리를 tmux/systemd 로 돌릴 수 있게 스크립트로 묶었다.

OpenMMO 서버에는 접속을 끊어도 캐릭터가 알아서 사냥하는 기능이 없다. 이 프로젝트도 서버 쪽 기능이 아니라 **항상 켜 둔 리눅스 머신(WSL2 포함)에서 돌아가는 클라이언트**다. 머신이 꺼지면 캐릭터도 로그아웃된다.

## 무엇이 되나

- 구글 계정으로 로그인한다. 비밀번호를 적지 않는다. 처음 한 번 터미널에 뜨는 URL과 코드를 아무 브라우저에서 입력하면 리프레시 토큰이 인스턴스 폴더에 저장되고, 그 뒤로는 묻지 않는다.
- 설정에 적은 캐릭터로 들어가 정해진 던전의 층 범위를 돌며 가장 가까운 몬스터를 공격한다. 층에 몬스터가 안 보이면 다음 층으로 내려가고, 범위 끝에서 처음 층으로 돌아온다.
- HP 가 기준 밑이면 물약, 배고프면 음식을 쓴다. 물약이 떨어지거나 HP 가 위험하면 귀환 주문서로 마을에 가서 전리품을 팔고 물약·음식·주문서를 목표 수량까지 산 뒤 다시 들어간다. 죽으면 부활하고 같은 흐름을 탄다.
- 해당 층 열쇠를 들고 있으면 상자를 연다. 보관 목록(`keep`)의 물건은 팔지 않는다.
- 로컬 시각 기준 시간표(`schedule`)로 던전·층·물약 기준을 바꾸거나 `rest = true` 로 마을에서 쉬게 할 수 있다. 요일 지정도 된다.
- 매 턴 상태를 JSON 으로 남기고(`scripts/status.sh`), 판단 기록은 하루 한 파일로 쌓아 3일 뒤 지운다.

LLM 호출이 없으므로 토큰 비용이 0 이다. 전투 명중·경로 탐색·층 이동은 전부 서버와 agent-client 의 기존 코드가 한다. `farm` 은 "무엇을 할지"만 JSON 으로 답한다.

## 구조

```
upstream/            OpenMMO 서브모듈 (59bf37be 에 고정)
overlay/             upstream 에 복사해 넣는 새 파일 — agent-client/src/farm/
patches/             upstream 기존 파일 수정분 (main.rs, orchestrator.rs, Cargo.toml)
config/example.toml  인스턴스 설정 예시
scripts/             설치·빌드·실행·상태 스크립트
systemd/             tmux 대신 쓸 사용자 유닛
configs/<이름>.toml  (git 제외) 인스턴스별 설정
instances/<이름>/    (git 제외) 작업 폴더: data/config.toml, data/google.json, logs/
```

`farm` 백엔드는 `agent-client` 의 `LlmBackend` 트레이트를 구현한다. 드라이버가 LLM 에게 묻는 자리에서 공유 상태(HP·가방·주변 몬스터·바닥 아이템·층·골드·상인·밤낮)를 읽어 `{"thought": ..., "actions": [...]}` 를 돌려준다. 액션은 agent-client 가 이미 받아들이는 것만 쓴다: `attack`, `move`(층 지정), `use`, `pickup`, `sell`, `buy`, `open_chest`, `respawn`.

- `overlay/agent-client/src/farm/config.rs` — `[npcs.farm]` 설정과 시간표 해석
- `overlay/agent-client/src/farm/policy.rs` — 판단 규칙. 순수 함수라 서버 없이 단위 테스트한다
- `overlay/agent-client/src/farm/status.rs` — 상태 JSON 기록
- `patches/0001-farm-backend.patch` — `llm = "farm"` 추가, 백엔드 생성 지점 연결, `chrono` 의존성

## 설치 (Ubuntu 24.04 / WSL2)

```bash
git clone https://github.com/Sudo42b/Open-MMO-Automode.git
cd Open-MMO-Automode
scripts/install-wsl.sh        # 빌드 도구, rustup stable, 서브모듈, 오버레이, 릴리스 빌드 (처음 5분쯤)
```

배포판 패키지의 cargo 1.75 로는 빌드되지 않는다 (`Cargo.lock` v4). 스크립트가 rustup 을 설치한다. WSL 에서는 `/mnt/c` 가 아니라 리눅스 홈(`~`)에 클론해야 빌드가 빠르다.

구글 로그인용 `client_secret` 은 저장소에 없다. OpenMMO 릴리스의 `agent-client-*.tar.gz` 를 받아 그 안의 `data/config.toml` 에서 `GOCSPX-` 로 시작하는 값을 복사한다. 이유와 자세한 절차는 `upstream/doc/AGENT_CLIENT_QUICKSTART.md` 에 있다.

## 설정

```bash
mkdir -p configs
cp config/example.toml configs/neon.toml
$EDITOR configs/neon.toml
```

`config/example.toml` 에 모든 항목이 주석과 함께 있다. 꼭 확인할 것:

| 항목 | 뜻 |
|---|---|
| `[auth] client_secret` | 위에서 복사한 값. 환경변수 `GOOGLE_CLI_CLIENT_SECRET` 으로 줘도 된다 |
| `[[npcs]] character_name` | 들어갈 캐릭터. 계정에 없으면 `character_class` 로 새로 만든다 |
| `[npcs.farm.plan] dungeon`, `floors` | 기본 사냥터. 던전 id(`old_crypt`)나 표시 이름(`Old Crypt`) |
| `potion_pct`, `emergency_pct` | 물약 기준, 귀환 기준 HP% |
| `potion_target`, `food_target`, `return_scroll_target` | 마을에서 채울 수량 |
| `loot`, `sell`, `keep` | 주울 것, 팔 것, 절대 안 팔 것 |
| `[[npcs.farm.schedule]]` | `at = "HH:MM-HH:MM"`, `days = ["sat","sun"]`, 바꿀 항목만 적는다. 나중 창이 이긴다 |
| `watch_port` | 인스턴스마다 다르게. `http://127.0.0.1:<포트>` 에서 지도와 판단을 본다 |

구글 계정 하나에 프로세스 하나다. 캐릭터(계정)를 여럿 돌리려면 `configs/` 에 파일을 여러 개 두고 각각 시작한다. 같은 계정을 브라우저에서 동시에 열면 나중에 붙은 쪽이 먼저 붙은 쪽을 끊는다.

## 실행

```bash
scripts/run.sh neon            # 첫 실행. 로그인 URL과 코드가 뜨면 브라우저에서 입력. Ctrl-C 로 종료
scripts/tmux-start.sh neon     # 무인 실행 (죽으면 10초 뒤 자동 재시작)
tmux attach -t automode-neon   # 들여다보기, Ctrl-b d 로 빠져나옴
scripts/status.sh              # 모든 인스턴스 한 줄 요약
watch -n 5 scripts/status.sh
scripts/tmux-stop.sh neon      # 중지 (이름 생략 시 전부)
```

GNU screen 을 쓰면 `screen -dmS automode-neon scripts/run.sh neon` 이면 된다. systemd 로 돌리려면 `systemd/automode@.service` 머리말을 따른다.

로그:

- `logs/<이름>.log` — 프로세스 출력 (tmux 실행 시)
- `instances/<이름>/logs/farm/<이름>.json` — 최신 상태 (HP, 층, 골드, 물약, 사망 수, 마지막 판단)
- `instances/<이름>/logs/transcripts/<이름>/<날짜>.log` — 턴마다의 판단과 액션, 3일 보관

## 시간표 동작

- 매 턴 로컬 시각으로 활성 창을 고른다. 던전 안에서 창이 바뀌어 사냥터가 달라지면, 다음 마을행 뒤에 새 던전으로 간다. 층 범위만 바뀌면 그 자리에서 범위 안으로 이동한다.
- `rest = true` 창에 들어가면 귀환 주문서로 나와 마을 광장에서 기다린다. 창이 끝나면 다시 들어간다.
- 밤에는 리카가 자므로 윅과 거래한다. 둘 중 보이는 쪽을 우선한다.

## 업스트림 갱신

```bash
cd upstream && git fetch && git checkout <새 커밋> && cd ..
scripts/apply-overlay.sh       # 패치가 안 맞으면 patches/ 를 손본다
scripts/build.sh
```

프로토콜 번호가 오르면 서버가 접속을 거절하므로(`Protocol vN required`), 서버 배포 뒤에는 서브모듈을 올려 다시 빌드한다.

## 한계와 주의

- 서버 쪽 무접속 모드가 아니다. 머신과 네트워크가 살아 있어야 한다.
- 몬스터 선택은 "같은 층, 사정거리 안, 가장 가까운 것"뿐이다. 보스 회피나 무리 수 판단은 없다. 감당 안 되는 층을 적으면 죽고 부활하기를 반복한다 (`deaths` 가 올라간다).
- 부활 지점(여관 2층)에서 던전까지의 길 찾기는 agent-client 의 이동 코드에 맡긴다. 실내에서 막히면 `move` 가 실패 이벤트를 내고 30초 뒤 다시 시도한다.
- 거래는 `buy` 한 번에 한 개씩 산다. 물약 5개면 다섯 턴이다.
- 층은 내려가기만 한다. agent-client 의 이동 코드가 위층으로 올라가는 `move` 에서 멈추는 일이 있어, 범위의 마지막 층에서 `linger_secs` 동안 리스폰을 기다린 뒤 **귀환 주문서로 나와** 보급하고 다시 들어간다. 한 바퀴에 주문서 한 장(800골드)이 든다.
- `min_interval_secs` 같은 판단 주기 값은 파일 맨 위가 아니라 `[[npcs]]` 안에 적어야 적용된다. farm 모드는 설정과 무관하게 1초/2초 상한을 건다.
- 가족 창고 입출고, 강화, 낚시, 파티는 하지 않는다. 필요하면 `policy.rs` 에 규칙을 더한다.

## 개발

```bash
cd upstream && cargo test --release -p agent-client farm     # 정책·시간표 단위 테스트
```

`overlay/` 와 `patches/` 를 고친 뒤에는 `scripts/apply-overlay.sh` 로 다시 입힌다. 패치는 `git -C upstream diff -- agent-client/src/main.rs agent-client/src/orchestrator.rs agent-client/Cargo.toml > patches/0001-farm-backend.patch` 로 다시 뽑는다.

## 라이선스

`upstream/` 은 OpenMMO 의 PolyForm Noncommercial License 1.0.0 을 따른다. `overlay/` 와 `patches/` 는 그 코드의 파생물이므로 같은 조건으로 배포한다. 스크립트와 문서도 동일하게 둔다. 상업적 사용은 OpenMMO 의 라이선스가 허용하지 않는다.
