# yuka-rs

用 Rust 重实现 Yuka System 的项目，面向后续复用原游戏资源的跨平台运行时。
当前提供 Windows 运行时和 Android 宿主，直接执行原始 YKS 脚本，读取 YKC 内的背景、立绘与 Ogg 音频，支持标题、选择支和存读档。

项目位置为 `E:\llvm-agent\yuka-rs`。本轮以用户提供的《情侣热恋中》汉化版作为真实样本。
格式和 VM 位于 library crate，桌面宿主使用 minifb、fontdue、image 和 rodio。

## 《情侣热恋中》完整游戏 APK

`bin/Loverable-Yuka-Rs-0.2.4.apk` 将 Rust 引擎、本机汉化版的五个原始 `data*.ykc` 与适配 Android 的 OP 视频一起打包。
应用包名为 `org.yukars.loverable`，最低版本为 Android 8.0，包含 `arm64-v8a` 和 `x86_64`。
APK 约 2.97 GiB，使用开发签名，构建后校验 APK 签名和 16 KiB 对齐。
资源采用 XZ preset 3 无损压缩，由 release 模式 Rust 解码器流式解压，替代旧版的纯 Java 解码。
首次启动会显示解包进度、速度与估计剩余时间，将资源流式恢复到应用私有目录并核对原文件的
长度及 SHA-256，完成后进入游戏标题。后续启动复用已完成的资源，解包中断后可重新打开应用继续准备。

安装 APK 后打开“情侣热恋中”，等待首次资源准备完成，再选择“开始游戏”。已有的 PC 剧情存档可从
标题“选项设置”或剧情底部 SYSTEM 的安卓工具中导入。应用继续使用下方说明的原版二进制存档格式，
安卓工具和暂停菜单提供完整 `Save` 目录导出。

0.2.2 的标题按钮、剧情文本框、底部按钮、普通选择项和存读档界面使用原 YKG 资源。
存读档按原脚本显示 20 页、每页 5 行，支持原 BMP 缩略图、章节、日期、备注与保存/覆盖/读取确认。
页码对应的文件编号为 `pageIndex * 10 + row`，例如第二页为 `savedata.011` 至 `savedata.015`。
001 自动存档槽保持原菜单的写保护，页码与行号保存到 `System.dat` 的 GlobalFlag85/86。
备注输入使用手机键盘并按原 GBK 编码写入原存档的 comment 字段。

0.2.3 实现右侧原版按钮的自动阅读、已读/全部快进和对话履历。自动阅读按正文长度留出阅读时间，
等待当前语音真正播放结束后再推进；快进跳过等待与转场，在未读页面（已读模式）、选择支和剧情结束时停止。
点击普通画面可停止自动/快进，打开菜单、履历或进入后台也会停止。已读状态使用原版 `Already.dat`，
进入标题不预读剧情。履历使用原版背景、按钮和语音图标，支持滑动、翻页、拖动滚动条和语音重听；
保存当前会话最近500条，开始新游戏或读档后重置，不改变原版存档文件结构。
完整游戏 APK 的图标从原版 `Loverable_cn.exe` 提取。

0.2.4 将右侧四个设置入口接到原版八页：画面、文本两页、声音、语音四页。所有单选、滑块、
静音和试听按钮都生效，设置写入原版 `System.dat`，旧剧情读档不覆盖当前设置。
支持逐字显示/点击显示全文、字体与描边/阴影、自动速度、快进范围、选择后继续、点击中断语音、
透明度、各类音量、逐角色音量与语音期间降低BGM。快进按钮按设置直接启用，长按可切换范围。
安卓返回键和外接鼠标右键按设置打开选项或隐藏文本；全屏/窗口对应 Android 系统栏显隐。
字体用 Android 的黑体/宋体家族替代，普通转场质量和BGM自动降低的幅度仍有宿主近似，见
`docs/android-settings.md`。菜单动画使用序列首帧，`selectM` 的特殊字号仍需进一步逐帧对照。

标题包含原版姓名输入、随机标题语音和BGM。附加内容支持80组CG及其差分、20段回想、29首BGM，
按原版全局进度解锁，不会自动解锁。回想执行原脚本，结束后返回鉴赏，回想中禁用手动存档。
设置右上角的“安卓工具”提供单档导入、完整 PC Save 目录导入/备份/导出与独立 OP 播放。
原版 `savedata.000` 记录上一个选择点，菜单可返回重选。

OP 源文件为游戏外置 `avi/lave_op.mpg`，构建时转为800×600 H.264/AAC，原文件不修改。
MP4直接通过 APK 文件描述符播放，无需首次解压；支持暂停/恢复、跳过和自然结束，剧情等待真实播放完成。

设备首次解包后的五个 YKC 文件 SHA-256 均与 PC 原包一致。完整 APK 已通过标题与新游戏点击、
快速存读档、后台自动保存后重启继续、PC 原档与回存档读取续行、选择支两个分支恢复及音频焦点回归。
资源准备中断后的恢复和已完成资源的复用也已检查。应用与展开资源合计约占 7.60 GB，另存的 APK 文件占用额外空间。
ARM64 当前完成交叉编译，实机运行、扬声器输出和完整路线的逐帧音画对照仍待验证。
OP 视频及特效边界见下方“当前演示边界”。

### 构建完整游戏 APK

以下命令在项目根目录的 PowerShell 7 中执行。资源准备需要 Python 3，EXE 图标提取需要 pefile 和 Pillow，Android 构建需要 SDK Platform、
Build Tools、NDK、Java 工具链及 XZ for Java 1.12。OP转换还需FFmpeg（也可使用 imageio-ffmpeg 提供的可执行文件）。构建脚本默认读取
`target/android-libs/xz-1.12.jar`，也可通过 `-XzJar` 指定位置。依赖采用 0BSD 许可证。
[XZ for Java](https://tukaani.org/xz/java.html)

```powershell
New-Item -ItemType Directory -Force -Path .\target\android-libs | Out-Null
curl.exe --fail --location 'https://repo.maven.apache.org/maven2/org/tukaani/xz/1.12/xz-1.12.jar' --output .\target\android-libs\xz-1.12.jar

$game = 'D:\gal\love loabe\【PC】情侣热恋中\情侣热恋中（ラブラブル)'
python .\android\prepare-game.py $game .\target\bundled-loverable\assets
python .\android\prepare-video.py $game .\target\bundled-loverable\assets
.\android\build.ps1 `
    -SdkPlatform .\target\android-sdk\android-35 `
    -BuildTools .\target\android-sdk\android-15 `
    -Ndk .\target\android-sdk\android-ndk-r27c `
    -JavaHome 'E:\DevEco Studio\jbr' `
    -IconExecutable (Join-Path $game 'Loverable_cn.exe') `
    -BundleDirectory .\target\bundled-loverable\assets
```

资源准备会校验每个 XZ 文件解压后的长度与 SHA-256，再写入资源清单。构建完成后校验 APK 签名和对齐。
本次五个原始包合计 4,417,273,802 字节，XZ 数据合计 3,136,805,240 字节，主机重解压后的五份 SHA-256
均与原包一致。APK 校验值和设备测试记录见 `android/TESTING.md`。
本地成品与资源构建产物位于已忽略的 `bin/` 和 `target/`，源仓库记录打包代码和验证说明。
安装及存档测试命令见 `android/TESTING.md`。

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

## Android 与原版存档

Android 宿主使用 Java 触摸界面、MediaPlayer 音频输出和 Rust JNI，最低版本为 Android 8.0。
通用播放器构建包含 `arm64-v8a` 和 `x86_64` 两种 ABI，当前产物路径为 `bin/Yuka-Rs-0.2.4.apk`。
构建脚本为 `android/build.ps1`，需要传入 SDK Platform、Build Tools、NDK 和 Java 工具链目录。

通用播放器安装后从“选项设置”中的安卓工具选择“导入游戏文件夹”，选中包含 `data*.ykc` 的游戏目录。
导入器把资源、已有 `Save` 和可选 `movies/*.mp4` 复制到应用私有存储，并在文档提供者支持时保留文件修改时间。
标题的“继续游戏”打开原版读档页。触摸推进对白，选择支直接点选，返回键打开暂停菜单。
暂停菜单可存读档，并可通过“导出完整存档目录”导出供 PC 使用的存档文件。

Android 使用原版 `savedata.NNN`、`System.dat` 和 `Already.dat` 二进制格式。
自动存档使用原脚本的 `savedata.001`，快速存档使用原版 `savedata.999`。
读取快速档时优先999，缺失时兼容早期 APK 的099槽。其他存档文件和后台恢复档可从安卓工具读取。
原格式桥接保存脚本 PC、跨文件和本地调用帧、Flag/String、图层、正文、姓名、计时器和声音状态。
GlobalFlag/GlobalString 与已读记录独立保存，读取较早的剧情槽时保留当前全局进度。
导出整个 `Save` 目录后，可把这些文件复制到相同汉化版本的 PC 游戏存档目录。

当前原格式兼容性以本机《情侣热恋中》汉化版样本为范围。已完成原档读取、40 页推进后的再次保存/读取、
新游戏无模板导出、真实选择支保存/恢复，以及全局整数、中文字符串、已读标记和失败加载回归。
隔离运行的原版 PC 程序已加载真实选择档 `savedata.006`，点击首项后回到调用章节并继续通过 11 个对白等待点。
通用播放器 0.2.0 在 Android 15 x86_64 模拟器中已完成 Android 生成 `savedata.005`、原版 PC 加载并推进、PC 回存 `savedata.021`、
Android 再读取和继续剧情的完整往返。原档读回再保存的正文、姓名、选择和图层帧一致性检查通过。
同一 APK 已恢复真实选择档 `savedata.006` 的两个分支，并通过音频焦点恢复测试和实际点击检查。
原 PC 完成选择后写出的 `savedata.022` 也已在 Android 读取、再保存和继续剧情。
ARM64 实机、扬声器输出及不同厂商设备的生命周期行为尚待验证。
原版字形、文字布局与转场效果仍沿用下述运行时边界。活跃子 Sprite、未支持的变换和粒子状态会报告具体错误。
原生结构与测试依据见 `docs/native-save.md`、`docs/native-save-runtime.md` 和 `android/TESTING.md`。

```powershell
$env:YUKA_TEST_GAME = 'path-to-the-supplied-game'
cargo test --lib runtime::native_persistence::tests -- --ignored
```

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

Windows 消息框为自绘布局，Android 界面采用上文所述原图布局，正文按设置逐字显示，字体效果可调整。
普通转场与蒙版转场使用交叉淡化，
`Effect` 的1/5模式使用逆向确认的纵向/二维震动轨迹。文字框移动、背景滚动和音频异步淡出已有实现。
剧情音频按所选声音组和脚本局部音量相乘。设备不可用时显示静音状态。

Windows/headless 的 `MoviePlay` 仍跳过并记录到报告，Android 使用上述真实视频播放。
其他作品验证仍待扩展；当前设置和鉴赏实现针对本机这份《情侣热恋中》汉化版。
Windows JSON 存档中的变量状态随该存档恢复，Android 原格式存档将全局变量和已读记录单独持久化。
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
发布版构建、61 项默认测试（55 项 library 测试和 6 项原格式 codec 测试）、6 项 Windows JSON 存档回归、
4 项原格式 runtime 回归、1 项原档 codec 实数据回归、`cargo fmt --check` 和 Clippy 零警告检查通过。
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
- `docs/native-save.md` 与 `docs/native-save-runtime.md` 记录原版存档格式、运行时还原和互通测试范围。
- `src/android.rs` 与 `android/` 提供 Android JNI、界面、音频、资源导入和 APK 构建。
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
4. 扩展 Android 实机、音频与生命周期回归。

## 许可证

代码采用 GPL-3.0-only。实现参考了 GPL-3.0 的 YukaTool 2 和 MIT 的 GARbro，
来源及声明保存在 `THIRD_PARTY_NOTICES.md`，完整 GPL 文本在 `LICENSE`。
