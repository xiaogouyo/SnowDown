#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if snowdown_lib::native_browser_host() {
        return;
    }
    snowdown_lib::run();
}
