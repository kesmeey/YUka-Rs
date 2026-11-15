# YKG000 图像容器

`src/graphics.rs` 提供平台无关的 YKG 容器解析、图像层读取和动画记录解析。当前接受 64 字节头的 `YKG000`，要求 encryption 字段为 0。

## 文件布局

所有头字段使用小端编码。图像数据内部保持自身格式的编码规则。

| 偏移 | 字节数 | 内容 |
| --- | --- | --- |
| `0x00` | 6 | `YKG000` |
| `0x06` | 2 | encryption |
| `0x08` | 4 | 头长，当前支持 `0x40` |
| `0x0c` | 28 | 保留数据，解析器原样保留 |
| `0x28` | 4 | color offset |
| `0x2c` | 4 | color length |
| `0x30` | 4 | alpha offset |
| `0x34` | 4 | alpha length |
| `0x38` | 4 | animation offset |
| `0x3c` | 4 | animation length |

offset 为文件绝对偏移。不存在的段使用 offset=0、length=0。解析器检查每段位于头后、未越过文件结尾且各段互不重叠。首版不猜测缺失的长度或偏移。GARbro 包含旧格式的回退处理，尚缺本地样本验证。

## 图像层

`Ykg::parse(&bytes)` 返回借用输入缓冲区的结构。`color` 和 `alpha` 分别提供完整原始 `data`、offset、length、格式和可识别的宽高。

- PNG 使用标准的 `89 50 4e 47 0d 0a 1a 0a` 签名。
- GNP 使用 `89 47 4e 50 0d 0a 1a 0a`。`normalized_bytes()` 只把签名恢复为 PNG，其余字节保持原样。返回的 PNG 可交给平台图像解码器。
- BMP 读取 OS/2 的 12 字节 DIB 或至少 40 字节的 Windows DIB 中的宽高。负高度对应从上向下排列，报告高度取绝对值。
- 未知格式仍保留原始 payload，format 为 `unknown`，宽高为空，导出扩展名为 `.bin`。

PNG/GNP 会检查 IHDR、chunk 范围、IDAT 的存在和 IEND。尚未验证 PNG CRC、解压像素、合并单独 alpha 层或提供渲染功能。BMP 也只验证基础结构及范围，完整像素解码留给后续图像后端。

## 动画记录

animation 段由 32 字节记录组成，每条包含八个小端 `u32`。

| 字段次序 | 解析字段 |
| --- | --- |
| 0–3 | x、y、width、height |
| 4 | duration |
| 5 | flags |
| 6 | unknown |
| 7 | action |

`frame_data` 保留原始数据，`frames` 提供结构化记录。各值不转换为运行时播放行为。上游工具把 flags 的 1、2 位命名为 Reverse、Loop，但本地样本还出现 4 和 6，因此代码保存完整数值。duration 的时间单位及 action 的执行规则仍需结合原引擎运行行为验证。

## 本地验证

仅检查用户提供的《情侣热恋中》目录。先通过独立 Python 读取归档目录及 YKG 头，再运行 Rust 的 `local_game_graphics` 测试逐个读取并解析全部 YKG。7403 个样本全部通过，Rust 测试耗时 9.97 秒，数量与独立扫描一致。

| 归档 | YKG 数量 | 主图像格式 | 带动画的 YKG | 动画记录 |
| --- | ---: | --- | ---: | ---: |
| data02.ykc | 2057 | PNG | 124 | 1334 |
| data03.ykc | 4074 | GNP | 0 | 0 |
| data04.ykc | 1272 | GNP | 0 | 0 |

7403 个头的 encryption 都为 0，头长都为 64，均未使用单独 alpha 段。

可在 PowerShell 中重跑 Rust 解析器的真实数据验证。

```powershell
$env:YUKA_GAME_DIR = 'D:\gal\love loabe\【PC】情侣热恋中\情侣热恋中（ラブラブル)'
cargo test --offline --lib graphics::tests::local_game_graphics -- --ignored --nocapture
```

该测试逐个读取本地归档中的 YKG，不把游戏图片加入代码仓库。常规单元测试使用独立构造的 1×1 PNG 和容器，检查 GNP 恢复、分层、未知 flags、截断、越界和段重叠。

## 格式参考

- [AtomCrafty/YukaTool 2 Ykg.cs](https://github.com/AtomCrafty/yukatool2/blob/main/src/Yuka.Core/IO/Formats/Ykg.cs)，GPL-3.0。
- [AtomCrafty/YukaTool 2 Animation.cs](https://github.com/AtomCrafty/yukatool2/blob/main/src/Yuka.Core/Graphics/Animation.cs)，GPL-3.0。
- [AtomCrafty/YukaTool 2 YukaGraphic.cs](https://github.com/AtomCrafty/yukatool2/blob/main/src/Yuka.Core/Graphics/YukaGraphic.cs)，GPL-3.0。
- [morkt/GARbro ImageYKG.cs](https://github.com/morkt/GARbro/blob/master/ArcFormats/Yuka/ImageYKG.cs)，MIT。

前三份参考文件已下载至 `references/`，以便追溯格式字段。
