use std::path::{Path, PathBuf};

use rand::seq::SliceRandom;

use crate::config::{RotateOrder, WallpaperSource};
use crate::fetch::Decoded;

const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp"];

/// 扫描文件夹中的图片（按文件名排序）
pub fn scan_folder(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                    if IMAGE_EXTS.contains(&ext.to_ascii_lowercase().as_str()) {
                        files.push(p);
                    }
                }
            }
        }
    }
    files.sort();
    files
}

/// 本地文件夹轮换状态机
pub struct Rotation {
    files: Vec<PathBuf>,
    idx: usize,
    queue: Vec<PathBuf>,
    order: RotateOrder,
}

impl Rotation {
    pub fn new(files: Vec<PathBuf>, order: RotateOrder) -> Self {
        let mut r = Self { files, idx: 0, queue: Vec::new(), order };
        r.rebuild();
        r
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    fn rebuild(&mut self) {
        match self.order {
            RotateOrder::Sequential => {}
            RotateOrder::Random => {
                self.queue = self.files.clone();
                let mut rng = rand::thread_rng();
                self.queue.shuffle(&mut rng);
            }
        }
    }

    /// 取下一张图片路径
    pub fn next_file(&mut self) -> Option<PathBuf> {
        if self.files.is_empty() {
            return None;
        }
        match self.order {
            RotateOrder::Sequential => {
                let f = self.files[self.idx % self.files.len()].clone();
                self.idx += 1;
                Some(f)
            }
            RotateOrder::Random => {
                if self.queue.is_empty() {
                    self.rebuild();
                }
                self.queue.pop()
            }
        }
    }
}

/// 生成磨砂背景：降采样 → 高斯模糊 → 升采样（低成本跨平台方案）
pub fn blurred(img: &Decoded) -> Decoded {
    let Some(src) = image::RgbaImage::from_raw(img.w, img.h, img.rgba.clone()) else {
        return Decoded { w: img.w, h: img.h, rgba: img.rgba.clone() };
    };
    let sw = (img.w / 4).max(8);
    let sh = (img.h / 4).max(8);
    let small = image::imageops::resize(&src, sw, sh, image::imageops::FilterType::Triangle);
    let blur = image::imageops::blur(&small, 5.0);
    let big = image::imageops::resize(&blur, img.w, img.h, image::imageops::FilterType::Triangle);
    Decoded {
        w: img.w,
        h: img.h,
        rgba: big.into_raw(),
    }
}

/// 根据配置决定当前应使用哪种壁纸来源
pub fn effective_source(cfg: &crate::config::WallpaperConfig) -> WallpaperSource {
    match cfg.source {
        WallpaperSource::Folder if cfg.folder.trim().is_empty() => WallpaperSource::Off,
        other => other,
    }
}
