#![forbid(unsafe_code)]

use std::time::{SystemTime, UNIX_EPOCH};

use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_RIGHT, DT_SINGLELINE, DT_TOP, DT_VCENTER,
    HFONT,
};
use windows_sys::Win32::UI::Controls::{DRAWITEMSTRUCT, ODS_SELECTED};

use crate::store::note_title;

use super::markdown;
use super::theme::{
    ACCENT, MANAGER_BG, MUTED, PANEL_BG, PANEL_HOVER, TEXT, note_palette, rgb,
};
use super::win32::{
    draw_text, fill_rect, fill_round_rect, stroke_round_rect, window_text, with_paint,
};
use super::{APP_NAME, SharedState};

pub(super) fn paint(
    window: HWND,
    visible_count: usize,
    font_title: HFONT,
    font_medium: HFONT,
) {
    with_paint(window, |hdc, rect| {
        fill_rect(hdc, &rect, MANAGER_BG);
        let icon = RECT {
            left: 28,
            top: 29,
            right: 50,
            bottom: 53,
        };
        fill_round_rect(hdc, &icon, ACCENT, 6);
        fill_round_rect(
            hdc,
            &RECT {
                left: 40,
                top: 41,
                right: 50,
                bottom: 53,
            },
            rgb(240, 112, 48),
            3,
        );
        draw_text(
            hdc,
            APP_NAME,
            RECT {
                left: 62,
                top: 22,
                right: 280,
                bottom: 58,
            },
            font_title,
            TEXT,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );
        draw_text(
            hdc,
            &format!("메모 {visible_count}개"),
            RECT {
                left: 24,
                top: 176,
                right: 180,
                bottom: 206,
            },
            font_medium,
            TEXT,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );
    });
}

pub(super) fn draw_note_card(
    draw: &DRAWITEMSTRUCT,
    shared: &SharedState,
    visible_ids: &[u64],
    font: HFONT,
    font_medium: HFONT,
) {
    if draw.itemID == u32::MAX {
        return;
    }
    let Some(id) = visible_ids.get(draw.itemID as usize).copied() else {
        return;
    };
    let store = shared.store.borrow();
    let Some(note) = store.note_by_id(id) else {
        return;
    };
    let palette = note_palette(note.color);
    let selected = draw.itemState & ODS_SELECTED != 0;
    let mut card = draw.rcItem;
    card.top += 5;
    card.bottom -= 7;
    card.right -= 4;
    let rendered_body = markdown::plain_text(&note.body);
    fill_round_rect(
            draw.hDC,
            &card,
            if selected {
                palette.selected
            } else {
                palette.card
            },
            14,
        );
    fill_round_rect(
            draw.hDC,
            &RECT {
                left: card.left,
                top: card.top,
                right: card.right,
                bottom: card.top + 6,
            },
            palette.top,
            6,
        );
    draw_text(
            draw.hDC,
            &note_title(&rendered_body),
            RECT {
                left: card.left + 24,
                top: card.top + 22,
                right: card.right - 96,
                bottom: card.top + 52,
            },
            font_medium,
            TEXT,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
        );
    draw_text(
            draw.hDC,
            &relative_time(note.id),
            RECT {
                left: card.right - 110,
                top: card.top + 22,
                right: card.right - 18,
                bottom: card.top + 52,
            },
            font,
            TEXT,
            DT_RIGHT | DT_VCENTER | DT_SINGLELINE,
        );
    draw_text(
            draw.hDC,
            &note_preview(&rendered_body),
            RECT {
                left: card.left + 24,
                top: card.top + 61,
                right: card.right - 20,
                bottom: card.bottom - 13,
            },
            font,
            MUTED,
            DT_LEFT | DT_TOP | DT_SINGLELINE | DT_END_ELLIPSIS,
        );
}

pub(super) fn draw_button(draw: &DRAWITEMSTRUCT, font: HFONT, header: bool) {
    let pressed = draw.itemState & ODS_SELECTED != 0;
    let color = if header {
        if pressed { PANEL_BG } else { MANAGER_BG }
    } else if pressed {
        PANEL_HOVER
    } else {
        PANEL_BG
    };
    if header {
        fill_rect(draw.hDC, &draw.rcItem, color);
    } else {
        fill_round_rect(draw.hDC, &draw.rcItem, color, 10);
        stroke_round_rect(draw.hDC, &draw.rcItem, rgb(82, 83, 80), 10);
    }
    let label = window_text(draw.hwndItem);
    draw_text(
        draw.hDC,
        &label,
        draw.rcItem,
        font,
        TEXT,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
}

fn note_preview(body: &str) -> String {
    let joined = body
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .skip(1)
        .collect::<Vec<_>>()
        .join("  ");
    let source = if joined.is_empty() { "내용 없음" } else { &joined };
    let mut chars = source.chars();
    let preview: String = chars.by_ref().take(52).collect();
    if chars.next().is_some() {
        format!("{preview}…")
    } else {
        preview
    }
}

fn relative_time(id: u64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let minutes = now.saturating_sub(id) / 60_000;
    match minutes {
        0 => "지금".into(),
        1..=59 => format!("{minutes}분 전"),
        60..=1_439 => format!("{}시간 전", minutes / 60),
        _ => format!("{}일 전", minutes / 1_440),
    }
}
