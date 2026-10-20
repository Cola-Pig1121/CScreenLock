use chrono::{DateTime, Local};

use crate::config::WidgetSpec;

/// 组件渲染数据（送 Slint 的属性）
#[derive(Debug, Clone)]
pub struct WidgetView {
    pub kind: String,
    pub x: f32,
    pub y: f32,
    pub title: String,
    pub subtitle: String,
}

const WEEKDAYS: [&str; 7] = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"];

/// 时钟/备忘录/文本组件 → 显示内容
pub fn render(spec: &WidgetSpec, now: DateTime<Local>) -> WidgetView {
    match spec.kind.as_str() {
        "clock" => {
            let hour12 = spec.prop_bool("hour12", false);
            let time = if hour12 {
                let h = now.format("%I:%M").to_string();
                let ampm = if now.format("%p").to_string() == "AM" { "上午" } else { "下午" };
                let h = h.trim_start_matches('0').to_string();
                format!("{ampm}{h}")
            } else {
                now.format("%H:%M").to_string()
            };
            let show_date = spec.prop_bool("date", true);
            let date = if show_date {
                let wd = WEEKDAYS[now.format("%u").to_string().parse::<usize>().unwrap_or(1) - 1];
                format!("{}月{}日 {}", now.format("%-m"), now.format("%-d"), wd)
            } else {
                String::new()
            };
            WidgetView {
                kind: spec.kind.clone(),
                x: spec.x,
                y: spec.y,
                title: time,
                subtitle: date,
            }
        }
        "note" => {
            let content = spec.prop("content", "（在设置中编辑备忘录内容）");
            let title = spec.prop("title", "备忘录");
            WidgetView {
                kind: spec.kind.clone(),
                x: spec.x,
                y: spec.y,
                title,
                subtitle: content,
            }
        }
        // text 及未来扩展的通用模板：title 直接显示 content
        _ => WidgetView {
            kind: spec.kind.clone(),
            x: spec.x,
            y: spec.y,
            title: spec.prop("content", ""),
            subtitle: spec.prop("subtitle", ""),
        },
    }
}
