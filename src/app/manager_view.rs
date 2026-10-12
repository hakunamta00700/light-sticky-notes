//! 관리 창을 그리는 표현 계층이다.
//!
//! 상태 변경 없이 전달받은 값만 사용하므로 렌더링과 창 동작을 분리할 수 있다. Win32의 위험한
//! 그리기 호출은 `win32`의 안전한 래퍼 뒤에 숨겨져 있어 이 모듈은 `unsafe`를 금지한다.

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
    Dpi, draw_text, fill_rect, fill_round_rect, stroke_round_rect, window_text, with_paint,
};
use super::{APP_NAME, SharedState};

/// 관리 창의 고정 배경, 앱 아이콘, 메모 개수를 그린다.
pub(super) fn paint(
    window: HWND,
    visible_count: usize,
    font_title: HFONT,
    font_medium: HFONT,
    dpi: Dpi,
) {
    // `with_paint`가 BeginPaint/EndPaint 쌍을 책임지므로 호출자는 HDC 수명을 관리하지 않는다.
    with_paint(window, |hdc, rect| {
        let scale = |value| dpi.scale(value);
        fill_rect(hdc, &rect, MANAGER_BG);
        let icon = RECT {
            left: scale(28),
            top: scale(29),
            right: scale(50),
            bottom: scale(53),
        };
        fill_round_rect(hdc, &icon, ACCENT, scale(6));
        fill_round_rect(
            hdc,
            &RECT {
                left: scale(40),
                top: scale(41),
                right: scale(50),
                bottom: scale(53),
            },
            rgb(240, 112, 48),
            scale(3),
        );
        draw_text(
            hdc,
            APP_NAME,
            RECT {
                left: scale(62),
                top: scale(22),
                right: scale(280),
                bottom: scale(58),
            },
            font_title,
            TEXT,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );
        draw_text(
            hdc,
            &format!("메모 {visible_count}개"),
            RECT {
                left: scale(24),
                top: scale(176),
                right: scale(180),
                bottom: scale(206),
            },
            font_medium,
            TEXT,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );
    });
}

/// owner-draw 목록 항목 하나를 포스트잇 카드로 그린다.
pub(super) fn draw_note_card(
    draw: &DRAWITEMSTRUCT,
    shared: &SharedState,
    visible_ids: &[u64],
    font: HFONT,
    font_medium: HFONT,
    dpi: Dpi,
) {
    if draw.itemID == u32::MAX {
        return;
    }
    let Some(id) = visible_ids.get(draw.itemID as usize).copied() else {
        return;
    };
    // 화면을 그리는 동안에는 저장소를 읽기 전용으로만 빌린다.
    let store = shared.store.borrow();
    let Some(note) = store.note_by_id(id) else {
        return;
    };
    let palette = note_palette(note.color);
    let selected = draw.itemState & ODS_SELECTED != 0;
    let scale = |value| dpi.scale(value);
    let mut card = draw.rcItem;
    card.top += scale(5);
    card.bottom -= scale(7);
    card.right -= scale(4);
    let rendered_body = markdown::plain_text(&note.body);
    fill_round_rect(
        draw.hDC,
        &card,
        if selected {
            palette.selected
        } else {
            palette.card
        },
        scale(14),
    );
    fill_round_rect(
        draw.hDC,
        &RECT {
            left: card.left,
            top: card.top,
            right: card.right,
            bottom: card.top + scale(6),
        },
        palette.top,
        scale(6),
    );
    draw_text(
            draw.hDC,
            &note_title(&rendered_body),
            RECT {
                left: card.left + scale(24),
                top: card.top + scale(22),
                right: card.right - scale(96),
                bottom: card.top + scale(52),
            },
            font_medium,
            TEXT,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
        );
    draw_text(
            draw.hDC,
            &relative_time(note.id),
            RECT {
                left: card.right - scale(110),
                top: card.top + scale(22),
                right: card.right - scale(18),
                bottom: card.top + scale(52),
            },
            font,
            TEXT,
            DT_RIGHT | DT_VCENTER | DT_SINGLELINE,
        );
    draw_text(
            draw.hDC,
            &note_preview(&rendered_body),
            RECT {
                left: card.left + scale(24),
                top: card.top + scale(61),
                right: card.right - scale(20),
                bottom: card.bottom - scale(13),
            },
            font,
            MUTED,
            DT_LEFT | DT_TOP | DT_SINGLELINE | DT_END_ELLIPSIS,
        );
}

/// 관리 창의 일반 버튼과 제목 표시줄 버튼을 공통 방식으로 그린다.
pub(super) fn draw_button(draw: &DRAWITEMSTRUCT, font: HFONT, header: bool, dpi: Dpi) {
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
        fill_round_rect(draw.hDC, &draw.rcItem, color, dpi.scale(10));
        stroke_round_rect(draw.hDC, &draw.rcItem, rgb(82, 83, 80), dpi.scale(10));
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

/// 본문의 두 번째 줄 이후를 카드 미리보기 문자열로 압축한다.
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

/// 타임스탬프 기반 ID를 사람이 이해하기 쉬운 상대 시간으로 바꾼다.
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
