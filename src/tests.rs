#[cfg(test)]
mod tests {
    use crate::config;
    use crate::wallpaper;

    #[test]
    fn password_hash_verify_roundtrip() {
        let mut cfg = config::Config::default();
        assert!(!config::verify(&cfg, "anything"), "未设置密码时不应放行");
        config::set_password(&mut cfg, "s3cret!");
        assert!(config::verify(&cfg, "s3cret!"));
        assert!(!config::verify(&cfg, "wrong"));
        assert!(!config::verify(&cfg, ""));
    }

    #[test]
    fn salt_differs_per_set() {
        let mut a = config::Config::default();
        let mut b = config::Config::default();
        config::set_password(&mut a, "same");
        config::set_password(&mut b, "same");
        assert_ne!(a.password_hash, b.password_hash, "相同密码不同盐应得到不同哈希");
    }

    #[test]
    fn v1_config_migrates_with_defaults() {
        // v1 时代的配置内容（无 wallpaper/widgets 字段）应能解析并补齐默认值
        let v1 = r#"{
            "idle_minutes": 5,
            "password_hash": "abc",
            "password_salt": "def",
            "autostart": true,
            "uapis_key": "k"
        }"#;
        let cfg = config::parse(v1).expect("v1 应可解析");
        assert_eq!(cfg.idle_minutes, 5);
        assert_eq!(cfg.wallpaper.source, config::WallpaperSource::Api);
        assert_eq!(cfg.wallpaper.rotate_minutes, 10);
        assert_eq!(cfg.widgets.len(), 1); // 默认时钟
        assert_eq!(cfg.widgets[0].kind, "clock");
    }

    #[test]
    fn rotation_sequential_cycles() {
        let files: Vec<std::path::PathBuf> =
            (0..3).map(|i| std::path::PathBuf::from(format!("f{i}.jpg"))).collect();
        let mut rot = wallpaper::Rotation::new(files.clone(), config::RotateOrder::Sequential);
        let got: Vec<_> = (0..5).map(|_| rot.next_file().unwrap()).collect();
        assert_eq!(got[0], files[0]);
        assert_eq!(got[3], files[0]);
        assert_eq!(got[4], files[1]);
    }

    #[test]
    fn rotation_random_covers_all() {
        let files: Vec<std::path::PathBuf> =
            (0..4).map(|i| std::path::PathBuf::from(format!("f{i}.jpg"))).collect();
        let mut rot = wallpaper::Rotation::new(files.clone(), config::RotateOrder::Random);
        let mut seen = std::collections::HashSet::new();
        for _ in 0..8 {
            seen.insert(rot.next_file().unwrap());
        }
        assert_eq!(seen.len(), 4, "随机轮换一轮应覆盖全部图片");
    }

    #[test]
    fn widget_render_clock_and_note() {
        use chrono::TimeZone;
        let now = chrono::Local.with_ymd_and_hms(2026, 10, 20, 21, 47, 0).unwrap();
        let mut props = std::collections::HashMap::new();
        props.insert("hour12".to_string(), "false".to_string());
        let clock = config::WidgetSpec {
            kind: "clock".into(),
            x: 0.5,
            y: 0.3,
            props,
        };
        let v = crate::widgets::render(&clock, now);
        assert_eq!(v.title, "21:47");
        assert!(v.subtitle.contains("10月20日"));
        assert!(v.subtitle.contains("周二"));

        let mut props2 = std::collections::HashMap::new();
        props2.insert("content".to_string(), "记得保存论文".to_string());
        let note = config::WidgetSpec {
            kind: "note".into(),
            x: 0.5,
            y: 0.6,
            props: props2,
        };
        let v2 = crate::widgets::render(&note, now);
        assert_eq!(v2.subtitle, "记得保存论文");
    }
}
