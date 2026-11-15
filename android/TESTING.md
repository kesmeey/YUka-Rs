# Android 集成测试

`tests/` 构建为独立的 `org.yukars.player.tests` instrumentation APK。测试在播放器应用 UID 下执行真实 JNI，会创建测试存档和音频缓存。

## 构建

主 APK 由 `build.ps1` 生成。测试脚本需要相同的签名密钥，默认使用 `bin/android-development.jks`，别名为 `yuka-rs`。Java 工具链需支持 `javac --release 8`，脚本使用 .NET ZipArchive 打包 class。

以下命令在项目根目录的 PowerShell 7 中执行。将工具路径和设备路径替换为本机配置。

```powershell
$test = @{
    Apk = '.\bin\Yuka-Rs-0.2.0.apk'
    SdkPlatform = '.\target\android-sdk\android-35'
    BuildTools = '.\target\android-sdk\android-15'
    JavaHome = 'E:\DevEco Studio\jbr'
    Adb = 'E:\platform-tools\adb.exe'
    Serial = 'emulator-5580'
    Game = '/data/user/0/org.yukars.player/files/game-smoke'
    Save = '/data/user/0/org.yukars.player/files/smoke-saves'
    Audio = '/data/user/0/org.yukars.player/cache/smoke-audio'
}
.\android\test.ps1 @test -BuildOnly
```

输出为 `bin/Yuka-Rs-tests.apk`。每次构建的中间文件保存在独立的 `target/android-tests/run-*` 目录，Java 源码以 `-Xlint:all -Werror` 编译。

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

## 本轮实测结果

验证设备为 AOSP Android 15 x86_64 模拟器，主 APK SHA-256 为 `5083D6EC05A63F5B4217CD8483C57BDCAAFD1FDD86D79BADCE6E2F0E96993069`。

- Android 生成 `savedata.005`，原版 PC 程序加载并推进后写出 `savedata.021`。
- Android 读取 `savedata.021`、再保存、再次读取并继续对白，结果为 `original_pc_round_trip=true` 和 `original_pc_continued=true`。
- 真实剧情选择档 `savedata.006` 在 Android 恢复并分别执行两个分支，结果为 `restored_choice_branches=2`。
- 原始 `savedata.001` 与原 PC 选择后回存的 `savedata.022` 均完成读取、再保存、再次读取和继续对白。
- 独立音频焦点测试通过前台恢复与引擎暂停状态检查。

上述四组存档场景及独立音频焦点测试均返回 `INSTRUMENTATION_CODE: -1`。
