//! 메모 목록을 보여 주고 여러 `NoteWindow` 객체의 수명을 조정한다.
//!
//! `ManagerWindow`는 자신의 컨트롤과 표시 상태를 캡슐화한다. 파일 저장은 `NoteStore`, 실제
//! 픽셀 그리기는 `manager_view`에 위임하여 창 객체가 모든 책임을 떠안지 않게 한다.

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

// Win32 컨트롤 ID는 WM_COMMAND 메시지에서 어떤 UI가 이벤트를 보냈는지 구분하는 식별자다.
const ID_LIST: usize = 1001;
const ID_SEARCH: usize = 1002;
const ID_NEW: usize = 1003;
const ID_DELETE: usize = 1004;
const ID_MANAGER_MIN: usize = 1005;
const ID_MANAGER_CLOSE: usize = 1006;
const ID_MANAGER_MORE: usize = 1007;

/// 관리 창 하나의 상태와 동작을 소유하는 객체다.
struct ManagerWindow {
    /// Windows가 생성한 최상위 창 핸들이다. `attach` 호출 전에는 null이다.
    window: HWND,
    /// 자주 접근하는 자식 컨트롤은 핸들을 캐시한다.
    list: HWND,
    search: HWND,
    /// 모든 메모 창과 공유하는 저장소 소유권이다.
    shared: Rc<SharedState>,
    /// 검색 결과에 현재 표시된 메모 ID만 별도로 보관한다.
    visible_ids: Vec<u64>,
    /// 같은 메모가 중복으로 열리지 않도록 ID와 창 핸들을 연결한다.
    open_notes: HashMap<u64, HWND>,
    /// Font와 Brush는 Drop에서 GDI 자원을 자동 해제하는 RAII 객체다.
    font: Font,
    font_medium: Font,
    font_title: Font,
    background_brush: Brush,
    panel_brush: Brush,
}

impl ManagerWindow {
    /// 아직 Win32 핸들이 연결되지 않은 관리 창 객체를 구성한다.
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

    /// 자식 컨트롤을 생성하고 초기 데이터와 레이아웃을 적용한다.
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
        // 배열로 묶으면 어느 하나라도 생성에 실패했는지 동일한 방식으로 검사할 수 있다.
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

    /// 현재 클라이언트 크기에 맞춰 모든 자식 컨트롤을 재배치한다.
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

    /// 검색어를 적용하여 목록 컨트롤과 `visible_ids`를 같은 순서로 다시 만든다.
    fn refresh_list(&mut self) {
        let query = window_text(self.search).to_lowercase();
        self.visible_ids.clear();
        clear_list(self.list);
        // RefCell의 불변 빌림은 이 블록이 끝나기 전에 명시적으로 해제한다. 그 뒤 다른 메서드가
        // 저장소를 가변로 빌릴 수 있다.
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

    /// 목록의 선택 인덱스를 도메인 ID로 변환한다.
    fn selected_id(&self) -> Option<u64> {
        selected_list_index(self.list).and_then(|index| self.visible_ids.get(index).copied())
    }

    /// 저장소에 빈 메모를 추가한 뒤 즉시 해당 메모 창을 연다.
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

    /// 사용자 확인을 받은 뒤 열린 창과 디스크의 메모를 함께 정리한다.
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

    /// 기존 창이 있으면 활성화하고, 없으면 새 `NoteWindow`를 만든다.
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

    /// WM_COMMAND의 숫자 조합을 의미 있는 객체 메서드 호출로 변환한다.
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

/// `CreateContext`가 객체를 소유한 상태에서 실제 Win32 창을 생성한다.
pub(crate) fn create_window(shared: Rc<SharedState>) -> Result<HWND, String> {
    let context = CreateContext::new(ManagerWindow::new(shared));
    // SAFETY: 문자열은 호출 동안 살아 있는 null 종료 UTF-16이고, lpParam은 같은 호출 동안
    // 유효한 CreateContext를 가리킨다. 객체 수명 이전은 window_host가 담당한다.
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

/// Windows가 요구하는 함수 포인터 형태를 객체 디스패처에 연결하는 얇은 어댑터다.
///
/// # Safety
/// Windows가 등록한 창 클래스의 콜백으로만 호출해야 한다.
pub(crate) unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: 네 인자는 Windows가 이 WNDPROC에 전달한 원본 값이다.
    unsafe { dispatch::<ManagerWindow>(window, message, wparam, lparam) }
}

/// 공통 창 호스트가 `ManagerWindow`를 다형적으로 다룰 수 있게 한다.
impl WindowController for ManagerWindow {
    /// 생성 시점에만 알 수 있는 HWND를 객체에 주입한다.
    fn attach(&mut self, window: HWND) {
        self.window = window;
    }

    /// 절차적인 메시지 번호를 캡슐화된 창 동작으로 라우팅한다.
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
                // SAFETY: WM_DRAWITEM의 lparam은 Windows가 DRAWITEMSTRUCT 주소로 보장한다.
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
