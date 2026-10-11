use std::ffi::c_void;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Dwm::{
    DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE, DwmSetWindowAttribute,
};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, LoadLibraryW};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use super::APP_NAME;

pub(crate) fn register_class(name: &str, procedure: WNDPROC) -> Result<(), String> {
    let class = wide(name);
    let window_class = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: procedure,
        hInstance: unsafe { GetModuleHandleW(null()) },
        hCursor: unsafe { LoadCursorW(null_mut(), IDC_ARROW) },
        hbrBackground: null_mut(),
        lpszClassName: class.as_ptr(),
        ..Default::default()
    };
    if unsafe { RegisterClassW(&window_class) } == 0 {
        return Err(format!("{name} 창 클래스를 등록하지 못했습니다."));
    }
    Ok(())
}

pub(crate) fn hit_test(window: HWND, lparam: LPARAM, title_height: i32) -> LRESULT {
    let mut point = POINT {
        x: (lparam as i16) as i32,
        y: ((lparam >> 16) as i16) as i32,
    };
    unsafe { ScreenToClient(window, &mut point) };
    let mut rect = RECT::default();
    unsafe { GetClientRect(window, &mut rect) };
    let edge = 7;
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

pub(crate) fn client_rect(window: HWND) -> RECT {
    let mut rect = RECT::default();
    unsafe { GetClientRect(window, &mut rect) };
    rect
}

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

pub(crate) fn move_window(window: HWND, x: i32, y: i32, width: i32, height: i32) {
    unsafe { MoveWindow(window, x, y, width, height, 1) };
}

pub(crate) fn set_control_font(control: HWND, font: HFONT) {
    unsafe { SendMessageW(control, WM_SETFONT, font as WPARAM, 1) };
}

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

pub(crate) fn invalidate(window: HWND, erase: bool) {
    unsafe { InvalidateRect(window, null(), erase as i32) };
}

pub(crate) fn invalidate_child(parent: HWND, id: usize, erase: bool) {
    let child = unsafe { GetDlgItem(parent, id as i32) };
    invalidate(child, erase);
}

pub(crate) fn focus(window: HWND) {
    unsafe { SetFocus(window) };
}

pub(crate) fn configure_list(list: HWND, item_height: isize) {
    unsafe { SendMessageW(list, LB_SETITEMHEIGHT, 0, item_height) };
}

pub(crate) fn clear_list(list: HWND) {
    unsafe {
        SendMessageW(list, WM_SETREDRAW, 0, 0);
        SendMessageW(list, LB_RESETCONTENT, 0, 0);
    }
}

pub(crate) fn add_list_item(list: HWND, text: &str) {
    let text = wide(text);
    unsafe { SendMessageW(list, LB_ADDSTRING, 0, text.as_ptr() as LPARAM) };
}

pub(crate) fn select_list_item(list: HWND, index: usize) {
    unsafe { SendMessageW(list, LB_SETCURSEL, index, 0) };
}

pub(crate) fn selected_list_index(list: HWND) -> Option<usize> {
    let selection = unsafe { SendMessageW(list, LB_GETCURSEL, 0, 0) } as isize;
    (selection >= 0).then_some(selection as usize)
}

pub(crate) fn finish_list_update(list: HWND) {
    unsafe { SendMessageW(list, WM_SETREDRAW, 1, 0) };
    invalidate(list, true);
}

pub(crate) fn restart_timer(window: HWND, id: usize, delay_ms: u32) {
    unsafe {
        KillTimer(window, id);
        SetTimer(window, id, delay_ms, None);
    }
}

pub(crate) fn stop_timer(window: HWND, id: usize) {
    unsafe { KillTimer(window, id) };
}

pub(crate) fn notify(window: HWND, message: u32, value: usize) {
    unsafe { SendMessageW(window, message, value, 0) };
}

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

pub(crate) fn close_window(window: HWND) {
    unsafe { SendMessageW(window, WM_CLOSE, 0, 0) };
}

pub(crate) fn restore_and_activate(window: HWND) {
    unsafe {
        ShowWindow(window, SW_RESTORE);
        SetForegroundWindow(window);
    }
}

pub(crate) fn minimize(window: HWND) {
    unsafe { ShowWindow(window, SW_MINIMIZE) };
}

pub(crate) fn is_window(window: HWND) -> bool {
    unsafe { IsWindow(window) != 0 }
}

pub(crate) fn show_popup_menu(
    owner: HWND,
    anchor_id: usize,
    items: &[(usize, &str, bool)],
) -> Option<usize> {
    let menu = unsafe { CreatePopupMenu() };
    if menu.is_null() {
        return None;
    }
    for (id, label, checked) in items {
        let flags = MF_STRING | if *checked { MF_CHECKED } else { 0 };
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
    unsafe { DestroyMenu(menu) };
    (selected != 0).then_some(selected)
}

pub(crate) fn create_font(size: i32, weight: i32, italic: bool) -> HFONT {
    unsafe {
        CreateFontW(
            -size,
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
    }
}

pub(crate) fn create_brush(color: COLORREF) -> HBRUSH {
    unsafe { CreateSolidBrush(color) }
}

pub(crate) fn delete_gdi_object(object: *mut c_void) {
    if !object.is_null() {
        unsafe { DeleteObject(object) };
    }
}

pub(crate) fn fill_rect(hdc: HDC, rect: &RECT, color: COLORREF) {
    let brush = unsafe { CreateSolidBrush(color) };
    unsafe {
        FillRect(hdc, rect, brush);
        DeleteObject(brush);
    }
}

pub(crate) fn fill_round_rect(
    hdc: HDC,
    rect: &RECT,
    color: COLORREF,
    radius: i32,
) {
    let brush = unsafe { CreateSolidBrush(color) };
    let pen = unsafe { CreatePen(PS_NULL, 0, color) };
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

pub(crate) fn draw_text(
    hdc: HDC,
    value: &str,
    mut rect: RECT,
    font: HFONT,
    color: COLORREF,
    format: u32,
) {
    let value = wide(value);
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

pub(crate) fn window_text(window: HWND) -> String {
    if window.is_null() {
        return String::new();
    }
    let length = unsafe { GetWindowTextLengthW(window) };
    let mut buffer = vec![0u16; length as usize + 1];
    unsafe { GetWindowTextW(window, buffer.as_mut_ptr(), buffer.len() as i32) };
    String::from_utf16_lossy(&buffer[..length as usize])
}

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

pub(crate) fn load_library(name: &str) -> bool {
    unsafe { !LoadLibraryW(wide(name).as_ptr()).is_null() }
}

pub(crate) fn show_and_update(window: HWND) {
    unsafe {
        ShowWindow(window, SW_SHOW);
        UpdateWindow(window);
    }
}

pub(crate) fn run_message_loop() {
    let mut message = MSG::default();
    while unsafe { GetMessageW(&mut message, null_mut(), 0, 0) } > 0 {
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

pub(crate) fn with_paint(window: HWND, draw: impl FnOnce(HDC, RECT)) {
    let mut paint = PAINTSTRUCT::default();
    let hdc = unsafe { BeginPaint(window, &mut paint) };
    let mut rect = RECT::default();
    unsafe { GetClientRect(window, &mut rect) };
    draw(hdc, rect);
    unsafe { EndPaint(window, &paint) };
}

pub(crate) fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
