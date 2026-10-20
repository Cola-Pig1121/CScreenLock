use std::io::Write;
use std::path::PathBuf;

/// 跨平台单实例：对配置目录下的锁文件持有独占锁，进程退出自动释放
pub struct Guard {
    file: std::fs::File,
    #[allow(dead_code)]
    path: PathBuf,
}

pub fn acquire() -> Option<Guard> {
    let mut dir = crate::config::config_dir_pub();
    dir.push("instance.lock");
    if let Some(parent) = dir.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&dir)
        .ok()?;
    file.try_lock().ok()?;
    let _ = file.write_all(std::process::id().to_string().as_bytes());
    Some(Guard { file, path: dir })
}

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}
