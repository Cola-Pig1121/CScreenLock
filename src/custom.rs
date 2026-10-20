//! 自定义锁屏控件：用户在 `<配置目录>/custom-widgets/*.slint` 编写 Slint 代码，
//! 运行时由 slint-interpreter 编译并作为独立置顶小窗叠加在锁屏上。
//!
//! 组件约定：
//! - 组件名必须为 `Widget`，且 `inherits Window`；
//! - 必须声明并绑定以下属性（interpreter 无法设置 Window 内建属性）：
//!   - `in property <string> win-title;` 且 `title: win-title;`（宿主注入统一标题，供置顶识别）
//!   - `in property <float> win-w;` / `win-h;` 且 `preferred-width: win-w * 1px;`（窗口逻辑尺寸）
//! - 可选属性：
//!   - `x-ratio` / `y-ratio`: float 0..1，控件在屏幕上的位置比例（默认 0.5 居中）
//!   - `time` / `date`: string，宿主每秒注入当前时间/日期，供时钟类控件显示

use std::path::Path;

use slint::ComponentHandle;
use slint_interpreter::{ComponentCompiler, ComponentInstance, Diagnostic, Value};

use crate::platform::monitors::Monitor;

pub fn widget_title() -> String {
    format!("ScreenLock-Widget-{}", std::process::id())
}

/// 编译目录内全部 .slint 控件（失败的跳过并打印诊断），返回实例列表。
pub fn compile_all(dir: &Path) -> Vec<ComponentInstance> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("slint") {
            continue;
        }
        match compile_one(&path) {
            Ok(inst) => out.push(inst),
            Err(e) => eprintln!("[custom-widget] {} 编译失败: {e}", path.display()),
        }
    }
    out
}

fn compile_one(path: &Path) -> Result<ComponentInstance, String> {
    let mut interp = ComponentCompiler::new();
    interp.set_style("material-light".to_string());
    let def = futures::executor::block_on(interp.build_from_path(path))
        .ok_or_else(|| first_diagnostic(&interp))?;
    let instance = def
        .create()
        .map_err(|e: slint::PlatformError| e.to_string())?;
    // 统一窗口标题（经约定属性 win-title），供置顶 shim 识别
    let _ = instance.set_property("win-title", Value::String(widget_title().into()));
    Ok(instance)
}

fn first_diagnostic(interp: &ComponentCompiler) -> String {
    interp
        .diagnostics()
        .first()
        .map(|d: &Diagnostic| d.to_string())
        .unwrap_or_else(|| "未知编译错误".to_string())
}

/// 定位并显示控件窗口（放在主显示器，按 x-ratio/y-ratio 计算位置）
pub fn show_all(instances: &[ComponentInstance], mon: Option<&Monitor>) {
    let Some(mon) = mon else { return };
    for inst in instances {
        let win = inst.window();
        // 尺寸经约定属性 win-w/win-h（逻辑像素），interpreter 不自动应用 preferred-*
        let pw = get_number(inst, "win-w", 300.0);
        let ph = get_number(inst, "win-h", 200.0);
        win.set_size(slint::WindowSize::Logical(slint::LogicalSize::new(pw, ph)));
        let size = win.size();
        let xr = get_ratio(inst, "x-ratio");
        let yr = get_ratio(inst, "y-ratio");
        let x = mon.x + ((mon.w - size.width as i32) as f32 * xr) as i32;
        let y = mon.y + ((mon.h - size.height as i32) as f32 * yr) as i32;
        win.set_position(slint::WindowPosition::Physical(slint::PhysicalPosition::new(
            x, y,
        )));
        let _ = win.show();
    }
}

fn get_number(inst: &ComponentInstance, name: &str, default: f32) -> f32 {
    match inst.get_property(name) {
        Ok(Value::Number(v)) => v as f32,
        _ => default,
    }
}

fn get_ratio(inst: &ComponentInstance, name: &str) -> f32 {
    match inst.get_property(name) {
        Ok(Value::Number(v)) => (v as f32).clamp(0.0, 1.0),
        _ => 0.5,
    }
}

/// 每秒刷新时间/日期上下文（组件未声明对应属性时静默忽略）
pub fn update_clock(instances: &[ComponentInstance], time: &str, date: &str) {
    for inst in instances {
        let _ = inst.set_property("time", Value::String(time.into()));
        let _ = inst.set_property("date", Value::String(date.into()));
    }
}

/// 关闭并销毁全部控件窗口
pub fn dispose(instances: &mut Vec<ComponentInstance>) {
    for inst in instances.drain(..) {
        if let Err(e) = inst.window().hide() {
            eprintln!("[custom-widget] hide failed: {e}");
        }
    }
}
