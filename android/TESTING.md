# Android 集成测试

`tests/` 构建为独立的 `org.yukars.player.tests` instrumentation APK。测试在播放器应用 UID 下执行真实 JNI，会创建测试存档和音频缓存。

## 《同居情侣热恋中》FD 0.2.7

完整包 `bin/Dousei-Loverable-Yuka-Rs-0.2.7.apk` 为 3,161,826,652 字节，包名 `org.yukars.dousei`，版本号 8。
SHA-256 为 `1E183799E94E537B9467F6588E3D4E4D53B8EF24DAA518EC6793D62BF66806DE`。
APK v2/v3 签名、ARM64/x86_64 及 16 KiB 对齐验收通过。四份 XZ 的解压字节与原档逐一匹配，
两份 MP4 均通过全流解码，APK 内六份载荷的长度与 SHA-256 均匹配资源清单。
发布包使用独立应用数据目录，包含四个游戏档和两部视频。测试台账、原始程序与个人存档均由本地另行保存。

FD 通过 `select01` 和 `select02` 为五条路线设置角色与 A/B/C 模式。在安卓播放器中完成选择后，
按原始变量约定进入对应的 `root_*.yks`。对 FD 中大小写不同但内容相同的三个档案条目复用同一资源，
非相同内容的歧义仍报错。FD 的额外语音通道 25–27 参与换句停止、自动等待、音量与 BGM 降低。
`KeyVoiceWait` 等待当前语音结束或手动推进，快进模式可跳过等待。

原始资源回归覆盖 15 种角色与模式组合的前 20 页，另将五条路线的 A 模式按首选分支执行到返回标题，
合计 23,664 个对白等待点。FD 的 C 模式二进制存档读取恢复了角色、模式、脚本位置和当前对白，并复现下一句。
样本的 `System.dat` 与 `Already.dat` 均通过只读解析及逐字节编码检查。FD 的 PC 原版程序读回安卓存档留待交叉验证。
Rust 库测试 59 项通过，另外显式执行上述本地 FD 测试与本篇的换句语音回归。格式和 Clippy 检查通过。

在独立 Android 15 x86_64 模拟器上安装该完整 APK。首次启动的四份资源校验记录与原档 SHA-256 和长度一致。
重新启动复用已准备资源。从界面打开五条路线及 A/B/C 选择，进入花穗 A 并通过快存按钮写出 `savedata.999`。
JNI 烟测通过推进对白、原格式剧情档保存和读回、SAF 导入导出，收到四组音频加载和播放事件。
手动点播 OP 后安卓解码器报告 800×600 视频首帧已渲染。ARM64 通过交叉编译和打包校验，手机实机试听仍待验证。

以下命令在本地验证同一 FD 样本。

```powershell
$env:YUKA_TEST_DOUSEI = 'D:\gal\love loabe\【PC】同居情侣热恋中\【PC】同居情侣热恋中'
cargo test --lib --locked dousei_ -- --ignored --nocapture
```

## 换句语音修复与完整测试包 0.2.6

0.2.6修复下一句由另一个角色发声时，上一句角色语音仍在播放的问题。
修复依据为原版KeyWait设置的换句标记：新句第一条语音播放前停止旧句的角色语音，
同一句内有意安排的多人合声继续保留。BGM和音效不受影响；“点击中断语音”仍按原设置生效。
依据、边界及回归测试见 `docs/voice-playback.md`。

完整测试包为 `bin/Loverable-Yuka-Rs-0.2.6.apk`，versionName为0.2.6、versionCode为7。
包名、签名和原版存档结构延续旧版，资源清单为完整包模式，可供新设备首次安装。
随包五份原始资源的XZ压缩数据、OP视频和运行时代码，不携带个人Save或解锁进度。
安装与测试反馈说明位于 `bin/Loverable-Yuka-Rs-0.2.6-测试说明.txt`。
本轮不向已连接手机安装或重启应用，0.2.6的实际设备覆盖安装和扬声器复测由测试者继续验证。

`cargo test --lib --locked` 结果为59项通过、0失败、14项需本地资源的测试默认跳过。
其中Android音频后端6项测试全部通过，新增回归验证旧语音stop先于新语音play、
同页合声与BGM/音效保留，以及静默页和原版播放形式的边界。
Java异步准备回调的requested、slot身份和generation保护经只读代码审查，未执行0.2.6设备instrumentation。
真实 `CHAPTER1-1.YKS` 的专项回归另行显式执行，推进72页，三次跨角色语音交接通过，
确认仅当前页语音处于播放状态且BGM保留。该测试静音运行，验证脚本和音频状态，不代表已试听设备输出。
`cargo fmt --check` 和 `cargo clippy --all-targets --locked -- -D warnings` 通过。

2026-11-17构建的完整APK大小3,183,326,629字节（约3.18GB / 2.97GiB），SHA-256为
`ACDAE381861E49CE2FC9F6C9B59A3ADA5FBF0D693A7C02E2F08A3BFA523F80F5`。
v2/v3签名与16KiB ELF/APK对齐校验通过。两个ABI的包内引擎逐字节匹配本次0.2.6编译输出，
包内完整资源清单与源清单相同，更新包限制标记未启用。
包内五份XZ及OP视频的长度、流式SHA-256逐项匹配源清单；视频保持STORED便于直接播放。
签名证书与旧包相同，打包成功后临时unsigned/aligned APK已删除，没有V4 idsig残留。

2026-11-18使用同一源码提交 `e00a7ff` 重新生成同名完整APK，大小仍为3,183,326,629字节，最新SHA-256为
`D3F52AC1D37E7111983A555BCDD3D1F2A7D8338514AA7CD562CC3B676C29CAE3`。
五份XZ与OP的流式SHA/长度、完整资源清单、0.2.6/code7、同签名及APK/ELF的16KiB对齐重新验收通过。
两份ABI引擎逐字节匹配编译输出，其SHA与11月17日构建一致；未夹带个人存档或签名密钥。
本次格式和Clippy检查通过，未安装到手机，未重复实机测试。校验文件随APK保存在 `bin/`。

## 手机震屏报错修复 0.2.5

连接的 Android 15 ARM64 手机在0.2.4执行
`yks/story/kaho/kaho_07.yks @1263 Effect(2,20,1)` 时报告未支持的震屏模式2。
进程仍在前台，是引擎主动暂停。用户要求保留现场，本轮对该手机只读取日志和截图，未安装、
停止、重启应用或运行 instrumentation。更新会结束原进程，需从已有存档继续。

本次按原版分派函数补齐模式1至6。Rust回归覆盖每一步位移、奇数幅度、默认值、简化、
快进、暂停与复位；只读语料回归覆盖175个脚本的442次Effect调用，在普通、简化和快进模式验证。
实现依据和可复现命令见 `docs/screen-shake.md`。

0.2.5的 `cargo test --lib --locked` 结果为57项通过、0失败、13项依赖本地资源的测试默认跳过。
其中全部Effect调用的本地语料测试另行显式执行并通过：模式1共33处、模式2共6处、
模式5共402处、模式6共1处。`cargo fmt --check` 和 `cargo clippy --all-targets --locked -- -D warnings` 通过。

更新包 `bin/Loverable-Yuka-Rs-0.2.5-update.apk` 的包名为 `org.yukars.loverable`，
versionName为0.2.5、versionCode为6，保留双ABI、原EXE图标、开发签名与OP视频。
包内不包含五份XZ游戏资源，只能复用完整版已经完成的资源；不会重新解压。
`BundledGameInstrumentation` 新增五项检查：已准备资源复用、新安装拒绝、错误文件长度拒绝、
错误receipt摘要拒绝和缺失文件拒绝，并检查存档、receipt及临时文件不被更改。

修复更新包大小46,515,251字节，SHA-256为
`3E51FC25E45D4733715D0AA753E977BC56D002E83675CA5B8324EAC4EEE811B9`。
签名v2/v3、两种ABI的ELF及APK的16KiB对齐校验均通过。证书SHA-256为
`98bb79a749376f8fad0ae32d7942c10b6aa516df72365bcc7d1b07162118c5d0`，与本机0.2.4播放器一致。
包内容检查确认更新标记为true、五条资源记录、没有XZ载荷，并保留40,988,694字节的未压缩MP4。
源资源清单没有被修改。构建使用共享API36 SDK/NDK29，应用目标仍为API35、最低API26。

全部Java应用与测试代码以 `--release 8 -Xlint:all -Werror` 编译成功，测试APK构建及签名校验通过。
本轮尝试在独立 `emulator-5582` 上执行新增解包器测试，但模拟器在初始化userdata镜像阶段
未完成启动，因此本轮instrumentation未执行，不能视为设备测试通过。临时模拟器使用独立目录，
没有使用物理手机执行替代测试；0.2.5的设备运行与实际覆盖安装仍待验证。

## 完整游戏 APK

`bin/Loverable-Yuka-Rs-0.2.6.apk` 的目标包名为 `org.yukars.loverable`。资源准备与构建命令见项目 README。
APK 携带五个原 YKC 包的 XZ preset 3 无损压缩数据，首次启动使用 Rust 原生解码器自动流式解包并验证原文件长度与 SHA-256。
完整资源目录为 `/data/user/0/org.yukars.loverable/files/game-bundled-loverable-cn`，其下的 `Save` 使用原版二进制格式。

将 APK 复制到设备并安装后，打开“情侣热恋中”，等待资源准备完成并进入标题。ADB 安装和启动命令如下。

```powershell
adb -s emulator-5580 install -r .\bin\Loverable-Yuka-Rs-0.2.6.apk
adb -s emulator-5580 shell am start -n org.yukars.loverable/org.yukars.player.MainActivity
```

解包器的独立回归使用小型合成资源，覆盖首次安装、重复启动、中断续传、资源更新、存档保留、
校验和错误处理与截断 XZ 文件的重试。运行方式如下。

```powershell
$bundleTest = @{
    Apk = '.\bin\Loverable-Yuka-Rs-0.2.6.apk'
    JavaHome = 'E:\DevEco Studio\jbr'
    XzJar = '.\target\android-libs\xz-1.12.jar'
    Adb = 'E:\platform-tools\adb.exe'
    Serial = 'emulator-5580'
    TargetPackage = 'org.yukars.loverable'
    Game = '/data/user/0/org.yukars.loverable/files/game-bundled-loverable-cn'
    Save = '/data/user/0/org.yukars.loverable/files/smoke-saves'
    Audio = '/data/user/0/org.yukars.loverable/cache/smoke-audio'
}
.\android\test.ps1 @bundleTest -Runner BundledGameInstrumentation
```

完整资源准备完成后，可复用同一配置运行真实 JNI 剧情及原格式存读档回归。

```powershell
.\android\test.ps1 @bundleTest -Runner SmokeInstrumentation
.\android\test.ps1 @bundleTest -Runner AudioFocusInstrumentation
.\android\test.ps1 @bundleTest -Runner OriginalUiInstrumentation
# PlaybackInstrumentation 的 OriginalSave 需指向真实的选择支存档，例如 savedata.006。
.\android\test.ps1 @bundleTest -OriginalSave '/data/user/0/org.yukars.loverable/files/pc-input/savedata.006' -Runner PlaybackInstrumentation
.\android\test.ps1 @bundleTest -Runner ActivityControlsInstrumentation
```

`-TargetPackage` 用于生成测试 APK 的目标包配置，测试代码的类名仍位于 `org.yukars.player`。
`-OriginalSave` 可传入设备上目标应用 UID 可读的 PC 原版剧情档路径，扩展验证读取、再保存和继续剧情。
完整 APK 的安装、首次启动和资源一致性实测结果记录如下。

### 设置、鉴赏与视频 0.2.4

最终完整APK为3,183,322,533字节，SHA-256：
`0ACBD952FB6332EBAD4E8C59E63FDDDC17FDFE74BC643BF124B5F61C7C414C54`。
通用APK为5,526,290字节，SHA-256：
`006E75FD3DC25DB7CDABADB6AD99E847B417C20785B04300BEEC6B4AC496A87A`。
versionCode5，双ABI，签名v2/v3与16KiB对齐通过。

`SettingsInstrumentation` 验证77张原版设置图像和69处逐像素裁切、八页导航、确认、单选、滑块、
静音和试听回调、原版System.dat保存与分类恢复默认，以及真实Activity的文本/语音/声音/透明度操作。
`MovieInstrumentation` 用实际MediaPlayer和Surface验证APK内MP4文件描述符（位于大于3GB的偏移）、
800×600解码、暂停恢复、10秒处PixelCopy帧、点击跳过防重复、接近结尾自然结束和错误回调。
`ExtrasInstrumentation` 验证新进度锁定、独立复制的用户全CG配置解锁、CG/差分界面、BGM暂停VM播放及原脚本回想。
全CG测试配置仅复制到独立测试会话，不会解锁应用正常游戏的进度。

```powershell
.\android\test.ps1 @bundleTest -Runner SettingsInstrumentation
.\android\test.ps1 @bundleTest -Runner MovieInstrumentation
.\android\test.ps1 @bundleTest -Runner ExtrasInstrumentation -FullProfile '/data/user/0/org.yukars.loverable/files/pc-input/fullCGSystem.dat'
```

双ABI包的设置、视频、鉴赏、阅读实际按钮、原PC选择档006共五套测试通过。
其中视频新增实际MainActivity视频请求/跳过和UI操作进行中发生错误时仍释放VM等待的回归；
完整Save导入新增SAF暂存、逐字节备份、修改时间、取消、同名槽原子发布及其他槽保留的检查。
对应最终报告位于 `evidence/android-final-024/`。此前原型的三套测试也全部通过；截图和报告分别位于 `evidence/android-settings-024/`、
`evidence/android-movie-024/` 和 `evidence/android-extras-024/`。
真实OP原文件243,234,820字节，构建时转换为40,988,694字节H.264/AAC，完整2848帧及音轨解码通过，
原文件摘要前后相同。MP4的SHA-256为`F3464226C5172508CF3CE3885477229475E517A80DBFB4CFDFA413E40BC84D86`。
本版本还通过原版震屏1/5轨迹、完整回想结束返回鉴赏、设置/姓名/原生存档、无效全局导入无修改及
有效PC全局导入保留原格式的真实资源回归。字体采用Android黑体/宋体家族，低画质转场时长和
BGM降低幅度有明确的宿主近似，见 `docs/android-settings.md`。
最新摘要对应的完整包再次通过设置整套回归；通用包从未导入游戏的状态实际打开了系统目录选择器，
取消后没有导入或删除数据。最终报告在 `evidence/android-latest-final-024/settings/`，
成品摘要与通用启动证明在 `evidence/android-final-024/latest-artifacts.json` 及同目录的generic证明文件。

### 阅读功能与解压优化 0.2.3

完整 APK 为3,141,842,206字节，SHA-256为
`0635C91035802B231965C57C9B75D0898E1807C0C4BB2F47952C4534CECB262B`。
通用播放器 APK 为5,018,386字节，SHA-256为
`2671F499369BB041C786931186931E312B2DF646E19E43000C7B45AB46870E62`。
两者均包含ARM64和x86_64，通过v2/v3签名及16KiB对齐校验，versionCode为4。

自动阅读等待正文阅读时间和实际语音完成回调，并保留300毫秒的语音后延迟。
已读快进测试先浏览五页，再回到开头，确认自动停在第六页未读对话；`Already.dat` 在销毁并重建
会话后保持已读状态。全部快进、手动取消、选择支拒绝自动选择和标题不预读首屏也有回归。
履历验证正文与语音导出不改变当前画面/脚本/音频通道，原版图像、滑动、滚动条、翻页和喇叭命中位置可单独测试。
历史语音重复导出使用各自独立的文件，过期请求的清理不会删掉新请求的语音。

`BundledGameInstrumentation` 增加原生 XZ 的大输入/输出缓冲区、单字节输入、截断footer与错误校验回归。
真实 `data03` 的前128 MiB在相同Android15 x86_64模拟器中，解码并计算SHA-256耗时：
旧Java4156毫秒，新Rust1979毫秒，两者摘要一致（约2.10倍吞吐量）。该结果不代表真实手机的完整解压时间。
主机完整1,710,660,649字节的同一资源纯解码耗时为Java44.58秒、Rust21.17秒。

设备基准命令（需完整游戏APK中的随包资源）：

```powershell
adb -s emulator-5580 shell am instrument -w -e benchmark true org.yukars.player.tests/org.yukars.player.BundledGameInstrumentation
```

原版EXE包含一个32×32图标，由PE资源解析提取并经AAPT2编译，未执行EXE。
完整包使用该图标，通用播放器使用仓库中的矢量图标。所有版本继续使用原版二进制剧情存档、全局和已读记录格式。

最终签名完整APK已在Android15 x86_64模拟器安装，`PlaybackInstrumentation` 八项和
`ActivityControlsInstrumentation` 四项全部通过。后者实际触摸原版自动按钮、普通画面取消、
快进弹窗的全部模式、履历按钮与返回，检查真实后台/前台切换后恢复手动模式。
`OriginalUiInstrumentation` 原档001通过195张图像、131个裁切、备注、缩略图和保存页回归；
`SmokeInstrumentation` 真实选择档006通过PC原档往返和两个分支恢复。
证据在 `evidence/android-playback-023/`。Rust默认56项、真实游戏阅读回归、原UI资源裁切、
四项原格式存档回归、fmt和Clippy检查通过。ARM64已编译，真实手机的速度与扬声器输出待验证。

### 原版 UI 0.2.2 回归

完整 APK 为 3,141,522,392 字节，SHA-256 为
`295EA4DD920E467E91DECEADC4F68867E8CB6BEACA8BF8A04478189699B20B9D`。
通用播放器 APK 为 4,715,040 字节，SHA-256 为
`47796B1B66900335AC217ACD03B1900AD67AC2D3C4E1EDAA8D645AE6AC292E5E`。
两者均包含 ARM64 和 x86_64，通过 v2/v3 签名及16 KiB对齐校验。完整 APK 已在 Android 15 模拟器安装。

`OriginalUiInstrumentation` 在独立缓存目录创建测试存档，仅读取输入的 PC 存档。
本机 Android 15 x86_64 测试覆盖 195 张原图/序列、131 个完整像素裁切对照、
原版存档 metadata 与 100×75 缩略图、中文备注写入读回、20页槽位编号、
损坏槽位隔离、损坏缩略图的正常存档加载，以及 Canvas 像素和触摸位置。

原 PC 程序在私有 Windows 桌面的隔离副本中提供对照截图。依据为 `m_save.yks`、`m_load.yks`、
`m_YesNo.yks`、`createText.yks` 和 `createMemu.yks`，资源来自原 YKC 文件。
实测图片和日志位于 `evidence/original-ui/` 与 `evidence/android-original-ui/`。

实际 Activity 点击检查覆盖标题、备注键盘、保存、覆盖取消、001自动槽写保护、
第二页写入011、存读档页切换、999快速档、后台保存及重启后保留页码。
自绘 GameView 关闭 Android 默认焦点高亮，以保持备注输入完成后的原图颜色。
`SmokeInstrumentation` 覆盖 PC 回存021继续剧情及选择006两个分支，音频焦点和随包资源回归通过。

最终 APK 从新游戏进入后，通过手机键盘和原版保存页生成011槽，备注为 `Android_UI_022_final`。
该文件 SHA-256 为 `45556A06C9C0988F7C759C809D8FB84F16ED6E10044DC47939B4516109B6C5FC`。
原版 PC 菜单显示正确备注、章节、日期和缩略图，读取后当前中文、消息框、姓名框和九个菜单按钮正常。
通过原版读档确认再次加载并推进，脚本等待位置从47到54，下一句中文正常。
对应日志和截图使用 `evidence/android-original-ui/final-apk-native-` 前缀。
五次最终 instrumentation 均返回 `INSTRUMENTATION_CODE: -1`，Activity 的11项实测检查全部通过。

设置/鉴赏、自动阅读/已读快进/履历、字体替代和菜单动画的实现边界见 README。
ARM64 已交叉编译，真实手机仍待验证。

### 完整游戏 APK 0.2.1 实测记录

主 APK 为 3,141,125,080 字节，SHA-256 为
`2ACD15AC0E42F6CDB01D44EA0AB3C2B09B6399613F4BD094319B063C5BC7E75E`。
签名校验通过，已在 AOSP Android 15 x86_64 模拟器安装成功。

五个原始 YKC 包合计 4,417,273,802 字节，XZ preset 3 数据合计 3,136,805,240 字节。
主机逐包重解压并计算 SHA-256，五个结果均与原包一致。设备首次启动完成五包解压后，也逐一计算 SHA-256，
五个结果与 PC 原始文件完全一致，记录位于 `evidence/android-bundled/device-archives.json`。
应用与展开资源合计约占 7.56 GB，另存的源 APK 文件占用额外空间。

首次资源准备的中断恢复已执行设备检查。强制停止应用后重新打开，已完成的 `data01.ykc` 被复用，
文件修改时间保持一致。完整准备完成并重启后，`data05.ykc` 的修改时间也保持一致。
`BundledGameInstrumentation` 的八项结果均为 `true`，覆盖安装、重复启动、
中断续传、保留存档的更新、校验失败、XZ 解包及截断文件重试。

通用播放器 0.2.1 的 `SmokeInstrumentation` 已通过原 PC 回存 `savedata.021` 的加载和继续剧情回归。
完整游戏目标包 `org.yukars.loverable` 已完成以下设备验证。

- 实际点击标题和新游戏进入剧情，快速存档后推进，再读快速档恢复页面。
- Home 后台自动保存，终止应用进程、重新启动并继续上次存档。
- `SmokeInstrumentation` 分别使用原始 `savedata.001` 和 PC 回存 `savedata.021`，均通过
  `original_pc_round_trip` 和 `original_pc_continued`。
- `SmokeInstrumentation` 使用真实选择档 `savedata.006`，恢复并执行两个分支。
- `BundledGameInstrumentation` 与 `AudioFocusInstrumentation` 通过。

上述 instrumentation 全部返回 `INSTRUMENTATION_CODE: -1`。原始日志位于 `target/android-bundled-*.log`，
标题、剧情、快读和重启恢复截图位于 `evidence/android-bundled/title.png`、`story.png`、`quickload.png` 和 `resume.png`。
ARM64 当前完成交叉编译，实机运行、扬声器输出和完整路线的逐帧音画对照仍待验证。
OP 视频及特效实现边界继续适用，具体见项目 README。

## 构建

主 APK 由 `build.ps1` 生成。测试脚本需要相同的签名密钥，默认使用 `bin/android-development.jks`，别名为 `yuka-rs`。Java 工具链需支持 `javac --release 8`，脚本使用 .NET ZipArchive 打包 class。
当前构建与测试均读取 XZ for Java 1.12，默认位置为 `target/android-libs/xz-1.12.jar`，可通过 `-XzJar` 指定。

以下命令在项目根目录的 PowerShell 7 中执行。将工具路径和设备路径替换为本机配置。

```powershell
$test = @{
    Apk = '.\bin\Yuka-Rs-0.2.2.apk'
    JavaHome = 'E:\DevEco Studio\jbr'
    Adb = 'E:\platform-tools\adb.exe'
    Serial = 'emulator-5580'
    Game = '/data/user/0/org.yukars.player/files/game-smoke'
    Save = '/data/user/0/org.yukars.player/files/smoke-saves'
    Audio = '/data/user/0/org.yukars.player/cache/smoke-audio'
}
.\android\test.ps1 @test -BuildOnly
```

输出为 `bin/Yuka-Rs-tests.apk`。每次构建的中间文件保存在系统临时目录下的独立运行目录，Java 源码以 `-Xlint:all -Werror` 编译。
工具路径可以显式指定，也可自动复用本机已验证的共享SDK；详见 `docs/toolchains.md`。
非 `-BuildOnly` 的测试必须提供明确的 `-Serial`，脚本不会自动选择已连接的手机。

## 执行

`Game` 指向设备上播放器 UID 可读取的原始 `data*.ykc` 所在目录。`Save` 和 `Audio` 指向该 UID 可写入的测试目录。`OriginalSave` 可选，用于指定一个真实 PC 存档；传入它可运行原版档的加载与再保存回归。真实资源和存档由测试者提供。

实测数据准备使用新建、支持 `adb root` 的 AOSP Android 15 x86_64 模拟器。逐个文件推送到完整目标路径，可正确处理 Windows 主机的目录分隔符。

```powershell
$source = 'D:\gal\love loabe\【PC】情侣热恋中\情侣热恋中（ラブラブル)'
& $test.Adb -s $test.Serial root
& $test.Adb -s $test.Serial wait-for-device
& $test.Adb -s $test.Serial install -r $test.Apk
& $test.Adb -s $test.Serial shell mkdir -p ($test.Game + '/Save')
foreach ($name in @('data01.ykc', 'data02.ykc', 'data03.ykc', 'data04.ykc', 'data05.ykc')) {
    & $test.Adb -s $test.Serial push (Join-Path $source $name) ($test.Game + '/' + $name)
}
foreach ($name in @('System.dat', 'Already.dat', 'savedata.001')) {
    & $test.Adb -s $test.Serial push (Join-Path $source ('Save/' + $name)) ($test.Game + '/Save/' + $name)
}
$appUid = [int](& $test.Adb -s $test.Serial shell stat -c %u '/data/user/0/org.yukars.player')
& $test.Adb -s $test.Serial shell chown -R "${appUid}:${appUid}" '/data/user/0/org.yukars.player/files'
& $test.Adb -s $test.Serial shell restorecon -R '/data/user/0/org.yukars.player/files'
$test.OriginalSave = $test.Game + '/Save/savedata.001'
.\android\test.ps1 @test
```

脚本安装主 APK 和测试 APK，并执行 `am instrument -w`。成功结果含 `INSTRUMENTATION_CODE: -1`，失败会显示所在阶段和异常。原始输出写入本次构建目录的 `instrumentation.txt`。进入原生测试阶段后，还会向 `Save/android-smoke-result.json` 写入结构化结果。

上述配置生成的原格式新存档为设备上的 `/data/user/0/org.yukars.player/files/smoke-saves/smoke.savedata.001`，PC 原档再保存结果为同目录的 `smoke.savedata.pc-roundtrip`。可以使用 `adb pull` 取回并用于 Windows 原格式解码与运行时回归。

## 覆盖范围

- SAF 游戏导入、单个 PC 存档导入和完整 Save 目录导出，逐字节检查 `savedata.001`、`System.dat` 与 `Already.dat`，核对原始文件名。
- 上次中断导入的临时目录回收、取消导入和重复文件名失败后的清理，以及已有游戏和存档的保留。
- JNI 创建会话、标题、开始游戏和连续推进三页。每个稳定页面最多等待 30 秒。
- 真实运行时写出原格式存档，再次加载后比较姓名与正文，并检查可见画面。
- 可选的 PC 原档加载、再保存和再次加载，比较正文、姓名、选择支与整帧像素。
- 普通 PC 对白档读回后继续推进一页。选择支存档读回后分别执行前两个分支。
- 暂停/恢复状态，以及音频 load/play 事件、slot/generation 和对应 Ogg 缓存文件。

测试 APK 的 fixture provider 只创建应用私有的合成测试数据，通过 URI 授权调用实际 SAF 复制代码。播放器交付 APK 包含 `app/src/main` 下的界面和宿主。

JNI 帧缓冲只包含游戏图层，文字由 Java 界面绘制。`PROLOGUE1.YKS` 在 code word `0x75` 加载黑色背景，
随后 `0x99`、`0xa0` 等对白等待点继续使用该背景，因此黑色帧缓冲需要结合正文与界面状态判断。

## 前台音频焦点回归

完整 JNI 烟测生成 Ogg 缓存后，可以单独运行音频焦点测试。

```powershell
.\android\test.ps1 @test -Runner AudioFocusInstrumentation
```

该测试启动播放器 Activity 满足 Android 15 的前台音频焦点条件，创建真实 MediaPlayer，模拟宿主收到永久 `AUDIOFOCUS_LOSS` 回调，再验证系统重新申请焦点成功。覆盖生命周期恢复、`resume_all` 恢复及引擎暂停期间保持音频暂停。结果字段为 `audio_focus_resume` 和 `paused_engine_guard`。

模拟器中已通过实际点击检查触摸交互。ARM64 实机、扬声器实际输出及不同厂商设备的生命周期行为仍需实机验证。

## 通用播放器 0.2.0 实测记录

验证设备为 AOSP Android 15 x86_64 模拟器，`Yuka-Rs-0.2.0.apk` SHA-256 为 `5083D6EC05A63F5B4217CD8483C57BDCAAFD1FDD86D79BADCE6E2F0E96993069`。

- Android 生成 `savedata.005`，原版 PC 程序加载并推进后写出 `savedata.021`。
- Android 读取 `savedata.021`、再保存、再次读取并继续对白，结果为 `original_pc_round_trip=true` 和 `original_pc_continued=true`。
- 真实剧情选择档 `savedata.006` 在 Android 恢复并分别执行两个分支，结果为 `restored_choice_branches=2`。
- 原始 `savedata.001` 与原 PC 选择后回存的 `savedata.022` 均完成读取、再保存、再次读取和继续对白。
- 独立音频焦点测试通过前台恢复与引擎暂停状态检查。

上述四组存档场景及独立音频焦点测试均返回 `INSTRUMENTATION_CODE: -1`。
