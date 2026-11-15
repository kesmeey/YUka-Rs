# Original Yuka save formats

The codec in `src/native_save.rs` reads and writes the binary formats used by the supplied Loverable Chinese executable. All integers are little endian. Every serialized payload offset is an absolute offset from the beginning of its file.

## Verified profile

The executable SHA-256 is `84aef0e66123edfdf265449292abe87b6d46aa268e66fdd219614fb436a329bf`.

Read-only sample validation covers the game's `Save/savedata.001`, `Save/System.dat`, `Save/Already.dat`, and the separately supplied full-CG `System.dat`. Unchanged parse-and-write operations reproduce each original file byte for byte. Editing variable banks and captions rebuilds the section offsets and retains unrelated records.

Native serializer entry points were inspected with Capstone against the executable's PE sections. Slot writing reaches `43e0c0` from `SaveDataSave` at `43e870`. The engine state writer is `43b3a0`, the script-frame writer is `42fe60`, the layer writer is `413c90`, and the sound writer is `459f10`. Original engine loading uses `4431e0` for engine state and `43b900` for script frames. Runtime details and the integration test are documented in `native-save-runtime.md`.

## Slot container

The slot begins with `Yuka  \0\0` and a `0x48`-byte header. Header fields below are pairs of absolute offset and byte length.

| Header offset | Content |
| --- | --- |
| `0x10` | Save caption, NUL-terminated |
| `0x18` | Chapter name, NUL-terminated |
| `0x20` | Engine state |
| `0x28` | Auxiliary payload retained by the codec |
| `0x30` | BMP thumbnail |
| `0x38` | Auxiliary payload retained by the codec |
| `0x40` | Window title, NUL-terminated |

The sample thumbnail is 100 by 75 pixels with a 24-bit BMP header and padded scanlines. Its total length is 22554 bytes.

The engine state header has length `0x15c`. Its variable and record sections are described below.

| State offset | Offset field and following field |
| --- | --- |
| `0x14` | Integer flags, byte length `0x40000` |
| `0x1c` | Sound linked-list head, record count |
| `0x24` | Layer linked-list head, record count |
| `0x2c` | Script-frame linked-list head, record count |
| `0x4c` | Sakura particle image path, byte length |
| `0x110` | String-bank index, byte length `0x80000` |
| `0x124` | Current page voice paths, tab-separated, and byte length |
| `0x13c` | First cursor path, byte length |
| `0x144` | Second cursor path, byte length |

There are 65536 signed 32-bit flags and 65536 string-index records. Each string record contains an absolute offset and byte length. A zero length represents an unallocated string. The original bank writer allocates `strlen + 2` bytes, including the terminator and one spare byte. The logical Rust string representation excludes both bytes. Parsed files retain their original spare-byte contents through the unchanged-file round trip.

## Script frames

Each frame begins with a `0x50`-byte header. Frames are ordered from oldest caller to active child. The `+4` field points to the next record. The final record points past its own payload, and the parent record count terminates traversal.

| Frame offset | Meaning |
| --- | --- |
| `0x08`, `0x0c` | Full script resource path offset and byte length |
| `0x10`, `0x14` | Code array offset and u32 count |
| `0x18`, `0x1c` | Index array offset and 16-byte record count |
| `0x20`, `0x24` | Deobfuscated script data offset and byte length |
| `0x28`, `0x2c` | Current and next code-word PCs |
| `0x30` | Assignment target index |
| `0x34` | Pending assignment byte |
| `0x38`, `0x3c` | Execution mode and inherited mode flag |
| `0x40`, `0x44` | Temporary-register records offset and count |
| `0x48`, `0x4c` | SelectID active flag and ID |

Script paths use one terminating NUL. A temporary record has three u32 fields. Kind 0 is uninitialized, kind 1 carries an i32 in its second field, and kind 2 carries a string offset and byte length. Temporary strings use `strlen + 2` bytes.

Native execution specializes script index kind `0x0a` into `0x0b` for integer temporaries and `0x0c` for string temporaries. The importer verifies these kinds against the corresponding temporary values. Function index records can hold cached process pointers in their fourth word. The VM ignores that cache during resource comparison and clears it when exporting.

`Vm::export_native_frames` flattens local call activations into native records. `Vm::from_native_frame` validates each record against the installed game resource and reconstructs suspended ScriptCall continuations. It retains the resource's original `Arc<Script>` and represents mutable integer-data words in VM memory. Native code, immutable data, index fields, local dimensions, and instruction boundaries are checked before accepting the frame.

Return assignments use frame header `+0x30` and `+0x34`. These correspond to native execution object fields `+0x3c` and `+0x40`. The assignments and return consumers are visible at `442f85` and `442aba`. Imported local calls can return through the same scheduler API used by external calls.

The native SelectID handler at `434960` stores its ID in execution field `+0x34` and sets field `+0x38`. Those become frame header `+0x4c` and `+0x48`. The binary codec retains them. The VM rejects active SelectID frames because this runtime profile has no implementation of that command. The tested Loverable scripts do not use it.

## Graphics and audio records

A layer header has length `0x130`. Resource paths are offset/length pairs at `0x5c` and `0x64`. The drawable table offset and count are at `0x6c`. The layer codec preserves the remaining header fields, including position, visibility, text formatting, colors, and effect settings.

A drawable has a `0x78`-byte header. Its variable payload pairs are at `0x54` for a text payload and `0x70` for extra animation data. The normal native writer emits individual text characters at `+0x3c` and clears the multi-character pointer. Animation frames occupy 32 bytes each in the extra payload.

A sound header has length `0x20`. It stores slot ID at `+8`, playing/loop/stream flags at `+0xc` through `+0xe`, path offset and length at `+0x10`, and volume at `+0x18`. The `+0x1c` word carries the third SoundLoadStream argument. Native loading restarts only sounds whose playing and loop flags are both set.

## System.dat

The system file starts with a `0x7c`-byte structure. Its size field is at `+0x10`. The current-game sample stores `YKAL` at `+0x14`, while the supplied full-CG profile stores the integer 20 there. The codec preserves that word and writes `YKAL` for new profiles. It holds 10000 global integer flags and 10000 global string slots.

| Offset | Content |
| --- | --- |
| `0x18`, `0x1c` | Flag bank offset and byte length |
| `0x20`, `0x24` | Font data offset and byte length |
| `0x44`, `0x48` | Trailing resource/string payload offset and byte length |
| `0x50`, `0x54` | String index offset and byte length |
| `0x68`, `0x6c` | Screen width and height |

The font sample contains a u32 record size of `0x4c`, followed by sixteen records of that size. The codec preserves this block and unknown header fields. Global bank strings use the same `strlen + 2` representation as slot strings.

`SystemDat::default` emits an empty font block. The original Loverable executable successfully initialized with this newly generated profile, loaded an Android-generated slot, and advanced through nine dialogue waits in the isolated compatibility test.

## Already.dat

The read-mark file begins with the 12 bytes `YKALREADY01\0`. The u32 at `+0xc` is the record count and the u32 at `+0x10` is the index offset. Each 16-byte index record contains filename offset, filename byte length, mark byte count, and marks offset. Filenames include a single terminating NUL.

The writer at `401120` emits names and marks before appending the index. The loader is at `401b10`. The execution path at `442483` through `4424bb` indexes read marks by the current script code-word PC. Unknown mark bytes and records remain available for round-trip preservation.

## Validation

`tests/native_save.rs` verifies rebuilt offset tables, negative flag values, string and register relocation, cyclic chains, truncated files, and offset overflow. Its ignored local-data test also checks the original AutoSave frame returning to root_Common at code-word PC `0x29`.

VM unit tests cover native temporary type specialization, integer data overrides, suspended host return assignments, and flattened local calls returning the same values as uninterrupted execution. Original files are read only. Generated compatibility files belong under the ignored `evidence` directory.

### Original executable interoperability

The original executable and resource archives were copied into `evidence/native-pc-compat`. Each run used a dedicated Windows desktop created through `CreateDesktopW`. Frida observed the original slot, state, and frame loaders. The test harness set the native asynchronous load request and sent Enter messages to that copy's window. A process-local foreground-query override allowed execution on the private desktop. The selection test additionally supplied a cursor position inside the first button through a process-local query override and sent mouse messages to that window. The original load/save functions, hit testing, and script interpreter executed their original code.

| Input | Original executable result |
| --- | --- |
| Original `savedata.001` | Restored root_Common PC 41 and AutoSave PC 0, then advanced through nine prologue1 dialogue waits |
| Rust re-export `savedata.002` | Restored prologue1 PC 964, then advanced through fifteen dialogue waits to PC 1247 |
| Rust new-game `savedata.003` | Restored prologue1 PC 47, advanced through nine dialogue waits, and wrote original-engine slot 020 at PC 54 |
| Android-generated `savedata.005` | Restored prologue1 PC 68, advanced through nine dialogue waits to PC 278, and wrote original-engine slot 021 at PC 153 |
| Android `savedata.005` with fresh Rust `System.dat` | Initialized with an empty font block and advanced through the same nine dialogue waits |
| Real chapter3-3 choice `savedata.006` | Restored its caller and SelectA at PC 187, selected the first option, returned to chapter3-3, and advanced through eleven waits from PC 338 to PC 556 |

Native PCs in this table refer to the waiting instruction. Yuka-Rs advances its public VM PC past the wait after displaying it. Original-engine slot 020 was subsequently imported by Yuka-Rs at next PC 56 and advanced to next PCs 63 and 70. Slot 022 was produced by the original writer at chapter3-3 PC 345 after the selection. These original-writer captures were taken synchronously at a KeyWait function entry.

Original-engine slot 021 subsequently completed Android JNI import, native re-export, re-import and continued dialogue in the Android 15 x86_64 emulator. The instrumentation compared dialogue, speaker, choices and the layer framebuffer across that round trip. This closes the Android slot 005 → original PC → original-writer slot 021 → Android execution path. Android also restored both branches of the real choice slot 006.

The final Android runs additionally imported the original slot 001 and the original PC's post-choice slot 022. Both completed native re-export/re-import and continued dialogue. All four slot scenarios and the independent audio-focus instrumentation returned successful completion.

The compatibility test records contain script paths, program counters, loader events, and process status. Tests did not inspect the rendered desktop visually. Original game-save SHA-256 values were rechecked after the runs and remained identical to their initial values.

Compatibility with other Yuka versions requires checking their header sizes and writer semantics. The current decoder reports unsupported layouts and malformed offsets as errors.
