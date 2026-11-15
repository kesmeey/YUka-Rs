# yuka-rs

用 Rust 重实现 Yuka System 的项目，面向后续复用原游戏资源的跨平台运行时。
当前已提供可交互的 Windows 序章 demo，直接执行原始 YKS 脚本，读取 YKC 内的背景、立绘与 Ogg 音频。

项目位置为 `E:\llvm-agent\yuka-rs`。本轮以用户提供的《情侣热恋中》汉化版作为真实样本。
格式和 VM 位于 library crate，桌面宿主使用 minifb、fontdue、image 和 rodio。

## 运行 Demo

双击项目目录中的 `run-demo.cmd`。已编译程序位于 `target/release/yuka-rs.exe`。
启动器默认读取上述游戏目录，可将其他目录作为第一个参数传入。

- 左键、空格或 Enter 推进对白。
- H 隐藏或恢复文字。隐藏状态下再次推进会先恢复文字。
- Esc 或窗口关闭按钮退出。窗口可调整大小，保持 4:3 画面。
- 默认播放序章前 100 个 `KeyWait` 页面，到达结束页后停留供查看。

本 demo 从 `yks\story\common\PROLOGUE1.YKS` 开始，采用原脚本默认主角名“爱泽晴树”与手动推进模式。
已覆盖海边开场、卧室与早餐场景、透明立绘、中文姓名和对白、滚动、交叉淡化、BGM、环境音和语音。
字体默认使用本机微软雅黑，可通过 `--font` 指定其他包含中文字形的 TTF/TTC。

```powershell
Set-Location 'E:\llvm-agent\yuka-rs'
$game = 'D:\gal\love loabe\【PC】情侣热恋中\情侣热恋中（ラブラブル)'
.\target\release\yuka-rs.exe demo $game

# 关闭音频，立即完成场景等待与转场，仍逐页点击推进
.\target\release\yuka-rs.exe demo $game --mute --fast

# 自动执行相同的前 100 页，并输出末帧与执行统计，文件目标应为新路径
.\target\release\yuka-rs.exe demo $game --headless --pages 100 --screenshot demo.png --report demo.json
```

`--pages` 调整演示页数，`--script` 可更换研究入口。已验证的可播放范围为当前汉化样本前 100 页。
`--headless` 关闭设备输出并跳过真实时间等待，通过同一个 VM、资源解码和宿主实现执行。

### 当前演示边界

消息框为自绘布局，正文即时显示，字体效果统一。普通转场与蒙版转场使用交叉淡化，
`Effect` 屏幕震动暂未实现。文字框移动、背景滚动和音频异步淡出已有实现。
音频统一使用 65% 主音量，再乘脚本设置的局部音量。设备不可用时显示静音状态。

标题菜单、选择支、存读档、视频、完整故事兼容性、其他作品验证和 Android APK 仍待实现。
Android 接入还需拆分桌面依赖并提供窗口、触摸、音频和资源访问宿主。
执行报告中的 `approximations` 记录本次路径使用到的效果近似。

## 已实现与验证范围

| 模块 | 当前能力 | 真实样本验证 |
| --- | --- | --- |
| `archive` | YKC001 / YKC002 目录、名称解码、边界检查、流式读取 | 5 个 YKC001 包全部读取成功。YKC002 使用合成测试 |
| `script` | YKS001 数据解混淆、段和符号表、符号指令解码、文字编码选项 | 175 个脚本，312,185 条指令全部通过结构检查 |
| `graphics` | YKG000 主图、alpha、动画记录，GNP 转标准 PNG 签名 | 7,403 个图像全部通过结构检查，1,334 条动画记录 |
| `vm` | 赋值、整数运算与比较、字符串拼接、if/else、调用/跳转/返回、共享变量和独立局部帧 | 序章前 100 页与四个传递依赖脚本连续执行 |
| `runtime` / `desktop` / `sound` | 跨脚本调度、图层合成、中文排版、输入、等待、滚动、交叉淡化、Ogg 播放和异步淡出 | 100 页自动执行，首屏与立绘截图检查，Windows 窗口运行 |
| 原生逆向 | Windows API 依赖、命令注册表、关键分发器与处理函数地址 | 339 个注册项，脚本引用的 142 个函数名全部匹配 |

100 页演示执行 1,264 步，加载 5 个脚本，解码 26 次图像资源和 56 次音频资源。
发布版构建、25 项默认测试、`cargo fmt --check` 和 Clippy 零警告检查通过。
另有一项依赖本地全部游戏图像的测试，默认忽略，初始格式验证阶段已单独运行通过。
音频计数表示建立解码器并确认首批样本，实际播放由音频线程继续解码。
前 100 页最长单页正文为 47 字符。真实样本截图覆盖海景、立绘 alpha 与中文姓名。
`verify` 单独验证格式结构及 Ogg capture pattern，完整原引擎行为一致性还需逐项比对。

`trace` 保留研究用途的严格初始化策略，停止原因明确显示未实现操作或宿主调用。
`Start.yks` 跟踪产生 12 个事件，
停在 code word 43 的 `ScriptCall("yks\\etc\\HP_Install.yks")`，停止原因是 `host_call`。

## 构建与使用

本机已有 Rust 工具链，依赖版本记录在 `Cargo.lock`。

```powershell
Set-Location 'E:\llvm-agent\yuka-rs'
cargo build --release --locked
cargo test --locked

$game = 'D:\gal\love loabe\【PC】情侣热恋中\情侣热恋中（ラブラブル)'
$exe = '.\target\release\yuka-rs.exe'

# 所有包的结构概览
& $exe inspect $game

# 列出原始脚本成员
& $exe list "$game\data01.ykc"

# 直接读取包内启动脚本并反汇编
& $exe script "$game\data01.ykc" --member 'Start.yks' --encoding gbk

# 单步执行到第一个宿主调用或未实现行为
& $exe trace "$game\data01.ykc" --member 'Start.yks' --encoding gbk

# 全量结构检查，JSON 中保留每个失败的包名、成员名和原因
& $exe verify $game --encoding gbk

# 将单个原始成员导出到新文件
& $exe extract "$game\data01.ykc" 'Start.yks' '.\extracted-start.yks'

# 检查并导出已知系统图，输出目录必须尚不存在
& $exe graphics "$game\data02.ykc" --member 'ykg\system\white.ykg' --output-dir '.\extracted-white'
```

成员名保留原始拼写，查找时兼容斜杠差异和 ASCII 大小写差异。
`--encoding` 可选 `shift-jis`、`gbk`、`utf8`。`demo` 默认 `gbk`，研究命令默认 `shift-jis`。
这份汉化版的启动标题用 `gbk` 可正确解码。解析器同时保留原始文字字节和解码错误标记，
其他作品及汉化补丁应按其实际编码选择。

`script`、`trace`、`graphics` 支持直接读取独立文件，或通过 `--member` 选择包内成员。
单个内存读取默认上限为 256 MiB，可用 `--max-bytes` 调整。`extract` 使用流式复制。
所有命令只读输入。导出命令拒绝覆盖已有目标。

## 代码和研究记录

- `src/archive.rs` 与 `docs/ykc.md` 对应归档结构。
- `src/script.rs`、`src/vm.rs` 与 `docs/yks.md` 对应脚本和执行语义。
- `src/graphics.rs` 与 `docs/ykg.md` 对应图像容器和动画记录。
- `src/runtime.rs`、`src/resources.rs`、`src/sound.rs`、`src/desktop.rs` 组成桌面 demo。
- `docs/demo-semantics.md` 记录序章入口、默认状态、相关原生命令依据和近似行为。
- `docs/native-analysis.md` 记录原程序身份、函数地址、静态语义和复现方法。
- `evidence/` 保存本机的完整 REA 输出、原生命令表、验证结果和启动脚本跟踪。

本机验证结果包括 `evidence/archive-inventory.json`、`evidence/game-verification.json`、
`evidence/command-coverage.json` 和 `evidence/start-trace.json`。
Git 忽略 `evidence/`、`references/`、`extracted/` 和构建输出。

REA 已完成 PE import 检查。该安装尚未配置深度反编译 provider，原生命令表与选定函数的
进一步分析使用现有 `pefile` 和 Capstone，证据文件分别标明工具来源。

## 后续引擎开发

1. 扩展标题菜单、选择支和完整故事路径的命令覆盖。
2. 比对原版转场、逐字显示、字体和音频组设置。
3. 实现存读档，并增加其他 Yuka 版本的真实回归样本。
4. 拆分桌面依赖，通过安卓宿主接入窗口、图形、音频、触摸和资源目录访问。

## 许可证

代码采用 GPL-3.0-only。实现参考了 GPL-3.0 的 YukaTool 2 和 MIT 的 GARbro，
来源及声明保存在 `THIRD_PARTY_NOTICES.md`，完整 GPL 文本在 `LICENSE`。
