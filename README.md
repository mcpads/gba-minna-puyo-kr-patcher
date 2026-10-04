# 모두의 뿌요뿌요 (GBA) 한글 패처

GBA용 《모두의 뿌요뿌요》(みんなでぷよぷよ) 일본판에 한글 패치를 적용하는 Rust 코드입니다. 원본 식별, LZ10 압축·해제, 대사 스트림 파싱과 재배치, 글리프 마스크에서 타일·맵을 만드는 화면별 변환, ARM7TDMI 훅, 쓰기 범위 감사와 BPS 생성·재적용 검증을 제공합니다.

배포용 BPS와 적용 방법은 [뿌요뿌요 시리즈 한글 번역 프로젝트](https://github.com/mcpads/puyo-puyo-kr-patch/tree/main/gba-minna-puyo)에서 제공합니다.

## 제공하지 않는 것

이 저장소에는 원본 ROM, 패치를 적용한 ROM, 번역 초안 JSON, 글리프 마스크, 한국어 그래픽, 원문 전사와 문안 결정 기록이 없습니다. 글리프 마스크를 폰트에서 만드는 준비 도구도 포함하지 않습니다. 따라서 이 저장소만으로는 배포 패치를 다시 만들 수 없습니다. 아래 입력을 직접 갖춘 경우에만 `build-foundation`이 ROM과 BPS를 생성합니다.

## 빌드와 테스트

```bash
cargo build --release
cargo test
```

기본 테스트는 합성 입력만 사용합니다. 글리프 마스크가 필요한 테스트는 `#[ignore = "requires ..."]`로 필요한 입력을 밝혀 두었습니다. 입력을 갖춘 뒤 `cargo test -- --ignored`로 실행하며, 입력이 없으면 성공으로 넘어가지 않고 실패합니다.

## 지원 원본

| 원본 | 게임 코드 / 리비전 | 크기 | SHA-256 |
| --- | --- | --- | --- |
| `Minna de Puyo Puyo (Japan) (En,Ja).gba` | APYJ / 0 | 8,388,608바이트 | `c6c5c6d73329d4af28d9a52ab0a000fdb6038c60969a0a0fce6b4a0ee3a7cc19` |

모든 명령은 시작할 때 원본의 크기, SHA-256, 헤더를 [`config/source.json`](config/source.json)과 대조하고 다르면 중단합니다.

```bash
cargo run --release -- verify-source --rom path/to/original.gba
```

## 빌드 입력

`build-foundation`은 작업 디렉터리 기준 상대 경로에서 다음 입력을 읽습니다. 입력 파일이 없으면 오류로 중단하며, 번역 초안·마스크·`config` 입력은 없는 경로를 오류에 밝힙니다.

| 입력 | 경로 | 비고 |
| --- | --- | --- |
| 번역 초안 | `assets/translations/*.json` | 화면·기능별 한국어 문안 |
| 글리프 마스크 | `assets/fonts/*-masks.json`, `bootstrap-mask.json`, `counter-label.json` | 폰트에서 미리 그린 글자 비트맵과 배치 |
| 손질 글리프 대체표 | `assets/fonts/galmuri11-glyph-overrides.json`, `galmuri9-glyph-overrides.json` | Galmuri에서 파생한 수정 글리프 |
| 한국어 그래픽 | `assets/graphics/{character-surfaces,course-surfaces,ticket-localized,effects}/` | 각 `manifest.json`과 그것이 가리키는 PNG·JSON |
| 원문 전사와 문안 결정 | `config/*.json` (`source.json` 제외) | 원문 해시 결속, 문안 결정, 원본 글리프 대응표 등 |

각 입력이 요구하는 파일 이름은 소스의 `crate::managed_input::read` 호출과 그래픽 매니페스트 읽기 코드에서 확인할 수 있습니다. 마스크와 대체표는 다음 폰트로 만들었습니다. 빌드는 폰트 파일을 읽지 않지만, 마스크에 기록된 폰트 SHA-256이 아래 값과 다르면 진행하지 않습니다. 각 폰트의 라이선스는 배포처에서 확인하세요.

| 폰트 | 배포처 | SHA-256 |
| --- | --- | --- |
| Galmuri11 v2.40.3 | [Galmuri](https://github.com/quiple/galmuri) | `2c709890595668f7bdb6df408420fda957dde0288e95b31a1cc17a2ab98b4b4f` |
| Galmuri9 v2.40.3 | [Galmuri](https://github.com/quiple/galmuri) | `5cb68052ee0a15571747e91c20f145e24b51bb459c6cd58226fafee78d9c0b16` |
| Galmuri7 v2.40.3 | [Galmuri](https://github.com/quiple/galmuri) | `3882bd35066c26b0392cd4963ff9b3c151041dec34adc9d5633d137d1d9b9855` |
| Neo둥근모 v1.600 | [neodgm](https://github.com/neodgm/neodgm) | `d61b60eccb731f8ca9c7da582e4a05a94db66b570471809950aa9a7261b941d6` |

## 패치 생성

입력을 갖춘 디렉터리에서 실행합니다. 출력 디렉터리는 새로 만들며, 이미 있으면 거부합니다.

```bash
# 배포 버전 이름과 빈 BPS 메타데이터로 생성
cargo run --release -- build-foundation \
  --rom path/to/original.gba \
  --out-dir out/0.1.0-beta \
  --release-version 0.1.0 --release-channel beta
```

출력 디렉터리에 `minna-puyo-ko-0.1.0-beta.gba`, `.bps`와 입력·검사 결과를 담은 `report.json`이 생깁니다. `--release-version`을 빼면 BPS 메타데이터에 비배포 표식을 넣은 `foundation-dev.gba`, `.bps`를 만듭니다.

## 그 밖의 명령

- `build-baseline`: 원본과 같은 ROM과 BPS를 만들어 빌드·BPS 왕복 경계만 검증합니다. 관리 입력이 필요 없습니다.
- `build-language-labels`: 언어 선택 화면 문구만 바꾸는 개발 빌드입니다.
- `export-effects`: 장식 효과 그래픽의 원본 PNG와 색 명세를 로컬로 내보냅니다.

사용법은 `cargo run -- help <명령>`으로 확인할 수 있습니다.

## 라이선스

이 저장소의 소스 코드는 [MIT License](LICENSE)로 제공합니다.
