//! 개별 메모 창의 배경과 owner-draw 버튼을 그린다.
//!
//! 이 모듈은 전달받은 색상과 상태를 표현할 뿐 저장소나 창 상태를 변경하지 않는다.

#![forbid(unsafe_code)]

use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    DT_CENTER, DT_SINGLELINE, DT_VCENTER, HFONT,
};
use windows_sys::Win32::UI::Controls::{DRAWITEMSTRUCT, ODS_SELECTED};

use crate::store::NoteColor;

use super::theme::{DARK_TEXT, TEXT, note_palette, rgb};
use super::win32::{
    Dpi, draw_text, fill_rect, fill_round_rect, stroke_round_rect, window_text, with_paint,
};

/// 같은 버튼 그리기 함수가 위치별 표현 차이를 알 수 있게 하는 의미 있는 열거형이다.
#[derive(Clone, Copy)]
pub(super) enum ButtonKind {
    Title,
    Pin,
    Manager,
    Toolbar,
}

/// 현재 메모 색상으로 제목 영역, 본문, 도구 모음 배경을 칠한다.
pub(super) fn paint(window: HWND, color: NoteColor, dpi: Dpi) {
    let palette = note_palette(color);
    with_paint(window, |hdc, rect| {
        let title_height = dpi.scale(52);
        let toolbar_height = dpi.scale(64);
        fill_rect(
            hdc,
            &RECT {
                left: 0,
                top: 0,
                right: rect.right,
                bottom: title_height,
            },
            palette.top,
        );
        fill_rect(
            hdc,
            &RECT {
                left: 0,
                top: title_height,
                right: rect.right,
                bottom: rect.bottom,
            },
            palette.body,
        );
        fill_rect(
            hdc,
            &RECT {
                left: 0,
                top: rect.bottom - toolbar_height,
                right: rect.right,
                bottom: rect.bottom,
            },
            palette.toolbar,
        );
    });
}

/// 버튼 종류, 눌림 상태, 핀 상태에 맞는 배경과 글자색을 선택한다.
pub(super) fn draw_button(
    draw: &DRAWITEMSTRUCT,
    color: NoteColor,
    pinned: bool,
    font: HFONT,
    kind: ButtonKind,
    dpi: Dpi,
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
        fill_round_rect(draw.hDC, &draw.rcItem, background, dpi.scale(10));
        stroke_round_rect(draw.hDC, &draw.rcItem, TEXT, dpi.scale(10));
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
