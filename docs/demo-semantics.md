# PROLOGUE1 demo 的脚本与原生命令依据

检查日期为 2026-11-15。游戏输入为本地《情侣热恋中》汉化版，脚本采用 GBK。所有 native VA 来自 `Loverable_cn.exe` 的 PE 首选基址 `0x400000`。样本 SHA-256 为 `84aef0e66123edfdf265449292abe87b6d46aa268e66fdd219614fb436a329bf`。

本次通过现有 Rust CLI 解码原始 `data01.ykc`，再用 `pefile` 和 Capstone 检查同一 EXE 的命令处理函数。以下 native 结论是静态分析结果。

## Demo 入口与状态

入口为 `yks\story\common\PROLOGUE1.YKS`，从 codeword `0` 执行。开场先载白色图片，再载海边背景 `ykg\bg\lv_bg_24a.YKG` 到 layer 0，坐标 `(0,-710)`，播放海浪与海鸥音效。第一次 `KeyWait` 位于 word `0x2f`。前四页为海边独白，适合默认展示与截图。

脚本随后进入起床、换装展示和早餐剧情。第 100 次 `KeyWait` 位于 word `0x97f`。立绘服装分支读取 `Flag[499]`，普通初始值 `0` 进入 `SC` 或后续 `PR` 素材分支，值 `1` 进入 `HH` 素材分支。代码路径中第一个常规立绘位于 word `0x2b6`，使用 layer 5。直接进入该故事时应提供以下名字状态。

| 值 | 原始脚本依据 |
| --- | --- |
| `GlobalString[20] = "爱泽"` | `OptionDefault.yks` word `0x111` 与 `0x112` |
| `GlobalString[21] = "晴树"` | `OptionDefault.yks` word `0x115` 与 `0x116` |
| `String[20] = GlobalString[20]` | `Start.yks` word `0x1a9` 与 `0x1aa` |
| `String[21] = GlobalString[21]` | `Start.yks` word `0x1ad` 与 `0x1ae` |
| `String[22] = "爱泽晴树"` | `Start.yks` word `0x1b1` 与 `0x1b2` 拼接前两项 |
| `Flag[285] = 0` | `Start.yks` word `0x1b8` 比较默认全名，匹配后在 `0x1c2` 赋值 |

直接入口省去了原游戏的标题、设置、字体与存档初始化脚本。Demo 的名字初始化应明确写入 VM 状态。新增游戏配置时需要读取对应游戏自己的初始化流程。

## 消息窗口与传递依赖

`createText.yks` 定义了四个消息相关 layer。

| Layer | 原始配置 |
| --- | --- |
| 11 | `GraphicLoadHide(11,"ykg\\system\\window\\window.ykg",3,473)` |
| 12 | `GraphicLoadHide(12,"ykg\\system\\window\\name.ykg",0,437)` |
| 13 | `damy.ykg`，`TextWindowSet(13,98,482,712,600)`，字体编号 0 |
| 14 | `damy.ykg`，`GraphicTextRect(14,0,442,220,40)`，字体编号 1 |

`DefaultTextWindowGroup(14,13,12,11)` 将上述对象组合。`showWindow.yks` 在 `Flag[235] == 1` 时直接返回，其他情况下显示 11、12、13、14 并将 `Flag[210]` 设为 1。`hideWindow.yks` 隐藏相同对象，并把 `Flag[210]` 设为 0。

前 100 页直接调用 `showWindow.yks`、`hideWindow.yks` 和 `yks\function\Msg_Effect.yks`。`Msg_Effect` 继续调用 `yks\system\getNowMode.yks`。

`getNowMode` 用宿主函数返回值分别赋值 `Flag[261] = SkipModeCheck()`、`Flag[262] = AlreadySkipModeCheck()`、`Flag[263] = AutoModeCheck()`。三个 native handler 分别为 `0x430db0`、`0x430ec0`、`0x430e30`，返回 type 1 的整数对象，value 为对应布尔状态的 0 或 1。普通逐页点击模式可以返回 0。实现跳过与自动播放时需要同步更新这些查询结果。

`Msg_Effect` 设置基准 `(Flag[92],Flag[93])=(3,473)`、`(Flag[94],Flag[95])=(0,437)`，位移幅度 `Flag[96]=30`，等待 `Flag[97]=0`。任意跳过状态为 1 时返回，否则按 `Flag[100]` 的 1、2、3 分别选择横向、纵向、交替位移，调用 `GraphicMove(11/12,x,y)`，最后恢复基准位置。宿主需要真正返回查询值，使脚本分支正常执行。

## 已验证的命令语义

下表列出本故事需要的调用形式。对其他重载形式的支持应逐项验证。

| 命令形式 | 已验证行为与 native 依据 |
| --- | --- |
| `GraphicLoad(id,path,x,y)` | `0x439b00` 查找或创建 id，四参分支加载 path，调用 `0x413c40` 设置 x/y，再调用 `0x413c20(true)` 显示 |
| `GraphicShow(id)` / `GraphicHide(id)` | `0x432ac0` / `0x432b10` 查找 id，调用 `0x413c20(true/false)`，后者写对象可见字段 `+0xe8` |
| `GraphicDelete(id)` | `0x432c40` 读取第一个参数，隐藏对象后调用 manager 的 `0x414550` 删除该 id，缺失对象可直接结束 |
| `GraphicScroll(id,time,x,y)` | `0x431ce0` 四参分支以目标坐标减当前坐标，再分别除第二参数计算 x/y 速度，并设置移动状态。与后续 `GraphicScrollWait(id)` 组合使用 |
| `DrawStop()` | `0x433780` 保留绘制状态并设置绘制停止标志，供之后的图层修改与转场使用 |
| `DrawStart()` | `0x4338d0` 恢复绘制，部分状态触发 transition 1 |
| `Transition(mode,time)` | `0x433910` 两参分支按原顺序调用 `0x423140(mode,time,null)`。mode 2 在 `0x4234ec` 跳表中进入 `0x4232be`，随后调用 `0x4227b0` |
| `UserTransition(mode,mask,time)` | `0x433a40` 加载第二参数图像，三参分支向 `0x422ff0` 传 mode、图像、time、空矩形。两参重载默认 time=1000 |
| `StrOut(text)` | `0x441ab0` 单参数分支使用 context 的默认文本对象。更多参数按 `(id,text)` 成对处理 |
| `StrOutNW(text)` | `0x441d30` 临时把逐字延迟设为 0，调用 `StrOut` 后恢复 |
| `StrOutNWC(id,text)` | `0x441d70` 按对象字体测量文本，设置水平居中位置，临时关闭逐字延迟，再调用 `StrOut`。本故事 id=14 为姓名 |
| `NameOut(text)` | `0x441f60` 从 context `+0x10` 取得姓名 layer id，前置此参数后调用 `StrOutNWC` |
| `PF()` / `PF(id,...)` | `0x43f600` 无参遍历全部 graphics，有参逐个查找 id，调用 `0x413960` 清理文字绘制项并更新区域。背景与立绘资源继续保留 |
| `FontSet(id,font)` | `0x433140` 将 font 写入该 graphics 对象的字体编号 `+0xec` |
| `Wait(time)` | `0x434420` 对比 `WINMM!timeGetTime` 差值，单位为毫秒，并在循环中处理运行状态 |
| `SoundLoadStream(slot,path)` | `0x43a8f0` 在声音 manager 中加载该 slot，设置流式标志 `+0x2d=1` |
| `SoundPlayLoop(slot,path)` / `SoundPlayLoop(slot)` | `0x43a940` 两参先加载路径，单参查找已有声音。设置 loop 字段 `+9=1` 后调用播放函数 |
| `SoundPlay(slot,path)` / `SoundPlay(slot)` | `0x441ff0` 按声音 slot 加载或查找后播放。该故事 slot 11 用于花穗语音，slot 150 用于一次性音效 |
| `SoundVolume(slot,value)` | `0x43a9f0` 设置局部音量。`0x459320` 合并 master、group、local 值，`0x4594b0` 除以 `0x49c4f8` 处 double 常量 `1000.0`。700 对应局部 0.7 增益 |
| `SoundDeleteToFade(slot,time)` | `0x434140` 默认 time=1000，调用 `0x458870` 设置淡出标志、结束后删除标志与斜率 `-current_volume/time`，立即返回。后续脚本与淡出同时推进 |

声音编号来自游戏初始化。`Start.yks` 为 BGM 设置组范围 1–9，为音效设置范围 100–599，另有语音组初始化。故事使用 slot 1 播放 BGM，11 播放花穗语音，150 播放一次性音效，160 播放海浪、闹钟、煎锅等循环音效。每个 slot 应独立保存资源、循环标志、播放进度和音量。

两参数 `SoundPlayLoop(slot,path)` 在 `0x43a966` 先调用 `0x45afa0` 删除旧 slot，再新建声音对象。构造函数 `0x45a770` 在 `0x45a7ee` 将局部音量设为 1000。因此用新路径替换声音时，之前该 slot 的局部音量也会重置。

图形 manager 的 `0x410bf0` 以对象 `+0x60` 的 id 升序插入双向链表。绘制函数 `0x411e60` 从 manager `+8` 的表头向 `+0x5c` 遍历，并检查指定 id 范围。因此 demo 采用 id 升序合成符合此对象顺序。show/hide 的状态应与资源是否存在分别保存。

## Sprite 与其他命令

本 EXE 注册表包含 `SpriteCreate`、`SpriteCreateHide`、`SpriteShow`、`SpriteHide`、`SpriteMove`、`SpriteDelete`、`SpriteMotionWait`、`SpriteIs`。前 100 页的场景立绘通过 `GraphicLoad` 操作。

`SpriteShow(parent,sprite)` 和 `SpriteHide(parent,sprite)` 分别位于 `0x433170` 与 `0x433200`，先用第一个参数查 graphics，再在其绘制项中按第二个参数匹配 sprite id，设置该绘制项可见状态。`SpriteCreate` 位于 `0x43da40`，读取六个整数参数，第一个为目标 graphics id，第二个为其内部 sprite id，第五个为源 graphics id。剩余参数传递到 `0x408590`，完整图像单元与定位语义仍待验证。该实现需要 graphics 内部绘制项模型。

339 项命令注册表中未找到 `GraphicDraw`。当前依据来自实际注册名与脚本调用，新增别名时需要相应样本证据。

## Demo 中的近似与边界

- 当前 demo 通过标准化字体与自绘消息框显示正文和姓名。原图层 11/12 的默认窗口资源、原版字体效果和逐字排版尚待复现。`createFont.yks` 的普通正文与姓名字体高度为 25，强调字体 4/5 分别为 30/36。
- `GraphicMove(11/12,...)` 可把原始窗口坐标变化映射到自绘消息框，以执行 `Msg_Effect`。显示结果仍受自绘窗口布局影响。
- 普通 `Transition` 的各 mode、用户蒙版转场在 demo 中统一使用交叉淡化。上表确认了参数传递与图像加载，逐像素效果算法尚待比对。
- `GraphicScroll` 在 demo 中按指定时间线性插值，原版滚动曲线和逐帧时序仍待比对。`Effect` 的屏幕震动需要独立实现或在运行报告中标记。
- 上述 native 与脚本证据覆盖当前样本。跨作品兼容性、完整标题设置流程、存读档和 Android 平台运行需要后续验证。

## 可复查文件

`evidence/demo-prologue1.txt` 保存故事反汇编。`demo-showWindow.txt`、`demo-hideWindow.txt`、`demo-msg-effect.txt`、`demo-getNowMode.txt`、`demo-createText.txt`、`demo-createFont.txt`、`demo-OptionDefault.txt` 保存初始化或依赖脚本。

`evidence/demo-handlers.py` 检查 EXE 的固定 SHA-256 并生成 `demo-handlers.txt` 与 `demo-helper-disassembly.txt`，后者包含声音音量常量、transition 跳表和相关 IAT 名字。完整命令表来源记录在 `docs/native-analysis.md`。
