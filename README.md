# yuka-rs

用 Rust 重实现 Yuka System 的项目，面向后续复用原游戏资源的跨平台运行时。
当前提供 Windows 运行时，直接执行原始 YKS 脚本，读取 YKC 内的背景、立绘与 Ogg 音频，支持标题、选择支和存读档。

项目位置为 `E:\llvm-agent\yuka-rs`。本轮以用户提供的《情侣热恋中》汉化版作为真实样本。
格式和 VM 位于 library crate，桌面宿主使用 minifb、fontdue、image 和 rodio。

## 交互式运行与存档

双击 `run-game.cmd` 启动带标题菜单的运行入口。本机最新预编译程序为 `bin/yuka-rs.exe`。
启动器优先使用此程序，也可使用标准 Cargo 输出 `target/release/yuka-rs.exe`。

```powershell
.\bin\yuka-rs.exe play 'D:\gal\love loabe\【PC】情侣热恋中\情侣热恋中（ラブラブル)' --save-dir .\saves
```

标题使用原游戏背景和 logo，菜单由本运行时绘制。“开始游戏”从原版 `root_Common.yks` 调度脚本进入，
页面数量开放。剧情完整兼容性按下方验证范围持续扩展。

- Esc 打开暂停菜单，冻结场景计时和音频，菜单提供继续、存档、读档和返回标题。
- F5 快速存档，F9 读取快速档。另提供三个手动存档槽和原脚本请求的自动存档。
- 存档在对白或选择等待且动画完成时可用。选择支支持方向键、Enter 和鼠标。
- 左键、空格、Enter 推进，H 隐藏文字。窗口关闭按钮退出。

启动器把存档写入项目 `saves/<game-id>/`，按资源目录索引标识区分游戏。存档使用 yuka-rs JSON 格式，
保存跨文件调用栈、本地帧、变量、图层、对白、选择及随机数状态。读取会核对资源目录索引和脚本内容指纹，
验证完成后整体替换会话。存档文件通过临时文件和重命名原子写入。音频在读档后从该资源开头播放。

六项本地集成测试覆盖 40→60 页回档重放、540→550 页跨序章返回、真实选择的两个分支重选、
外部调用栈、连续覆盖和损坏存档拒绝。比较内容包括图层像素、位置、滤镜、文字和脚本位置。

## 连续剧情回归

以下测试使用本地汉化样本，跳过实时等待和设备输出，每次选择第一个选项。全部执行到原脚本的
尾声结束和返回标题跳转，报告 `end_reason` 为 `return_to_title`。

| 入口与测试路径 | 对白等待页数 | 脚本步数 | 选择次数 |
| --- | ---: | ---: | ---: |
| `root_Common` → 公共线五章 → `root_Tinatu` → 尾声 | 11,779 | 235,275 | 33 |
| `root_Kaho` → 尾声 | 4,656 | 96,193 | 13 |
| `root_Tugumi` → 尾声 | 4,631 | 83,330 | 15 |
| `root_Nanako` → 尾声 | 4,756 | 80,149 | 15 |
| `root_Satuki` → 尾声 | 2,871 | 57,577 | 11 |

测试覆盖章节调度、普通及外部 YKG 加载、色彩滤镜、透明度渐变、多个循环 Sprite、演职员表计时和尾声。
真实播放使用同一套 VM 和命令实现。全部选项组合及完整流程的逐帧音画对照仍待验证。
窗口已检查标题布局、早期剧情、立绘和中文。桌面操作停止后，F5/F9 的实际按键复测保留待验。

```powershell
$game = 'D:\gal\love loabe\【PC】情侣热恋中\情侣热恋中（ラブラブル)'
$choices = ('1,' * 99) + '1'
.\bin\yuka-rs.exe demo $game --script 'yks\root\root_Common.yks' --headless --pages 20000 --choices $choices --report complete.json

$env:YUKA_GAME_DIR = $game
cargo test --test persistence -- --ignored
```

未提供足够的 `--choices` 时，headless 在下一个选择支处结束本次探测，并把选项写入报告。

## 运行 Demo

双击项目目录中的 `run-demo.cmd` 运行 100 页回归演示。
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

`--pages` 调整演示页数，`--script` 可更换研究入口，完整路径验证范围见上表。
`--headless` 关闭设备输出并跳过真实时间等待，通过同一个 VM、资源解码和宿主实现执行。

### 当前演示边界

消息框为自绘布局，正文即时显示，字体效果统一。普通转场与蒙版转场使用交叉淡化，
`Effect` 屏幕震动暂未实现。文字框移动、背景滚动和音频异步淡出已有实现。
音频统一使用 65% 主音量，再乘脚本设置的局部音量。设备不可用时显示静音状态。

`MoviePlay` 当前跳过 OP 视频并记录到报告。原版设置/鉴赏菜单、独立全局解锁存储、
其他作品验证和 Android APK 仍待实现。存档中的变量状态会随该存档恢复。
Android 接入还需拆分桌面依赖并提供窗口、触摸、音频和资源访问宿主。
执行报告中的 `approximations` 记录本次路径使用到的效果近似。

## 已实现与验证范围

| 模块 | 当前能力 | 真实样本验证 |
| --- | --- | --- |
| `archive` | YKC001 / YKC002 目录、名称解码、边界检查、流式读取 | 5 个 YKC001 包全部读取成功。YKC002 使用合成测试 |
| `script` | YKS001 数据解混淆、段和符号表、符号指令解码、文字编码选项 | 175 个脚本，312,185 条指令全部通过结构检查 |
| `graphics` | YKG000 主图、alpha、动画记录，GNP 转标准 PNG 签名 | 7,403 个图像全部通过结构检查，1,334 条动画记录 |
| `vm` | 整数运算与比较、字符串拼接、if/else、Select/Case、跨脚本调用/跳转/返回、状态快照 | 公共线连贯路径、五个角色入口、存档重放与分支回归 |
| `runtime` / `desktop` / `sound` | 图层/子 Sprite 合成、三种滤镜、中文、菜单、存读档、计时、滚动、淡化、Ogg 与音量渐变 | 上述五条路径全部返回标题，首屏与立绘截图检查，Windows 窗口运行 |
| 原生逆向 | Windows API 依赖、命令注册表、关键分发器与处理函数地址 | 339 个注册项，脚本引用的 142 个函数名全部匹配 |

100 页演示执行 1,264 步，加载 5 个脚本，解码 26 次图像资源和 56 次音频资源。
发布版构建、38 项默认测试、6 项本地存档集成测试、`cargo fmt --check` 和 Clippy 零警告检查通过。
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
输入资源以只读方式打开。导出命令拒绝覆盖已有目标，游戏存档槽可原子覆盖。

## 代码和研究记录

- `src/archive.rs` 与 `docs/ykc.md` 对应归档结构。
- `src/script.rs`、`src/vm.rs` 与 `docs/yks.md` 对应脚本和执行语义。
- `src/graphics.rs` 与 `docs/ykg.md` 对应图像容器和动画记录。
- `src/runtime.rs`、`src/resources.rs`、`src/sound.rs`、`src/desktop.rs` 组成桌面 demo。
- `docs/demo-semantics.md` 记录序章入口、默认状态、相关原生命令依据和近似行为。
- `docs/playback-semantics.md` 记录连续故事、选择、滤镜、图集动画和演职员表的原生依据。
- `docs/native-analysis.md` 记录原程序身份、函数地址、静态语义和复现方法。
- `evidence/` 保存本机的完整 REA 输出、原生命令表、验证结果和启动脚本跟踪。

本机验证结果包括 `evidence/archive-inventory.json`、`evidence/game-verification.json`、
`evidence/command-coverage.json` 和 `evidence/start-trace.json`。
Git 忽略 `evidence/`、`references/`、`extracted/` 和构建输出。

REA 已完成 PE import 检查。该安装尚未配置深度反编译 provider，原生命令表与选定函数的
进一步分析使用现有 `pefile` 和 Capstone，证据文件分别标明工具来源。

## 后续引擎开发

1. 扩展完整故事路径与原版设置菜单的命令覆盖。
2. 比对原版转场、逐字显示、字体和音频组设置。
3. 扩展存档兼容性检查，增加其他 Yuka 版本的真实回归样本。
4. 拆分桌面依赖，通过安卓宿主接入窗口、图形、音频、触摸和资源目录访问。

## 许可证

代码采用 GPL-3.0-only。实现参考了 GPL-3.0 的 YukaTool 2 和 MIT 的 GARbro，
来源及声明保存在 `THIRD_PARTY_NOTICES.md`，完整 GPL 文本在 `LICENSE`。
