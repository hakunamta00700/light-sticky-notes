#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(unused_unsafe)]

mod app;
mod store;

fn main() {
    app::launch();
}
