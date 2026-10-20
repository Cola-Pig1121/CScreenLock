use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;

const APP_NAME: &str = "ScreenLock";
pub const DEFAULT_IDLE_MIN: u64 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WallpaperSource {
    /// uapis 随机图片接口
    Api,
    /// 本地文件夹轮换
    Folder,
    /// 纯色背景
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RotateOrder {
    Random,
    Sequential,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WallpaperConfig {
    pub source: WallpaperSource,
    pub folder: String,
    /// 轮换间隔分钟数，0 = 不轮换
    pub rotate_minutes: u64,
    pub order: RotateOrder,
}

impl Default for WallpaperConfig {
    fn default() -> Self {
        Self {
            source: WallpaperSource::Api,
            folder: String::new(),
            rotate_minutes: 10,
            order: RotateOrder::Random,
        }
    }
}

/// 锁屏组件实例：kind + 相对坐标(0..1) + 任意字符串参数
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WidgetSpec {
    pub kind: String,
    #[serde(default = "default_x")]
    pub x: f32,
    #[serde(default = "default_y")]
    pub y: f32,
    #[serde(default)]
    pub props: HashMap<String, String>,
}

fn default_x() -> f32 {
    0.5
}
fn default_y() -> f32 {
    0.5
}

impl WidgetSpec {
    pub fn prop(&self, key: &str, fallback: &str) -> String {
        self.props.get(key).cloned().unwrap_or_else(|| fallback.to_string())
    }
    pub fn prop_bool(&self, key: &str, fallback: bool) -> bool {
        match self.props.get(key).map(|s| s.as_str()) {
            Some("true") | Some("1") | Some("yes") => true,
            Some("false") | Some("0") | Some("no") => false,
            _ => fallback,
        }
    }
}

/// 内置组件模板（设置页"添加组件"下拉用，与 app.slint 中列表保持一致）
#[allow(dead_code)]
pub const WIDGET_TEMPLATES: &[(&str, &str)] = &[
    ("clock", "屏保时钟"),
    ("note", "屏保备忘录"),
    ("text", "自定义文本"),
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub idle_minutes: u64,
    pub password_hash: String,
    pub password_salt: String,
    pub autostart: bool,
    pub uapis_key: String,
    pub wallpaper: WallpaperConfig,
    pub widgets: Vec<WidgetSpec>,
}

impl Default for Config {
    fn default() -> Self {
        let mut props = HashMap::new();
        props.insert("hour12".to_string(), "false".to_string());
        props.insert("date".to_string(), "true".to_string());
        Self {
            idle_minutes: DEFAULT_IDLE_MIN,
            password_hash: String::new(),
            password_salt: String::new(),
            autostart: false,
            uapis_key: String::new(),
            wallpaper: WallpaperConfig::default(),
            // 默认提供一个居中时钟组件（参照设计稿）
            widgets: vec![WidgetSpec {
                kind: "clock".to_string(),
                x: 0.5,
                y: 0.32,
                props,
            }],
        }
    }
}

fn config_dir() -> PathBuf {
    let mut dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    dir.push(APP_NAME);
    dir
}

/// 配置目录（供单实例锁文件等使用）
pub fn config_dir_pub() -> PathBuf {
    config_dir()
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

fn widgets_dir() -> PathBuf {
    config_dir().join("widgets.d")
}

/// 自定义控件目录：<配置目录>/custom-widgets/*.slint（用户编写 Slint 代码）
pub fn custom_widgets_dir() -> PathBuf {
    config_dir().join("custom-widgets")
}

/// 解析配置内容（v1 文件缺少的新字段由 serde(default) 自动补齐 = 迁移）
pub fn parse(s: &str) -> Result<Config, String> {
    serde_json::from_str(s).map_err(|e| e.to_string())
}

/// 加载主配置；首次运行时写入带默认密码(123456)的配置并返回 fresh=true。
/// widgets.d 扩展组件由 `load_extra_widgets()` 单独加载（不参与主配置保存）。
pub fn load() -> (Config, bool) {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(s) => match parse(&s) {
            Ok(c) => (c, false),
            Err(_) => {
                let mut c = Config::default();
                set_password(&mut c, "123456");
                let _ = save(&c);
                (c, true)
            }
        },
        Err(_) => {
            let mut c = Config::default();
            set_password(&mut c, "123456");
            let _ = save(&c);
            (c, true)
        }
    }
}

/// widgets.d 目录：每个 *.json 文件是一个 WidgetSpec 数组或单个对象
/// （开放扩展入口：用户无需改主配置即可添加组件实例）
pub fn load_extra_widgets() -> Vec<WidgetSpec> {
    let mut out = Vec::new();
    let dir = widgets_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Ok(s) = std::fs::read_to_string(&p) {
            if let Ok(list) = serde_json::from_str::<Vec<WidgetSpec>>(&s) {
                out.extend(list);
            } else if let Ok(one) = serde_json::from_str::<WidgetSpec>(&s) {
                out.push(one);
            }
        }
    }
    out
}

pub fn save(cfg: &Config) -> std::io::Result<()> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;
    let json = serde_json::to_string_pretty(cfg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    std::fs::write(config_path(), json)
}

pub fn random_salt() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..16).map(|_| format!("{:02x}", rng.gen::<u8>())).collect()
}

pub fn hash_password(salt: &str, pw: &str) -> String {
    let mut h = Sha256::new();
    h.update(salt.as_bytes());
    h.update(pw.as_bytes());
    h.finalize().iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn set_password(cfg: &mut Config, pw: &str) {
    cfg.password_salt = random_salt();
    cfg.password_hash = hash_password(&cfg.password_salt, pw);
}

/// 校验密码。未设置密码时任何输入均不通过（需先在设置中配置）。
pub fn verify(cfg: &Config, input: &str) -> bool {
    if cfg.password_hash.is_empty() {
        return false;
    }
    hash_password(&cfg.password_salt, input) == cfg.password_hash
}

// ---------- uapis 密钥：不落地明文 ----------
// 优先级：环境变量 SCREENLOCK_UAPIS_KEY > 系统密钥环(keyring)。
// config.json 中的 uapis_key 字段仅作为旧版本迁移来源，保存后清空。

const KEYRING_SERVICE: &str = "ScreenLock";
const KEYRING_USER: &str = "uapis_key";

fn keyring_entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).map_err(|e| e.to_string())
}

/// 读取 uapis 密钥（环境变量优先，其次系统密钥环；均无则空）
pub fn get_uapis_key() -> String {
    if let Ok(v) = std::env::var("SCREENLOCK_UAPIS_KEY") {
        if !v.trim().is_empty() {
            return v.trim().to_string();
        }
    }
    keyring_entry()
        .ok()
        .and_then(|e| e.get_password().ok())
        .unwrap_or_default()
}

/// 保存 uapis 密钥到系统密钥环（空值表示删除），并确保 config 中不留明文
pub fn set_uapis_key(cfg: &mut Config, key: &str) -> Result<(), String> {
    let entry = keyring_entry()?;
    if key.is_empty() {
        let _ = entry.delete_credential();
    } else {
        entry
            .set_password(key)
            .map_err(|e| format!("写入系统密钥环失败：{e}"))?;
    }
    cfg.uapis_key = String::new();
    Ok(())
}
