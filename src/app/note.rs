use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::rc::Rc;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::store::NoteColor;

use super::markdown::{self, InlineFormat};
use super::note_view::{self, ButtonKind};
use super::theme::{TEXT, note_palette};
use super::win32::{
    client_rect, close_window, create_brush, create_child_control, create_font,
    create_owner_button, delete_gdi_object, disable_native_border, focus, hit_test, minimize,
    move_child, move_window, notify, restart_timer, restore_and_activate, set_control_font,
    set_cue_banner, set_topmost, show_error, show_popup_menu, stop_timer, wide,
};
use super::{NOTE_CLASS, SharedState, WM_NOTE_CLOSED, WM_REFRESH_NOTES};

const ID_EDITOR: usize = 2001;
const ID_NOTE_MENU: usize = 2002;
const ID_NOTE_MORE: usize = 2003;
const ID_NOTE_PIN: usize = 2004;
const ID_NOTE_MIN: usize = 2005;
const ID_NOTE_CLOSE: usize = 2006;
const ID_BOLD: usize = 2010;
const ID_ITALIC: usize = 2011;
const ID_UNDERLINE: usize = 2012;
const ID_STRIKE: usize = 2013;
const ID_BULLET: usize = 2014;
const ID_SHOW_MANAGER: usize = 2015;
const ID_COLOR_YELLOW: usize = 2101;
const ID_COLOR_GREEN: usize = 2102;
const ID_COLOR_BLUE: usize = 2103;
const ID_COLOR_PINK: usize = 2104;
const ID_COLOR_PURPLE: usize = 2105;
const ID_COLOR_GRAY: usize = 2106;

const SAVE_TIMER: usize = 1;
const SAVE_DELAY_MS: u32 = 500;

struct NoteState {
    window: HWND,
    editor: HWND,
    shared: Rc<SharedState>,
    manager: HWND,
    note_id: u64,
    color: NoteColor,
    dirty: bool,
    suppress_change: bool,
    pinned: bool,
    font: HFONT,
    font_button: HFONT,
    note_brush: HBRUSH,
}

impl NoteState {
    fn new(shared: Rc<SharedState>, manager: HWND, note_id: u64) -> Box<Self> {
        let color = shared
            .store
            .borrow()
            .note_by_id(note_id)
            .map(|note| note.color)
            .unwrap_or_default();
        let palette = note_palette(color);
        Box::new(Self {
            window: null_mut(),
            editor: null_mut(),
            shared,
            manager,
            note_id,
            color,
            dirty: false,
            suppress_change: false,
            pinned: false,
            font: create_font(22, FW_NORMAL as i32, false),
            font_button: create_font(18, FW_NORMAL as i32, false),
            note_brush: create_brush(palette.body),
        })
    }

    fn initialize(&mut self) -> Result<(), String> {
        self.editor = create_child_control(
            self.window,
            markdown::RICH_EDIT_CLASS,
            WS_CHILD
                | WS_VISIBLE
                | ES_LEFT as u32
                | ES_MULTILINE as u32
                | ES_AUTOVSCROLL as u32
                | ES_WANTRETURN as u32
                | ES_NOHIDESEL as u32,
            ID_EDITOR,
        );
        let controls = [
            self.editor,
            create_owner_button(self.window, "☰", ID_NOTE_MENU),
            create_owner_button(self.window, "•••", ID_NOTE_MORE),
            create_owner_button(self.window, "⌖", ID_NOTE_PIN),
            create_owner_button(self.window, "—", ID_NOTE_MIN),
            create_owner_button(self.window, "×", ID_NOTE_CLOSE),
            create_owner_button(self.window, "B", ID_BOLD),
            create_owner_button(self.window, "I", ID_ITALIC),
            create_owner_button(self.window, "U", ID_UNDERLINE),
            create_owner_button(self.window, "S", ID_STRIKE),
            create_owner_button(self.window, "☷", ID_BULLET),
            create_owner_button(self.window, "목록 열기", ID_SHOW_MANAGER),
        ];
        if controls.iter().any(|handle| handle.is_null()) {
            return Err("메모 창의 컨트롤을 만들지 못했습니다.".into());
        }
        set_control_font(self.editor, self.font);
        markdown::configure(self.editor, note_palette(self.color).body, TEXT);
        set_cue_banner(self.editor, "기억할 내용을 입력하세요");
        self.load();
        self.layout();
        focus(self.editor);
        Ok(())
    }

    fn layout(&self) {
        let rect = client_rect(self.window);
        let width = rect.right.max(1);
        let height = rect.bottom.max(1);
        let toolbar_y = height - 56;
        move_child(self.window, ID_NOTE_MENU, 4, 4, 44, 44);
        move_child(self.window, ID_NOTE_MORE, width - 190, 4, 44, 44);
        move_child(self.window, ID_NOTE_PIN, width - 146, 4, 44, 44);
        move_child(self.window, ID_NOTE_MIN, width - 102, 4, 44, 44);
        move_child(self.window, ID_NOTE_CLOSE, width - 54, 4, 48, 44);
        move_window(
            self.editor,
            20,
            72,
            (width - 40).max(1),
            (height - 146).max(1),
        );
        for (offset, id) in [ID_BOLD, ID_ITALIC, ID_UNDERLINE, ID_STRIKE, ID_BULLET]
            .iter()
            .enumerate()
        {
            move_child(
                self.window,
                *id,
                10 + offset as i32 * 48,
                toolbar_y,
                42,
                44,
            );
        }
        move_child(
            self.window,
            ID_SHOW_MANAGER,
            (width - 132).max(250),
            toolbar_y,
            120,
            44,
        );
    }

    fn load(&mut self) {
        self.suppress_change = true;
        let body = self
            .shared
            .store
            .borrow()
            .note_by_id(self.note_id)
            .map(|note| note.body.clone())
            .unwrap_or_default();
        markdown::load(self.editor, &body, TEXT);
        self.suppress_change = false;
        self.dirty = false;
    }

    fn mark_dirty(&mut self) {
        if self.suppress_change {
            return;
        }
        self.dirty = true;
        restart_timer(self.window, SAVE_TIMER, SAVE_DELAY_MS);
    }

    fn save(&mut self) {
        if !self.dirty {
            return;
        }
        let body = markdown::to_markdown(self.editor);
        let result = {
            let mut store = self.shared.store.borrow_mut();
            if let Some(note) = store.note_by_id_mut(self.note_id) {
                note.body = body;
            }
            store
                .index_of(self.note_id)
                .ok_or_else(|| "메모를 찾을 수 없습니다.".to_string())
                .and_then(|index| store.save(index).map_err(|error| error.to_string()))
        };
        if let Err(error) = result {
            stop_timer(self.window, SAVE_TIMER);
            show_error(self.window, &format!("메모를 저장하지 못했습니다. {error}"));
            return;
        }
        self.dirty = false;
        stop_timer(self.window, SAVE_TIMER);
        notify(self.manager, WM_REFRESH_NOTES, 0);
    }

    fn toggle_pin(&mut self) {
        self.pinned = !self.pinned;
        set_topmost(self.window, self.pinned);
        super::win32::invalidate_child(self.window, ID_NOTE_PIN, true);
    }

    fn show_color_menu(&mut self) {
        let items: Vec<_> = color_choices()
            .into_iter()
            .map(|(id, color, label)| (id, label, color == self.color))
            .collect();
        let selected = show_popup_menu(self.window, ID_NOTE_MORE, &items);
        if let Some(color) = selected.and_then(color_from_command) {
            self.set_color(color);
        }
    }

    fn set_color(&mut self, color: NoteColor) {
        if color == self.color {
            return;
        }
        self.save();
        if self.dirty {
            return;
        }
        if let Err(error) = self.shared.store.borrow_mut().set_color(self.note_id, color) {
            show_error(
                self.window,
                &format!("메모 색상을 저장하지 못했습니다. {error}"),
            );
            return;
        }
        self.color = color;
        let palette = note_palette(color);
        delete_gdi_object(self.note_brush);
        self.note_brush = create_brush(palette.body);
        markdown::set_background(self.editor, palette.body);
        super::win32::invalidate(self.editor, true);
        super::win32::invalidate(self.window, true);
        notify(self.manager, WM_REFRESH_NOTES, 0);
    }

    fn toggle_inline(&mut self, kind: InlineFormat) {
        markdown::toggle_inline(self.editor, kind);
        focus(self.editor);
        self.mark_dirty();
    }

    fn toggle_bullet(&mut self) {
        markdown::toggle_bullet(self.editor);
        focus(self.editor);
        self.mark_dirty();
    }

    fn handle_command(&mut self, id: usize, notification: u32) {
        match (id, notification) {
            (ID_EDITOR, EN_CHANGE) => self.mark_dirty(),
            (ID_NOTE_CLOSE, BN_CLICKED) => close_window(self.window),
            (ID_NOTE_MIN, BN_CLICKED) => minimize(self.window),
            (ID_NOTE_PIN, BN_CLICKED) => self.toggle_pin(),
            (ID_NOTE_MENU | ID_SHOW_MANAGER, BN_CLICKED) => {
                restore_and_activate(self.manager)
            }
            (ID_NOTE_MORE, BN_CLICKED) => self.show_color_menu(),
            (
                ID_COLOR_YELLOW
                | ID_COLOR_GREEN
                | ID_COLOR_BLUE
                | ID_COLOR_PINK
                | ID_COLOR_PURPLE
                | ID_COLOR_GRAY,
                0,
            ) => {
                if let Some(color) = color_from_command(id) {
                    self.set_color(color);
                }
            }
            (ID_BOLD, BN_CLICKED) => self.toggle_inline(InlineFormat::Bold),
            (ID_ITALIC, BN_CLICKED) => self.toggle_inline(InlineFormat::Italic),
            (ID_UNDERLINE, BN_CLICKED) => self.toggle_inline(InlineFormat::Underline),
            (ID_STRIKE, BN_CLICKED) => self.toggle_inline(InlineFormat::Strike),
            (ID_BULLET, BN_CLICKED) => self.toggle_bullet(),
            _ => {}
        }
    }
}

impl Drop for NoteState {
    fn drop(&mut self) {
        delete_gdi_object(self.font);
        delete_gdi_object(self.font_button);
        delete_gdi_object(self.note_brush);
    }
}

pub(crate) fn create_window(
    shared: Rc<SharedState>,
    manager: HWND,
    note_id: u64,
) -> Result<HWND, String> {
    let state_ptr = Box::into_raw(NoteState::new(shared, manager, note_id));
    let window = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            wide(NOTE_CLASS).as_ptr(),
            wide("메모").as_ptr(),
            WS_POPUP | WS_THICKFRAME | WS_MINIMIZEBOX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            630,
            760,
            manager,
            null_mut(),
            GetModuleHandleW(null()),
            state_ptr as *mut c_void,
        )
    };
    if window.is_null() {
        unsafe { drop(Box::from_raw(state_ptr)) };
        return Err("메모 창을 만들지 못했습니다.".into());
    }
    unsafe {
        ShowWindow(window, SW_SHOW);
        UpdateWindow(window);
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
        let state = unsafe { (*create).lpCreateParams as *mut NoteState };
        unsafe {
            (*state).window = window;
            SetWindowLongPtrW(window, GWLP_USERDATA, state as isize);
        }
    }
    let state = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) as *mut NoteState };
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
        WM_NCHITTEST => hit_test(window, lparam, 52),
        WM_SIZE if !state.is_null() => {
            unsafe { (*state).layout() };
            0
        }
        WM_PAINT if !state.is_null() => {
            unsafe { note_view::paint(window, (*state).color) };
            0
        }
        WM_ERASEBKGND => 1,
        WM_CTLCOLOREDIT if !state.is_null() => {
            let hdc = wparam as HDC;
            unsafe {
                SetBkColor(hdc, note_palette((*state).color).body);
                SetTextColor(hdc, TEXT);
                (*state).note_brush as LRESULT
            }
        }
        WM_DRAWITEM if !state.is_null() => {
            let draw = unsafe { &*(lparam as *const DRAWITEMSTRUCT) };
            let kind = match draw.CtlID as usize {
                ID_NOTE_PIN => ButtonKind::Pin,
                ID_NOTE_MENU | ID_NOTE_MORE | ID_NOTE_MIN | ID_NOTE_CLOSE => ButtonKind::Title,
                ID_SHOW_MANAGER => ButtonKind::Manager,
                _ => ButtonKind::Toolbar,
            };
            unsafe {
                note_view::draw_button(
                    draw,
                    (*state).color,
                    (*state).pinned,
                    (*state).font_button,
                    kind,
                )
            };
            1
        }
        WM_COMMAND if !state.is_null() => {
            let id = wparam & 0xffff;
            let notification = ((wparam >> 16) & 0xffff) as u32;
            unsafe { (*state).handle_command(id, notification) };
            0
        }
        WM_TIMER if !state.is_null() && wparam == SAVE_TIMER => {
            unsafe { (*state).save() };
            0
        }
        WM_GETMINMAXINFO => {
            let info = lparam as *mut MINMAXINFO;
            if !info.is_null() {
                unsafe {
                    (*info).ptMinTrackSize.x = 380;
                    (*info).ptMinTrackSize.y = 320;
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
        WM_NCDESTROY if !state.is_null() => {
            let note_id = unsafe { (*state).note_id };
            let manager = unsafe { (*state).manager };
            unsafe {
                SetWindowLongPtrW(window, GWLP_USERDATA, 0);
                if IsWindow(manager) != 0 {
                    SendMessageW(manager, WM_NOTE_CLOSED, note_id as WPARAM, 0);
                }
                drop(Box::from_raw(state));
                DefWindowProcW(window, message, wparam, lparam)
            }
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

fn color_choices() -> [(usize, NoteColor, &'static str); 6] {
    [
        (ID_COLOR_YELLOW, NoteColor::Yellow, "노랑"),
        (ID_COLOR_GREEN, NoteColor::Green, "초록"),
        (ID_COLOR_BLUE, NoteColor::Blue, "파랑"),
        (ID_COLOR_PINK, NoteColor::Pink, "분홍"),
        (ID_COLOR_PURPLE, NoteColor::Purple, "보라"),
        (ID_COLOR_GRAY, NoteColor::Gray, "회색"),
    ]
}

fn color_from_command(id: usize) -> Option<NoteColor> {
    color_choices()
        .into_iter()
        .find_map(|(choice, color, _)| (choice == id).then_some(color))
}
