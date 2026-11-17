# Android option settings

The option screens use the supplied `m_screen`, `m_text01/02`, `m_sound`,
`m_voice01..04`, `vo_volume`, `vo_test`, `createFont`, `createText`,
`voiceSetting`, and `Start.yks` scripts as their source of IDs and defaults.
No private settings fields are appended to native save files.

| GlobalFlag | Meaning | Default/range |
| --- | --- | --- |
| 2 | Gothic / Mincho font family | 0, 0..1 |
| 3 | Already-read / all-text skip | 0, 0..1 |
| 4 | Text character delay in milliseconds | 20, 0..66 |
| 5 | Auto base delay, plus 50 ms per character unless zero | 2000, 0..6600 |
| 10 | Reduced / normal / disabled transitions | 1, 0..2 |
| 11 | Secondary click opens options / hides text | 0, 0..1 |
| 12 / 13 | Resume auto / skip after choosing a branch | 1, 0..1 |
| 14 | Plain / outline / shadow font effect | 1, 0..2 |
| 15 | Transparent / translucent / opaque text windows | 1, 0..2 |
| 16 | Manual dialogue click stops voice | 0, 0..1 |
| 19 | Automatic BGM reduction while voice plays | 0, 0..1 |
| 30..32 / 33..35 | Text / border RGB channels | white / black, 0..255 |
| 100 / 101 | BGM / effects group volume | 650, 0..1000 |
| 102 / 103 / 104 | All / female / male voice assignment sliders | 1000, 0..1000 |
| 110..121 | Twelve individual voice groups | 1000, 0..1000 |

Aggregate voice sliders assign their child flags, matching `vo_volume`; they
are not multiplied again with individual volume. The original fourth voice
page swaps the assignment targets of the manager and Ryuusei sliders in
`vo_volume`, while displaying their normal `voiceSetting` flags. The Android
menu assigns the displayed character's actual flag consistently.

The original `Start.yks` initializes options only when GlobalFlag60 is zero.
The host follows that policy, including migrating profiles produced by older
versions that omitted this setup. Unrelated progress and read markers remain
intact. Setting updates validate the whole patch before changing VM state and
write the original `System.dat` atomically. Loading an older story slot keeps
the current global preferences. Starting a new game initializes local name
strings from GlobalString20/21; the nondefault-name flag is ordinary Flag285.

Sound groups are the original slot ranges: BGM1..9, effects100..599,
history/title group99, and individual voices10..21. Effective output is the
script volume times the selected group volume. The old fixed65% multiplier
is removed. BGM ducking currently uses an immediate50% multiplier, restored
when the actual voice completion callback arrives or the voice is stopped or
muted; this multiplier/envelope has not been measured against the executable.
The reduced-transition mode caps crossfade duration at100ms, while disabled
mode immediately settles transitions and graphic opacity changes. These are
host approximations rather than recovered native quality algorithms.

Android renders typewriter text using Unicode code-point counts, the native
text-speed flag, and pause-adjusted monotonic time. A first click can reveal
the current text without advancing. Auto mode also waits for text completion,
actual audible voice completion, and a300ms final pause. The original
selection scripts now preserve their mode snapshots and honor flags12/13.
Android font families map to its sans-serif/serif families rather than bundled
Microsoft fonts. Native exports preserve configured window transparency and
text color through the known native layer/glyph fields.

The title uses `lv_bgm13.ogg` and the original random heroine title calls.
Only audio resumes at the title: the story VM remains paused and does not
mark the first unseen page as read. Separate settings/history voice players
use the stored voice gains.

Explicit PC-profile import reads a separate staged folder and requires
`System.dat`. It parses that file, optional `Already.dat`, and every encoded
global string before changing current memory or output files. Missing read
records preserve the current `Already.dat`; supplied records replace them.
Known option values are clamped to their menu ranges, while unrelated progress
flags, strings, font blocks, and opaque profile metadata retain native format.
The input folder is not modified. The host publishes each profile file through
atomic rename and rolls back the read-record file if publishing the system
file fails. The Android importer separately backs up the current Save folder
before importing selected story slots. Recollection state is ordinary Flag280;
its UI disables manual saving without restricting the native codec used by
tests and import/export.

Movie requests expose a unique ID, relative game resource path, and volume.
Story `MoviePlay` suspends the VM until completion. Menu OP requests restore
the original wait state without advancing the story. Stale media callbacks
are ignored; completion while backgrounded resumes scripts only after the
next foreground tick. Desktop/headless movie behavior remains explicitly
reported as skipped.
