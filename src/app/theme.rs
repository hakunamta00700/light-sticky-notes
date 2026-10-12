//! 애플리케이션의 색상 디자인 토큰을 정의한다.
//!
//! 색상 값을 뷰 코드에 흩뿌리지 않고 이 모듈에 모아 두면 테마를 일관되게 변경할 수 있다.

#![forbid(unsafe_code)]

use windows_sys::Win32::Foundation::COLORREF;

use crate::store::NoteColor;

// 관리 창 전체에서 공유하는 색상 상수다. `const`는 실행 중 할당 없이 값에 직접 접근한다.
pub(crate) const MANAGER_BG: COLORREF = rgb(13, 14, 14);
pub(crate) const PANEL_BG: COLORREF = rgb(39, 40, 39);
pub(crate) const PANEL_HOVER: COLORREF = rgb(48, 49, 47);
pub(crate) const ACCENT: COLORREF = rgb(246, 214, 18);
pub(crate) const TEXT: COLORREF = rgb(247, 244, 234);
pub(crate) const MUTED: COLORREF = rgb(188, 184, 173);
pub(crate) const DARK_TEXT: COLORREF = rgb(20, 20, 16);

/// Win32가 사용하는 `COLORREF`의 BGR 비트 배열로 RGB 값을 변환한다.
pub(crate) const fn rgb(red: u32, green: u32, blue: u32) -> COLORREF {
    red | (green << 8) | (blue << 16)
}

/// 하나의 메모 색상에서 파생되는 모든 화면 색상을 묶은 값 객체다.
#[derive(Clone, Copy)]
pub(crate) struct NotePalette {
    pub(crate) top: COLORREF,
    pub(crate) body: COLORREF,
    pub(crate) toolbar: COLORREF,
    pub(crate) card: COLORREF,
    pub(crate) selected: COLORREF,
}

/// 도메인 색상 `NoteColor`를 실제 렌더링용 팔레트로 변환한다.
pub(crate) fn note_palette(color: NoteColor) -> NotePalette {
    match color {
        NoteColor::Yellow => NotePalette {
            top: ACCENT,
            body: rgb(128, 113, 0),
            toolbar: rgb(124, 109, 0),
            card: rgb(119, 105, 0),
            selected: rgb(134, 119, 0),
        },
        NoteColor::Green => NotePalette {
            top: rgb(143, 208, 91),
            body: rgb(57, 103, 64),
            toolbar: rgb(52, 96, 59),
            card: rgb(50, 91, 56),
            selected: rgb(64, 113, 70),
        },
        NoteColor::Blue => NotePalette {
            top: rgb(91, 181, 232),
            body: rgb(45, 92, 119),
            toolbar: rgb(40, 85, 111),
            card: rgb(39, 81, 105),
            selected: rgb(52, 103, 131),
        },
        NoteColor::Pink => NotePalette {
            top: rgb(236, 137, 157),
            body: rgb(125, 67, 79),
            toolbar: rgb(116, 61, 72),
            card: rgb(108, 57, 68),
            selected: rgb(136, 75, 88),
        },
        NoteColor::Purple => NotePalette {
            top: rgb(180, 151, 220),
            body: rgb(86, 72, 111),
            toolbar: rgb(79, 66, 103),
            card: rgb(73, 62, 95),
            selected: rgb(96, 81, 123),
        },
        NoteColor::Gray => NotePalette {
            top: rgb(186, 188, 183),
            body: rgb(82, 85, 82),
            toolbar: rgb(75, 78, 75),
            card: rgb(70, 72, 70),
            selected: rgb(94, 97, 93),
        },
    }
}
