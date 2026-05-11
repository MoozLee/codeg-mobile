// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn main() {
    // When called as a git credential helper, handle it immediately and exit.
    // This avoids starting the full Tauri GUI runtime.
    if std::env::args().any(|a| a == "--credential-helper") {
        codeg_lib::git_credential::run_credential_helper();
        return;
    }

    codeg_lib::run()
}

// Desktop/server only — on iOS/Android the Tauri mobile shell provides its
// own entry via `codeg_lib::mobile_main()` declared in `lib.rs`.
#[cfg(any(target_os = "ios", target_os = "android"))]
fn main() {}
