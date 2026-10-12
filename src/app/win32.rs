//! `windows-sys`의 원시 FFI를 애플리케이션이 사용하기 쉬운 작은 함수와 RAII 객체로 감싼다.
//!
//! 대부분의 앱 코드는 이 모듈의 안전한 API만 호출한다. 운영체제와 직접 맞닿는 `unsafe`를 한곳에
//! 모으면 검토할 범위가 작아지고, 문자열 및 포인터의 수명 조건도 반복해서 처리하지 않아도 된다.

use std::ffi::c_void;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Dwm::{
    DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE, DwmSetWindowAttribute,
};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, LoadLibraryW};
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForSystem, GetDpiForWindow,
    SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use super::APP_NAME;

/// Windows의 기본 논리 DPI다. 화면 배율 100%가 96 DPI에 해당한다.
const DEFAULT_DPI: u32 = 96;

/// 논리 픽셀을 현재 모니터의 물리 픽셀로 바꾸는 DPI 값 객체다.
///
/// 화면 치수를 단순한 `i32`로 흩어 놓지 않고 이 타입을 통과시키면, 어떤 값이 100% 배율을
/// 기준으로 작성되었는지 코드에서 분명하게 드러난다.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Dpi(u32);

impl Dpi {
    /// 주 모니터의 DPI를 읽는다. Windows가 0을 반환하는 예외 상황에는 96 DPI를 사용한다.
    pub(crate) fn system() -> Self {
        // SAFETY: 인자가 없고 Windows가 프로세스의 시스템 DPI 값을 복사해 반환한다.
        Self(unsafe { GetDpiForSystem() }.max(DEFAULT_DPI))
    }

    /// 이미 생성된 창이 현재 놓인 모니터의 DPI를 읽는다.
    pub(crate) fn for_window(window: HWND) -> Self {
        // SAFETY: 호출자는 생성이 끝난 유효한 HWND를 전달한다.
        Self(unsafe { GetDpiForWindow(window) }.max(DEFAULT_DPI))
    }

    /// `WM_DPICHANGED`의 `wParam` 하위 단어에 담긴 새 가로 DPI를 값 객체로 만든다.
    pub(crate) fn from_message(wparam: WPARAM) -> Self {
        Self(((wparam & 0xffff) as u32).max(DEFAULT_DPI))
    }

    /// 96 DPI 기준의 논리 픽셀을 현재 배율의 물리 픽셀로 반올림한다.
    pub(crate) fn scale(self, logical_pixels: i32) -> i32 {
        let scaled = i64::from(logical_pixels) * i64::from(self.0);
        let rounded = if scaled >= 0 {
            scaled + i64::from(DEFAULT_DPI / 2)
        } else {
            scaled - i64::from(DEFAULT_DPI / 2)
        };
        (rounded / i64::from(DEFAULT_DPI)) as i32
    }
}

/// Windows가 창 전체를 비트맵으로 확대하지 않도록 프로세스를 모니터별 DPI 인식 모드로 바꾼다.
///
/// 창이나 GDI 자원을 만들기 전에 한 번 호출해야 한다. 실패하더라도 기존 DPI 모드로 실행할 수
/// 있으므로 결과는 오류로 승격하지 않는다.
pub(crate) fn enable_per_monitor_dpi_awareness() {
    // SAFETY: Windows가 정의한 의사 핸들 상수를 전달하며, UI 생성 전 단일 스레드에서 호출한다.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
}

/// `WM_DPICHANGED`가 제안한 위치와 크기를 창에 적용한다.
pub(crate) fn apply_dpi_suggested_rect(window: HWND, lparam: LPARAM) {
    // SAFETY: 이 함수는 WM_DPICHANGED 처리 중에만 호출되므로 lparam은 Windows가 소유한 RECT를
    // 가리킨다. RECT는 SetWindowPos 호출이 끝날 때까지만 읽고 보관하지 않는다.
    let Some(rect) = (unsafe { (lparam as *const RECT).as_ref() }) else {
        return;
    };
    // SAFETY: window는 메시지를 받은 유효한 HWND이고, 계산한 너비와 높이는 RECT에서 얻었다.
    unsafe {
        SetWindowPos(
            window,
            null_mut(),
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            SWP_NOACTIVATE | SWP_NOZORDER,
        )
    };
}

/// 창 클래스 이름과 콜백을 Windows에 등록한다.
pub(crate) fn register_class(name: &str, procedure: WNDPROC) -> Result<(), String> {
    let class = wide(name);
    let window_class = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: procedure,
        // SAFETY: null 모듈 이름은 현재 실행 모듈의 핸들을 요청하는 Win32 규약이다.
        hInstance: unsafe { GetModuleHandleW(null()) },
        // SAFETY: 시스템이 소유한 기본 화살표 커서를 ID로 요청한다.
        hCursor: unsafe { LoadCursorW(null_mut(), IDC_ARROW) },
        hbrBackground: null_mut(),
        lpszClassName: class.as_ptr(),
        ..Default::default()
    };
    // SAFETY: WNDCLASSW가 참조하는 class 버퍼는 동기 호출이 끝날 때까지 살아 있다.
    if unsafe { RegisterClassW(&window_class) } == 0 {
        return Err(format!("{name} 창 클래스를 등록하지 못했습니다."));
    }
    Ok(())
}

/// 마우스 위치가 테두리, 제목 표시줄, 본문 중 어디인지 판정한다.
/// 반환값을 통해 기본 테두리 없이도 이동과 크기 조절이 가능하다.
pub(crate) fn hit_test(
    window: HWND,
    lparam: LPARAM,
    title_height: i32,
    resize_edge: i32,
) -> LRESULT {
    let mut point = POINT {
        x: (lparam as i16) as i32,
        y: ((lparam >> 16) as i16) as i32,
    };
    // SAFETY: point는 쓰기 가능한 지역 변수이고 window는 현재 메시지의 창이다.
    unsafe { ScreenToClient(window, &mut point) };
    let mut rect = RECT::default();
    unsafe { GetClientRect(window, &mut rect) };
    let edge = resize_edge;
    if point.y < edge && point.x < edge {
        return HTTOPLEFT as LRESULT;
    }
    if point.y < edge && point.x >= rect.right - edge {
        return HTTOPRIGHT as LRESULT;
    }
    if point.y >= rect.bottom - edge && point.x < edge {
        return HTBOTTOMLEFT as LRESULT;
    }
    if point.y >= rect.bottom - edge && point.x >= rect.right - edge {
        return HTBOTTOMRIGHT as LRESULT;
    }
    if point.y < edge {
        return HTTOP as LRESULT;
    }
    if point.y >= rect.bottom - edge {
        return HTBOTTOM as LRESULT;
    }
    if point.x < edge {
        return HTLEFT as LRESULT;
    }
    if point.x >= rect.right - edge {
        return HTRIGHT as LRESULT;
    }
    if point.y < title_height {
        return HTCAPTION as LRESULT;
    }
    HTCLIENT as LRESULT
}

/// DWM이 자동으로 그리는 외곽선을 비활성화한다.
pub(crate) fn disable_native_border(window: HWND) {
    let color = DWMWA_COLOR_NONE;
    unsafe {
        DwmSetWindowAttribute(
            window,
            DWMWA_BORDER_COLOR as u32,
            (&color as *const u32).cast(),
            std::mem::size_of::<u32>() as u32,
        );
    }
}

/// 부모가 직접 그리는 `BS_OWNERDRAW` 버튼을 생성한다.
pub(crate) fn create_owner_button(
    window: HWND,
    text: &str,
    id: usize,
) -> HWND {
    unsafe {
        CreateWindowExW(
            0,
            wide("BUTTON").as_ptr(),
            wide(text).as_ptr(),
            WS_CHILD | WS_VISIBLE | BS_OWNERDRAW as u32,
            0,
            0,
            0,
            0,
            window,
            id as *mut c_void,
            GetModuleHandleW(null()),
            null(),
        )
    }
}

/// 클래스 이름과 스타일로 일반 자식 컨트롤을 생성한다.
pub(crate) fn create_child_control(
    parent: HWND,
    class: &str,
    style: u32,
    id: usize,
) -> HWND {
    unsafe {
        CreateWindowExW(
            0,
            wide(class).as_ptr(),
            null(),
            style,
            0,
            0,
            0,
            0,
            parent,
            id as *mut c_void,
            GetModuleHandleW(null()),
            null(),
        )
    }
}

/// 창의 클라이언트 영역을 값으로 복사해 반환한다.
pub(crate) fn client_rect(window: HWND) -> RECT {
    let mut rect = RECT::default();
    unsafe { GetClientRect(window, &mut rect) };
    rect
}

/// 부모와 컨트롤 ID로 자식 창을 찾아 이동한다.
pub(crate) fn move_child(
    parent: HWND,
    id: usize,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) {
    unsafe { MoveWindow(GetDlgItem(parent, id as i32), x, y, width, height, 1) };
}

/// 이미 알고 있는 창 핸들을 새 위치와 크기로 이동한다.
pub(crate) fn move_window(window: HWND, x: i32, y: i32, width: i32, height: i32) {
    unsafe { MoveWindow(window, x, y, width, height, 1) };
}

/// 컨트롤에 폰트를 지정하고 즉시 다시 그리게 한다.
pub(crate) fn set_control_font(control: HWND, font: HFONT) {
    unsafe { SendMessageW(control, WM_SETFONT, font as WPARAM, 1) };
}

/// 편집 컨트롤이 비어 있을 때 표시할 안내 문구를 설정한다.
pub(crate) fn set_cue_banner(control: HWND, text: &str) {
    const EM_SETCUEBANNER: u32 = 0x1501;
    unsafe {
        SendMessageW(
            control,
            EM_SETCUEBANNER,
            0,
            wide(text).as_ptr() as LPARAM,
        )
    };
}

/// 지정한 창 영역을 무효화하여 다음 페인트 메시지에서 다시 그리게 한다.
pub(crate) fn invalidate(window: HWND, erase: bool) {
    unsafe { InvalidateRect(window, null(), erase as i32) };
}

/// 부모 창과 ID로 자식 컨트롤을 찾아 다시 그리게 한다.
pub(crate) fn invalidate_child(parent: HWND, id: usize, erase: bool) {
    let child = unsafe { GetDlgItem(parent, id as i32) };
    invalidate(child, erase);
}

/// 키보드 포커스를 지정한 창으로 옮긴다.
pub(crate) fn focus(window: HWND) {
    unsafe { SetFocus(window) };
}

/// owner-draw 목록의 항목 높이를 고정한다.
pub(crate) fn configure_list(list: HWND, item_height: isize) {
    unsafe { SendMessageW(list, LB_SETITEMHEIGHT, 0, item_height) };
}

/// 다시 그리기를 잠시 멈추고 목록 내용을 비운다.
pub(crate) fn clear_list(list: HWND) {
    unsafe {
        SendMessageW(list, WM_SETREDRAW, 0, 0);
        SendMessageW(list, LB_RESETCONTENT, 0, 0);
    }
}

/// Rust 문자열을 UTF-16으로 바꿔 목록에 한 항목을 추가한다.
pub(crate) fn add_list_item(list: HWND, text: &str) {
    let text = wide(text);
    unsafe { SendMessageW(list, LB_ADDSTRING, 0, text.as_ptr() as LPARAM) };
}

/// 목록에서 지정한 인덱스를 선택한다.
pub(crate) fn select_list_item(list: HWND, index: usize) {
    unsafe { SendMessageW(list, LB_SETCURSEL, index, 0) };
}

/// Win32의 음수 오류 값을 `None`으로 변환하여 안전한 Rust 선택값으로 돌려준다.
pub(crate) fn selected_list_index(list: HWND) -> Option<usize> {
    let selection = unsafe { SendMessageW(list, LB_GETCURSEL, 0, 0) } as isize;
    (selection >= 0).then_some(selection as usize)
}

/// 목록 갱신이 끝났음을 알리고 한 번만 다시 그린다.
pub(crate) fn finish_list_update(list: HWND) {
    unsafe { SendMessageW(list, WM_SETREDRAW, 1, 0) };
    invalidate(list, true);
}

/// 같은 ID의 기존 타이머를 취소하고 새 지연 시간으로 시작한다.
pub(crate) fn restart_timer(window: HWND, id: usize, delay_ms: u32) {
    unsafe {
        KillTimer(window, id);
        SetTimer(window, id, delay_ms, None);
    }
}

/// 더 이상 필요하지 않은 창 타이머를 중지한다.
pub(crate) fn stop_timer(window: HWND, id: usize) {
    unsafe { KillTimer(window, id) };
}

/// 앱 내부 사용자 메시지를 다른 창에 동기적으로 전달한다.
pub(crate) fn notify(window: HWND, message: u32, value: usize) {
    unsafe { SendMessageW(window, message, value, 0) };
}

/// 창의 항상 위 속성만 바꾸고 위치와 크기는 보존한다.
pub(crate) fn set_topmost(window: HWND, enabled: bool) {
    unsafe {
        SetWindowPos(
            window,
            if enabled { HWND_TOPMOST } else { HWND_NOTOPMOST },
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
    };
}

/// 예/아니요 메시지 상자를 Rust의 `bool` 결과로 변환한다.
pub(crate) fn confirm(owner: HWND, message: &str) -> bool {
    unsafe {
        MessageBoxW(
            owner,
            wide(message).as_ptr(),
            wide(APP_NAME).as_ptr(),
            MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2,
        ) == IDYES
    }
}

/// WM_CLOSE를 큐에 넣는다.
/// 비동기 `PostMessageW`를 사용하므로 현재 객체의 가변 빌림이 끝난 뒤 닫기 처리가 실행된다.
pub(crate) fn close_window(window: HWND) {
    unsafe { PostMessageW(window, WM_CLOSE, 0, 0) };
}

/// 최소화된 창을 복원하고 사용자 입력의 대상으로 만든다.
pub(crate) fn restore_and_activate(window: HWND) {
    unsafe {
        ShowWindow(window, SW_RESTORE);
        SetForegroundWindow(window);
    }
}

/// 창을 최소화한다.
pub(crate) fn minimize(window: HWND) {
    unsafe { ShowWindow(window, SW_MINIMIZE) };
}

/// 캐시된 HWND가 아직 실제 창을 가리키는지 확인한다.
pub(crate) fn is_window(window: HWND) -> bool {
    unsafe { IsWindow(window) != 0 }
}

/// 그리기 컨텍스트의 배경색과 글자색을 함께 설정한다.
pub(crate) fn set_dc_colors(hdc: HDC, background: COLORREF, text: COLORREF) {
    unsafe {
        SetBkColor(hdc, background);
        SetTextColor(hdc, text);
    }
}

/// WM_GETMINMAXINFO가 제공한 구조체에 최소 창 크기를 기록한다.
pub(crate) fn set_min_track_size(lparam: LPARAM, width: i32, height: i32) {
    let info = lparam as *mut MINMAXINFO;
    if !info.is_null() {
        // SAFETY: 이 함수는 WM_GETMINMAXINFO 처리에서만 호출하며 lparam은 MINMAXINFO를 가리킨다.
        unsafe {
            (*info).ptMinTrackSize.x = width;
            (*info).ptMinTrackSize.y = height;
        }
    }
}

/// GetMessage 루프에 종료 코드를 게시한다.
pub(crate) fn quit_message_loop() {
    unsafe { PostQuitMessage(0) };
}

/// 지정한 자식 컨트롤 아래에 체크 가능한 팝업 메뉴를 표시한다.
/// 취소하면 `None`, 항목을 고르면 해당 명령 ID를 반환한다.
pub(crate) fn show_popup_menu(
    owner: HWND,
    anchor_id: usize,
    items: &[(usize, &str, bool)],
) -> Option<usize> {
    // SAFETY: 반환된 HMENU는 아래에서 DestroyMenu로 정확히 한 번 해제한다.
    let menu = unsafe { CreatePopupMenu() };
    if menu.is_null() {
        return None;
    }
    for (id, label, checked) in items {
        let flags = MF_STRING | if *checked { MF_CHECKED } else { 0 };
        // SAFETY: UTF-16 레이블은 동기 AppendMenuW 호출이 끝날 때까지 유효하다.
        unsafe { AppendMenuW(menu, flags, *id, wide(label).as_ptr()) };
    }
    let mut anchor = RECT::default();
    unsafe {
        GetWindowRect(GetDlgItem(owner, anchor_id as i32), &mut anchor);
        SetForegroundWindow(owner);
    }
    let selected = unsafe {
        TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTALIGN | TPM_TOPALIGN,
            anchor.right,
            anchor.bottom,
            0,
            owner,
            null(),
        )
    } as usize;
    // SAFETY: menu는 이 함수가 생성하고 아직 해제하지 않은 유효한 HMENU다.
    unsafe { DestroyMenu(menu) };
    (selected != 0).then_some(selected)
}

/// Win32 `HFONT`의 단독 소유권을 나타내는 RAII 객체다.
///
/// 이 타입은 `Clone`이나 `Copy`를 구현하지 않으므로 같은 핸들을 두 객체가 중복 해제할 수 없다.
pub(crate) struct Font(HFONT);

impl Font {
    /// Segoe UI 폰트를 만들고 반환된 핸들을 소유한다.
    pub(crate) fn new(size: i32, weight: i32, italic: bool, dpi: Dpi) -> Self {
        // SAFETY: 모든 인자는 값이며 face name 버퍼는 동기 호출 동안 유효하다.
        Self(unsafe {
            CreateFontW(
                -dpi.scale(size),
                0,
                0,
                0,
                weight,
                italic as u32,
                0,
                0,
                DEFAULT_CHARSET as u32,
                OUT_DEFAULT_PRECIS as u32,
                CLIP_DEFAULT_PRECIS as u32,
                CLEARTYPE_QUALITY as u32,
                (DEFAULT_PITCH | FF_DONTCARE) as u32,
                wide("Segoe UI").as_ptr(),
            )
        })
    }

    /// Win32 함수에 잠시 빌려줄 원시 핸들을 반환한다. 소유권은 여전히 `Font`에 있다.
    pub(crate) fn handle(&self) -> HFONT {
        self.0
    }
}

impl Drop for Font {
    /// 스코프를 벗어날 때 소유한 GDI 폰트를 자동 해제한다.
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: 이 핸들은 Font::new가 생성했고 이 Drop에서 한 번만 해제한다.
            unsafe { DeleteObject(self.0) };
        }
    }
}

/// Win32 `HBRUSH`의 단독 소유권을 나타내는 RAII 객체다.
pub(crate) struct Brush(HBRUSH);

impl Brush {
    /// 단색 브러시를 생성하여 소유한다.
    pub(crate) fn new(color: COLORREF) -> Self {
        Self(unsafe { CreateSolidBrush(color) })
    }

    /// 소유권을 넘기지 않고 원시 브러시 핸들만 빌려준다.
    pub(crate) fn handle(&self) -> HBRUSH {
        self.0
    }
}

impl Drop for Brush {
    /// 브러시 객체가 사라질 때 GDI 자원을 해제한다.
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: Brush::new가 생성한 단독 소유 핸들을 정확히 한 번 해제한다.
            unsafe { DeleteObject(self.0) };
        }
    }
}

/// 임시 단색 브러시로 직사각형을 채우고 즉시 자원을 정리한다.
pub(crate) fn fill_rect(hdc: HDC, rect: &RECT, color: COLORREF) {
    let brush = unsafe { CreateSolidBrush(color) };
    unsafe {
        FillRect(hdc, rect, brush);
        DeleteObject(brush);
    }
}

/// 채움색을 가진 둥근 직사각형을 그린다.
pub(crate) fn fill_round_rect(
    hdc: HDC,
    rect: &RECT,
    color: COLORREF,
    radius: i32,
) {
    let brush = unsafe { CreateSolidBrush(color) };
    let pen = unsafe { CreatePen(PS_NULL, 0, color) };
    // 선택 전 객체를 기억했다가 복원해야 HDC가 삭제된 객체를 계속 참조하지 않는다.
    let old_brush = unsafe { SelectObject(hdc, brush) };
    let old_pen = unsafe { SelectObject(hdc, pen) };
    unsafe {
        RoundRect(
            hdc,
            rect.left,
            rect.top,
            rect.right,
            rect.bottom,
            radius,
            radius,
        );
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        DeleteObject(brush);
        DeleteObject(pen);
    }
}

/// 내부를 채우지 않고 둥근 테두리만 그린다.
pub(crate) fn stroke_round_rect(
    hdc: HDC,
    rect: &RECT,
    color: COLORREF,
    radius: i32,
) {
    let brush = unsafe { GetStockObject(NULL_BRUSH) };
    let pen = unsafe { CreatePen(PS_SOLID, 1, color) };
    let old_brush = unsafe { SelectObject(hdc, brush) };
    let old_pen = unsafe { SelectObject(hdc, pen) };
    unsafe {
        RoundRect(
            hdc,
            rect.left,
            rect.top,
            rect.right,
            rect.bottom,
            radius,
            radius,
        );
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        DeleteObject(pen);
    }
}

/// 지정한 폰트와 색상, 정렬 플래그로 UTF-16 텍스트를 그린다.
pub(crate) fn draw_text(
    hdc: HDC,
    value: &str,
    mut rect: RECT,
    font: HFONT,
    color: COLORREF,
    format: u32,
) {
    let value = wide(value);
    // HDC에 임시 폰트를 선택한 뒤 반드시 이전 폰트로 복원한다.
    let old_font = unsafe { SelectObject(hdc, font) };
    unsafe {
        SetBkMode(hdc, TRANSPARENT as i32);
        SetTextColor(hdc, color);
        DrawTextW(
            hdc,
            value.as_ptr(),
            (value.len() - 1) as i32,
            &mut rect,
            format,
        );
        SelectObject(hdc, old_font);
    }
}

/// Win32 창의 UTF-16 제목/본문을 Rust `String`으로 복사한다.
pub(crate) fn window_text(window: HWND) -> String {
    if window.is_null() {
        return String::new();
    }
    let length = unsafe { GetWindowTextLengthW(window) };
    let mut buffer = vec![0u16; length as usize + 1];
    unsafe { GetWindowTextW(window, buffer.as_mut_ptr(), buffer.len() as i32) };
    String::from_utf16_lossy(&buffer[..length as usize])
}

/// 오류 내용을 앱 이름을 제목으로 하는 메시지 상자에 표시한다.
pub(crate) fn show_error(owner: HWND, message: &str) {
    unsafe {
        MessageBoxW(
            owner,
            wide(message).as_ptr(),
            wide(APP_NAME).as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

/// 이름으로 DLL을 로드하고 성공 여부만 안전한 bool로 노출한다.
pub(crate) fn load_library(name: &str) -> bool {
    unsafe { !LoadLibraryW(wide(name).as_ptr()).is_null() }
}

/// 창을 표시한 뒤 보류 중인 페인트를 즉시 수행한다.
pub(crate) fn show_and_update(window: HWND) {
    unsafe {
        ShowWindow(window, SW_SHOW);
        UpdateWindow(window);
    }
}

/// 현재 UI 스레드의 표준 Windows 메시지 루프를 실행한다.
pub(crate) fn run_message_loop() {
    let mut message = MSG::default();
    // GetMessageW가 0을 반환하면 WM_QUIT을 받은 것이므로 반복을 끝낸다.
    while unsafe { GetMessageW(&mut message, null_mut(), 0, 0) } > 0 {
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

/// BeginPaint/EndPaint의 짝을 감싸고 그리기 클로저에 HDC와 영역을 빌려준다.
///
/// `FnOnce`는 이 콜백이 정확히 한 번 호출됨을 타입으로 표현한다.
pub(crate) fn with_paint(window: HWND, draw: impl FnOnce(HDC, RECT)) {
    let mut paint = PAINTSTRUCT::default();
    let hdc = unsafe { BeginPaint(window, &mut paint) };
    let mut rect = RECT::default();
    unsafe { GetClientRect(window, &mut rect) };
    draw(hdc, rect);
    unsafe { EndPaint(window, &paint) };
}

/// Rust UTF-8 문자열을 Win32가 기대하는 null 종료 UTF-16 버퍼로 변환한다.
pub(crate) fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::Dpi;

    #[test]
    fn scales_logical_pixels_for_common_windows_zoom_levels() {
        assert_eq!(Dpi(96).scale(100), 100);
        assert_eq!(Dpi(120).scale(100), 125);
        assert_eq!(Dpi(144).scale(100), 150);
        assert_eq!(Dpi(192).scale(100), 200);
    }

    #[test]
    fn rounds_scaled_pixels_symmetrically() {
        assert_eq!(Dpi(144).scale(1), 2);
        assert_eq!(Dpi(144).scale(-1), -2);
    }
}
