//! 개별 포스트잇 창의 편집 상태와 사용자 동작을 캡슐화한다.
//!
//! `NoteWindow`는 자동 저장 타이머, 선택한 색상, 고정 여부를 소유한다. Markdown 해석은
//! `markdown`, 그리기는 `note_view`, 파일 입출력은 `NoteStore`에 위임한다.

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
    Brush, Dpi, Font, apply_dpi_suggested_rect, client_rect, close_window,
    create_child_control, create_owner_button, disable_native_border, focus, hit_test,
    invalidate, is_window, minimize, move_child, move_window, notify, restart_timer,
    restore_and_activate, set_control_font, set_cue_banner, set_dc_colors,
    set_min_track_size, set_topmost, show_and_update, show_error, show_popup_menu,
    stop_timer, wide,
};
use super::window_host::{CreateContext, MessageResult, WindowController, dispatch};
use super::{NOTE_CLASS, SharedState, WM_NOTE_CLOSED, WM_REFRESH_NOTES};

// 컨트롤 ID는 버튼과 편집기에서 발생한 WM_COMMAND를 객체 메서드로 연결하는 키다.
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

// 사용자가 입력을 멈춘 뒤 500ms가 지나야 저장하여 키 입력마다 디스크를 쓰지 않는다.
const SAVE_TIMER: usize = 1;
const SAVE_DELAY_MS: u32 = 500;

/// 열린 메모 창 하나의 UI 상태와 동작을 소유한다.
struct NoteWindow {
    /// 현재 메모 창과 편집 컨트롤의 Win32 핸들이다.
    window: HWND,
    editor: HWND,
    /// 저장소는 다른 창과 공유하지만 메모 ID와 UI 상태는 이 객체만 소유한다.
    shared: Rc<SharedState>,
    manager: HWND,
    note_id: u64,
    color: NoteColor,
    /// `dirty`는 저장되지 않은 변경을, `suppress_change`는 프로그램이 넣은 텍스트를 구분한다.
    dirty: bool,
    suppress_change: bool,
    pinned: bool,
    /// 현재 모니터 배율을 캡슐화해 레이아웃과 폰트가 같은 기준으로 확대되게 한다.
    dpi: Dpi,
    /// GDI 자원은 RAII 타입이므로 객체가 사라질 때 자동으로 해제된다.
    font: Font,
    font_button: Font,
    note_brush: Brush,
}

impl NoteWindow {
    /// 저장소에서 초기 색상을 읽어 아직 HWND가 없는 창 객체를 만든다.
    fn new(shared: Rc<SharedState>, manager: HWND, note_id: u64, dpi: Dpi) -> Self {
        let color = shared
            .store
            .borrow()
            .note_by_id(note_id)
            .map(|note| note.color)
            .unwrap_or_default();
        let palette = note_palette(color);
        Self {
            window: null_mut(),
            editor: null_mut(),
            shared,
            manager,
            note_id,
            color,
            dirty: false,
            suppress_change: false,
            pinned: false,
            dpi,
            font: Font::new(22, FW_NORMAL as i32, false, dpi),
            font_button: Font::new(18, FW_NORMAL as i32, false, dpi),
            note_brush: Brush::new(palette.body),
        }
    }

    /// Rich Edit과 도구 버튼을 만들고 저장된 메모를 화면에 올린다.
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
        set_control_font(self.editor, self.font.handle());
        markdown::configure(self.editor, note_palette(self.color).body, TEXT);
        set_cue_banner(self.editor, "기억할 내용을 입력하세요");
        self.load();
        self.layout();
        focus(self.editor);
        Ok(())
    }

    /// 창 크기에 맞춰 제목 표시줄, 편집기, 서식 도구 모음을 배치한다.
    fn layout(&self) {
        let rect = client_rect(self.window);
        let width = rect.right.max(1);
        let height = rect.bottom.max(1);
        let scale = |value| self.dpi.scale(value);
        let toolbar_y = height - scale(56);
        move_child(
            self.window,
            ID_NOTE_MENU,
            scale(4),
            scale(4),
            scale(44),
            scale(44),
        );
        move_child(
            self.window,
            ID_NOTE_MORE,
            width - scale(190),
            scale(4),
            scale(44),
            scale(44),
        );
        move_child(
            self.window,
            ID_NOTE_PIN,
            width - scale(146),
            scale(4),
            scale(44),
            scale(44),
        );
        move_child(
            self.window,
            ID_NOTE_MIN,
            width - scale(102),
            scale(4),
            scale(44),
            scale(44),
        );
        move_child(
            self.window,
            ID_NOTE_CLOSE,
            width - scale(54),
            scale(4),
            scale(48),
            scale(44),
        );
        move_window(
            self.editor,
            scale(20),
            scale(72),
            (width - scale(40)).max(1),
            (height - scale(146)).max(1),
        );
        for (offset, id) in [ID_BOLD, ID_ITALIC, ID_UNDERLINE, ID_STRIKE, ID_BULLET]
            .iter()
            .enumerate()
        {
            move_child(
                self.window,
                *id,
                scale(10 + offset as i32 * 48),
                toolbar_y,
                scale(42),
                scale(44),
            );
        }
        move_child(
            self.window,
            ID_SHOW_MANAGER,
            (width - scale(132)).max(scale(250)),
            toolbar_y,
            scale(120),
            scale(44),
        );
    }

    /// 새 모니터의 DPI에 맞춰 폰트와 모든 자식 컨트롤을 다시 구성한다.
    fn update_dpi(&mut self, dpi: Dpi) {
        if self.dpi == dpi {
            return;
        }
        self.dpi = dpi;
        let font = Font::new(22, FW_NORMAL as i32, false, dpi);
        let font_button = Font::new(18, FW_NORMAL as i32, false, dpi);
        // Rich Edit가 새 핸들을 저장한 뒤 기존 Font를 교체하고 Drop한다.
        set_control_font(self.editor, font.handle());
        self.font = font;
        self.font_button = font_button;
        self.layout();
        invalidate(self.window, true);
    }

    /// Markdown 원문을 저장소에서 복제한 뒤 서식이 적용된 Rich Edit 내용으로 변환한다.
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

    /// 사용자 편집을 더티 상태로 표시하고 디바운스 타이머를 처음부터 다시 시작한다.
    fn mark_dirty(&mut self) {
        if self.suppress_change {
            return;
        }
        self.dirty = true;
        restart_timer(self.window, SAVE_TIMER, SAVE_DELAY_MS);
    }

    /// Rich Edit의 시각적 서식을 Markdown으로 직렬화하여 저장한다.
    fn save(&mut self) {
        if !self.dirty {
            return;
        }
        let body = markdown::to_markdown(self.editor);
        // 가변 빌림의 범위를 블록으로 제한한다. 블록이 끝나면 RefMut이 해제되어 관리자 창이
        // 같은 저장소를 다시 읽을 수 있다.
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

    /// 항상 위 속성을 전환하고 핀 버튼을 다시 그린다.
    fn toggle_pin(&mut self) {
        self.pinned = !self.pinned;
        set_topmost(self.window, self.pinned);
        super::win32::invalidate_child(self.window, ID_NOTE_PIN, true);
    }

    /// 현재 색상을 체크한 팝업 메뉴를 표시한다.
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

    /// 미저장 본문을 먼저 보존한 뒤 색상 모델과 화면 자원을 함께 교체한다.
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
        // 새 Brush를 대입하면 기존 Brush의 Drop이 자동 호출되어 GDI 누수를 막는다.
        self.note_brush = Brush::new(palette.body);
        markdown::set_background(self.editor, palette.body);
        super::win32::invalidate(self.editor, true);
        super::win32::invalidate(self.window, true);
        notify(self.manager, WM_REFRESH_NOTES, 0);
    }

    /// 선택 영역의 인라인 서식을 토글한다.
    fn toggle_inline(&mut self, kind: InlineFormat) {
        markdown::toggle_inline(self.editor, kind);
        focus(self.editor);
        self.mark_dirty();
    }

    /// 현재 줄의 글머리표를 토글한다.
    fn toggle_bullet(&mut self) {
        markdown::toggle_bullet(self.editor);
        focus(self.editor);
        self.mark_dirty();
    }

    /// 버튼/편집기 알림을 도메인 의미가 있는 메서드 호출로 변환한다.
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

/// 특정 메모 ID를 표시하는 Win32 창을 생성한다.
pub(crate) fn create_window(
    shared: Rc<SharedState>,
    manager: HWND,
    note_id: u64,
) -> Result<HWND, String> {
    let dpi = Dpi::for_window(manager);
    let context = CreateContext::new(NoteWindow::new(shared, manager, note_id, dpi));
    // SAFETY: lpParam은 호출 동안 살아 있는 CreateContext이며 문자열은 null 종료 UTF-16이다.
    // 성공 시 컨트롤러 소유권은 window_host의 Rc 강한 참조로 이전된다.
    let window = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            wide(NOTE_CLASS).as_ptr(),
            wide("메모").as_ptr(),
            WS_POPUP | WS_THICKFRAME | WS_MINIMIZEBOX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            dpi.scale(630),
            dpi.scale(760),
            manager,
            null_mut(),
            GetModuleHandleW(null()),
            context.as_ptr(),
        )
    };
    if window.is_null() {
        return Err("메모 창을 만들지 못했습니다.".into());
    }
    show_and_update(window);
    Ok(window)
}

/// Win32 콜백을 `NoteWindow` 객체의 메시지 처리로 전달하는 어댑터다.
///
/// # Safety
/// Windows가 이 창 클래스에 등록한 WNDPROC로만 호출해야 한다.
pub(crate) unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: 인자는 Windows가 현재 콜백에 제공한 원본 메시지 값이다.
    unsafe { dispatch::<NoteWindow>(window, message, wparam, lparam) }
}

/// 공통 창 호스트가 메모 창을 동일한 수명 규칙으로 관리하게 한다.
impl WindowController for NoteWindow {
    fn attach(&mut self, window: HWND) {
        self.window = window;
    }

    /// Win32 메시지를 편집, 저장, 그리기 같은 객체 동작으로 변환한다.
    fn handle_message(
        &mut self,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> MessageResult {
        match message {
            WM_CREATE => {
                disable_native_border(self.window);
                self.update_dpi(Dpi::for_window(self.window));
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
            WM_NCHITTEST => MessageResult::Handled(hit_test(
                self.window,
                lparam,
                self.dpi.scale(52),
                self.dpi.scale(7),
            )),
            WM_SIZE => {
                self.layout();
                MessageResult::Handled(0)
            }
            WM_PAINT => {
                note_view::paint(self.window, self.color, self.dpi);
                MessageResult::Handled(0)
            }
            WM_ERASEBKGND => MessageResult::Handled(1),
            WM_CTLCOLOREDIT => {
                let hdc = wparam as HDC;
                set_dc_colors(hdc, note_palette(self.color).body, TEXT);
                MessageResult::Handled(self.note_brush.handle() as LRESULT)
            }
            WM_DRAWITEM => {
                // SAFETY: WM_DRAWITEM에서 lparam은 Windows가 만든 DRAWITEMSTRUCT를 가리킨다.
                let Some(draw) = (unsafe { (lparam as *const DRAWITEMSTRUCT).as_ref() }) else {
                    return MessageResult::Handled(0);
                };
                let kind = match draw.CtlID as usize {
                    ID_NOTE_PIN => ButtonKind::Pin,
                    ID_NOTE_MENU | ID_NOTE_MORE | ID_NOTE_MIN | ID_NOTE_CLOSE => {
                        ButtonKind::Title
                    }
                    ID_SHOW_MANAGER => ButtonKind::Manager,
                    _ => ButtonKind::Toolbar,
                };
                note_view::draw_button(
                    draw,
                    self.color,
                    self.pinned,
                    self.font_button.handle(),
                    kind,
                    self.dpi,
                );
                MessageResult::Handled(1)
            }
            WM_COMMAND => {
                let id = wparam & 0xffff;
                let notification = ((wparam >> 16) & 0xffff) as u32;
                self.handle_command(id, notification);
                MessageResult::Handled(0)
            }
            WM_TIMER if wparam == SAVE_TIMER => {
                self.save();
                MessageResult::Handled(0)
            }
            WM_GETMINMAXINFO => {
                set_min_track_size(lparam, self.dpi.scale(380), self.dpi.scale(320));
                MessageResult::Handled(0)
            }
            WM_DPICHANGED => {
                self.update_dpi(Dpi::from_message(wparam));
                apply_dpi_suggested_rect(self.window, lparam);
                MessageResult::Handled(0)
            }
            WM_CLOSE => {
                self.save();
                MessageResult::Destroy
            }
            _ => MessageResult::Default,
        }
    }

    /// 창이 제거되면 관리 창의 `open_notes` 캐시에서도 해당 ID를 제거하도록 알린다.
    fn on_destroy(&mut self) {
        if is_window(self.manager) {
            notify(self.manager, WM_NOTE_CLOSED, self.note_id as usize);
        }
    }
}

/// 색상 메뉴 ID와 도메인 색상, 사용자 표시 문자열의 대응표다.
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

/// Windows 메뉴 명령 ID를 안전한 `NoteColor` 값으로 변환한다.
fn color_from_command(id: usize) -> Option<NoteColor> {
    color_choices()
        .into_iter()
        .find_map(|(choice, color, _)| (choice == id).then_some(color))
}
