#![forbid(unsafe_code)]

use windows_sys::Win32::Foundation::COLORREF;

use crate::store::NoteColor;

pub(crate) const MANAGER_BG: COLORREF = rgb(13, 14, 14);
pub(crate) const PANEL_BG: COLORREF = rgb(39, 40, 39);
pub(crate) const PANEL_HOVER: COLORREF = rgb(48, 49, 47);
pub(crate) const ACCENT: COLORREF = rgb(246, 214, 18);
pub(crate) const TEXT: COLORREF = rgb(247, 244, 234);
pub(crate) const MUTED: COLORREF = rgb(188, 184, 173);
pub(crate) const DARK_TEXT: COLORREF = rgb(20, 20, 16);

pub(crate) const fn rgb(red: u32, green: u32, blue: u32) -> COLORREF {
    red | (green << 8) | (blue << 16)
}

#[derive(Clone, Copy)]
pub(crate) struct NotePalette {
    pub(crate) top: COLORREF,
    pub(crate) body: COLORREF,
    pub(crate) toolbar: COLORREF,
    pub(crate) card: COLORREF,
    pub(crate) selected: COLORREF,
}

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
