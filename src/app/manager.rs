use std::collections::HashMap;
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
    Brush, Font, add_list_item, clear_list, client_rect, close_window, configure_list, confirm,
    create_child_control, create_owner_button, disable_native_border, finish_list_update,
    hit_test, invalidate, is_window, minimize, move_child, move_window, restore_and_activate,
    quit_message_loop, select_list_item, selected_list_index, set_control_font, set_cue_banner,
    set_dc_colors, set_min_track_size, show_error, wide, window_text,
};
use super::window_host::{CreateContext, MessageResult, WindowController, dispatch};
use super::{APP_NAME, MANAGER_CLASS, SharedState, WM_NOTE_CLOSED, WM_REFRESH_NOTES};

const ID_LIST: usize = 1001;
const ID_SEARCH: usize = 1002;
const ID_NEW: usize = 1003;
const ID_DELETE: usize = 1004;
const ID_MANAGER_MIN: usize = 1005;
const ID_MANAGER_CLOSE: usize = 1006;
const ID_MANAGER_MORE: usize = 1007;

struct ManagerWindow {
    window: HWND,
    list: HWND,
    search: HWND,
    shared: Rc<SharedState>,
    visible_ids: Vec<u64>,
    open_notes: HashMap<u64, HWND>,
    font: Font,
    font_medium: Font,
    font_title: Font,
    background_brush: Brush,
    panel_brush: Brush,
}

impl ManagerWindow {
    fn new(shared: Rc<SharedState>) -> Self {
        Self {
            window: null_mut(),
            list: null_mut(),
            search: null_mut(),
            shared,
            visible_ids: Vec::new(),
            open_notes: HashMap::new(),
            font: Font::new(18, FW_NORMAL as i32, false),
            font_medium: Font::new(18, FW_SEMIBOLD as i32, false),
            font_title: Font::new(20, FW_SEMIBOLD as i32, false),
            background_brush: Brush::new(MANAGER_BG),
            panel_brush: Brush::new(PANEL_BG),
        }
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
        set_control_font(self.search, self.font.handle());
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

pub(crate) fn create_window(shared: Rc<SharedState>) -> Result<HWND, String> {
    let context = CreateContext::new(ManagerWindow::new(shared));
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
            context.as_ptr(),
        )
    };
    if window.is_null() {
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
    unsafe { dispatch::<ManagerWindow>(window, message, wparam, lparam) }
}

impl WindowController for ManagerWindow {
    fn attach(&mut self, window: HWND) {
        self.window = window;
    }

    fn handle_message(
        &mut self,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> MessageResult {
        match message {
            WM_CREATE => {
                disable_native_border(self.window);
                match self.initialize() {
                    Ok(()) => MessageResult::Handled(0),
                    Err(error) => {
                        show_error(self.window, &error);
                        MessageResult::Handled(-1)
                    }
                }
            }
            WM_NCCALCSIZE => MessageResult::Handled(0),
            WM_NCPAINT => MessageResult::Handled(0),
            WM_NCACTIVATE => MessageResult::Handled(1),
            WM_NCHITTEST => MessageResult::Handled(hit_test(self.window, lparam, 68)),
            WM_SIZE => {
                self.layout();
                MessageResult::Handled(0)
            }
            WM_PAINT => {
                manager_view::paint(
                    self.window,
                    self.visible_ids.len(),
                    self.font_title.handle(),
                    self.font_medium.handle(),
                );
                MessageResult::Handled(0)
            }
            WM_ERASEBKGND => MessageResult::Handled(1),
            WM_CTLCOLORLISTBOX => {
                let hdc = wparam as HDC;
                set_dc_colors(hdc, MANAGER_BG, TEXT);
                MessageResult::Handled(self.background_brush.handle() as LRESULT)
            }
            WM_CTLCOLOREDIT => {
                let hdc = wparam as HDC;
                set_dc_colors(hdc, PANEL_BG, TEXT);
                MessageResult::Handled(self.panel_brush.handle() as LRESULT)
            }
            WM_DRAWITEM => {
                let Some(draw) = (unsafe { (lparam as *const DRAWITEMSTRUCT).as_ref() }) else {
                    return MessageResult::Handled(0);
                };
                if draw.CtlID == ID_LIST as u32 {
                    manager_view::draw_note_card(
                        draw,
                        &self.shared,
                        &self.visible_ids,
                        self.font.handle(),
                        self.font_medium.handle(),
                    )
                } else {
                    let header = matches!(
                        draw.CtlID as usize,
                        ID_MANAGER_MIN | ID_MANAGER_CLOSE | ID_MANAGER_MORE
                    );
                    manager_view::draw_button(draw, self.font.handle(), header)
                }
                MessageResult::Handled(1)
            }
            WM_COMMAND => {
                let id = wparam & 0xffff;
                let notification = ((wparam >> 16) & 0xffff) as u32;
                self.handle_command(id, notification);
                MessageResult::Handled(0)
            }
            WM_REFRESH_NOTES => {
                self.refresh_list();
                MessageResult::Handled(0)
            }
            WM_NOTE_CLOSED => {
                self.open_notes.remove(&(wparam as u64));
                MessageResult::Handled(0)
            }
            WM_GETMINMAXINFO => {
                set_min_track_size(lparam, 440, 540);
                MessageResult::Handled(0)
            }
            WM_CLOSE => MessageResult::Destroy,
            WM_DESTROY => {
                quit_message_loop();
                MessageResult::Handled(0)
            }
            _ => MessageResult::Default,
        }
    }
}
