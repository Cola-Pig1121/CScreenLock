# CScreenLock — 跨平台锁屏

Rust + [Slint](https://slint.dev)（Material 3 风格 + [lucide](https://lucide.dev) 图标）编写的屏幕锁定工具：无操作达到设定时间后，全部显示器进入全屏置顶锁屏，必须输入密码才能继续使用。程序常驻系统托盘。

## 功能

- **空闲自动锁屏**：默认 10 分钟无操作后锁定（可在设置中调整 1~1440 分钟），锁定所有显示器并持续保持置顶。
- **壁纸三来源**（设置 → 壁纸设置）：
  - `uapis 随机图片`：`https://uapis.cn/api/v1/random/image?category=acg&type=pc`（免费接口）；
  - `本地文件夹轮换`：选择文件夹后按 随机/顺序 轮换其中的 jpg/png/webp；
  - `纯色背景`：不加载图片。
  - 轮换间隔可设 0~1440 分钟（0 = 不轮换）。
- **密码解锁**：锁屏按 `Enter` → 背景切换为磨砂玻璃（预模糊壁纸 + 半透明叠加）并浮现 M3 风格密码输入框（真实输入框，光标闪烁、支持 IME，眼睛图标切换明文），输入密码按 `Enter` 或点击"解锁"。密码以加盐 SHA-256 哈希存储。
- **内置锁屏组件**（设置 → 锁屏组件）：「屏保时钟」「屏保备忘录」「自定义文本」模板，可添加多个、调整位置、编辑内容；也可在配置目录 `widgets.d/*.json` 声明式添加。
- **自定义控件（写 Slint 代码）**：把 `.slint` 文件放入配置目录 `custom-widgets/`，下次锁屏由 `slint-interpreter` 运行时编译，作为独立置顶小窗渲染在锁屏上。组件约定：
  ```slint
  export component Widget inherits Window {
      in property <float> x-ratio: 0.85;   // 屏幕位置比例 0~1
      in property <float> y-ratio: 0.06;
      in property <string> time;            // 宿主每秒注入 HH:MM
      in property <string> date;            // 宿主每秒注入 日期
      preferred-width: 220px; preferred-height: 110px;
      // ...自由绘制
  }
  ```
  示例见 [`samples/`](samples/)（时钟卡片、进度条卡片）。
- **托盘常驻**：右键 → 设置 / 立即锁定 / 开机自启动 / 退出。
- **开机自启动（可选）**：Windows 注册表 Run / macOS LaunchAgent / Linux XDG autostart，启动时以系统实际状态同步勾选。
- **uapis 密钥（可选）**：免费接口可不填；填写后保存在**系统密钥环**（Windows 凭据管理器 / macOS Keychain / Linux Secret Service），config.json 不落明文；也支持环境变量 `SCREENLOCK_UAPIS_KEY` 优先。请求时以 `x-api-key` 头发送。

## 平台支持

| 平台 | 状态 |
|---|---|
| Windows | 完整支持（已实测） |
| Linux (X11) | 代码级支持（托盘/空闲/自启/置顶/多屏），未经实机验证 |
| Linux (Wayland) | 空闲检测依赖 ScreenSaver DBus；窗口层级由合成器决定 |
| macOS | 代码级支持（托盘/空闲/自启/多屏），未经实机验证 |

## 构建

```bash
cargo build --release            # 产物 target/release/screen-lock(.exe)
cargo build                      # debug：锁屏右下角额外有「强制退出」按钮
```

UI 固定 Material 3 浅色样式（`material-light`），不随系统深色模式变化；图标库 `lucide-slint` 经 build.rs 注册为 `@lucide`。

## 配置

`<系统配置目录>/ScreenLock/config.json`（Windows: `%APPDATA%\ScreenLock\`）：

```json
{
  "idle_minutes": 10,
  "password_hash": "<加盐SHA-256>",
  "password_salt": "<随机盐>",
  "autostart": false,
  "uapis_key": "",
  "wallpaper": { "source": "api", "folder": "", "rotate_minutes": 10, "order": "random" },
  "widgets": [ { "kind": "clock", "x": 0.5, "y": 0.32, "props": { "hour12": "false", "date": "true" } } ]
}
```

首次运行自动生成，**默认解锁密码 `123456`**，请打开设置修改。v1 配置自动迁移；旧配置中的明文 uapis 密钥会自动迁移到系统密钥环并清除。

目录结构：

```
ScreenLock/
├── config.json          主配置
├── widgets.d/*.json     声明式组件实例（扩展）
└── custom-widgets/*.slint  自定义控件（Slint 代码）
```

## 忘记密码 / 被卡住

release 版锁屏仅密码可解。结束进程即可解除：

```
taskkill /IM screen-lock.exe /F        # Windows
pkill -f screen-lock                   # macOS / Linux
```

删除配置文件后重启，密码重置为默认 `123456`。

## 已知边界

- 「软锁定」：不拦截 `Ctrl+Alt+Del`、`Win+L` 等系统级热键。
- 图片下载失败时锁屏正常显示（黑底），不影响解锁。
- 自定义控件窗口 v1 固定放在主显示器；编译失败的文件跳过并在控制台输出诊断（debug 版可见）。
