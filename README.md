# Feather Notes

스티커 메모를 대신하는 초경량 Windows 메모 앱입니다. 브라우저 런타임이나 무거운 GUI 프레임워크 없이 Rust와 Win32 시스템 컨트롤만 사용해 메모리 사용량을 작게 유지합니다.

## 기능

- 여러 메모 생성, 선택, 삭제
- 입력 후 350ms가 지나면 자동 저장
- 창을 다른 창 위에 고정하는 토글
- UTF-8 일반 텍스트 파일 저장
- 설치 프로그램과 계정, 네트워크 연결이 필요 없는 단일 실행 파일

메모는 `%LOCALAPPDATA%\FeatherNotes\notes`에 개별 `.txt` 파일로 저장됩니다. 앱이 종료되어도 다른 편집기로 내용을 읽거나 백업할 수 있습니다.

테스트나 휴대용 실행처럼 저장 위치를 바꿔야 할 때는 실행 전에 `FEATHER_NOTES_DATA_DIR` 환경 변수를 원하는 폴더로 지정할 수 있습니다.

## 기술 스택과 저메모리 원칙

- Rust 2024 edition
- `windows-sys`를 통한 Win32 API 직접 사용
- 운영체제가 제공하는 ListBox, Edit, Button 컨트롤 재사용
- 메모를 읽기 쉬운 개별 텍스트 파일로 저장해 데이터베이스 엔진 미사용
- 릴리스 빌드에 LTO, 크기 최적화, 심볼 제거 적용

## 사전 요구 사항

- Windows 10 또는 Windows 11
- Rust 1.85 이상과 Cargo

## 실행

```powershell
cargo run --release
```

릴리스 실행 파일은 `target\release\feather-notes.exe`에 생성됩니다.

## 테스트와 검사

```powershell
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release
```

실행 중인 앱의 실제 메모리를 확인하려면 다음 명령을 사용할 수 있습니다.

```powershell
Get-Process feather-notes | Select-Object ProcessName, `
  @{Name='WorkingSetMB'; Expression={[math]::Round($_.WorkingSet64 / 1MB, 2)}}, `
  @{Name='PrivateMB'; Expression={[math]::Round($_.PrivateMemorySize64 / 1MB, 2)}}
```

운영체제 버전, 글꼴 캐시, 보안 프로그램에 따라 측정값은 달라질 수 있으므로 고정된 수치를 보장하기보다 릴리스 빌드를 같은 조건에서 비교하는 것이 좋습니다.

현재 개발 환경의 2026-10-09 기준 스모크 테스트에서는 빈 메모 1개를 연 릴리스 빌드가 Working Set 8.95MB, Private Memory 1.84MB, 134 handles를 사용했고 실행 파일 크기는 276KB였습니다. 이는 성능 기준선이지 모든 PC에서의 보장값은 아닙니다.

## 배포

`cargo build --release` 후 생성된 `target\release\feather-notes.exe` 하나를 복사해 실행하면 됩니다. 사용자 메모는 실행 파일과 분리되어 로컬 앱 데이터 폴더에 남습니다.

## 라이선스

MIT
