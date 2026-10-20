/// 系统级无操作时长（毫秒）。
/// 依赖 `system-idle-time`：Windows / macOS / Linux(X11 + Wayland via ScreenSaver DBus)。
/// 返回 None 表示当前环境不支持（如部分 Wayland 合成器），此时仅支持手动锁定。
pub fn idle_ms() -> Option<u64> {
    system_idle_time::get_idle_time()
        .ok()
        .map(|d| d.as_millis() as u64)
}
