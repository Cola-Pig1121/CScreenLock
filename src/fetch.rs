use std::io::Read;

/// 固定使用 uapis 随机图片接口（category=acg, type=pc），仅允许 https 主机名硬编码
const IMAGE_URL: &str = "https://uapis.cn/api/v1/random/image?category=acg&type=pc";

/// 已解码位图（RGBA8，自顶向下，可直接构造 slint::Image）
pub struct Decoded {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

pub fn fetch_url(url: &str, key: &str) -> Result<Decoded, String> {
    ensure_safe_url(url)?;
    let mut req = ureq::get(url);
    if !key.is_empty() {
        req = req.set("x-api-key", key);
    }
    let resp = req
        .call()
        .map_err(|e| format!("请求失败: {e}"))?;
    let mut buf = Vec::new();
    resp.into_reader()
        .read_to_end(&mut buf)
        .map_err(|e| format!("读取失败: {e}"))?;
    if buf.len() > 30 * 1024 * 1024 {
        return Err("图片过大".to_string());
    }
    decode(&buf)
}

/// 请求前校验：仅允许 http/https，且主机不得指向 localhost/环回/私有/保留地址。
/// 当前内置 URL 为公网常量，此校验为纵深防御（防止未来可配置化后引入 SSRF）。
fn ensure_safe_url(url: &str) -> Result<(), String> {
    let lower = url.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
        .ok_or("仅允许 http/https 协议")?;
    let host = rest.split('/').next().unwrap_or("");
    let host = host.rsplit('@').next().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host);
    if host.is_empty() {
        return Err("URL 缺少主机名".to_string());
    }
    if host == "localhost" || host.ends_with(".localhost") {
        return Err("拒绝访问本地地址".to_string());
    }
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        let blocked = match ip {
            std::net::IpAddr::V4(v4) => {
                let o = v4.octets();
                v4.is_loopback()
                    || v4.is_private()
                    || v4.is_link_local()
                    || v4.is_unspecified()
                    || v4.is_multicast()
                    || (o[0] == 100 && (o[1] & 0xC0) == 64) // 100.64.0.0/10 CGNAT
                    || (o[0] == 198 && (o[1] & 0xFE) == 18) // 198.18.0.0/15 benchmark
            }
            std::net::IpAddr::V6(v6) => {
                let s0 = v6.segments()[0];
                v6.is_loopback()
                    || v6.is_unspecified()
                    || v6.is_multicast()
                    || (s0 & 0xFE00) == 0xFC00 // fc00::/7 ULA
                    || (s0 & 0xFFC0) == 0xFE80 // fe80::/10 link-local
            }
        };
        if blocked {
            return Err("拒绝访问本地/私有/保留地址".to_string());
        }
    }
    Ok(())
}

pub fn decode(bytes: &[u8]) -> Result<Decoded, String> {
    let img = image::load_from_memory(bytes).map_err(|e| format!("解码失败: {e}"))?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    Ok(Decoded {
        w,
        h,
        rgba: rgba.into_raw(),
    })
}

/// 从本地文件读取图片
pub fn load_file(path: &std::path::Path) -> Result<Decoded, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("读取失败: {e}"))?;
    decode(&bytes)
}

/// 异步拉取 uapis 图片，完成后通过回调（在主线程执行）交付结果。
/// 供 UI 线程外调用：内部起线程下载，结果经 slint::invoke_from_event_loop 送回。
pub fn spawn_fetch(key: String, tx: std::sync::mpsc::Sender<Option<Decoded>>) {
    let _ = std::thread::Builder::new()
        .spawn(move || {
            let result = fetch_url(IMAGE_URL, &key).ok();
            let _ = tx.send(result);
        })
        .map(|_| ())
        .unwrap_or(());
}
