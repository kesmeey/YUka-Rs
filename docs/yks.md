# YKS001 脚本格式与解释器记录

本项目以用户提供的 `Loverable_cn.exe` 和 `data01.ykc` 为实际样本。YKS001 的区段布局、符号类型和指令分组以 AtomCrafty 的 YukaTool 2 为格式参考，并使用本地游戏全部 175 个脚本检验。对应原始源码保存在 `references/yukatool2-script/`，项目沿用 GPL-3.0 许可。格式参考来自 [Disassembler.cs](https://github.com/AtomCrafty/yukatool2/blob/main/src/Yuka.Core/Script/Binary/Disassembler.cs)、[DataElement.cs](https://github.com/AtomCrafty/yukatool2/blob/main/src/Yuka.Core/Script/Data/DataElement.cs) 和 [Decompiler.cs](https://github.com/AtomCrafty/yukatool2/blob/main/src/Yuka.Core/Script/Decompiler.cs)。

## 文件布局

多字节整数使用 little endian。头长至少 48 字节。所有区段范围均相对于脚本文件开始处，符号中的数据偏移相对于 data 区段开始处。解析器拒绝截断、区段重叠、超出文件范围和未支持的加密标记，保留未知头字段和未知符号原始字段。

| 文件偏移 | 长度 | 含义 |
|---|---|---|
| `0x00` | 6 | `YKS001` |
| `0x06` | 2 | data 区段处理标记，0 为原始字节，1 为每字节 XOR `0xAA` |
| `0x08` | 4 | header_size |
| `0x0c` | 4 | 未知字段 |
| `0x10` | 4 | code 区段偏移 |
| `0x14` | 4 | code 的 u32 字数 |
| `0x18` | 4 | index 区段偏移 |
| `0x1c` | 4 | index 记录数，每条 16 字节 |
| `0x20` | 4 | data 区段偏移 |
| `0x24` | 4 | data 字节数 |
| `0x28` | 4 | 局部寄存器数量 |
| `0x2c` | 4 | 未知字段 |

字符串以 NUL 结尾。`Script::parse` 默认按 Shift-JIS 解码，`parse_with_encoding` 还支持 GBK 和 UTF-8。每个字符串同时保存原始字节和解码错误标记，供核对汉化补丁。当前用户版本的中文标题用 GBK 能正确显示。不能将这一观察外推到所有 Yuka 汉化版。

## index 记录

记录由 `type, field1, field2, field3` 四个 u32 组成。字段没有专门注明时仍原样保留。

| type | 解析类型 | 已知字段 |
|---|---|---|
| `0x00` | Function | field1 指向函数名，field2 在工具编译器中记录最后使用位置 |
| `0x01` | Control | field1 指向标签/操作符名，field2 指向有符号 i32 link |
| `0x04` | Integer | field2 指向有符号 i32 值 |
| `0x05` | String | field2 指向字符串 |
| `0x07` | SpecialString | field1 指向特殊字符串名称 |
| `0x08` | IntegerVariable | field1 指向变量区名称，field3 指向变量编号 i32 |
| `0x09` | StringVariable | 同上，值类型为字符串 |
| `0x0a` | Local | field2 为局部寄存器编号 |

本次实际脚本出现了 `0, 1, 4, 5, 8, 9, 0x0a`。其他 type 作为 `Unknown` 保留，执行或作为顶层指令解码时明确报错。YKS002 当前明确拒绝，尚无对应样本检验。

## 指令流

code 存放 index 引用。遇到 Function 引用后，下一 u32 是实参个数，再后面是相应数量的 index 引用。Control 引用构成单字标签，变量、Local 和可写 Integer 引用构成单字赋值目标。反汇编中的地址是 code 的 u32 字索引，和文件字节偏移不同。

标签 link 中非负数可能表示 code 字索引，操作符也使用特殊 link 值，例如当前样本的 `+` 使用 65535。实际跳转必须同时检查目标范围、指令边界和目标标签身份，不能仅凭数值落在 code 范围内接受。

`functions()` 返回静态符号清单。`disassemble()` 返回按边界分组的指令，不运行函数。`format_instruction()` 用于生成可读分析输出。

## 已实现的执行子集

`Vm::step` 支持赋值、局部寄存器、变量区、可写 data 整数、整数表达式、字符串拼接与相等比较、`if` / `else` 块、同脚本单标签参数的 `ScriptCall` / `ScriptJump`、`Return`，以及 host call 暂停。整数运算包括 `+ - * / % = < >`，支持一元正负号，优先级依次为比较、加减、乘除取余，同级从左向右求值。加减乘按 i32 回绕，除数或被除数为零时 `/` 返回零。除法溢出和无效取余返回错误。

`if` 的 false 分支跳至后续 `{` 的 link，可能指向结束 `}` 或 else 块的 `{`。顺序执行遇到 `else` 时，跳过后续块并从对应 `}` 之后继续。所有目标必须位于指令边界，非法 link 保留失败位置并返回错误。循环和菜单跳转等尚未实现的控制函数仍由宿主边界报告，并拒绝普通函数恢复。

`Vm::new(&Script)` 默认对未初始化变量返回错误。游戏宿主可显式调用 `initialize_default_banks()`，为 `Flag` / `GlobalFlag` 的 0..10000 编号提供初值零，为 `String` / `GlobalString` 的 0..65536 编号提供空字符串。初始化采用惰性默认值，`memory()` 仅列出实际写入的槽位。游戏名称等启动状态通过 `set_variable` 显式传入。

宿主函数通过 `Event::HostCall` 报告，反复调用 `step` 会再次得到同一个待处理事件。宿主完成普通函数后调用 `resume_host` 恢复。已识别的脚本控制函数以及带标签参数的函数拒绝这一恢复方式。返回值必须符合赋值目标类型，验证失败时保留当前调用。

VM 持有 `Arc<Script>`，`new_owned` 可直接共享已解析脚本。跨文件调度器在字符串参数的 `ScriptCall` 上暂停 caller，新建 callee 并导入 `shared_memory()`。callee 根 frame 返回或结束后，将其共享变量用 `replace_shared_memory` 回灌 caller，再用 `resume_script_call(return_value)` 完成调用。此接口仅接受待处理的单路径 `ScriptCall`。外部 `ScriptJump` 加载目标后调用 `resume_script_jump(Arc<Script>)`，替换当前 frame，自动保留共享变量和此前的本地调用 frame。本地 caller 同时保存其原始 script，之后 `Return` 可恢复对应脚本与 PC。Local 和 data 覆盖层只属于所在脚本 frame，共享变量导入会保留它们。

原始程序的静态反汇编支持以下执行规则，完整记录在 `evidence/native-disassembly.txt`，函数地址均针对 SHA-256 为 `84aef0e66123edfdf265449292abe87b6d46aa268e66fdd219614fb436a329bf` 的本地汉化 EXE。这里的证据是静态控制流，尚未完成原始引擎与 Rust 的动态逐步对照。

| 规则 | 对应地址 |
|---|---|
| `=` 单参数返回整数或字符串结果，由分发器写回赋值目标 | handler `0x43ab80`，写回 `0x442ac4` 起 |
| 整数表达式生成后缀队列，比较优先级 7、加减 8、乘除取余 9 | `0x43ad2b` 至 `0x43ae17` |
| 字符串表达式拼接原始字节，`=` 比较两侧拼接结果 | `0x43af06` 至 `0x43b266` |
| `if` 的条件为零时将 next PC 设为后续 `{` 的 link | `0x434f60` 至 `0x434fbc` |
| `ScriptCall(:label)` 创建同文件新 frame 并将 PC 设为标签 link | `0x43dc18`、`0x43dce6` 至 `0x43dcf6` |
| `ScriptJump(:label)` 直接将 next PC 设为 link | `0x43dec7` 起 |
| `Return` 将 next PC 设为 code_count，可带一个整数返回值 | `0x434350` 至 `0x434395` |
| 每个调用重新复制 code/index/data，重新分配 Local 值记录 | loader `0x439160`，复制点 `0x439337`、`0x43936f`、`0x4393bf` |
| Flag/String 位于共享 context，GlobalFlag/GlobalString 位于进程静态区 | context `+0x104`、`+0x40104`，静态 `0x54ed30`、`0x558970` |
| 当前 frame 结束后 unlink/free 并恢复上一 frame | runner `0x4480e0`，释放 `0x44817c` 调用 `0x43e060` |

Rust 为同文件调用保存 caller 的 Local 和 data 整数覆盖层，callee 从初始脚本数据开始。返回时恢复 caller 覆盖层，保留共享变量区更新。原始 code/index 通过不可变结构共享，当前实现的指令不会修改这些部分。

## 本地启动脚本的预期执行

`Start.yks` 前三条赋值设置 `GlobalFlag[1000]`、游戏标题和版本字符串。之后调用 `SETUP_BOOTCHK`，该子程序只有 `Return()`，再调用 `SETUP_SYSTEM`。独立 VM 可以继续到 code 字索引 43，即 `ScriptCall("yks\\etc\\HP_Install.yks")`，并在文件脚本加载边界暂停，等待宿主调度。

普通剧情 `PROLOGUE1.YKS` 开头会调用 `showWindow.yks`。其中 `Flag[235] = 1` 成立时直接返回，否则显示 11 至 14 图层并设置 `Flag[210] = 1`。后续人物差分使用 `Flag[499]` 条件和 else 块，消息框震动脚本使用整数加减、本地标签跳转及嵌套文件调用。这些路径使用上述 VM 子集，具体图形、音频、输入及跨文件调度由 runtime 提供。

回归测试覆盖 data XOR、所有前缀截断、坏 index 引用、参数区越界、未知符号、host 暂停保持、未初始化读取、调用 frame 隔离和共享变量、参数区伪造跳转标签、表达式优先级和失败位置、字符串原始字节、条件块双分支、跨文件调用返回接口，以及禁止将脚本控制函数按普通 host 返回恢复。
