/// 物理显示器几何（像素坐标，虚拟桌面系）
#[derive(Debug, Clone, Copy)]
pub struct Monitor {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[cfg(windows)]
pub fn list() -> Vec<Monitor> {
    use windows::Win32::Foundation::{BOOL, LPARAM, RECT};
    use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO};

    unsafe extern "system" fn cb(hm: HMONITOR, _hdc: HDC, _r: *mut RECT, data: LPARAM) -> BOOL {
        let vec = &mut *(data.0 as *mut Vec<Monitor>);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(hm, &mut mi).as_bool() {
            let rc = mi.rcMonitor;
            vec.push(Monitor {
                x: rc.left,
                y: rc.top,
                w: rc.right - rc.left,
                h: rc.bottom - rc.top,
            });
        }
        BOOL(1)
    }

    let mut out: Vec<Monitor> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            HDC::default(),
            None,
            Some(cb),
            LPARAM(&mut out as *mut _ as isize),
        );
    }
    out
}

#[cfg(target_os = "linux")]
pub fn list() -> Vec<Monitor> {
    use x11rb::connection::Connection;
    use x11rb::protocol::randr::{CRTC, ConnectionExt as RandrExt};
    use x11rb::rust_connection::RustConnection;

    let mut out = Vec::new();
    let Ok((conn, screen)) = RustConnection::connect(None) else {
        return out;
    };
    let root = match conn.setup().roots.get(screen) {
        Some(s) => s.root,
        None => return out,
    };
    let Ok(sr) = conn.xrandr_get_screen_resources_current(root) else {
        return out;
    };
    let Ok(sr) = sr.reply() else { return out };
    for output in sr.outputs {
        let Ok(oi) = conn.xrandr_get_output_info(output, sr.config_timestamp) else {
            continue;
        };
        let Ok(oi) = oi.reply() else { continue };
        if oi.crtc == CRTC::NONE {
            continue;
        }
        let Ok(ci) = conn.xrandr_get_crtc_info(oi.crtc, sr.config_timestamp) else {
            continue;
        };
        let Ok(ci) = ci.reply() else { continue };
        out.push(Monitor {
            x: ci.x,
            y: ci.y,
            w: ci.width as i32,
            h: ci.height as i32,
        });
    }
    out
}

#[cfg(target_os = "macos")]
pub fn list() -> Vec<Monitor> {
    use core_graphics::display::{active_displays, CGDisplayBounds};
    // 未在本机验证（macOS 分支）：依赖 core-graphics 的 active_displays API
    let ids = active_displays(16).unwrap_or_default();
    ids.into_iter()
        .map(|id| {
            let r = CGDisplayBounds(id);
            Monitor {
                x: r.origin.x as i32,
                y: r.origin.y as i32,
                w: r.size.width as i32,
                h: r.size.height as i32,
            }
        })
        .collect()
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub fn list() -> Vec<Monitor> {
    Vec::new()
}
