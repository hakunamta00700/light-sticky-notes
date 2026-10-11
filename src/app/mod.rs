mod manager;
mod manager_view;
mod markdown;
mod note;
mod note_view;
mod theme;
mod win32;

use std::cell::RefCell;
use std::ptr::null_mut;
use std::rc::Rc;

use windows_sys::Win32::UI::WindowsAndMessaging::WM_APP;

use crate::store::{NoteStore, default_data_dir};
use win32::{load_library, register_class, run_message_loop, show_and_update, show_error};

pub(crate) const APP_NAME: &str = "Feather Notes";
pub(crate) const MANAGER_CLASS: &str = "FeatherNotesManager";
pub(crate) const NOTE_CLASS: &str = "FeatherNotesNote";
pub(crate) const WM_REFRESH_NOTES: u32 = WM_APP + 1;
pub(crate) const WM_NOTE_CLOSED: u32 = WM_APP + 2;

pub(crate) struct SharedState {
    pub(crate) store: RefCell<NoteStore>,
}

pub(crate) fn launch() {
    if let Err(error) = run() {
        show_error(null_mut(), &error);
    }
}

fn run() -> Result<(), String> {
    if !load_library("Msftedit.dll") {
        return Err("Windows Rich Edit 구성 요소를 불러오지 못했습니다.".into());
    }
    register_class(MANAGER_CLASS, Some(manager::window_proc))?;
    register_class(NOTE_CLASS, Some(note::window_proc))?;
    let shared = Rc::new(SharedState {
        store: RefCell::new(
            NoteStore::open(default_data_dir())
                .map_err(|error| format!("메모 저장소를 열지 못했습니다. {error}"))?,
        ),
    });
    let window = manager::create_window(shared)?;
    show_and_update(window);
    run_message_loop();
    Ok(())
}
