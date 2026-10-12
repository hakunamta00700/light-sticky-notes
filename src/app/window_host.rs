//! Win32의 절차적인 창 콜백을 Rust 객체 메서드로 연결한다.
//!
//! Windows는 전역 함수 포인터인 `WNDPROC`를 호출하지만, 애플리케이션은 상태를 가진
//! `WindowController` 객체로 동작한다. 이 모듈이 두 방식 사이의 수명과 포인터 변환을 전담한다.

use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, DefWindowProcW, DestroyWindow, GWLP_USERDATA, GetWindowLongPtrW,
    SetWindowLongPtrW, WM_NCCREATE, WM_NCDESTROY,
};

/// 컨트롤러가 하나의 Windows 메시지를 처리한 결과다.
pub(crate) enum MessageResult {
    /// 메시지를 처리했으며 포함된 값을 Windows에 반환한다.
    Handled(LRESULT),
    /// 처리하지 않았으므로 기본 창 프로시저에 위임한다.
    Default,
    /// 현재 메시지 처리가 끝난 뒤 안전하게 창을 파괴한다.
    Destroy,
}

/// 상태를 소유한 창 객체가 구현해야 하는 공통 인터페이스다.
///
/// Rust의 trait은 다른 언어의 인터페이스와 비슷하지만, 구현 상속 없이 필요한 행위만 공유한다.
pub(crate) trait WindowController {
    /// Windows가 만든 실제 창 핸들을 객체에 연결한다.
    fn attach(&mut self, window: HWND);

    /// Win32 메시지를 객체의 동작으로 변환한다.
    fn handle_message(&mut self, message: u32, wparam: WPARAM, lparam: LPARAM) -> MessageResult;

    /// 창이 완전히 제거되기 직전에 필요한 정리 동작을 수행한다.
    fn on_destroy(&mut self) {}
}

/// `CreateWindowExW`가 동기적으로 실행되는 동안 컨트롤러를 살아 있게 유지한다.
///
/// `Rc`는 생성 호출자와 Windows 창이 같은 컨트롤러를 공동 소유하게 하고, `RefCell`은 Win32의
/// 재진입 가능한 메시지 처리에서 중복 가변 참조가 생기는 것을 실행 시점에 차단한다.
pub(crate) struct CreateContext<T> {
    controller: Rc<RefCell<T>>,
}

impl<T> CreateContext<T> {
    /// 아직 어느 창에도 연결되지 않은 컨트롤러를 만든다.
    pub(crate) fn new(controller: T) -> Self {
        Self {
            controller: Rc::new(RefCell::new(controller)),
        }
    }

    /// Win32의 `lpParam`에 전달할 불투명 포인터를 얻는다.
    /// 컨텍스트 자체가 생성 호출이 반환될 때까지 살아 있으므로 포인터도 그동안 유효하다.
    pub(crate) fn as_ptr(&self) -> *mut c_void {
        (self as *const Self).cast_mut().cast()
    }
}

/// 원시 Win32 메시지를 적절한 `WindowController` 객체로 전달한다.
///
/// # Safety
///
/// `window`, `message`, `wparam`, `lparam`은 Windows가 등록된 `WNDPROC`에 전달한 값이어야 한다.
/// 특히 `WM_NCCREATE`의 `lparam`은 같은 `T`로 만든 `CreateContext<T>`를 가리켜야 한다.
pub(crate) unsafe fn dispatch<T: WindowController>(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = lparam as *const CREATESTRUCTW;
        if create.is_null() {
            return 0;
        }
        // SAFETY: WM_NCCREATE에서는 Windows가 유효한 CREATESTRUCTW 포인터를 전달하며,
        // create_window가 lpCreateParams에 동일한 T의 CreateContext 주소를 넣었다.
        let context = unsafe { ((*create).lpCreateParams as *const CreateContext<T>).as_ref() };
        let Some(context) = context else {
            return 0;
        };
        if let Ok(mut controller) = context.controller.try_borrow_mut() {
            controller.attach(window);
        } else {
            return 0;
        }
        // 창이 파괴될 때까지 유지할 강한 참조 하나를 원시 포인터로 넘긴다.
        let owned = Rc::into_raw(Rc::clone(&context.controller));
        // SAFETY: owned는 Rc::into_raw가 만든 안정적인 주소이며 WM_NCDESTROY에서 정확히 한 번 회수한다.
        unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, owned as isize) };
    }

    // SAFETY: 이 창 클래스의 GWLP_USERDATA에는 위에서 저장한 RefCell<T> 포인터만 들어간다.
    let pointer = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *const RefCell<T>;
    if pointer.is_null() {
        // SAFETY: 처리할 객체가 없으므로 Windows가 제공한 원래 매개변수를 기본 프로시저에 전달한다.
        return unsafe { DefWindowProcW(window, message, wparam, lparam) };
    }

    if message == WM_NCDESTROY {
        // 먼저 슬롯을 비워 재진입하는 메시지가 이미 해제 중인 객체를 찾지 못하게 한다.
        // SAFETY: window는 현재 콜백이 처리 중인 유효한 창이다.
        unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, 0) };
        // SAFETY: pointer는 Rc::into_raw로 만든 강한 참조이며 이 분기에서 한 번만 회수한다.
        let controller = unsafe { Rc::from_raw(pointer) };
        if let Ok(mut controller) = controller.try_borrow_mut() {
            controller.on_destroy();
        }
        // SAFETY: Windows가 요구하는 기본 비클라이언트 영역 정리를 계속 수행한다.
        return unsafe { DefWindowProcW(window, message, wparam, lparam) };
    }

    let result = {
        // SAFETY: WM_NCDESTROY 전까지 원시 Rc 강한 참조가 객체를 살아 있게 유지한다.
        let controller = unsafe { &*pointer };
        // 재진입 때문에 이미 빌린 상태라면 UB 대신 기본 처리로 안전하게 물러난다.
        match controller.try_borrow_mut() {
            Ok(mut controller) => controller.handle_message(message, wparam, lparam),
            Err(_) => MessageResult::Default,
        }
    };

    match result {
        MessageResult::Handled(result) => result,
        MessageResult::Default => {
            // SAFETY: 컨트롤러가 처리하지 않은 원본 메시지를 그대로 위임한다.
            unsafe { DefWindowProcW(window, message, wparam, lparam) }
        }
        MessageResult::Destroy => {
            // 컨트롤러의 RefMut은 위 블록에서 이미 해제되었다. 따라서 DestroyWindow가 동기적으로
            // WM_NCDESTROY를 보내더라도 같은 객체의 빌림과 충돌하지 않는다.
            // SAFETY: window는 현재 메시지를 받은 유효한 창 핸들이다.
            unsafe { DestroyWindow(window) };
            0
        }
    }
}
