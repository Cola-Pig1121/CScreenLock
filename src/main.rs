#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

mod config;
mod custom;
mod fetch;
mod platform;
mod tests;
mod wallpaper;
mod widgets;

slint::include_modules!();

use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::sync::mpsc;
use std::time::Duration;

use chrono::Local;
use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode};

use config::{Config, WallpaperSource, WidgetSpec};

fn lock_title() -> String {
    format!("ScreenLock-{}", std::process::id())
}

fn source_index(src: WallpaperSource) -> i32 {
    match src {
        WallpaperSource::Api => 0,
        WallpaperSource::Folder => 1,
        WallpaperSource::Off => 2,
    }
}

fn index_source(i: i32) -> WallpaperSource {
    match i {
        1 => WallpaperSource::Folder,
        2 => WallpaperSource::Off,
        _ => WallpaperSource::Api,
    }
}

fn kind_label(kind: &str) -> &'static str {
    match kind {
        "clock" => "屏保时钟",
        "note" => "屏保备忘录",
        _ => "自定义文本",
    }
}

fn decoded_to_image(d: &fetch::Decoded) -> slint::Image {
    let buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&d.rgba, d.w, d.h);
    slint::Image::from_rgba8(buf)
}

/// 设置页的组件编辑工作副本
#[derive(Clone)]
struct WidgetEditData {
    kind: String,
    x: f32,
    y: f32,
    content: String,
    hour12: bool,
    date: bool,
}

impl From<&WidgetSpec> for WidgetEditData {
    fn from(s: &WidgetSpec) -> Self {
        Self {
            kind: s.kind.clone(),
            x: s.x,
            y: s.y,
            content: s.prop("content", ""),
            hour12: s.prop_bool("hour12", false),
            date: s.prop_bool("date", true),
        }
    }
}

impl WidgetEditData {
    fn to_spec(&self) -> WidgetSpec {
        let mut props = std::collections::HashMap::new();
        match self.kind.as_str() {
            "clock" => {
                props.insert("hour12".into(), self.hour12.to_string());
                props.insert("date".into(), self.date.to_string());
            }
            _ => {
                props.insert("content".into(), self.content.clone());
            }
        }
        WidgetSpec {
            kind: self.kind.clone(),
            x: self.x,
            y: self.y,
            props,
        }
    }
}

type AppRc = Rc<RefCell<App>>;

struct App {
    app: Weak<RefCell<App>>,
    cfg: Config,
    /// widgets.d 加载的扩展组件（设置页不编辑、保存时不回写）
    extra_widgets: Vec<WidgetSpec>,
    locked: bool,
    lock_windows: Vec<LockWindow>,
    /// 用户自定义控件（slint-interpreter 运行时实例）
    custom_widgets: Vec<slint_interpreter::ComponentInstance>,
    rotation: Option<wallpaper::Rotation>,
    settings: Option<SettingsWindow>,
    tray: Option<AppTray>,
    widget_edits: Vec<WidgetEditData>,
    fetch_tx: mpsc::Sender<Option<fetch::Decoded>>,
    fetch_rx: Rc<RefCell<mpsc::Receiver<Option<fetch::Decoded>>>>,
    rotate_timer: Option<Timer>,
    _timers: Vec<Timer>,
}

fn main() {
    let _single = match platform::single::acquire() {
        Some(g) => g,
        None => return, // 已有实例在运行
    };

    let (mut cfg, fresh) = config::load();
    // 以系统实际状态为准同步自启开关（注册表/plist/desktop 文件）
    cfg.autostart = platform::autostart::is_enabled();
    // 旧版本遗留的明文密钥：迁移到系统密钥环并从配置中清除
    if !cfg.uapis_key.is_empty() {
        let legacy = cfg.uapis_key.clone();
        if config::set_uapis_key(&mut cfg, &legacy).is_ok() {
            let _ = config::save(&cfg);
        }
    }
    let app = App::new(cfg);

    if fresh {
        // 首次启动：打开设置窗口提示修改默认密码(123456)
        open_settings(&app);
    }

    if slint::run_event_loop().is_err() {
        // 事件循环错误无需处理，退出清理
    }

    {
        let mut b = app.borrow_mut();
        b.close_lock_windows();
        b.tray = None;
    }
}

impl App {
    fn new(cfg: Config) -> AppRc {
        let (tx, rx) = mpsc::channel();
        let app: AppRc = Rc::new(RefCell::new(Self {
            app: Weak::new(),
            cfg,
            extra_widgets: config::load_extra_widgets(),
            locked: false,
            lock_windows: Vec::new(),
            custom_widgets: Vec::new(),
            rotation: None,
            settings: None,
            tray: None,
            widget_edits: Vec::new(),
            fetch_tx: tx,
            fetch_rx: Rc::new(RefCell::new(rx)),
            rotate_timer: None,
            _timers: Vec::new(),
        }));
        {
            let mut b = app.borrow_mut();
            b.app = Rc::downgrade(&app);
        }
        setup_tray(&app);
        setup_timers(&app);
        app
    }

    // ---------- 锁屏 ----------

    fn lock(&mut self) {
        if self.locked {
            return;
        }
        self.locked = true;

        let mons = platform::monitors::list();
        let targets: Vec<(i32, i32, i32, i32)> = if mons.is_empty() {
            Vec::new()
        } else {
            mons.iter().map(|m| (m.x, m.y, m.w, m.h)).collect()
        };
        let show_quit = cfg!(debug_assertions);
        let mut wins = Vec::new();
        if targets.is_empty() {
            if let Ok(win) = LockWindow::new() {
                win.set_title_text(SharedString::from(lock_title()));
                win.set_full_screen_flag(true);
                win.set_show_quit(show_quit);
                bind_lock_callbacks(&win, &self.app);
                if let Err(e) = win.show() {
                    eprintln!("[dbg] lock window show failed: {e}");
                }
                wins.push(win);
            }
        } else {
            for &(x, y, w, h) in &targets {
                if let Ok(win) = LockWindow::new() {
                    win.set_title_text(SharedString::from(lock_title()));
                    win.set_mon_w(w as f32);
                    win.set_mon_h(h as f32);
                    win.set_show_quit(show_quit);
                    bind_lock_callbacks(&win, &self.app);
                    if let Err(e) = win.show() {
                    eprintln!("[dbg] lock window show failed: {e}");
                }
                    // 物理坐标覆盖对应显示器（含任务栏区域）
                    win.window().set_size(slint::WindowSize::Physical(slint::PhysicalSize::new(
                        w as u32,
                        h as u32,
                    )));
                    win.window()
                        .set_position(slint::WindowPosition::Physical(slint::PhysicalPosition::new(
                            x, y,
                        )));
                    wins.push(win);
                }
            }
        }
        self.lock_windows = wins;

        // 用户自定义控件（custom-widgets/*.slint）
        let cw = custom::compile_all(&config::custom_widgets_dir());
        custom::show_all(&cw, mons.first());
        let now = Local::now();
        custom::update_clock(
            &cw,
            &now.format("%H:%M").to_string(),
            &now.format("%-m月%-d日").to_string(),
        );
        self.custom_widgets = cw;

        self.start_wallpaper();
        self.push_widgets();
        platform::topmost::enforce(&lock_title(), &custom::widget_title());
    }

    fn unlock(&mut self) {
        self.close_lock_windows();
        self.locked = false;
    }

    fn close_lock_windows(&mut self) {
        custom::dispose(&mut self.custom_widgets);
        for win in self.lock_windows.drain(..) {
            let _ = win.hide();
        }
        if let Some(t) = self.rotate_timer.take() {
            t.stop();
        }
    }

    /// 密码提交（TextInput Enter / 解锁按钮），由 .slint 的 password-entered 回调触发
    fn handle_password(&mut self, input: &str) {
        if !self.locked {
            return;
        }
        if config::verify(&self.cfg, input) {
            self.unlock();
        } else {
            for win in &self.lock_windows {
                win.set_pwd_text(SharedString::default());
                win.set_error_msg(SharedString::from("密码错误，请重试"));
            }
        }
    }

    // ---------- 壁纸 ----------

    fn start_wallpaper(&mut self) {
        let src = wallpaper::effective_source(&self.cfg.wallpaper);
        if src != WallpaperSource::Off && self.cfg.wallpaper.rotate_minutes > 0 {
            let t = Timer::default();
            let aw = self.app.clone();
            let ms = self.cfg.wallpaper.rotate_minutes * 60_000;
            t.start(TimerMode::Repeated, Duration::from_millis(ms), move || {
                let Some(a) = aw.upgrade() else { return };
                let mut b = a.borrow_mut();
                if b.locked {
                    b.next_wallpaper();
                }
            });
            if let Some(old) = self.rotate_timer.replace(t) {
                old.stop();
            }
        } else if let Some(old) = self.rotate_timer.take() {
            old.stop();
        }
        match src {
            WallpaperSource::Api => {
                fetch::spawn_fetch(config::get_uapis_key(), self.fetch_tx.clone());
            }
            WallpaperSource::Folder => {
                self.rebuild_rotation();
                self.next_wallpaper();
            }
            WallpaperSource::Off => self.apply_wallpaper(None),
        }
    }

    fn next_wallpaper(&mut self) {
        let src = wallpaper::effective_source(&self.cfg.wallpaper);
        match src {
            WallpaperSource::Api => {
                fetch::spawn_fetch(config::get_uapis_key(), self.fetch_tx.clone());
            }
            WallpaperSource::Folder => {
                let path = match self.rotation.as_mut().and_then(|r| r.next_file()) {
                    Some(p) => p,
                    None => {
                        self.rebuild_rotation();
                        match self.rotation.as_mut().and_then(|r| r.next_file()) {
                            Some(p) => p,
                            None => {
                                self.apply_wallpaper(None);
                                return;
                            }
                        }
                    }
                };
                match fetch::load_file(&path) {
                    Ok(d) => self.apply_wallpaper(Some(d)),
                    Err(_) => {}
                }
            }
            WallpaperSource::Off => self.apply_wallpaper(None),
        }
    }

    fn rebuild_rotation(&mut self) {
        let dir = std::path::PathBuf::from(self.cfg.wallpaper.folder.trim());
        let files = wallpaper::scan_folder(&dir);
        self.rotation = Some(wallpaper::Rotation::new(files, self.cfg.wallpaper.order));
    }

    fn apply_wallpaper(&mut self, d: Option<fetch::Decoded>) {
        let (img, blur) = match &d {
            Some(dec) => {
                let base = decoded_to_image(dec);
                let blurred = wallpaper::blurred(dec);
                (base, decoded_to_image(&blurred))
            }
            None => (slint::Image::default(), slint::Image::default()),
        };
        for win in &self.lock_windows {
            win.set_wallpaper(img.clone());
            win.set_blurred(blur.clone());
        }
    }

    // ---------- 组件 ----------

    fn all_widgets(&self) -> Vec<WidgetSpec> {
        self.cfg
            .widgets
            .iter()
            .chain(self.extra_widgets.iter())
            .cloned()
            .collect()
    }

    fn push_widgets(&self) {
        let now = Local::now();
        let data: Vec<WidgetData> = self
            .all_widgets()
            .iter()
            .map(|s| {
                let v = widgets::render(s, now);
                WidgetData {
                    kind: SharedString::from(v.kind.as_str()),
                    x: v.x,
                    y: v.y,
                    title: SharedString::from(v.title.as_str()),
                    subtitle: SharedString::from(v.subtitle.as_str()),
                }
            })
            .collect();
        let model = ModelRc::new(Rc::new(slint::VecModel::from(data)));
        for win in &self.lock_windows {
            win.set_widgets(model.clone());
        }
    }

    // ---------- 设置 ----------

    fn refresh_widget_edits(&mut self) {
        self.widget_edits = self.cfg.widgets.iter().map(WidgetEditData::from).collect();
    }

    fn widget_edit_model(&self) -> ModelRc<WidgetEdit> {
        let rows: Vec<WidgetEdit> = self
            .widget_edits
            .iter()
            .enumerate()
            .map(|(i, e)| WidgetEdit {
                index: i as i32,
                kind: SharedString::from(e.kind.as_str()),
                kind_label: SharedString::from(kind_label(&e.kind)),
                x: e.x,
                y: e.y,
                content: SharedString::from(e.content.as_str()),
                hour12: e.hour12,
                date: e.date,
            })
            .collect();
        ModelRc::new(Rc::new(slint::VecModel::from(rows)))
    }

    fn populate_settings(&self) {
        let Some(s) = &self.settings else { return };
        s.set_pwd1(SharedString::default());
        s.set_pwd2(SharedString::default());
        s.set_idle_min(self.cfg.idle_minutes as i32);
        s.set_uapis_key(SharedString::from(config::get_uapis_key()));
        s.set_autostart(self.cfg.autostart);
        s.set_source_index(source_index(self.cfg.wallpaper.source));
        s.set_folder(SharedString::from(self.cfg.wallpaper.folder.as_str()));
        s.set_rotate_min(self.cfg.wallpaper.rotate_minutes as i32);
        s.set_order_index(match self.cfg.wallpaper.order {
            config::RotateOrder::Random => 0,
            config::RotateOrder::Sequential => 1,
        });
        s.set_widget_list(self.widget_edit_model());
    }

    fn save_settings(&mut self) -> Result<(), String> {
        let Some(s) = &self.settings else {
            return Ok(());
        };
        let pwd1 = s.get_pwd1().to_string();
        let pwd2 = s.get_pwd2().to_string();
        let idle = s.get_idle_min().max(1) as u64;
        let key = s.get_uapis_key().trim().to_string();
        let auto = s.get_autostart();
        let src = index_source(s.get_source_index());
        let folder = s.get_folder().trim().to_string();
        let rot = s.get_rotate_min().clamp(0, 1440) as u64;
        let order = if s.get_order_index() == 1 {
            config::RotateOrder::Sequential
        } else {
            config::RotateOrder::Random
        };

        if !pwd1.is_empty() {
            if pwd1 != pwd2 {
                return Err("两次输入的新密码不一致".to_string());
            }
            config::set_password(&mut self.cfg, &pwd1);
        }
        if src == WallpaperSource::Folder && folder.is_empty() {
            return Err("请选择壁纸文件夹".to_string());
        }
        if auto != self.cfg.autostart {
            platform::autostart::set_enabled(auto).map_err(|e| format!("设置开机自启失败：{e}"))?;
            self.cfg.autostart = auto;
        }
        self.cfg.idle_minutes = idle;
        config::set_uapis_key(&mut self.cfg, &key)?;
        self.cfg.wallpaper = config::WallpaperConfig {
            source: src,
            folder,
            rotate_minutes: rot,
            order,
        };
        self.cfg.widgets = self.widget_edits.iter().map(|e| e.to_spec()).collect();
        config::save(&self.cfg).map_err(|e| format!("保存配置失败：{e}"))?;

        self.rebuild_rotation();
        if self.locked {
            self.start_wallpaper();
            self.push_widgets();
        }
        if let Some(t) = &self.tray {
            t.set_autostart_checked(self.cfg.autostart);
        }
        Ok(())
    }
}

// ---------- 回调接线 ----------

fn bind_lock_callbacks(win: &LockWindow, app: &Weak<RefCell<App>>) {
    let aw = app.clone();
    win.on_password_entered(move |text| {
        if let Some(a) = aw.upgrade() {
            a.borrow_mut().handle_password(text.as_str());
        }
    });
    let aw = app.clone();
    win.on_quit_requested(move || {
        // debug 强制退出：解锁并退出程序
        if let Some(a) = aw.upgrade() {
            a.borrow_mut().unlock();
        }
        let _ = slint::quit_event_loop();
    });
}

fn setup_tray(app: &AppRc) {
    let Ok(tray) = AppTray::new() else { return };
    tray.set_autostart_checked(app.borrow().cfg.autostart);
    let aw = Rc::downgrade(app);
    tray.on_settings_requested({
        let aw = aw.clone();
        move || {
            if let Some(a) = aw.upgrade() {
                open_settings(&a);
            }
        }
    });
    tray.on_lock_now_requested({
        let aw = aw.clone();
        move || {
            if let Some(a) = aw.upgrade() {
                a.borrow_mut().lock();
            }
        }
    });
    tray.on_autostart_toggled({
        let aw = aw.clone();
        move || {
            let Some(a) = aw.upgrade() else { return };
            let mut b = a.borrow_mut();
            let enable = !b.cfg.autostart;
            if platform::autostart::set_enabled(enable).is_ok() {
                b.cfg.autostart = enable;
                let _ = config::save(&b.cfg);
                if let Some(t) = &b.tray {
                    t.set_autostart_checked(enable);
                }
            }
        }
    });
    tray.on_quit_requested(|| {
        let _ = slint::quit_event_loop();
    });
    let _ = tray.show();
    app.borrow_mut().tray = Some(tray);
}

fn open_settings(app: &AppRc) {
    let weak = Rc::downgrade(app);
    {
        let mut b = app.borrow_mut();
        if b.settings.is_none() {
            let Ok(s) = SettingsWindow::new() else { return };
            bind_settings_callbacks(&s, &weak);
            b.settings = Some(s);
        }
        b.refresh_widget_edits();
        b.populate_settings();
        if let Some(s) = &b.settings {
            let _ = s.show();
        }
    }
}

fn bind_settings_callbacks(s: &SettingsWindow, app: &Weak<RefCell<App>>) {
    let aw = app.clone();
    s.on_save_settings(move || {
        let Some(a) = aw.upgrade() else { return };
        let result = {
            let mut b = a.borrow_mut();
            b.save_settings()
        };
        let b = a.borrow();
        if let Some(settings) = &b.settings {
            match result {
                Ok(()) => {
                    settings.set_saved_msg(true);
                    let aw2 = aw.clone();
                    let t = Timer::default();
                    t.start(TimerMode::SingleShot, Duration::from_millis(1600), move || {
                        if let Some(a) = aw2.upgrade() {
                            let b = a.borrow();
                            if let Some(s) = &b.settings {
                                s.set_saved_msg(false);
                            }
                        }
                    });
                    // 一次性浮层计时器：leak 由进程退出统一回收（数量可忽略）
                    std::mem::forget(t);
                }
                Err(e) => {
                    eprintln!("save failed: {e}");
                    settings.set_saved_msg(false);
                }
            }
        }
    });
    let aw = app.clone();
    s.on_pick_folder(move || {
        let Some(a) = aw.upgrade() else { return };
        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
            let b = a.borrow();
            if let Some(s) = &b.settings {
                s.set_folder(SharedString::from(dir.display().to_string()));
            }
        }
    });
    let aw = app.clone();
    s.on_add_widget(move |template| {
        let Some(a) = aw.upgrade() else { return };
        let kind = match template {
            1 => "note",
            2 => "text",
            _ => "clock",
        };
        let mut b = a.borrow_mut();
        b.widget_edits.push(WidgetEditData {
            kind: kind.to_string(),
            x: 0.5,
            y: 0.5,
            content: String::new(),
            hour12: false,
            date: true,
        });
        let model = b.widget_edit_model();
        if let Some(s) = &b.settings {
            s.set_widget_list(model);
        }
    });
    let aw = app.clone();
    s.on_remove_widget(move |idx| {
        let Some(a) = aw.upgrade() else { return };
        let mut b = a.borrow_mut();
        let i = idx as usize;
        if i < b.widget_edits.len() {
            b.widget_edits.remove(i);
        }
        let model = b.widget_edit_model();
        if let Some(s) = &b.settings {
            s.set_widget_list(model);
        }
    });
    let aw = app.clone();
    s.on_update_widget(move |idx, x, y, content, hour12, date| {
        let Some(a) = aw.upgrade() else { return };
        let mut b = a.borrow_mut();
        let i = idx as usize;
        if let Some(e) = b.widget_edits.get_mut(i) {
            e.x = x;
            e.y = y;
            e.content = content.to_string();
            e.hour12 = hour12;
            e.date = date;
        }
        let model = b.widget_edit_model();
        if let Some(s) = &b.settings {
            s.set_widget_list(model);
        }
    });
}

fn setup_timers(app: &AppRc) {
    // 空闲检测 + 置顶保持
    let idle_timer = Timer::default();
    {
        let aw = Rc::downgrade(app);
        idle_timer.start(TimerMode::Repeated, Duration::from_secs(2), move || {
            let Some(a) = aw.upgrade() else { return };
            let (already_locked, should_lock) = {
                let b = a.borrow();
                let settings_open = b
                    .settings
                    .as_ref()
                    .map(|s| s.window().is_visible())
                    .unwrap_or(false);
                let should = !b.locked
                    && !settings_open
                    && matches!(platform::idle::idle_ms(), Some(idle)
                        if idle >= b.cfg.idle_minutes * 60_000);
                (b.locked, should)
            };
            if should_lock {
                a.borrow_mut().lock();
            } else if already_locked {
                platform::topmost::enforce(&lock_title(), &custom::widget_title());
            }
        });
    }
    app.borrow_mut()._timers.push(idle_timer);

    // 下载结果轮询
    let fetch_timer = Timer::default();
    {
        let aw = Rc::downgrade(app);
        fetch_timer.start(TimerMode::Repeated, Duration::from_millis(500), move || {
            let Some(a) = aw.upgrade() else { return };
            let rx = {
                let b = a.borrow();
                b.fetch_rx.clone()
            };
            let msg = rx.borrow().try_recv().ok().flatten();
            if let Some(d) = msg {
                let mut b = a.borrow_mut();
                if b.locked {
                    b.apply_wallpaper(Some(d));
                }
            }
        });
    }
    app.borrow_mut()._timers.push(fetch_timer);

    // 时钟刷新
    let clock_timer = Timer::default();
    {
        let aw = Rc::downgrade(app);
        clock_timer.start(TimerMode::Repeated, Duration::from_secs(1), move || {
            let Some(a) = aw.upgrade() else { return };
            let b = a.borrow();
            if b.locked {
                b.push_widgets();
                let now = Local::now();
                custom::update_clock(
                    &b.custom_widgets,
                    &now.format("%H:%M").to_string(),
                    &now.format("%-m月%-d日").to_string(),
                );
            }
        });
    }
    app.borrow_mut()._timers.push(clock_timer);
}
