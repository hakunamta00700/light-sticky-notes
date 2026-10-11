use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::rc::Rc;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::store::note_title;

use super::manager_view;
use super::markdown;
use super::note;
use super::theme::{MANAGER_BG, PANEL_BG, TEXT};
use super::win32::{
    add_list_item, clear_list, client_rect, configure_list, create_brush,
    close_window, confirm, create_child_control, create_font, create_owner_button,
    delete_gdi_object, disable_native_border, finish_list_update, hit_test, invalidate,
    is_window, minimize, move_child, move_window, restore_and_activate, select_list_item,
    selected_list_index, set_control_font, set_cue_banner, show_error, wide, window_text,
};
use super::{APP_NAME, MANAGER_CLASS, SharedState, WM_NOTE_CLOSED, WM_REFRESH_NOTES};

const ID_LIST: usize = 1001;
const ID_SEARCH: usize = 1002;
const ID_NEW: usize = 1003;
const ID_DELETE: usize = 1004;
const ID_MANAGER_MIN: usize = 1005;
const ID_MANAGER_CLOSE: usize = 1006;
const ID_MANAGER_MORE: usize = 1007;

struct ManagerState {
    window: HWND,
    list: HWND,
    search: HWND,
    shared: Rc<SharedState>,
    visible_ids: Vec<u64>,
    open_notes: HashMap<u64, HWND>,
    font: HFONT,
    font_medium: HFONT,
    font_title: HFONT,
    background_brush: HBRUSH,
    panel_brush: HBRUSH,
}

impl ManagerState {
    fn new(shared: Rc<SharedState>) -> Box<Self> {
        Box::new(Self {
            window: null_mut(),
            list: null_mut(),
            search: null_mut(),
            shared,
            visible_ids: Vec::new(),
            open_notes: HashMap::new(),
            font: create_font(18, FW_NORMAL as i32, false),
            font_medium: create_font(18, FW_SEMIBOLD as i32, false),
            font_title: create_font(20, FW_SEMIBOLD as i32, false),
            background_brush: create_brush(MANAGER_BG),
            panel_brush: create_brush(PANEL_BG),
        })
    }

    fn initialize(&mut self) -> Result<(), String> {
        self.search = create_child_control(
            self.window,
            "EDIT",
            WS_CHILD | WS_VISIBLE | ES_LEFT as u32 | ES_AUTOHSCROLL as u32,
            ID_SEARCH,
        );
        self.list = create_child_control(
            self.window,
            "LISTBOX",
            WS_CHILD
                | WS_VISIBLE
                | WS_VSCROLL
                | LBS_NOTIFY as u32
                | LBS_OWNERDRAWFIXED as u32
                | LBS_HASSTRINGS as u32
                | LBS_NOINTEGRALHEIGHT as u32,
            ID_LIST,
        );
        let controls = [
            self.search,
            self.list,
            create_owner_button(self.window, "+  새 메모", ID_NEW),
            create_owner_button(self.window, "메모 삭제", ID_DELETE),
            create_owner_button(self.window, "•••", ID_MANAGER_MORE),
            create_owner_button(self.window, "—", ID_MANAGER_MIN),
            create_owner_button(self.window, "×", ID_MANAGER_CLOSE),
        ];
        if controls.iter().any(|handle| handle.is_null()) {
            return Err("관리 창의 컨트롤을 만들지 못했습니다.".into());
        }
        set_control_font(self.search, self.font);
        set_cue_banner(self.search, "메모 검색");
        configure_list(self.list, 126);
        self.refresh_list();
        self.layout();
        Ok(())
    }

    fn layout(&self) {
        let rect = client_rect(self.window);
        let width = rect.right.max(1);
        let height = rect.bottom.max(1);
        let margin = 24;
        let action_y = 88;
        let action_height = 60;
        let gap = 10;
        let action_width = ((width - margin * 2 - gap) / 2).max(1);
        move_child(self.window, ID_MANAGER_MORE, width - 160, 12, 48, 44);
        move_child(self.window, ID_MANAGER_MIN, width - 108, 12, 48, 44);
        move_child(self.window, ID_MANAGER_CLOSE, width - 56, 12, 48, 44);
        move_child(self.window, ID_NEW, margin, action_y, action_width, action_height);
        move_child(
            self.window,
            ID_DELETE,
            margin + action_width + gap,
            action_y,
            action_width,
            action_height,
        );
        move_window(
            self.search,
            (width - 220).max(180),
            action_y + action_height + 24,
            196,
            40,
        );
        move_window(
            self.list,
            margin,
            action_y + action_height + 80,
            width - margin * 2,
            (height - action_y - action_height - 100).max(1),
        );
    }

    fn refresh_list(&mut self) {
        let query = window_text(self.search).to_lowercase();
        self.visible_ids.clear();
        clear_list(self.list);
        let store = self.shared.store.borrow();
        for note in store.notes() {
            if !query.is_empty() && !note.body.to_lowercase().contains(&query) {
                continue;
            }
            self.visible_ids.push(note.id);
            let title = note_title(&markdown::plain_text(&note.body));
            add_list_item(self.list, &title);
        }
        drop(store);
        if !self.visible_ids.is_empty() {
            select_list_item(self.list, 0);
        }
        finish_list_update(self.list);
        invalidate(self.window, false);
    }

    fn selected_id(&self) -> Option<u64> {
        selected_list_index(self.list).and_then(|index| self.visible_ids.get(index).copied())
    }

    fn new_note(&mut self) {
        let result = self.shared.store.borrow_mut().create_note();
        match result {
            Ok(index) => {
                let id = self.shared.store.borrow().notes()[index].id;
                self.refresh_list();
                self.open_note(id);
            }
            Err(error) => {
                show_error(self.window, &format!("새 메모를 만들지 못했습니다. {error}"))
            }
        }
    }

    fn delete_selected(&mut self) {
        let Some(id) = self.selected_id() else {
            return;
        };
        if !confirm(self.window, "선택한 메모를 삭제할까요?") {
            return;
        }
        if let Some(note_window) = self.open_notes.get(&id).copied() {
            close_window(note_window);
        }
        let index = self.shared.store.borrow().index_of(id);
        if let Some(index) = index {
            if let Err(error) = self.shared.store.borrow_mut().delete(index) {
                show_error(self.window, &format!("메모를 삭제하지 못했습니다. {error}"));
            }
        }
        self.refresh_list();
    }

    fn open_note(&mut self, id: u64) {
        if let Some(window) = self.open_notes.get(&id).copied() {
            if is_window(window) {
                restore_and_activate(window);
                return;
            }
            self.open_notes.remove(&id);
        }
        match note::create_window(self.shared.clone(), self.window, id) {
            Ok(window) => {
                self.open_notes.insert(id, window);
            }
            Err(error) => show_error(self.window, &error),
        }
    }

    fn handle_command(&mut self, id: usize, notification: u32) {
        match (id, notification) {
            (ID_NEW, BN_CLICKED) => self.new_note(),
            (ID_DELETE | ID_MANAGER_MORE, BN_CLICKED) => self.delete_selected(),
            (ID_MANAGER_MIN, BN_CLICKED) => minimize(self.window),
            (ID_MANAGER_CLOSE, BN_CLICKED) => close_window(self.window),
            (ID_LIST, LBN_SELCHANGE | LBN_DBLCLK) => {
                if let Some(id) = self.selected_id() {
                    self.open_note(id);
                }
            }
            (ID_SEARCH, EN_CHANGE) => self.refresh_list(),
            _ => {}
        }
    }
}

impl Drop for ManagerState {
    fn drop(&mut self) {
        delete_gdi_object(self.font);
        delete_gdi_object(self.font_medium);
        delete_gdi_object(self.font_title);
        delete_gdi_object(self.background_brush);
        delete_gdi_object(self.panel_brush);
    }
}

pub(crate) fn create_window(shared: Rc<SharedState>) -> Result<HWND, String> {
    let state_ptr = Box::into_raw(ManagerState::new(shared));
    let window = unsafe {
        CreateWindowExW(
            WS_EX_APPWINDOW,
            wide(MANAGER_CLASS).as_ptr(),
            wide(APP_NAME).as_ptr(),
            WS_POPUP | WS_THICKFRAME | WS_MINIMIZEBOX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            560,
            760,
            null_mut(),
            null_mut(),
            GetModuleHandleW(null()),
            state_ptr as *mut c_void,
        )
    };
    if window.is_null() {
        unsafe { drop(Box::from_raw(state_ptr)) };
        return Err("관리 창을 만들지 못했습니다.".into());
    }
    Ok(window)
}

pub(crate) unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = lparam as *const CREATESTRUCTW;
        let state = unsafe { (*create).lpCreateParams as *mut ManagerState };
        unsafe {
            (*state).window = window;
            SetWindowLongPtrW(window, GWLP_USERDATA, state as isize);
        }
    }
    let state = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) as *mut ManagerState };
    match message {
        WM_CREATE if !state.is_null() => {
            disable_native_border(window);
            match unsafe { (*state).initialize() } {
                Ok(()) => 0,
                Err(error) => {
                    show_error(window, &error);
                    -1
                }
            }
        }
        WM_NCCALCSIZE => 0,
        WM_NCPAINT => 0,
        WM_NCACTIVATE => 1,
        WM_NCHITTEST => hit_test(window, lparam, 68),
        WM_SIZE if !state.is_null() => {
            unsafe { (*state).layout() };
            0
        }
        WM_PAINT if !state.is_null() => {
            unsafe {
                manager_view::paint(
                    window,
                    (*state).visible_ids.len(),
                    (*state).font_title,
                    (*state).font_medium,
                )
            };
            0
        }
        WM_ERASEBKGND => 1,
        WM_CTLCOLORLISTBOX if !state.is_null() => {
            let hdc = wparam as HDC;
            unsafe {
                SetBkColor(hdc, MANAGER_BG);
                SetTextColor(hdc, TEXT);
                (*state).background_brush as LRESULT
            }
        }
        WM_CTLCOLOREDIT if !state.is_null() => {
            let hdc = wparam as HDC;
            unsafe {
                SetBkColor(hdc, PANEL_BG);
                SetTextColor(hdc, TEXT);
                (*state).panel_brush as LRESULT
            }
        }
        WM_DRAWITEM if !state.is_null() => {
            let draw = unsafe { &*(lparam as *const DRAWITEMSTRUCT) };
            if draw.CtlID == ID_LIST as u32 {
                unsafe {
                    manager_view::draw_note_card(
                        draw,
                        &(*state).shared,
                        &(*state).visible_ids,
                        (*state).font,
                        (*state).font_medium,
                    )
                }
            } else {
                let header = matches!(
                    draw.CtlID as usize,
                    ID_MANAGER_MIN | ID_MANAGER_CLOSE | ID_MANAGER_MORE
                );
                unsafe { manager_view::draw_button(draw, (*state).font, header) }
            }
            1
        }
        WM_COMMAND if !state.is_null() => {
            let id = wparam & 0xffff;
            let notification = ((wparam >> 16) & 0xffff) as u32;
            unsafe { (*state).handle_command(id, notification) };
            0
        }
        WM_REFRESH_NOTES if !state.is_null() => {
            unsafe { (*state).refresh_list() };
            0
        }
        WM_NOTE_CLOSED if !state.is_null() => {
            unsafe { (*state).open_notes.remove(&(wparam as u64)) };
            0
        }
        WM_GETMINMAXINFO => {
            let info = lparam as *mut MINMAXINFO;
            if !info.is_null() {
                unsafe {
                    (*info).ptMinTrackSize.x = 440;
                    (*info).ptMinTrackSize.y = 540;
                }
            }
            0
        }
        WM_CLOSE => {
            unsafe { DestroyWindow(window) };
            0
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            0
        }
        WM_NCDESTROY if !state.is_null() => unsafe {
            SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            drop(Box::from_raw(state));
            DefWindowProcW(window, message, wparam, lparam)
        },
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}
