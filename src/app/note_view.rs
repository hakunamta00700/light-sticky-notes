#![forbid(unsafe_code)]

use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    DT_CENTER, DT_SINGLELINE, DT_VCENTER, HFONT,
};
use windows_sys::Win32::UI::Controls::{DRAWITEMSTRUCT, ODS_SELECTED};

use crate::store::NoteColor;

use super::theme::{DARK_TEXT, TEXT, note_palette, rgb};
use super::win32::{
    draw_text, fill_rect, fill_round_rect, stroke_round_rect, window_text, with_paint,
};

#[derive(Clone, Copy)]
pub(super) enum ButtonKind {
    Title,
    Pin,
    Manager,
    Toolbar,
}

pub(super) fn paint(window: HWND, color: NoteColor) {
    let palette = note_palette(color);
    with_paint(window, |hdc, rect| {
        fill_rect(
            hdc,
            &RECT {
                left: 0,
                top: 0,
                right: rect.right,
                bottom: 52,
            },
            palette.top,
        );
        fill_rect(
            hdc,
            &RECT {
                left: 0,
                top: 52,
                right: rect.right,
                bottom: rect.bottom,
            },
            palette.body,
        );
        fill_rect(
            hdc,
            &RECT {
                left: 0,
                top: rect.bottom - 64,
                right: rect.right,
                bottom: rect.bottom,
            },
            palette.toolbar,
        );
    });
}

pub(super) fn draw_button(
    draw: &DRAWITEMSTRUCT,
    color: NoteColor,
    pinned: bool,
    font: HFONT,
    kind: ButtonKind,
) {
    let pressed = draw.itemState & ODS_SELECTED != 0;
    let title_button = matches!(kind, ButtonKind::Title | ButtonKind::Pin);
    let manager_button = matches!(kind, ButtonKind::Manager);
    let palette = note_palette(color);
    let background = if title_button {
        if pressed {
            palette.toolbar
        } else {
            palette.top
        }
    } else if manager_button {
        if pressed {
            palette.body
        } else {
            palette.toolbar
        }
    } else if pressed {
        palette.body
    } else {
        palette.toolbar
    };
    if manager_button {
        fill_round_rect(draw.hDC, &draw.rcItem, background, 10);
        stroke_round_rect(draw.hDC, &draw.rcItem, TEXT, 10);
    } else {
        fill_rect(draw.hDC, &draw.rcItem, background)
    }
    let label = window_text(draw.hwndItem);
    let text_color = if matches!(kind, ButtonKind::Pin) && pinned {
        rgb(167, 61, 20)
    } else if title_button {
        DARK_TEXT
    } else {
        TEXT
    };
    draw_text(
        draw.hDC,
        &label,
        draw.rcItem,
        font,
        text_color,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
}
