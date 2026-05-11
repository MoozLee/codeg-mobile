fn main() {
    #[cfg(any(feature = "tauri-runtime", feature = "mobile-runtime"))]
    tauri_build::build();
}
