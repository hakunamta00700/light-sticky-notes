use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, DefWindowProcW, DestroyWindow, GWLP_USERDATA, GetWindowLongPtrW,
    SetWindowLongPtrW, WM_NCCREATE, WM_NCDESTROY,
};

pub(crate) enum MessageResult {
    Handled(LRESULT),
    Default,
    Destroy,
}

pub(crate) trait WindowController {
    fn attach(&mut self, window: HWND);

    fn handle_message(
        &mut self,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> MessageResult;

    fn on_destroy(&mut self) {}
}

pub(crate) struct CreateContext<T> {
    controller: Rc<RefCell<T>>,
}

impl<T> CreateContext<T> {
    pub(crate) fn new(controller: T) -> Self {
        Self {
            controller: Rc::new(RefCell::new(controller)),
        }
    }

    pub(crate) fn as_ptr(&self) -> *mut c_void {
        (self as *const Self).cast_mut().cast()
    }
}

pub(crate) unsafe fn dispatch<T: WindowController>(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = lparam as *const CREATESTRUCTW;
        if create.is_null() {
            return 0;
        }
        let context = unsafe { ((*create).lpCreateParams as *const CreateContext<T>).as_ref() };
        let Some(context) = context else {
            return 0;
        };
        if let Ok(mut controller) = context.controller.try_borrow_mut() {
            controller.attach(window);
        } else {
            return 0;
        }
        let owned = Rc::into_raw(Rc::clone(&context.controller));
        unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, owned as isize) };
    }

    let pointer = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) }
        as *const RefCell<T>;
    if pointer.is_null() {
        return unsafe { DefWindowProcW(window, message, wparam, lparam) };
    }

    if message == WM_NCDESTROY {
        unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, 0) };
        let controller = unsafe { Rc::from_raw(pointer) };
        if let Ok(mut controller) = controller.try_borrow_mut() {
            controller.on_destroy();
        }
        return unsafe { DefWindowProcW(window, message, wparam, lparam) };
    }

    let result = {
        let controller = unsafe { &*pointer };
        match controller.try_borrow_mut() {
            Ok(mut controller) => controller.handle_message(message, wparam, lparam),
            Err(_) => MessageResult::Default,
        }
    };

    match result {
        MessageResult::Handled(result) => result,
        MessageResult::Default => unsafe { DefWindowProcW(window, message, wparam, lparam) },
        MessageResult::Destroy => {
            unsafe { DestroyWindow(window) };
            0
        }
    }
}
