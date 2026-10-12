//! 애플리케이션의 조립 지점(composition root)이다.
//!
//! 어떤 창과 서비스가 존재하는지 선언하고, 공유 저장소를 만든 뒤 Windows 메시지 루프를
//! 시작한다. 각 창의 세부 동작은 하위 모듈에 위임한다.

// 각 모듈은 창 제어, 그리기, 문서 변환, OS 연동처럼 하나의 책임만 가진다.
mod manager;
mod manager_view;
mod markdown;
mod note;
mod note_view;
mod theme;
mod win32;
mod window_host;

use std::cell::RefCell;
use std::ptr::null_mut;
use std::rc::Rc;

use windows_sys::Win32::UI::WindowsAndMessaging::WM_APP;

use crate::store::{NoteStore, default_data_dir};
use win32::{
    enable_per_monitor_dpi_awareness, load_library, register_class, run_message_loop,
    show_and_update, show_error,
};

pub(crate) const APP_NAME: &str = "Feather Notes";
pub(crate) const MANAGER_CLASS: &str = "FeatherNotesManager";
pub(crate) const NOTE_CLASS: &str = "FeatherNotesNote";
pub(crate) const WM_REFRESH_NOTES: u32 = WM_APP + 1;
pub(crate) const WM_NOTE_CLOSED: u32 = WM_APP + 2;

/// 여러 창이 함께 사용하는 애플리케이션 상태다.
///
/// `Rc`는 단일 UI 스레드에서 저장소의 소유권을 공유한다. `RefCell`은 컴파일 시점 대신 실행
/// 시점에 빌림 규칙을 검사하여, 여러 창이 필요할 때만 저장소를 가변로 빌릴 수 있게 한다.
pub(crate) struct SharedState {
    pub(crate) store: RefCell<NoteStore>,
}

/// 오류를 Windows 메시지 상자로 변환하는 최상위 실행 함수다.
pub(crate) fn launch() {
    if let Err(error) = run() {
        show_error(null_mut(), &error);
    }
}

/// 필요한 창 클래스를 등록하고 관리 창을 만든 뒤 이벤트 루프에 진입한다.
fn run() -> Result<(), String> {
    // 창 생성 전에 DPI 인식 모드를 정해야 Windows가 완성된 화면을 비트맵으로 확대하지 않는다.
    enable_per_monitor_dpi_awareness();
    // Rich Edit 컨트롤은 DLL을 먼저 로드해야 창 클래스 이름을 사용할 수 있다.
    if !load_library("Msftedit.dll") {
        return Err("Windows Rich Edit 구성 요소를 불러오지 못했습니다.".into());
    }
    register_class(MANAGER_CLASS, Some(manager::window_proc))?;
    register_class(NOTE_CLASS, Some(note::window_proc))?;
    // 관리 창과 모든 메모 창이 동일한 NoteStore를 바라보도록 Rc로 감싼다.
    let shared = Rc::new(SharedState {
        store: RefCell::new(
            NoteStore::open(default_data_dir())
                .map_err(|error| format!("메모 저장소를 열지 못했습니다. {error}"))?,
        ),
    });
    let window = manager::create_window(shared)?;
    show_and_update(window);
    // GetMessage/DispatchMessage 반복은 사용자가 마지막 창을 닫을 때까지 계속된다.
    run_message_loop();
    Ok(())
}
