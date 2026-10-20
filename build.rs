use std::{collections::HashMap, path::PathBuf};

fn main() {
    // Material 3 样式（浅色变体）：界面按浅色设计，不随系统深色模式变化
    std::env::set_var("SLINT_STYLE", "material-light");
    // 注册 lucide 图标库供 .slint 使用：import { IconDisplay, IconSet } from "@lucide";
    let library = HashMap::from([("lucide".to_string(), PathBuf::from(lucide_slint::lib()))]);
    let config = slint_build::CompilerConfiguration::new().with_library_paths(library);
    slint_build::compile_with_config("src/app.slint", config).expect("Slint build failed");
}
