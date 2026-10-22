/// 锁屏期间的输入拦截：低级键盘钩子（WH_KEYBOARD_LL）。
/// Windows 实现：吞掉 Alt/Win 键与 Ctrl+Esc，阻断 Alt+Tab、Alt+F4、Win+Tab、
/// Win+D、Win+L、Ctrl+Esc 等切换/逃逸路径；解锁时卸载。
/// 注意：Ctrl+Alt+Del 属于系统安全序列，任何用户态程序都无法拦截（文档已说明）。
#[cfg(windows)]
mod imp {
    use std::cell::Cell;

    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_ESCAPE, VK_LMENU, VK_LWIN, VK_MENU, VK_RMENU, VK_RWIN,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT,
        WH_KEYBOARD_LL,
    };

    thread_local! {
        static HOOK: Cell<Option<HHOOK>> = const { Cell::new(None) };
    }

    unsafe extern "system" fn proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
            let vk = kb.vkCode;
            // 吞掉 Alt / Win（按下与抬起都拦截，系统不会认为修饰键被按住）
            if vk == VK_MENU.0 as u32
                || vk == VK_LMENU.0 as u32
                || vk == VK_RMENU.0 as u32
                || vk == VK_LWIN.0 as u32
                || vk == VK_RWIN.0 as u32
            {
                return LRESULT(1);
            }
            // Ctrl+Esc 打开开始菜单
            if vk == VK_ESCAPE.0 as u32
                && (GetAsyncKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0
            {
                return LRESULT(1);
            }
        }
        let _ = wparam;
        HOOK.with(|h| CallNextHookEx(h.get().unwrap_or_default(), code, wparam, lparam))
    }

    pub fn install() {
        HOOK.with(|h| {
            if h.get().is_some() {
                return;
            }
            let ok = unsafe {
                let module = GetModuleHandleW(None).unwrap_or_default();
                SetWindowsHookExW(WH_KEYBOARD_LL, Some(proc), module, 0)
            };
            if let Ok(hh) = ok {
                h.set(Some(hh));
            }
        });
    }

    pub fn uninstall() {
        HOOK.with(|h| {
            if let Some(hh) = h.take() {
                unsafe {
                    let _ = UnhookWindowsHookEx(hh);
                }
            }
        });
    }
}

#[cfg(windows)]
pub use imp::{install, uninstall};

#[cfg(not(windows))]
pub fn install() {}

#[cfg(not(windows))]
pub fn uninstall() {}
