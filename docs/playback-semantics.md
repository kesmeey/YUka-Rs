# 连续剧情与选择支的原生依据

本记录研究当前《情侣热恋中》汉化样本的序章、公共线和相应宿主命令。输入脚本来自 `data01.ykc`，以 GBK 解码。原生地址对应 `Loverable_cn.exe` 首选基址 `0x400000`，SHA-256 为 `84aef0e66123edfdf265449292abe87b6d46aa268e66fdd219614fb436a329bf`。

以下命令语义由原始脚本与 PE 静态分析核对。真实执行的覆盖范围由每次 runtime 报告单独记录。

## 剧情入口与章节生命周期

标题脚本“开始”分支最终 `ScriptJump("yks\\root\\root_Common.yks")`。此脚本为连续剧情调度入口，负责章节名、自动存档、场景清理和角色路线切换。故事文件末尾使用 `Return()`，由调用者继续安排下一段。

首次进入时，`prlogue_skip.yks` 设置 `Flag[100]=1`。`GlobalFlag[150]==0` 时直接返回，继而从序章开始。已播放过序章时会提供“从最初开始”和“跳过序章”的选择。选择 2 后，`root_common` 跳至第一章。

| 顺序 | 故事文件 | 静态 `KeyWait` 数量 |
| --- | --- | --- |
| 序章 | `PROLOGUE1.YKS` | 545 |
| 序章 | `PROLOGUE2.YKS` | 737 |
| 第一章 | `CHAPTER1-1.YKS`、`CHAPTER1-2.YKS`、`CHAPTER1-3.YKS` | 495、626、418 |
| 第二章 | `CHAPTER2-1.YKS`、`CHAPTER2-2.YKS`、`CHAPTER2-3.YKS` | 497、359、411 |
| 第三章 | `CHAPTER3-1.YKS`、`CHAPTER3-2.YKS`、`CHAPTER3-3.YKS` | 544、438、331 |
| 第四章 | `CHAPTER4-1.YKS`、`CHAPTER4-2.YKS`、`CHAPTER4-3.YKS` | 411、431、570 |
| 第五章 | `CHAPTER5-1.YKS`、`CHAPTER5-2.YKS`、`CHAPTER5-3.YKS` | 477、375、378 |

这些数量统计脚本所有分支中的 `KeyWait`。一次实际游玩的页数由条件和选择结果决定。

每章开始时，`String[10]` 保存章节标题，`Index(String[10])` 登记章节，`WindowNameSet(...)` 设置窗口名，`AutoSave.yks` 执行 `SaveDataSave(1,"[AUTO SAVE]")`。自动存档位于新章节故事调用之前，应保留当前整个脚本调用栈与共享变量。

`clear_main.yks` 在每段故事结束后执行。它根据 `Flag[205]` 决定是否停止全部声音，隐藏消息框，清除正文，删除场景图层 0–9、98、99，加载黑色层 0，然后用可写 data 整数作为动态下标，将 `Flag[1..199]` 清零。`Flag[0]` 用作循环计数器。VM 必须保留这种动态下标行为，其他共享变量继续传递。

`PROLOGUE1` 与 `PROLOGUE2` 之间调用 `days_eye2.yks`。序章结束后调用 `playMovie.YKS`，播放 `avi\lave_op.mpg`，随后把 `GlobalFlag[150]` 设为 1。各章之间使用 `days_eye.yks`。第五章结束后，根据 `Flag[300]` 的 1–5 分别跳至 Tinatu、Kaho、Tugumi、Nanako、Satuki 的 root 脚本。

## 第一处故事选择

第一次普通剧情选择位于 `CHAPTER1-2.YKS` code word `0x1b24`。此前在本脚本内有 215 个静态 `KeyWait`，从两段序章起合计有 1,992 个静态等待点。

调用前设置 `Flag[101]=4`，并写入四个选项字符串。

| 字符串 | 文本 | 选择后的流程 |
| --- | --- | --- |
| `String[1]` | 给琴宫发邮件 | `Flag[100]=1`，增加 `Flag[1500]`，进入 `LABEL_A1` |
| `String[2]` | 给花穗发邮件 | `Flag[100]=2`，增加 `Flag[1510]`，进入 `LABEL_B1` |
| `String[3]` | 给奈奈子小姐发邮件 | `Flag[100]=3`，增加 `Flag[1530]`，进入 `LABEL_C1` |
| `String[4]` | 今天算了 | `Flag[100]=4`，进入 `LABEL_D1` |

原始字符串还包含用于居中的全角空格。调用 `selectA.yks` 后，调用者把 `Flag[100]` 复制到 `Flag[390]`，再执行上述条件分支。

`selectA` 根据 `Flag[101]` 支持 1–5 个选项，加载选项背景，再使用 `Select()` 和 `Case(...)` 描述交互。其四项分支的字索引如下。

| 位置 | 指令 |
| --- | --- |
| `0x181` | `Select()` |
| `0x183` | 外层 `{`，link 为 `0x1d4` |
| `0x184` | `Case(String[1],108,76,78,65,716,122,101,100)` |
| `0x18f` | 第一项的 `{`，link 为 `0x197` |
| `0x190` | 赋值目标 `Flag[100]` |
| `0x191` | `=(1)` |
| `0x194` | `ScriptJump(:FC_RETURN)` |
| `0x197` | 第一项的 `}` |
| `0x198` | 第二项 `Case(...)` |

`Case` 参数依次提供文本、文本 x/y、点击矩形 left/top/right/bottom，以及两个声音编号。当前选项脚本末两项为 hover 101、click 100。各选项正文中的赋值和跳转由脚本决定，宿主应在选中后继续执行对应正文。

## Select 的原生控制流

`Select` handler `0x447a40` 设置 context 的选择模式，调用 `0x443a90`。后者从当前 next PC 扫描块结构，记录 `{` / `}` 的嵌套深度。`0x443d62` 仅处理外层深度 1 的 `Case`，`0x443d93..0x443d9f` 为每项保存 `Case.word + argc + 2`，即该项后续 `{` 的位置。

选择完成时，`0x4478d8..0x4478e7` 读取被选项保存的位置，写入脚本 frame 的 next PC。后续解释器从该块执行。取消或特殊选择模式另有路径，当前普通 `selectA` 的各项正文都会跳至 `FC_RETURN`。

因此，VM 应在 pending `Select` 上提供选项列表和专门的恢复入口。选择恢复需要验证条目来自当前 Select 块、目标位于指令边界，并清除 pending 状态。普通 `resume_host` 无法表达这里的跳转。

`FC_RETURN` 清页、恢复字体与自动/跳过状态，删除 40–45 选项图层，然后返回调用者。存档时若停在选择支，应保留原始 pending Select 和选项状态，读档后继续显示相同选择。

## 新增图形与声音命令

| 调用形式 | 已核对语义 |
| --- | --- |
| `RGB(0xRRGGBB)` | `0x430bf0` 将高低颜色字节交换，返回 Windows COLORREF，最低字节为 R |
| `RGB(r,g,b)` | 同 handler 的三参分支返回 `r + (g<<8) + (b<<16)` |
| `GraphicAddColor(id,color)` | `0x433000` 查找图层，`0x414d80` 设置该层加色值并重建显示数据。重新 `GraphicLoad` 会清零加色与减色字段 |
| `GraphicLoad(id,path)` | `0x439b00` 两参分支加载图像并显示，保留现有层位置和透明度。四参形式还调用 `0x413c40` 设置新 x/y。已有层的 add/sub 颜色会被显式清零。新建对象的初始位置与透明度为零 |
| `GraphicTransmittance(id,value)` | `0x432510` 写入层的透明度值，0 为不透明，1000 为完全透明。可见性仍由 Show/Hide 决定 |
| `GraphicTransmittanceScroll(id,target,time)` | `0x432560` 保存目标透明度，按 `(target-current)/time` 设置变化速度，异步推进 |
| `GraphicTransmittanceScrollWait(id,...)` | `0x4325b0` 等待给定层的透明度速度归零，同时处理运行状态 |
| `SoundVolumeToFade(id,target,time)` | `0x4341e0` 查找声音，调用 `0x4587b0` 设置音量变化。省略 time 时为 1000 毫秒，声音资源继续保留 |
| `Random(n)` | `0x43512b` 使用 `n+1` 为除数，返回闭区间 `0..=n` 的整数；days_eye2 的 `Random(4)` 可选择五个角色声音 |
| `LF()` / `LF(id)` | `0x430670` 对默认文字层或指定层调用 `0x414700`，推进到下一行 |
| `GraphicFilter(id,1)` | `0x432450` 创建 type 10001 的可见滤镜层。图形排序走到此层时对先前已合成内容施加棕褐色滤镜 |

序章后半的 `RGB(1048576)` 即 `RGB(0x100000)`，结果为 16，对应加红 16。运行时保留 COLORREF 通道顺序，送入 RGBA 合成时再提取各通道。

公共线的额外图形调用集中在少数段落。`CHAPTER2-2` 使用透明度渐变，`CHAPTER3-1` 使用 filter 1 和换行，`CHAPTER3-2` 使用固定透明度，`CHAPTER4-1` 使用换行，`CHAPTER5-1` 使用固定透明度和渐变。`PROLOGUE2` 使用 `SoundVolumeToFade(160,1000,1000)`。

## GraphicLoad 的外部 YKG 参数

`CHAPTER2-2` word `0x1247` 调用 `GraphicLoad(98,"YKG\\BG\\white.ykg","ykg\\system\\black_m.ykg",0,0)`，随后调用 `GraphicTransmittanceScroll(98,1000,600)`。这个五参形式中的第二个路径进入外部遮罩加载入口，但本 EXE 对 YKG 的处理会重新解码其 color 图像，替换第一次加载的 color。

| 地址 | 同一图像对象的传递与实际操作 |
| --- | --- |
| `0x439c43..0x439c59` | 五参分支调用 `0x411410(first_path,second_path)`，this 为当前 graphics 对象 |
| `0x411417` | 将入口 this 保存为 ESI |
| `0x411423..0x411426` | ECX=ESI，调用 `0x40d870(first_path)` |
| `0x411435..0x41143c` | 第一张图像成功后，ECX=同一 ESI，调用 `0x40da70(second_path)` |
| `0x40dab8` | `0x40da70` 把 this 保存为 EBP |
| `0x40dbb3..0x40dbb6` | YKG 分支令 ECX=EBP，直接调用普通 YKG 解码器 `0x40d240` |
| `0x40d262` | YKG 解码器将 this 存在局部 `esp+0x10` |
| `0x40d397..0x40d39d` | 取同一 this，为 color PNG 调用 `0x40cc00` |
| `0x40cc27`、`0x40cdac..0x40cdae` | PNG 解码器保存 this 为 EBX，再以 ECX=EBX 调用 `0x40c9c0` 创建 color surface |
| `0x40c9c9..0x40c9cd` | 新建 surface 前，ECX=同一对象，调用 `0x40c640` 清理旧图像数据 |
| `0x40c640..0x40c732` | 清理原 color/alpha、surface 和文件名等图像基类字段；未保留第一张图片副本 |
| `0x40dbdb..0x40dbe0`、`0x407760..0x4077a0` | 后处理仅将第二路径复制到对象的遮罩文件名字段 `+0x30` |

这条路径没有把 this 改为 alpha 子对象，也没有在返回时恢复第一次加载的 color。`black_m.ykg` 的 color 段为 800×600 PNG，alpha 段为空。实际像素全部为 RGBA `(0,0,0,255)`，所以此调用最终产生不透明黑色层，之后按脚本淡出。

裸 BMP 与 PNG 在 `0x40da70` 中分别走 `0x4077b0` 和 `0x407880` 专门入口，其 alpha 转换规则应单独验证。上述 YKG 行为限定于当前 EXE 及已检查的 color-only 外部 YKG。

复用图层时，`0x439b5a..0x439b72` 清零 `+0x12ac`、`+0x12b0` 及三个颜色效果标志；透明度字段 `+0x10f8` 保留。图像重新分配只清理图像基类字段，图层的 x/y、透明度属于其他字段。

## 不存在图层的移动请求

`GraphicScroll` 与 `GraphicMove` 对不存在的目标图层执行空操作。四参 `GraphicScroll` 分支在 `0x431d5b` 通过 `0x410d40` 查找图层，`0x431d62..0x431d64` 检测空指针并跳到 `0x431d91` 返回 0。`GraphicMove` 在 `0x432418` 查找后，经 `0x43241d..0x43241f` 的空指针分支直接到 `0x432441` 返回 0。`GraphicScrollWait` 的 `0x43219a..0x4321ac` 同样跳过不存在的图层。`Nanako_06` 的 word `0x1cf` 对此前已经清理的 slot 8 调用四参滚动，连续运行需保留这种原生行为。

## 滤镜的像素公式

原生 CPU 渲染路径在 `0x4120d6` 调用 `0x406ce0`。读取像素的 B/G/R 通道，先归一化到 0–1，再计算 `Y = 0.299R + 0.587G + 0.114B`。

- 输出 R 为 `clamp(Y + 0.078512, 0, 1) * 255`。
- 输出 G 为 `clamp(Y + 0.031304 - 0.039984, 0, 1) * 255`。
- 输出 B 为 `clamp(Y - 0.161252, 0, 1) * 255`。

原生实现以截断方式转换成字节。常量位于 `0x49c840..0x49c878`。滤镜层本身没有普通图片资源，后续更高层的文字和菜单继续按原色合成。GPU 路径使用独立 shader，逐像素浮点舍入尚未比较。

`GraphicFilter(id,2)` 在 `0x4324bf` 创建 type 10002 的可见灰度层。合成分发 `0x4120c1` 调用 `0x406f40`。像素循环 `0x406fe0..0x40700a` 计算整数 `gray = (3 * R + 6 * G + B) / 10`，向下取整后同时写入 RGB，保留 alpha。这里的加权比例直接来自整数指令。

`GraphicFilter(id,3)` 创建 type 10003 的可见滤镜层。原生合成分发 `0x4120ac` 调用 `0x407040`，其像素循环 `0x4070e0..0x4070fe` 分别将三个颜色字节替换为 `255 - channel`，保留 alpha 字节。该模式同样处理滤镜层之前已经合成的内容。`TINATU07` 在 code word `0x5ac` 使用这一模式。Rust 图层分别记录 sepia、grayscale 和 invert 状态，创建一种滤镜时将其余两种状态清零。

## 章间与演职员表 Sprite

`days_eye.yks` 创建可见空图层 90，隐藏加载动画源 91，然后调用 `SpriteCreate(90,0,0,0,91,0)`。`SpriteCreate` 在 `0x43da40` 解析父层、sprite ID、相对 x、相对 y、来源层、动画序列编号。`0x4085b9..0x4085c3` 用最后一个参数比较 source 的序列数量，`0x40861e..0x40866d` 根据序列编号取起点和记录数量，复制到独立 sprite 状态。父层保留自身图像，各 sprite 作为按 ID 排序的子绘制项。

`days_eyc01.ykg` 为 1600×3000 的图集。十个有效 frame 均为 800×600，前五个在 x=0，后五个在 x=800，y 依次为 0、600、1200、1800、2400。各 frame 的 duration 为 300、flags 为 2，末项为全零 sentinel。原生 `0x407d27..0x4080ad` 将全零的 32 字节记录作为动画序列分隔，组内计数不包含此记录。`0x40842c..0x408433` 把非零 flags 写入 sprite 的播放模式，`0x408210` 中 mode 2 对应 `0x408234..0x408257`，到达末帧后把当前帧编号重置为零。Rust 对已经核对的 flags 2 序列按 duration 循环播放，其他 flags/action 组合明确报错。

`roll_kaho.yks` 和 `roll_tinatu.yks` 在透明父层 80 下创建 ID 1–5 的子 sprite，来源为隐藏层 81–85。`maru01` 的图集为 1515×505，包含三个 505×505 frame。`maru02` 为 1100×275，包含四个 275×275 frame，按 x=825、550、275、0 的顺序播放。`maru03` 为 885×177，包含五个 177×177 frame。各 frame 均为 duration 60、flags 2、unknown 0、action 0，以全零项结束。动画分别以 180、240、300 毫秒周期循环。

父层的移动、显隐和透明度应用到全部子 sprite，子 sprite 的相对位置及显隐单独保存。删除某个 sprite 只移除该 ID 的子绘制项，删除或重载父层、来源层时也清理对应动画和绘制项。存档仍限制在没有活动 sprite 的稳定剧情点。加速的无窗口检查冻结在序列首帧，普通窗口播放使用时钟推进。

## 演职员表与路线结束

演职员表由原脚本逐张显示 16 个 staff 图层，使用 `GraphicTransmittanceScroll` 和 `TimerStart(1)` / `TimerWait(1,8000)` 排列时间，末尾 `Wait(4000)`。首次到达时 `GlobalFlag[190]` 从 0 设为 1，并使用 `ScriptSkipMode(0)`。再次到达使用模式 2。

`roll_e.yks` 调用 `ScriptSkipMode(1)`，把声音 1 在 5000 毫秒内淡出删除，用白色图层过渡 2000 毫秒，调用无参 `SakuraEffect()`，再清理普通图层和 50–88 的演职员表图层。无参 `SakuraEffect` 在 `0x433d4a` 将激活标记 `0x4c40e4` 清零，随后遍历 `0x4b4140..0x4b4164` 的十个粒子图层 ID，删除存在的图层并将登记 ID 置为 -1。没有创建过粒子效果时，此分支为空操作。

Tinatu、Kaho 的 root 脚本在演职员表之后继续调用各自 EP，完成清理和 `HistoryReset()` 后执行 `GlobalFlag[200]=1`，等待 2000 毫秒，然后 `ScriptJump("yks\\title\\title_main.yks")`。连续播放须保留这一完整过程，结局状态由原脚本写入。

## 本次探测与证据

基于初始提交 `8303dcb` 执行 `demo --headless --pages 2000`，完成 443 页、6,028 步，在 `PROLOGUE1` word `11148` 的 `RGB` 处明确停止。报告在 `evidence/progression-probe.json`。同版直接进入 `root_common`，首个未实现命令为 word 24 的 `Index`，报告在 `evidence/root-common-probe.json`。

`evidence/playback-research.py` 批量反汇编普通剧情与调度脚本，并在验证 EXE 哈希后生成关键 handler 反汇编。`playback-script-summary.json` 记录命令集合和静态调用链，`playback-scripts/` 保存本地脚本证据，`playback-handlers.txt` 保存原生指令。上述证据目录已被 Git 忽略。

2026-11-15 从公共线或指定角色 root 执行，并对遇到的选择使用第一个选项，已得到以下 `finished=true`、`end_reason=return_to_title` 的报告。

| 入口 | 页数 | VM 步数 | 本地报告 |
| --- | --- | --- | --- |
| `root_Common.yks` → `root_Tinatu.yks` | 11779 | 235275 | `evidence/playback-full-first-choice-d.json` |
| `root_Kaho.yks` | 4656 | 96193 | `evidence/kaho-path-b.json` |
| `root_Tugumi.yks` | 4631 | 83330 | `evidence/tugumi-path-a.json` |
| `root_Nanako.yks` | 4756 | 80149 | `evidence/nanako-path-d.json` |
| `root_Satuki.yks` | 2871 | 57577 | `evidence/satuki-path-a.json` |

这些运行覆盖角色 root 到 EP 和标题回跳的相应选择路径。新增兼容性声明继续以具体 runtime 报告及可重现的选择序列为依据。`evidence/roll-kaho-graphics-host.json` 单独记录演职员表 179 步、21 张图像、5 个 sprite、14 次 TimerWait 的完成结果。102 个 story、roll、function 脚本的静态参数检查只发现滤镜模式 1、2、3，以及章间和演职员表两类 SpriteCreate 调用，其动画序列参数均为 0。
