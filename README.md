# Feather Notes

Feather Notes는 Rust와 Win32로 만든 가벼운 Windows 스티커 메모 앱입니다. 브라우저 런타임이나
별도의 GUI 프레임워크 없이 네이티브 Windows 컨트롤을 사용합니다.

## 주요 기능

- 메모 목록 검색, 생성, 열기, 삭제
- 여러 메모를 각각 독립된 창으로 표시
- 입력이 500ms 동안 멈추면 자동 저장하는 디바운스 저장
- 굵게, 기울임, 밑줄, 취소선, 글머리표 서식
- 화면에는 서식이 적용된 문장을 표시하고 파일에는 Markdown으로 저장
- 노랑, 초록, 파랑, 분홍, 보라, 회색 메모 색상
- 메모 창 항상 위 고정
- Windows 150% 배율을 포함한 Per-Monitor DPI Aware V2 지원
- UTF-8 텍스트 파일 기반의 로컬 저장

## 다운로드

[최신 GitHub Release](https://github.com/hakunamta00700/light-sticky-notes/releases/latest)에서
`feather-notes-v0.1.0-windows-x86_64.exe`를 내려받아 실행합니다. 설치 과정은 필요하지 않습니다.

현재 실행 파일은 코드 서명이 되어 있지 않으므로 Windows SmartScreen 경고가 표시될 수 있습니다.
릴리스에 함께 제공되는 SHA-256 파일로 다운로드한 바이너리를 검증할 수 있습니다.

```powershell
Get-FileHash .\feather-notes-v0.1.0-windows-x86_64.exe -Algorithm SHA256
```

## 데이터 저장 위치

메모는 다음 경로에 개별 텍스트 파일로 저장됩니다.

```text
%LOCALAPPDATA%\FeatherNotes\notes
```

실행 파일과 메모 데이터가 분리되어 있으므로 앱을 교체하거나 업데이트해도 기존 메모가 유지됩니다.
테스트용 저장 위치가 필요하면 실행 전에 `FEATHER_NOTES_DATA_DIR` 환경 변수를 지정할 수 있습니다.

## 직접 빌드

요구 사항:

- Windows 10 또는 Windows 11 x64
- 최신 Rust stable 툴체인

```powershell
git clone https://github.com/hakunamta00700/light-sticky-notes.git
cd light-sticky-notes
cargo build --release
```

완성된 실행 파일은 `target\release\feather-notes.exe`에 생성됩니다.

## 검증

```powershell
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release
```

GitHub Actions의 Windows CI도 같은 검증을 실행하고 릴리스 빌드를 아티팩트로 제공합니다.

## 코드 구조

- `src/app/manager.rs`: 메모 목록 창 객체와 동작
- `src/app/note.rs`: 개별 메모 창 객체와 자동 저장
- `src/app/window_host.rs`: Win32 메시지와 `WindowController` 객체 연결
- `src/app/win32.rs`: Win32 FFI 안전 래퍼, DPI 값 객체, RAII 자원
- `src/app/markdown.rs`: Rich Edit와 Markdown 사이의 변환
- `src/store.rs`: 파일 저장과 메모 도메인 모델

프로젝트는 안전한 Rust를 기본으로 하며, 불가피한 `unsafe`는 Win32 FFI 경계에 제한합니다.

## 개인정보 및 네트워크

Feather Notes는 메모를 로컬 파일에만 저장하며 메모 내용을 외부 서버로 전송하지 않습니다.

## 라이선스

[MIT License](LICENSE)
