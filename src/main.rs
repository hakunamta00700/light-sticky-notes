//! Feather Notes 실행 파일의 진입점이다.
//!
//! 실제 기능은 `app`과 `store` 모듈에 두고, 이 파일은 프로그램 시작만 담당한다. 진입점을
//! 작게 유지하면 운영체제 연동 코드와 도메인 로직을 독립적으로 읽고 테스트하기 쉽다.

// 릴리스 빌드에서는 별도의 콘솔 창을 만들지 않는 Windows GUI 프로그램으로 실행한다.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// `unsafe fn` 내부에서도 위험한 연산마다 명시적인 `unsafe` 블록을 요구한다.
#![deny(unsafe_op_in_unsafe_fn)]
// 이미 안전한 함수 호출을 불필요하게 `unsafe`로 감싸는 실수를 컴파일 단계에서 막는다.
#![deny(unused_unsafe)]

// `mod` 선언은 다른 소스 파일을 현재 크레이트의 모듈 트리에 포함한다.
mod app;
mod store;

fn main() {
    // 애플리케이션 생성과 Windows 메시지 루프는 app 모듈이 캡슐화한다.
    app::launch();
}
