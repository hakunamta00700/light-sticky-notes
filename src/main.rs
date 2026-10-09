#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod store;

use std::ffi::c_void;
use std::ptr::{null, null_mut};

use store::{NoteStore, default_data_dir, note_title};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{COLOR_WINDOW, DEFAULT_GUI_FONT, GetStockObject, UpdateWindow};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::{EM_SETLIMITTEXT, EM_SETSEL};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const APP_NAME: &str = "Feather Notes";
const CLASS_NAME: &str = "FeatherNotesWindow";
const ID_LIST: usize = 1001;
const ID_EDITOR: usize = 1002;
const ID_NEW: usize = 1003;
const ID_DELETE: usize = 1004;
const ID_PIN: usize = 1005;
const SAVE_TIMER: usize = 1;

struct AppState {
    window: HWND,
    list: HWND,
    editor: HWND,
    pin_button: HWND,
    store: NoteStore,
    current: usize,
    dirty: bool,
    suppress_change: bool,
    pinned: bool,
}

impl AppState {
    unsafe fn create(window: HWND) -> Result<Box<Self>, String> {
        let instance = unsafe { GetModuleHandleW(null()) };
        let list = unsafe {
            CreateWindowExW(
                WS_EX_CLIENTEDGE,
                wide("LISTBOX").as_ptr(),
                null(),
                WS_CHILD | WS_VISIBLE | WS_VSCROLL | LBS_NOTIFY as u32,
                0,
                0,
                0,
                0,
                window,
                ID_LIST as *mut c_void,
                instance,
                null(),
            )
        };
        let editor = unsafe {
            CreateWindowExW(
                WS_EX_CLIENTEDGE,
                wide("EDIT").as_ptr(),
                null(),
                WS_CHILD
                    | WS_VISIBLE
                    | WS_VSCROLL
                    | ES_LEFT as u32
                    | ES_MULTILINE as u32
                    | ES_AUTOVSCROLL as u32
                    | ES_WANTRETURN as u32,
                0,
                0,
                0,
                0,
                window,
                ID_EDITOR as *mut c_void,
                instance,
                null(),
            )
        };
        let new_button = unsafe { create_button(window, instance, "새 메모", ID_NEW) };
        let delete_button = unsafe { create_button(window, instance, "삭제", ID_DELETE) };
        let pin_button = unsafe { create_button(window, instance, "항상 위: 꺼짐", ID_PIN) };
        if [list, editor, new_button, delete_button, pin_button]
            .iter()
            .any(|handle| handle.is_null())
        {
            return Err("Windows 컨트롤을 만들지 못했습니다.".into());
        }

        let font = unsafe { GetStockObject(DEFAULT_GUI_FONT) };
        for control in [list, editor, new_button, delete_button, pin_button] {
            unsafe { SendMessageW(control, WM_SETFONT, font as WPARAM, 1) };
        }

        let store = NoteStore::open(default_data_dir())
            .map_err(|error| format!("메모 저장소를 열지 못했습니다: {error}"))?;
        Ok(Box::new(Self {
            window,
            list,
            editor,
            pin_button,
            store,
            current: 0,
            dirty: false,
            suppress_change: false,
            pinned: false,
        }))
    }

    unsafe fn initialize(&mut self) {
        unsafe {
            SendMessageW(self.editor, EM_SETLIMITTEXT, 2_000_000, 0);
            self.refresh_list();
            self.load_current();
            self.layout();
        }
    }

    unsafe fn layout(&self) {
        let mut rect = RECT::default();
        unsafe { GetClientRect(self.window, &mut rect) };
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        let margin = 10;
        let sidebar = 180;
        let button_height = 30;
        let gap = 8;
        let pin_width = 112;
        let small_width = (sidebar - gap) / 2;

        unsafe {
            MoveWindow(
                self.list,
                margin,
                margin,
                sidebar,
                (height - margin * 3 - button_height).max(1),
                1,
            );
            MoveWindow(
                GetDlgItem(self.window, ID_NEW as i32),
                margin,
                height - margin - button_height,
                small_width,
                button_height,
                1,
            );
            MoveWindow(
                GetDlgItem(self.window, ID_DELETE as i32),
                margin + small_width + gap,
                height - margin - button_height,
                small_width,
                button_height,
                1,
            );
            MoveWindow(
                self.editor,
                margin * 2 + sidebar,
                margin,
                (width - sidebar - margin * 3).max(1),
                (height - margin * 3 - button_height).max(1),
                1,
            );
            MoveWindow(
                self.pin_button,
                width - margin - pin_width,
                height - margin - button_height,
                pin_width,
                button_height,
                1,
            );
        }
    }

    unsafe fn refresh_list(&self) {
        unsafe { SendMessageW(self.list, LB_RESETCONTENT, 0, 0) };
        for note in self.store.notes() {
            let title = wide(&note_title(&note.body));
            unsafe { SendMessageW(self.list, LB_ADDSTRING, 0, title.as_ptr() as LPARAM) };
        }
        unsafe { SendMessageW(self.list, LB_SETCURSEL, self.current, 0) };
    }

    unsafe fn load_current(&mut self) {
        self.suppress_change = true;
        let body = self
            .store
            .notes()
            .get(self.current)
            .map(|note| note.body.as_str())
            .unwrap_or_default();
        let body = wide(body);
        unsafe {
            SetWindowTextW(self.editor, body.as_ptr());
            SendMessageW(self.editor, EM_SETSEL, 0, 0);
        }
        self.suppress_change = false;
        self.dirty = false;
    }

    unsafe fn capture_editor(&mut self) {
        let length = unsafe { GetWindowTextLengthW(self.editor) };
        let mut buffer = vec![0u16; length as usize + 1];
        unsafe { GetWindowTextW(self.editor, buffer.as_mut_ptr(), buffer.len() as i32) };
        if let Some(note) = self.store.note_mut(self.current) {
            note.body = String::from_utf16_lossy(&buffer[..length as usize]);
        }
    }

    unsafe fn save(&mut self) {
        if !self.dirty {
            return;
        }
        unsafe { self.capture_editor() };
        if let Err(error) = self.store.save(self.current) {
            unsafe { show_error(self.window, &format!("메모를 저장하지 못했습니다: {error}")) };
            return;
        }
        self.dirty = false;
        unsafe {
            KillTimer(self.window, SAVE_TIMER);
            self.refresh_list();
        }
    }

    unsafe fn mark_dirty(&mut self) {
        if self.suppress_change {
            return;
        }
        self.dirty = true;
        unsafe {
            KillTimer(self.window, SAVE_TIMER);
            SetTimer(self.window, SAVE_TIMER, 350, None);
        }
    }

    unsafe fn select_note(&mut self) {
        let selection = unsafe { SendMessageW(self.list, LB_GETCURSEL, 0, 0) } as isize;
        if selection < 0 || selection as usize == self.current {
            return;
        }
        unsafe { self.save() };
        self.current = selection as usize;
        unsafe { self.load_current() };
    }

    unsafe fn new_note(&mut self) {
        unsafe { self.save() };
        match self.store.create_note() {
            Ok(index) => {
                self.current = index;
                unsafe {
                    self.refresh_list();
                    self.load_current();
                    SetFocus(self.editor);
                }
            }
            Err(error) => unsafe {
                show_error(self.window, &format!("새 메모를 만들지 못했습니다: {error}"))
            },
        }
    }

    unsafe fn delete_note(&mut self) {
        unsafe { self.save() };
        let prompt = wide("현재 메모를 삭제할까요?");
        let title = wide(APP_NAME);
        if unsafe {
            MessageBoxW(
                self.window,
                prompt.as_ptr(),
                title.as_ptr(),
                MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2,
            )
        } != IDYES
        {
            return;
        }
        match self.store.delete(self.current) {
            Ok(index) => {
                self.current = index;
                unsafe {
                    self.refresh_list();
                    self.load_current();
                }
            }
            Err(error) => unsafe {
                show_error(self.window, &format!("메모를 삭제하지 못했습니다: {error}"))
            },
        }
    }

    unsafe fn toggle_pin(&mut self) {
        self.pinned = !self.pinned;
        let insert_after = if self.pinned { HWND_TOPMOST } else { HWND_NOTOPMOST };
        unsafe {
            SetWindowPos(
                self.window,
                insert_after,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
            let label = if self.pinned {
                "항상 위: 켜짐"
            } else {
                "항상 위: 꺼짐"
            };
            SetWindowTextW(self.pin_button, wide(label).as_ptr());
        }
    }
}

fn main() {
    if let Err(error) = unsafe { run() } {
        unsafe { show_error(null_mut(), &error) };
    }
}

unsafe fn run() -> Result<(), String> {
    let instance = unsafe { GetModuleHandleW(null()) };
    let class_name = wide(CLASS_NAME);
    let window_class = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        hCursor: unsafe { LoadCursorW(null_mut(), IDC_ARROW) },
        hbrBackground: (COLOR_WINDOW + 1) as usize as *mut c_void,
        lpszClassName: class_name.as_ptr(),
        ..Default::default()
    };
    if unsafe { RegisterClassW(&window_class) } == 0 {
        return Err("Windows 창 클래스를 등록하지 못했습니다.".into());
    }

    let window = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            wide(APP_NAME).as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            760,
            500,
            null_mut(),
            null_mut(),
            instance,
            null(),
        )
    };
    if window.is_null() {
        return Err("메인 창을 만들지 못했습니다.".into());
    }

    let mut state = unsafe { AppState::create(window)? };
    let state_ptr = (&mut *state) as *mut AppState;
    unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, state_ptr as isize) };
    let _owned_state = Box::into_raw(state);
    unsafe {
        (*state_ptr).initialize();
        ShowWindow(window, SW_SHOW);
        UpdateWindow(window);
    }

    let mut message = MSG::default();
    while unsafe { GetMessageW(&mut message, null_mut(), 0, 0) } > 0 {
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(())
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let state = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) as *mut AppState };
    match message {
        WM_SIZE if !state.is_null() => unsafe {
            (*state).layout();
            0
        },
        WM_COMMAND if !state.is_null() => {
            let id = wparam & 0xffff;
            let notification = (wparam >> 16) & 0xffff;
            unsafe {
                match (id, notification as u32) {
                    (ID_EDITOR, EN_CHANGE) => (*state).mark_dirty(),
                    (ID_LIST, LBN_SELCHANGE) => (*state).select_note(),
                    (ID_NEW, BN_CLICKED) => (*state).new_note(),
                    (ID_DELETE, BN_CLICKED) => (*state).delete_note(),
                    (ID_PIN, BN_CLICKED) => (*state).toggle_pin(),
                    _ => {}
                }
            }
            0
        }
        WM_TIMER if !state.is_null() && wparam == SAVE_TIMER => unsafe {
            (*state).save();
            0
        },
        WM_GETMINMAXINFO => {
            if !lparam.is_negative() {
                let info = lparam as *mut MINMAXINFO;
                unsafe {
                    (*info).ptMinTrackSize.x = 560;
                    (*info).ptMinTrackSize.y = 360;
                }
            }
            0
        }
        WM_CLOSE => {
            if !state.is_null() {
                unsafe { (*state).save() };
            }
            unsafe { DestroyWindow(window) };
            0
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            0
        }
        WM_NCDESTROY => {
            if !state.is_null() {
                unsafe {
                    SetWindowLongPtrW(window, GWLP_USERDATA, 0);
                    drop(Box::from_raw(state));
                }
            }
            unsafe { DefWindowProcW(window, message, wparam, lparam) }
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

unsafe fn create_button(window: HWND, instance: *mut c_void, text: &str, id: usize) -> HWND {
    unsafe {
        CreateWindowExW(
            0,
            wide("BUTTON").as_ptr(),
            wide(text).as_ptr(),
            WS_CHILD | WS_VISIBLE | BS_PUSHBUTTON as u32,
            0,
            0,
            0,
            0,
            window,
            id as *mut c_void,
            instance,
            null(),
        )
    }
}

unsafe fn show_error(owner: HWND, message: &str) {
    unsafe {
        MessageBoxW(
            owner,
            wide(message).as_ptr(),
            wide(APP_NAME).as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
