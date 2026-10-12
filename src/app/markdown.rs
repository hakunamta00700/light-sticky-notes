//! Markdown 원문과 Windows Rich Edit의 시각적 서식을 서로 변환한다.
//!
//! 디스크에는 이식 가능한 Markdown 문자열을 저장하지만 화면에서는 굵게, 기울임 등의 결과를
//! 직접 보여 준다. 순수 파서/직렬화 로직과 Win32 메시지 경계를 같은 모듈 안에서 구분한다.

use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, WPARAM};
use windows_sys::Win32::UI::Controls::{EM_GETSEL, EM_REPLACESEL, EM_SETLIMITTEXT, EM_SETSEL};
use windows_sys::Win32::UI::WindowsAndMessaging::{SendMessageW, SetWindowTextW, WM_USER};

use super::win32::{wide, window_text};

// Rich Edit이 이해하는 메시지, 마스크, 효과 비트다. Win32 상수와 Rust 모델 사이의 경계다.
pub(crate) const RICH_EDIT_CLASS: &str = "RICHEDIT50W";

const EM_GETCHARFORMAT: u32 = WM_USER + 58;
const EM_GETEVENTMASK: u32 = WM_USER + 59;
const EM_SETBKGNDCOLOR: u32 = WM_USER + 67;
const EM_SETCHARFORMAT: u32 = WM_USER + 68;
const EM_SETEVENTMASK: u32 = WM_USER + 69;
const ENM_CHANGE: isize = 0x0000_0001;
const SCF_SELECTION: usize = 0x0001;
const SCF_ALL: usize = 0x0004;
const CFM_BOLD: u32 = 0x0000_0001;
const CFM_ITALIC: u32 = 0x0000_0002;
const CFM_UNDERLINE: u32 = 0x0000_0004;
const CFM_STRIKEOUT: u32 = 0x0000_0008;
const CFM_COLOR: u32 = 0x4000_0000;
const CFE_BOLD: u32 = 0x0000_0001;
const CFE_ITALIC: u32 = 0x0000_0002;
const CFE_UNDERLINE: u32 = 0x0000_0004;
const CFE_STRIKEOUT: u32 = 0x0000_0008;
const FORMAT_MASK: u32 = CFM_BOLD | CFM_ITALIC | CFM_UNDERLINE | CFM_STRIKEOUT;

/// 툴바에서 토글할 수 있는 인라인 서식 종류다.
#[derive(Clone, Copy)]
pub(crate) enum InlineFormat {
    Bold,
    Italic,
    Underline,
    Strike,
}

impl InlineFormat {
    /// Rich Edit이 요구하는 `(마스크, 효과)` 비트 조합으로 변환한다.
    fn flags(self) -> (u32, u32) {
        match self {
            Self::Bold => (CFM_BOLD, CFE_BOLD),
            Self::Italic => (CFM_ITALIC, CFE_ITALIC),
            Self::Underline => (CFM_UNDERLINE, CFE_UNDERLINE),
            Self::Strike => (CFM_STRIKEOUT, CFE_STRIKEOUT),
        }
    }
}

/// Win32 `CHARFORMATW` 앞부분과 메모리 배치가 호환되는 FFI 구조체다.
/// `repr(C)`는 Rust가 필드 순서를 임의로 바꾸지 못하게 한다.
#[repr(C)]
struct RichCharFormat {
    cb_size: u32,
    mask: u32,
    effects: u32,
    height: i32,
    offset: i32,
    text_color: COLORREF,
    charset: u8,
    pitch_and_family: u8,
    face_name: [u16; 32],
}

impl RichCharFormat {
    /// FFI 구조체 크기와 변경할 비트를 초기화한다.
    fn new(mask: u32, effects: u32) -> Self {
        Self {
            cb_size: std::mem::size_of::<Self>() as u32,
            mask,
            effects,
            height: 0,
            offset: 0,
            text_color: 0,
            charset: 0,
            pitch_and_family: 0,
            face_name: [0; 32],
        }
    }

    /// 빌더 패턴처럼 값을 소비하고 수정된 값을 다시 반환한다.
    fn with_text_color(mut self, color: COLORREF) -> Self {
        self.text_color = color;
        self
    }
}

/// 플랫폼과 무관하게 표현한 문자 서식 값 객체다.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct TextStyle {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
}

impl TextStyle {
    /// Rust의 bool 필드를 Win32 효과 비트 집합으로 압축한다.
    fn effects(self) -> u32 {
        (if self.bold { CFE_BOLD } else { 0 })
            | (if self.italic { CFE_ITALIC } else { 0 })
            | (if self.underline { CFE_UNDERLINE } else { 0 })
            | (if self.strike { CFE_STRIKEOUT } else { 0 })
    }

    /// Win32 효과 비트 집합을 안전한 Rust 값으로 복원한다.
    fn from_effects(effects: u32) -> Self {
        Self {
            bold: effects & CFE_BOLD != 0,
            italic: effects & CFE_ITALIC != 0,
            underline: effects & CFE_UNDERLINE != 0,
            strike: effects & CFE_STRIKEOUT != 0,
        }
    }
}

/// UTF-16 위치 범위에 동일한 서식이 적용된 구간이다.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StyleRun {
    start: u32,
    end: u32,
    style: TextStyle,
}

/// Markdown을 파싱한 화면 문자열과 서식 구간의 묶음이다.
struct RenderedMarkdown {
    text: String,
    runs: Vec<StyleRun>,
}

/// Rich Edit에서 읽어 낸 문자 하나와 해당 문자의 서식이다.
#[derive(Clone, Copy)]
struct StyledChar {
    value: char,
    style: TextStyle,
}

/// 새 편집기의 용량, 변경 알림, 기본 색상을 설정한다.
pub(crate) fn configure(editor: HWND, background: COLORREF, text_color: COLORREF) {
    // SAFETY: editor는 이 앱이 생성한 Rich Edit HWND이며, default_color 포인터는 동기 호출이
    // 끝날 때까지 유효하다.
    unsafe {
        SendMessageW(editor, EM_SETLIMITTEXT, 2_000_000, 0);
        set_background(editor, background);
        let event_mask = SendMessageW(editor, EM_GETEVENTMASK, 0, 0);
        SendMessageW(editor, EM_SETEVENTMASK, 0, event_mask | ENM_CHANGE);
        let default_color = RichCharFormat::new(CFM_COLOR, 0).with_text_color(text_color);
        SendMessageW(
            editor,
            EM_SETCHARFORMAT,
            SCF_ALL,
            (&default_color as *const RichCharFormat) as LPARAM,
        );
    }
}

/// Rich Edit의 배경색을 변경한다.
pub(crate) fn set_background(editor: HWND, color: COLORREF) {
    // SAFETY: EM_SETBKGNDCOLOR는 포인터가 아닌 색상 값을 LPARAM으로 받는다.
    unsafe { SendMessageW(editor, EM_SETBKGNDCOLOR, 0, color as LPARAM) };
}

/// Markdown을 파싱하여 마커 없는 텍스트와 실제 글자 서식으로 편집기에 적재한다.
pub(crate) fn load(editor: HWND, source: &str, text_color: COLORREF) {
    let rendered = render(source);
    // SAFETY: 모든 UTF-16 문자열과 RichCharFormat 값은 동기 SendMessageW 호출 동안 살아 있다.
    unsafe {
        SetWindowTextW(editor, wide(&rendered.text).as_ptr());
        SendMessageW(editor, EM_SETSEL, 0, -1);
        let reset = RichCharFormat::new(FORMAT_MASK | CFM_COLOR, 0).with_text_color(text_color);
        SendMessageW(
            editor,
            EM_SETCHARFORMAT,
            SCF_SELECTION,
            (&reset as *const RichCharFormat) as LPARAM,
        );
        for run in rendered.runs {
            SendMessageW(editor, EM_SETSEL, run.start as WPARAM, run.end as LPARAM);
            let format = RichCharFormat::new(FORMAT_MASK, run.style.effects());
            SendMessageW(
                editor,
                EM_SETCHARFORMAT,
                SCF_SELECTION,
                (&format as *const RichCharFormat) as LPARAM,
            );
        }
        SendMessageW(editor, EM_SETSEL, 0, 0);
    }
}

/// 현재 선택 영역의 지정된 인라인 서식을 켜거나 끈다.
pub(crate) fn toggle_inline(editor: HWND, kind: InlineFormat) {
    let (mask, effect) = kind.flags();
    let mut current = RichCharFormat::new(mask, 0);
    // SAFETY: current는 호출 동안 쓰기 가능한 RichCharFormat이며 메시지는 동기적으로 완료된다.
    unsafe {
        SendMessageW(
            editor,
            EM_GETCHARFORMAT,
            SCF_SELECTION,
            (&mut current as *mut RichCharFormat) as LPARAM,
        );
    }
    let enabled = current.mask & mask != 0 && current.effects & effect != 0;
    let updated = RichCharFormat::new(mask, if enabled { 0 } else { effect });
    // SAFETY: updated는 동기 호출이 읽는 동안 스택에 살아 있다.
    unsafe {
        SendMessageW(
            editor,
            EM_SETCHARFORMAT,
            SCF_SELECTION,
            (&updated as *const RichCharFormat) as LPARAM,
        );
    }
}

/// 현재 줄 앞의 글머리표 문자와 공백을 추가하거나 제거한다.
pub(crate) fn toggle_bullet(editor: HWND) {
    let mut start = 0u32;
    let mut end = 0u32;
    // SAFETY: start와 end는 메시지가 동기적으로 채우는 유효한 u32 포인터다.
    unsafe {
        SendMessageW(
            editor,
            EM_GETSEL,
            (&mut start as *mut u32) as WPARAM,
            (&mut end as *mut u32) as LPARAM,
        )
    };
    let text = window_text(editor);
    // Rich Edit의 선택 위치는 UTF-8 바이트가 아니라 UTF-16 코드 단위이므로 같은 표현으로 바꾼다.
    let encoded: Vec<u16> = text.encode_utf16().collect();
    let cursor = (start as usize).min(encoded.len());
    let line_start = encoded[..cursor]
        .iter()
        .rposition(|value| *value == b'\n' as u16)
        .map(|index| index + 1)
        .unwrap_or(0);
    let has_bullet = encoded.get(line_start) == Some(&('•' as u16))
        && encoded.get(line_start + 1) == Some(&(b' ' as u16));
    let (replace_end, replacement, selection_start, selection_end) = if has_bullet {
        (
            line_start + 2,
            "",
            start.saturating_sub(2),
            end.saturating_sub(2),
        )
    } else {
        (line_start, "• ", start + 2, end + 2)
    };
    // SAFETY: replacement의 UTF-16 버퍼는 각 동기 메시지가 반환될 때까지 유효하다.
    unsafe {
        SendMessageW(editor, EM_SETSEL, line_start, replace_end as LPARAM);
        SendMessageW(
            editor,
            EM_REPLACESEL,
            1,
            wide(replacement).as_ptr() as LPARAM,
        );
        SendMessageW(
            editor,
            EM_SETSEL,
            selection_start as WPARAM,
            selection_end as LPARAM,
        );
    }
}

/// 편집기의 문자별 시각적 서식을 읽어 Markdown 문자열로 직렬화한다.
pub(crate) fn to_markdown(editor: HWND) -> String {
    let text = window_text(editor);
    let mut selection_start = 0u32;
    let mut selection_end = 0u32;
    // SAFETY: 선택 범위를 받을 두 u32 변수가 동기 호출 동안 유효하다.
    unsafe {
        SendMessageW(
            editor,
            EM_GETSEL,
            (&mut selection_start as *mut u32) as WPARAM,
            (&mut selection_end as *mut u32) as LPARAM,
        );
    }

    // 문자열의 각 Unicode scalar value와 Rich Edit 서식을 결합한 중간 표현을 만든다.
    let mut styled = Vec::with_capacity(text.chars().count());
    let mut position = 0u32;
    for value in text.chars() {
        let width = value.len_utf16() as u32;
        if value == '\r' {
            continue;
        }
        let style = if value == '\n' {
            TextStyle::default()
        } else {
            editor_style_at(editor, position, position + width)
        };
        styled.push(StyledChar { value, style });
        position += width;
    }
    // SAFETY: 저장 전에 기억한 선택 위치를 같은 유효한 편집기에 복원한다.
    unsafe {
        SendMessageW(
            editor,
            EM_SETSEL,
            selection_start as WPARAM,
            selection_end as LPARAM,
        );
    }
    styled_chars_to_markdown(&styled)
}

/// 목록 미리보기에서 사용할 마커 없는 일반 텍스트를 만든다.
pub(crate) fn plain_text(source: &str) -> String {
    render(source).text
}

/// 지원하는 Markdown 문법을 한 번 순회하며 화면 텍스트와 서식 구간으로 파싱한다.
fn render(source: &str) -> RenderedMarkdown {
    let mut text = String::new();
    let mut runs: Vec<StyleRun> = Vec::new();
    let mut style = TextStyle::default();
    let mut index = 0usize;
    let mut position = 0u32;
    let mut line_start = true;

    // index는 UTF-8 바이트 위치이고 position은 Rich Edit용 UTF-16 위치라는 차이가 중요하다.
    while index < source.len() {
        let rest = &source[index..];
        if rest.starts_with("\r\n") || rest.starts_with('\n') {
            append_char(&mut text, &mut runs, &mut position, '\n', style);
            index += if rest.starts_with("\r\n") { 2 } else { 1 };
            line_start = true;
            continue;
        }
        if line_start && (rest.starts_with("- ") || rest.starts_with("* ")) {
            append_char(
                &mut text,
                &mut runs,
                &mut position,
                '•',
                TextStyle::default(),
            );
            append_char(
                &mut text,
                &mut runs,
                &mut position,
                ' ',
                TextStyle::default(),
            );
            index += 2;
            line_start = false;
            continue;
        }
        if let Some(escaped) = rest.strip_prefix('\\')
            && let Some(value) = escaped.chars().next()
        {
            append_char(&mut text, &mut runs, &mut position, value, style);
            index += 1 + value.len_utf8();
            line_start = false;
            continue;
        }
        if rest.starts_with("**") && (style.bold || rest[2..].contains("**")) {
            style.bold = !style.bold;
            index += 2;
            continue;
        }
        if rest.starts_with("~~") && (style.strike || rest[2..].contains("~~")) {
            style.strike = !style.strike;
            index += 2;
            continue;
        }
        if rest.starts_with("<u>") && (style.underline || rest[3..].contains("</u>")) {
            style.underline = true;
            index += 3;
            continue;
        }
        if rest.starts_with("</u>") && style.underline {
            style.underline = false;
            index += 4;
            continue;
        }
        if rest.starts_with('_') && (style.italic || rest[1..].contains('_')) {
            style.italic = !style.italic;
            index += 1;
            continue;
        }
        if rest.starts_with('*') && (style.italic || rest[1..].contains('*')) {
            style.italic = !style.italic;
            index += 1;
            continue;
        }
        let value = rest.chars().next().unwrap_or_default();
        append_char(&mut text, &mut runs, &mut position, value, style);
        index += value.len_utf8();
        line_start = false;
    }
    RenderedMarkdown { text, runs }
}

/// 문자 하나를 결과에 추가하고, 인접한 동일 서식은 하나의 `StyleRun`으로 병합한다.
fn append_char(
    text: &mut String,
    runs: &mut Vec<StyleRun>,
    position: &mut u32,
    value: char,
    style: TextStyle,
) {
    let start = *position;
    text.push(value);
    let end = start + value.len_utf16() as u32;
    *position = end;
    if style == TextStyle::default() {
        return;
    }
    if let Some(last) = runs.last_mut()
        && last.end == start
        && last.style == style
    {
        last.end = end;
        return;
    }
    runs.push(StyleRun { start, end, style });
}

/// 지정한 UTF-16 범위의 Rich Edit 서식을 읽는다.
fn editor_style_at(editor: HWND, start: u32, end: u32) -> TextStyle {
    // SAFETY: editor는 유효한 Rich Edit이며 두 메시지는 포인터를 호출 이후 보관하지 않는다.
    unsafe { SendMessageW(editor, EM_SETSEL, start as WPARAM, end as LPARAM) };
    let mut format = RichCharFormat::new(FORMAT_MASK, 0);
    // SAFETY: format은 동기 호출 동안 유효한 쓰기 가능한 FFI 구조체다.
    unsafe {
        SendMessageW(
            editor,
            EM_GETCHARFORMAT,
            SCF_SELECTION,
            (&mut format as *mut RichCharFormat) as LPARAM,
        );
    }
    TextStyle::from_effects(format.effects)
}

/// 문자별 서식 중간 표현을 중첩 순서가 올바른 Markdown으로 변환한다.
fn styled_chars_to_markdown(chars: &[StyledChar]) -> String {
    let mut output = String::new();
    let mut active = TextStyle::default();
    let mut line_start = true;
    let mut index = 0usize;

    while index < chars.len() {
        let current = chars[index];
        if current.value == '\n' {
            close_style(&mut output, active);
            active = TextStyle::default();
            output.push('\n');
            line_start = true;
            index += 1;
            continue;
        }
        if line_start
            && current.value == '•'
            && chars.get(index + 1).map(|next| next.value) == Some(' ')
        {
            close_style(&mut output, active);
            active = TextStyle::default();
            output.push_str("- ");
            line_start = false;
            index += 2;
            continue;
        }
        if active != current.style {
            close_style(&mut output, active);
            open_style(&mut output, current.style);
            active = current.style;
        }
        if line_start
            && current.value == '-'
            && chars.get(index + 1).map(|next| next.value) == Some(' ')
        {
            output.push_str("\\-");
        } else {
            push_escaped_char(&mut output, current.value);
        }
        line_start = false;
        index += 1;
    }
    close_style(&mut output, active);
    output
}

/// 새 서식 구간을 시작하는 Markdown 마커를 바깥쪽부터 기록한다.
fn open_style(output: &mut String, style: TextStyle) {
    if style.bold {
        output.push_str("**");
    }
    if style.italic {
        output.push('_');
    }
    if style.underline {
        output.push_str("<u>");
    }
    if style.strike {
        output.push_str("~~");
    }
}

/// 열린 마커의 역순으로 닫아 올바른 중첩을 유지한다.
fn close_style(output: &mut String, style: TextStyle) {
    if style.strike {
        output.push_str("~~");
    }
    if style.underline {
        output.push_str("</u>");
    }
    if style.italic {
        output.push('_');
    }
    if style.bold {
        output.push_str("**");
    }
}

/// Markdown 문법으로 오해될 수 있는 일반 문자를 백슬래시로 이스케이프한다.
fn push_escaped_char(output: &mut String, value: char) {
    if matches!(value, '\\' | '*' | '_' | '~' | '<') {
        output.push('\\');
    }
    output.push(value);
}

#[cfg(test)]
mod tests {
    // 순수 함수인 파서와 직렬화기는 Windows 창 없이 빠르게 검증할 수 있다.
    use super::*;

    #[test]
    fn renders_supported_markdown_without_showing_markers() {
        let rendered = render("**굵게**와 _기울임_, <u>밑줄</u>, ~~취소~~\n- 항목");
        assert_eq!(rendered.text, "굵게와 기울임, 밑줄, 취소\n• 항목");
        assert!(rendered.runs.iter().any(|run| run.style.bold));
        assert!(rendered.runs.iter().any(|run| run.style.italic));
        assert!(rendered.runs.iter().any(|run| run.style.underline));
        assert!(rendered.runs.iter().any(|run| run.style.strike));
    }

    #[test]
    fn serializes_visual_styles_back_to_markdown() {
        let bold = TextStyle {
            bold: true,
            ..TextStyle::default()
        };
        let plain = TextStyle::default();
        let chars = [
            StyledChar {
                value: '굵',
                style: bold,
            },
            StyledChar {
                value: '게',
                style: bold,
            },
            StyledChar {
                value: '\n',
                style: plain,
            },
            StyledChar {
                value: '•',
                style: plain,
            },
            StyledChar {
                value: ' ',
                style: plain,
            },
            StyledChar {
                value: '항',
                style: plain,
            },
            StyledChar {
                value: '목',
                style: plain,
            },
        ];
        assert_eq!(styled_chars_to_markdown(&chars), "**굵게**\n- 항목");
    }

    #[test]
    fn preserves_escaped_markdown_as_literal_text() {
        let rendered = render(r"파일\_이름과 \*기호\*");
        assert_eq!(rendered.text, "파일_이름과 *기호*");
        assert!(rendered.runs.is_empty());
    }
}
