/// 把标题匹配的所有窗口重新置顶（Slint 稳定版无 always_on_top API 的补偿 shim）。
/// - `lock_title`：锁屏窗口，置顶并覆盖对应显示器；
/// - `widget_title`：自定义控件窗口，仅置顶（位置由控件自身管理）。
/// 每 2 秒调用一次，既维持多屏覆盖，也防御其他程序抢置顶。
pub fn enforce(lock_title: &str, widget_title: &str) {
    #[cfg(windows)]
    enforce_windows(lock_title, widget_title);
    #[cfg(target_os = "linux")]
    enforce_x11(lock_title, widget_title);
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = (lock_title, widget_title);
    }
}

#[cfg(windows)]
fn enforce_windows(lock_title: &str, widget_title: &str) {
    use std::cell::RefCell;
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextLengthW, GetWindowTextW, SetForegroundWindow, SetWindowPos,
        HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    };

    thread_local! {
        static FOUND: RefCell<Vec<(HWND, String)>> = const { RefCell::new(Vec::new()) };
    }

    // 回调内不传外部指针：收集 (hwnd, title)，匹配在 Rust 侧完成
    unsafe extern "system" fn cb(hwnd: HWND, _data: LPARAM) -> BOOL {
        let len = GetWindowTextLengthW(hwnd);
        if len > 0 {
            let mut buf = vec![0u16; len as usize + 2];
            let n = GetWindowTextW(hwnd, &mut buf);
            if n > 0 {
                let s = String::from_utf16_lossy(&buf[..n as usize]);
                FOUND.with(|f| f.borrow_mut().push((hwnd, s)));
            }
        }
        BOOL(1)
    }

    FOUND.with(|f| f.borrow_mut().clear());
    unsafe {
        let _ = EnumWindows(Some(cb), LPARAM(0));
    }
    // 先提锁屏，再提控件：保证自定义控件窗口叠在锁屏之上
    let found = FOUND.with(|f| f.borrow().clone());
    unsafe {
        for (hwnd, title) in &found {
            if title == lock_title {
                let _ = SetWindowPos(
                    *hwnd,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
                // 夺回焦点：防止程序弹窗等把键盘焦点从锁屏移走
                let _ = SetForegroundWindow(*hwnd);
            }
        }
        for (hwnd, title) in &found {
            // 兜底：未按约定声明 win-title 的控件窗口，标题为 winit 默认的 exe 路径
            if title == widget_title || title.contains("screen-lock") {
                // 先摘掉 topmost 再设回，强制移到 topmost 组最上层
                // （对已是 topmost 的窗口直接设 TOPMOST 不会改变 Z 序）
                let _ = SetWindowPos(
                    *hwnd,
                    HWND_NOTOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
                let _ = SetWindowPos(
                    *hwnd,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
        }
    }
}

/// Linux X11：设置 _NET_WM_STATE_ABOVE。Wayland 无此概念（合成器决定层级），跳过。
#[cfg(target_os = "linux")]
fn enforce_x11(lock_title: &str, widget_title: &str) {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, PropMode};
    use x11rb::rust_connection::RustConnection;

    let Ok((conn, screen)) = RustConnection::connect(None) else {
        return;
    };
    let root = match conn.setup().roots.get(screen) {
        Some(s) => s.root,
        None => return,
    };
    let run = || -> Option<()> {
        let above = conn
            .intern_atom(false, b"_NET_WM_STATE_ABOVE")
            .ok()?
            .reply()
            .ok()?
            .atom;
        let state = conn
            .intern_atom(false, b"_NET_WM_STATE")
            .ok()?
            .reply()
            .ok()?
            .atom;
        let wm_name = conn
            .intern_atom(false, b"_NET_WM_NAME")
            .ok()?
            .reply()
            .ok()?
            .atom;
        let tree = conn.query_tree(root).ok()?.reply().ok()?;
        for win in tree.children {
            let Ok(props) = conn.get_property(false, win, wm_name, AtomEnum::NONE, 0, 256) else {
                continue;
            };
            let Ok(props) = props.reply() else { continue };
            let name = String::from_utf8_lossy(&props.value).to_string();
            if name == lock_title || name == widget_title || name.contains("screen-lock") {
                let _ =
                    conn.change_property32(PropMode::REPLACE, win, state, AtomEnum::ATOM, &[above]);
            }
        }
        let _ = conn.flush();
        Some(())
    };
    let _ = run();
}
