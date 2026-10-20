/// 开机自启动（自实现，三平台）：
/// - Windows：HKCU\Software\Microsoft\Windows\CurrentVersion\Run 注册表值
/// - macOS：~/Library/LaunchAgents/dev.colapig.screenlock.plist
/// - Linux：~/.config/autostart/screenlock.desktop（XDG autostart）
pub fn set_enabled(enable: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    if enable {
        let _ = &exe; // 各平台自行取用
    }
    #[cfg(windows)]
    {
        windows::set(enable, &exe)
    }
    #[cfg(target_os = "macos")]
    {
        macos::set(enable, &exe)
    }
    #[cfg(target_os = "linux")]
    {
        linux::set(enable, &exe)
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        let _ = (enable, exe);
        Err("当前平台不支持开机自启动".to_string())
    }
}

pub fn is_enabled() -> bool {
    #[cfg(windows)]
    {
        windows::is_enabled()
    }
    #[cfg(target_os = "macos")]
    {
        macos::is_enabled()
    }
    #[cfg(target_os = "linux")]
    {
        linux::is_enabled()
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        false
    }
}

#[cfg(windows)]
mod windows {
    use std::path::Path;
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
        KEY_READ, KEY_SET_VALUE, REG_SAM_FLAGS, REG_SZ,
    };

    const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

    fn open(flags: REG_SAM_FLAGS) -> Result<HKEY, String> {
        unsafe {
            let wide: Vec<u16> = RUN_KEY.encode_utf16().chain(std::iter::once(0)).collect();
            let mut hkey = HKEY::default();
            let err = RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR::from_raw(wide.as_ptr()),
                0,
                flags,
                &mut hkey,
            );
            if err == ERROR_SUCCESS {
                Ok(hkey)
            } else {
                Err(format!("打开注册表 Run 键失败：{err:?}"))
            }
        }
    }

    pub fn set(enable: bool, exe: &Path) -> Result<(), String> {
        unsafe {
            let hkey = open(KEY_SET_VALUE)?;
            let r = if enable {
                let bytes: Vec<u8> = exe
                    .display()
                    .to_string()
                    .encode_utf16()
                    .flat_map(|c| c.to_le_bytes())
                    .collect();
                RegSetValueExW(hkey, w!("ScreenLock"), 0, REG_SZ, Some(&bytes))
            } else {
                RegDeleteValueW(hkey, w!("ScreenLock"))
            };
            let _ = RegCloseKey(hkey);
            if r == ERROR_SUCCESS || (!enable && r.0 == 2) {
                // 2 = ERROR_FILE_NOT_FOUND：本来就没开，删除视为成功
                Ok(())
            } else {
                Err(format!("写入注册表失败：{r:?}"))
            }
        }
    }

    pub fn is_enabled() -> bool {
        match open(KEY_READ) {
            Ok(hkey) => {
                let mut size = 0u32;
                let err = unsafe {
                    windows::Win32::System::Registry::RegQueryValueExW(
                        hkey,
                        w!("ScreenLock"),
                        None,
                        None,
                        None,
                        Some(&mut size),
                    )
                };
                unsafe {
                    let _ = RegCloseKey(hkey);
                }
                err == ERROR_SUCCESS
            }
            Err(_) => false,
        }
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::path::{Path, PathBuf};

    fn plist_path() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        let mut p = PathBuf::from(home);
        p.push("Library");
        p.push("LaunchAgents");
        p.push("dev.colapig.screenlock.plist");
        Some(p)
    }

    pub fn set(enable: bool, exe: &Path) -> Result<(), String> {
        let path = plist_path().ok_or("无法定位 HOME 目录")?;
        if enable {
            std::fs::create_dir_all(path.parent().ok_or("无效的 plist 路径")?)
                .map_err(|e| e.to_string())?;
            let content = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>dev.colapig.screenlock</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>
"#,
                exe.display()
            );
            std::fs::write(&path, content).map_err(|e| e.to_string())
        } else {
            let _ = std::fs::remove_file(path);
            Ok(())
        }
    }

    pub fn is_enabled() -> bool {
        plist_path().map(|p| p.exists()).unwrap_or(false)
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::path::{Path, PathBuf};

    fn desktop_path() -> Option<PathBuf> {
        let cfg = dirs::config_dir()?;
        Some(cfg.join("autostart").join("screenlock.desktop"))
    }

    pub fn set(enable: bool, exe: &Path) -> Result<(), String> {
        let path = desktop_path().ok_or("无法定位 XDG 配置目录")?;
        if enable {
            std::fs::create_dir_all(path.parent().ok_or("无效的 desktop 路径")?)
                .map_err(|e| e.to_string())?;
            let content = format!(
                "[Desktop Entry]\nType=Application\nName=ScreenLock\nExec={}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
                exe.display()
            );
            std::fs::write(&path, content).map_err(|e| e.to_string())
        } else {
            let _ = std::fs::remove_file(path);
            Ok(())
        }
    }

    pub fn is_enabled() -> bool {
        desktop_path().map(|p| p.exists()).unwrap_or(false)
    }
}
