// Hide the extra console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Workaround: disable the WebKitGTK DMA-BUF renderer on Linux to avoid
    // `EGL_BAD_PARAMETER` crashes on newer Mesa (CachyOS, Arch, Fedora, etc.).
    // See https://github.com/tauri-apps/tauri/issues/9394
    #[cfg(target_os = "linux")]
    {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    ai_switch_lib::run();
}
