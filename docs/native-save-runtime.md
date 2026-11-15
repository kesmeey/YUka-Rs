# Native save runtime bridge

This implementation targets the supplied Chinese Loverable executable with SHA-256 `84aef0e66123edfdf265449292abe87b6d46aa268e66fdd219614fb436a329bf`. The binary container layout is documented in `native-save.md`. Runtime conversion is implemented in `src/runtime/native_persistence.rs` and global persistence in `src/runtime/native_globals.rs`.

## Save and load entry points

`Runtime::save_native` writes original `savedata.NNN` containers. `Runtime::load_native` parses and validates the complete replacement session before replacing the active runtime. Stable dialogue and choice boundaries are supported. Android automatic saves use `savedata.001`, matching the supplied `AutoSave.yks` argument to `SaveDataSave`. The Windows menu continues to use its existing JSON slots.

The original `SaveDataSave` handler at `0x43e870` calls `0x43e780`, then `0x43e0c0`. Engine state serialization begins at `0x43b3a0`. Certain native menu modes copy the temporary `savedata.---` snapshot through `0x42fc80` and update its comment through `0x42fe00`.

`SaveDataLoad` at `0x436400` sets the requested slot in `0x562814` and a pending flag in `0x562810`. The script runner at `0x4480e0` processes that request through `0x448080` and `0x447c50`. The engine object has address `0x4c4178`. The state reader is `0x4431e0`.

Original saves contain the active graphical, audio and script state. Loading resumes the saved code word. At a dialogue boundary, the exporter stores the `KeyWait` word as current PC and the following instruction as next PC. Import executes that `KeyWait` once and presents the restored text. A pending `Select` similarly re-enters the original selector and recreates its case list. Script commands preceding the boundary have already contributed their stored state.

The outer header contains no timestamp. `0x42fa40` calls `0x42f470`, which reaches the file-attribute helper at `0x426950`. That helper calls `GetFileTime` at `0x4269c2` and converts the file time to local time at `0x4269dc` through `0x4269f4`. Atomic replacement produces a new filesystem modification time.

## Script activations

Each native frame has a `0x50`-byte header. The writer is `0x42fe60`, and the reader is `0x43b900`. Local label calls and cross-file calls both become ordered native frames. Suspended callers point at their return PC and retain their assignment target. The VM importer locates the preceding `ScriptCall` and reconstructs its continuation.

| Frame field | Native object field | Meaning |
| --- | --- | --- |
| `0x28` | `0x0c` | Current code-word PC |
| `0x2c` | `0x10` | Next code-word PC |
| `0x30` | `0x3c` | Pending assignment target element index |
| `0x34` | byte `0x40` | Assignment target is active |
| `0x38` | `0xc4` | Execution mode |
| `0x3c` | byte `0xc8` | Inherited native execution flag |
| `0x40`, `0x44` | `0x2c`, `0x30` | Local value table and count |
| `0x48`, `0x4c` | `0x38 != 0`, `0x34` | SelectID active flag and ID |

Assignment target behavior is visible at `0x442f85` through `0x442f96`. Return-value storage tests the active byte at `0x442aba`, resolves the target index at `0x442acc`, and clears it at `0x442f3a`. Runtime-exported frames use manual execution mode 1.

`SelectID` at `0x434960` sets the separate ID and active flag at `0x434998` and `0x43499b`. Selection read-history lookup uses them at `0x443dca` through `0x443dd9`. The codec preserves both fields, while the VM rejects an active SelectID during import. The tested game scripts use the ordinary selection path.

Native local element type `0x0a` changes to `0x0b` after integer assignment at `0x442e8a`, or `0x0c` after string assignment at `0x442ed0`. Resource validation recognizes those runtime type changes and checks the corresponding 12-byte local record. Cached function addresses are discarded. Embedded code and symbol tables are checked against the installed script resource. Supported mutable integer data words are restored separately.

## Engine state

The state header has `0x15c` bytes. Local `Flag` values comprise 65,536 signed integers. Local `String` values use 65,536 absolute-offset and length pairs. Those banks belong to the slot. Global variables belong to `System.dat`.

| State field | Meaning |
| --- | --- |
| `0x08` | Default text layer |
| `0x34`, `0x38` | Native save and load enable flags |
| `0x4c`, `0x50` | Particle image path |
| `0x64` | Particle effect active flag |
| `0x98`, `0x9c` | Selector normal and hover text colors |
| `0xa0`, `0xa4`, `0xa8` | Selector mode and mode arguments |
| `0xac` through `0xbc` | Selector target layer, offsets and item spacing |
| `0xc8` through `0x104` | Sixteen timers, stored as negative elapsed milliseconds modulo 2³² |
| `0x124`, `0x128` | Current page voice paths separated by tab bytes |
| `0x130`, `0x134` | Selector previously-read normal and hover colors |
| `0x13c`, `0x140` | Primary cursor resource path |
| `0x144`, `0x148` | Secondary cursor resource path |
| `0x154` | Native input mask |

Imported template fields retain their original values unless the runtime owns and updates that state. New snapshots initialize the verified ordinary-story defaults. Active particle state is rejected. Sixteen timer values are converted to the platform's monotonic clock.

`SelectMode` at `0x434630` and `SelectPosition` at `0x434710` update the selector fields. Re-entering `Select` reads its target layer at `0x443c73`, resolves it at `0x443c7d`, and uses that layer's text cursor at `0x443e05`. The exporter retains the actual target layer and original button visibility while the runtime displays its own choice menu. The caller frame remains suspended until the selected original `Case` body returns.

## Graphics and text

The layer writer at `0x413c90` emits a `0x130`-byte record. The reader at `0x414e20` restores it. Supported fields include slot ID, image resources, position, visibility, inverse-thousandths transmittance, additive COLORREF, and the verified sepia, grayscale and invert layer types.

| Layer field | Meaning |
| --- | --- |
| `0x08`, `0x0c` | Layer ID and type |
| `0x20`, `0x24`, `0x28` | X, Y and visibility |
| `0x2c`, `0x30` | Font and text color |
| `0x34`, `0x38` | Text cursor |
| `0x3c` through `0x48` | Text window rectangle |
| `0x5c`, `0x60` | Image path |
| `0x64`, `0x68` | External image path |
| `0x6c`, `0x70` | Drawable array and count |
| `0x74` | Transmittance, 0 opaque and 1000 transparent |
| `0x124`, `0x128` | Additive and subtractive color |

The verified five-argument `GraphicLoad` YKG path replaces the earlier color image. Import resolves that final image. Unsupported external resource formats, subtractive colors, active transforms, and general drawables outside the known text layers are rejected.

Completed scrolling can leave velocity doubles in layer fields `0x90` and `0x98`. The writer at `0x413e64` through `0x413e86` always stores those values. Native tick code at `0x41560e` branches directly to `0x4158c7` when scroll mode `0x88` is zero. Import therefore accepts and preserves dormant velocity values with mode zero. Original-executable slot 021 contains this state with Y velocity 0.1125 and final position `(0, 0)`. Active scrolling remains unsupported during native import.

Drawables use `0x78`-byte headers. Type 3 is text. The native reader at `0x415392` selects either the one/two-byte character at `0x3c` or a string addressed by `0x54` and `0x58`, then constructs it through `0x4080f0` or `0x408170`. Text-layer UI records from imported snapshots are retained by the exporter.

Fresh snapshots reconstruct the supplied game's `createText.yks` and `createMemu.yks` setup. Layers 11 and 12 contain the original message and name images at `(3, 473)` and `(0, 437)`, with transmittance 300. Hidden layers 101–103 supply the menu sprite sheets. Layer 13 retains the nine original normal-state menu sprites, their native frame records, and the key-wait icon position. Their visibility follows `showWindow`, including the empty name frame during narration. Existing imported layer visibility and menu records are retained. The name layer's `GraphicTextRect` has height 40, while the dialogue layer retains its original text-window bottom of 600.

Export reconstructs ordinary body and speaker glyphs using a 25-pixel double-byte advance, 13-pixel single-byte advance and 32-pixel line spacing. The saved bitmap bounds include the original glyph padding, producing 29×28 bounds for a double-byte glyph. Native drawable fields `0x30`, `0x64`, and `0x6c` use the verified ordinary-story zero defaults, with `0x68` set to `-1`. These fields and bounds were compared with a Chinese dialogue slot written by the original executable. Encoded characters retain the original inline representation at `0x3c`. An isolated original executable loaded the corrected fresh snapshot with readable Chinese dialogue, the original windows and all nine menu buttons, then reloaded through its own Load menu and continued to the next dialogue. The runtime uses its own font renderer, so glyph shape and layout fidelity remain bounded by that renderer. Import derives logical line breaks from stored glyph coordinates and the text-window boundary. A line break exactly coincident with automatic wrapping is ambiguous in the native glyph representation. Layer translation is applied once when converting between glyph-local coordinates and scene offsets.

Stable cropped layers and live child sprites currently cannot be exported. The regular save eligibility check also excludes active scene and audio transitions. The thumbnail is a 100×75, 24-bit BMP of 22,554 bytes.

## Android menu previews

The Android save/load pages follow the supplied game's `m_save.yks`, `m_load.yks`, and `m_YesNo.yks`.
Each of twenty pages contains five rows. The file number is `pageIndex * 10 + row`, with row ranging from one to five.
Slot 001 is the automatic save, and `q_save.yks` / `q_load.yks` use slot 999. Android retains a read fallback for the earlier host's slot 099.
GlobalFlag85 and GlobalFlag86 retain the last page and row in the original System.dat.

`NativeSavePreview` reads the 0x48-byte container header and bounded referenced fields through `Read + Seek`.
The menu shows the chapter as TITLE, the comment as MEMO, the file modification time as TIME, and the original BMP thumbnail.
It requests five slots when opening or changing pages. A thumbnail decode error retains readable slot metadata and allows the full loader to validate the state.
Text fields are limited to 64 KiB and the thumbnail to 16 MiB during preview decoding.
The full native state loader still validates the complete VM snapshot on restoration.

Manual remarks are encoded with the existing native text codec before writing. Default manual and quick-save remarks are empty.
Original UI pictures stay in their YKC archives. JNI exposes decoded ARGB images and a validated first frame for each requested sprite sequence.
Android excludes layers 11–14 while composing the scene, then draws the message window and text at the original UI coordinates.
The original layers remain available to native save export. Android uses a substitute system font and the original default window opacity.

## Audio

`0x459f10` writes sound records and `0x45b8d0` restores them. Each record has `0x20` bytes. Fields `0x0c`, `0x0d` and `0x0e` hold playing, looping and streamed flags. The resource path is at `0x10` and `0x14`, and volume at `0x18` ranges from 0 to 1000. Field `0x1c` retains the stream-load argument.

The native loader restarts a sound only when both playing and looping are set. Runtime import follows that behavior. Looping sound restarts at the beginning of the resource. Nonzero stream-start arguments are rejected.

The supplied `voiceSetting.yks` maps groups 3 through 14 to voice slots 10 through 21. `createHistory.yks` selects those groups with `HistorySoundGroupSet`. The sound dispatcher tests that membership at `0x442149` through `0x442186`, records the resource path at `0x4421e0` through `0x4421ff`, and joins multiple paths with the tab byte stored at `0x49f8dc`. `PF` calls the history collector at `0x43f613`, which clears the accumulated paths at `0x43f573`. Runtime persistence tracks this ordinary-story voice list and the `WindowNameSet` title. Voice-group mapping currently follows this verified game's setup.

## Global data and read history

`System.dat` stores 10,000 global flags and string entries. Existing unknown fields and auxiliary font data are preserved. Successful slot loading preserves the running session's global variables and read history. The first import into an unconfigured runtime can initialize those records from the slot directory. Restart flushes global data before rebuilding the story session.

`Already.dat` uses the original `YKALREADY01` representation. The dispatcher at `0x442483` through `0x4424bb` marks the current script code word. The runtime marks its corresponding instruction before execution and retains previously imported records. Native global filenames are resolved case-insensitively and written using their existing spelling.

## Validation

Four ignored integration tests require `YUKA_TEST_GAME` to point to the supplied game directory. Their output stays under ignored `evidence/` directories.

- Original `savedata.001` import, native export, re-import, 40 further dialogue pages and a second native checkpoint compare PC and dialogue. Fresh-game export is exercised independently.
- The real `selectA.yks` script enters a two-option choice, saves, reloads, and executes the same original second `Case` body and return path.
- `CHAPTER3-3.YKS` reaches its first actual story choice after thirteen pages. Its native snapshot retains both script frames and the visible original choice artwork. Both choices return to the caller and advance another five pages.
- Global integer and GBK Chinese string changes survive loading an older slot and restarting. The generated `Already.dat` contains the root code-word marker. A truncated slot leaves the current PC and dialogue intact.

```powershell
$env:YUKA_TEST_GAME = 'path-to-the-supplied-game'
cargo test --lib runtime::native_persistence::tests -- --ignored
```

These tests exercise the Rust reader and writer against original files and scripts. In an isolated original-executable process, the Rust-exported ordinary-story snapshot loaded and continued through fifteen pages. A subsequent original-executable save imported into the Rust runtime at PC 56 and advanced through PC 63 and PC 70.

The original executable also loaded the real chapter3-3 choice snapshot `savedata.006`. Its own hit testing accepted a click on the first option, returned to the saved caller, and continued through eleven dialogue waits from native PC 338 to PC 556. The original writer produced `savedata.022` at native PC 345 after that selection. Native PCs identify the waiting instruction, and the Rust public PC points to the following instruction.

Android 15 x86_64 emulator testing completed a full cross-engine round trip. Android generated slot 005, the original PC executable loaded it and continued through nine waits, and its writer produced slot 021. Android then imported slot 021, exported and imported it again with matching dialogue, speaker, choices and layer framebuffer, and continued the story. The same APK restored both branches of choice slot 006. Foreground audio-focus recovery and actual interface clicks were also tested.

The final instrumentation runs for original slot 001, PC-returned slot 021, choice slot 006 and post-choice PC-returned slot 022 all completed successfully. Slots 001, 021 and 022 passed native re-export/re-import and continued dialogue. Slot 006 restored and executed both choices. The independent audio-focus instrumentation also passed.

The tested APK SHA-256 is `5083D6EC05A63F5B4217CD8483C57BDCAAFD1FDD86D79BADCE6E2F0E96993069`. ARM64 hardware, speaker output and manufacturer-specific lifecycle behavior require further device testing. Other Yuka builds require their own format and execution validation.
