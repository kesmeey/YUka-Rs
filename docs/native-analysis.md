# Loverable_cn.exe 静态逆向记录

本记录用于给 Rust 脚本解释器提供原程序的可复查依据。检查日期为 2026-11-15。

## 样本身份

| 项目 | 观察值 |
| --- | --- |
| 输入 | `D:\gal\love loabe\【PC】情侣热恋中\情侣热恋中（ラブラブル)\Loverable_cn.exe` |
| 大小 | 815104 字节 |
| SHA-256 | `84aef0e66123edfdf265449292abe87b6d46aa268e66fdd219614fb436a329bf` |
| 格式 | PE32，x86 |
| 首选 ImageBase | `0x400000` |
| 入口 VA | `0x473876` |

下文 VA 采用 PE 首选映像基址。当前样本的 `.text`、`.rdata`、第一段 `.data` 恰好具有相同的 RVA 和文件偏移，因此这些段内 `VA = 0x400000 + 文件偏移`。其他段需要按 section 映射，不能套用这一关系。

## REA 的实际使用结果

已使用用户提供的 `D:\gal\rea\scripts\rea.mjs`，版本 6.8.0，执行 `inspect-pe-imports`。完整 REA Evidence 在 `evidence/native-imports.json`。

REA 报告完整读取了 9 个导入 DLL、234 个函数。包括 `d3d8.dll!Direct3DCreate8`、GDI 字体与文字 API、WINMM 混音器、Windows 文件 API 和 COM 对象创建。EXE 字符串同时出现 `ddraw.dll`、`dsound.dll`、`DirectDrawCreate`、`DirectSoundCreate`，这些字符串只能作为动态装载路径的候选线索。

`providers --json` 的结果保存在 `evidence/native-providers.json`。当时 Ghidra 安装目录未配置、IDA MCP 配置未提供、Kuna 路径未配置，Hopper 在该 Windows 主机不可用。后面的指令观察来自已有的 Python `pefile` 和 Capstone，完整标明了工具来源。没有执行目标游戏，也没有进行动态调用观测。

## 原生命令注册表

从字符串地址引用找到的表结构，经脚本分发器读取指令交叉验证。

| 项目 | 值 |
| --- | --- |
| 表首 VA | `0x4B43C8` |
| 表首文件偏移 | `0xB43C8` |
| 单项长度 | 12 字节 |
| 有效项 | 339 |
| 终止项 VA | `0x4B53AC` |

每项包含三个 little-endian `u32`，依次为 NUL 结尾的命令名指针、处理函数指针、原始标志。终止项指向空字符串且函数指针为零。标志在本样本中为 0 或 1，其完整语义尚未确定。分发器在 context 的 `+0x80130` 字节等于 1 时检查该标志，并跳过标志不为 1 的调用。

验证点位于 `0x4425DE` 到 `0x442648` 的名字查找与索引缓存，以及 `0x442A6C` 读取函数指针、`0x442A79` 执行间接调用的指令。这里的函数名来自表中的字符串。

| 命令 | 函数 VA |
| --- | --- |
| `StrOut` | `0x441AB0` |
| `StrOutNW` | `0x441D30` |
| `StrOutNWC` | `0x441D70` |
| `NameOut` | `0x441F60` |
| `=` | `0x43AB80` |
| `If` / `if` | `0x434F60` |
| `ScriptCall` | `0x43DBB0` |
| `ScriptJump` | `0x43DE70` |
| `Return` | `0x434350` |
| `ScriptExit` | `0x433700` |
| `DefaultFontSet` | `0x43A780` |

完整的 339 项及文件偏移、原始标志保存在 `evidence/native-command-table.json`。名称中包含 `=`，只用字母正则筛选会漏掉这一项。

## 脚本对象与符号记录

从分发器连续指令推导的字段关系如下。字段名是分析用名称。

| 脚本对象偏移 | 含义依据 |
| --- | --- |
| `+0x08` | 当前脚本名称指针，用于 ScriptCall 与错误信息 |
| `+0x0C` | 当前 PC，以 code 的 `u32` 元素为单位 |
| `+0x10` | next PC，循环结束写回当前 PC |
| `+0x14` | code 元素数组指针 |
| `+0x18` | code 元素数量 |
| `+0x1C` | 16 字节符号记录数组指针 |
| `+0x20` | 符号记录数量 |
| `+0x24` | data 字节数组指针 |
| `+0x2C` | 动态局部变量的 12 字节值记录数组 |
| `+0x3C` | 等待赋值的目标符号序号 |
| `+0x40` | 是否等待写回函数返回值 |

分发器从 `code[pc]` 取得符号序号，以 `symbol_id * 16` 定位记录。type 为 0 时按函数调用处理，`code[pc+1]` 为参数数量，后续元素为参数符号序号。`0x442A30..0x442A42` 将 `next_pc` 设置为 `pc + argc + 2`，处理函数可再修改它。

函数记录的 `+4` 是命令名字在 data 中的偏移。`+12` 在名字首次解析后被改写为注册表序号加一，因此运行时文件映像会带有缓存状态。其他类型对 `+8`、`+12` 有各自用途。

## 已确认的基本语义

### 变量与返回值

遇到可写符号时，分发器记录赋值目标，将 `next_pc` 加一。随后执行函数，其返回指针若非空则可写回已记录目标。写回完成清除等待标记。

`=` 的单参数分支创建 12 字节结果记录。整数结果以 tag 1 存放在 `+4`，字符串结果以 tag 2 存放复制的字符串指针于 `+8`。整数与字符串分支取决于参数符号 type，具体跳转表也保存在扫描结果。表达式支持范围需要结合完整分支继续验证。

`GlobalFlag` 和 `Flag` 通过 `data[symbol+12]` 指向的 `i32` 读取数组序号。前者使用全局整数 bank，后者使用 context 的整数 bank。`GlobalString` 和 `String` 使用对应字符串 bank。观察到写回范围分别为 `0 <= index < 10000` 与 `0 <= index < 65536`。部分读取分支的边界条件与写回不同，Rust 实现应明确检查合法索引，不能复制潜在越界行为。

### 流程控制

- `ScriptJump` 的单个 type 1 控制参数从 data 中取 link 值，非负时直接设置 next PC。该路径没有额外加一。
- `ScriptCall` 的单个 type 1 控制参数先调用 `0x43B380` 为当前脚本建立调用帧，随后按标签名字查找目标记录，将 link 值直接设置为新帧的 current PC 和 next PC。字符串参数具有加载其他脚本的分支。
- `Return` 将当前帧 next PC 设置为 code count。恰好一个参数时，它还创建 tag 1 整数返回值。
- `If` / `if` 在第一个整数参数为零时检查后面的 `{` 控制符号，并把该符号的 link 值写入 next PC。
- 普通流程遇到 `else` 时检查随后 `{` 的 link，并设 `next_pc = link + 1`。这与 false `If` 的落点处理不同。

### 整数表达式

`=` 的多参数整数表达式分支包含运算符优先级栈。观察到比较 `= < >` 的优先级为 7，加减为 8，乘除取余为 9。同级运算符会弹栈，因此为左结合。

`+`、`-`、`*` 使用 32 位 x86 算术。比较采用有符号语义并产生 0 或 1。除法使用 `idiv`，在分子或分母为零时直接产生 0。取余分支使用 `idiv` 的余数且没有相同的除零检查。Rust 解释器应对原程序会发生 CPU 异常的输入返回明确错误。

上述内容来自静态指令，未通过原游戏运行轨迹进行差分验证。字符比较、嵌套表达式、错误恢复和完整调用帧生命周期仍需继续研究。

### 同脚本调用的帧隔离

`0x42F330` 创建并清零一个 0x120 字节的 frame。frame 的 `+0`、`+4` 构成前后链接，context 的 `+4` 指向当前尾帧。`0x439160` 从资源读取脚本后，分别重新分配并复制 code、index、data 数组，再根据 local count 分配每项 12 字节的局部变量数组。对应分配调用位于 `0x439337`、`0x43936F`、`0x4393BF` 和 `0x43942B`。

因此 `ScriptCall(:label)` 即使调用同一个脚本，也拥有独立的 data 值、index 缓存和 Local 值。`Flag` / `String` 位于 context 中，在调用间共享。`GlobalFlag` / `GlobalString` 位于静态内存中。

`0x4480E0` 外层运行器从 context 尾部取当前帧并调用 `0x4422E0` 分发器。`0x448171..0x44817C` 检查该帧 current PC 是否达到 code count，达到则调用 `0x43E060` 解除链表关联并释放帧。`0x448188` 重新读取 context 尾帧并继续运行。调用者在 ScriptCall 之前已保存的 next PC 因而能够在子帧结束后继续执行。

## 复现

在具有已有 `pefile`、`capstone` 的 Python 环境执行以下命令，会重建命令表和选定地址的反汇编。脚本检查精确 SHA-256，防止将本版本地址误用于其他 EXE。

```powershell
python .\evidence\native-scan.py `
  'D:\gal\love loabe\【PC】情侣热恋中\情侣热恋中（ラブラブル)\Loverable_cn.exe' `
  .\evidence
```

REA 导入表可以用以下命令重新生成。

```powershell
node 'D:\gal\rea\scripts\rea.mjs' inspect-pe-imports `
  'D:\gal\love loabe\【PC】情侣热恋中\情侣热恋中（ラブラブル)\Loverable_cn.exe' `
  --json > .\evidence\native-imports.json
```

`native-disassembly.txt` 保存所选择的指令范围。范围末尾用于界定本次观察，只有明确指出的处理函数才按函数完整边界解释。游戏图像、脚本和音频未写入这些证据文件。
