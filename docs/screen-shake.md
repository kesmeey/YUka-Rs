# Screen shake

The original `Effect` dispatcher at VA0x433bb0 passes its integer arguments to
VA0x422630. Its jump table at VA0x422798 dispatches modes 1 through 6. Static
native investigation used the read-only executable with SHA-256
84aef0e66123edfdf265449292abe87b6d46aa268e66fdd219614fb436a329bf.
Local reproduction is `evidence/effect-probe.py` with Capstone and pefile.

The offsets below use screen coordinates (positive x right, positive y down).
The second argument is the interval in milliseconds per step and the third is
the amplitude `a` (defaults: -1 and 5 respectively).

| Mode | Native routine | Trajectory | Steps | Default interval |
| --- | --- | --- | --- | --- |
| 1 | 0x4183f0 | `(0,a), (0,-a)` repeated twice | 4 | 100 ms |
| 2 | 0x418690 | `(a,0), (-a,0)` repeated twice | 4 | 100 ms |
| 3 | 0x418930 | `(a,-a), (-a,a)` repeated twice | 4 | 100 ms |
| 4 | 0x418bd0 | `(-a,-a), (a,a)` repeated twice | 4 | 100 ms |
| 5 | 0x418e70 | Twelve offsets below with `s=a/2, q=s/2` | 12 | 50 ms |
| 6 | 0x419110 | Twelve offsets below with `s=a, q=s/2` | 12 | 50 ms |

Modes 5 and 6 populate identical twelve-point arrays on the native stack at
offsets 0x18 through 0x74. Mode 5 first halves the amplitude; mode 6 does not.
Division truncates toward zero. Their source offsets are:

```
(-s,-s), (s,-s), (-s,s), (s,s), (-q,-q), (s,s),
(s,-s), (-s,-s), (-s,q), (0,-s), (-q,s), (s,-s)
```

All six native routines block until their steps finish, capture the viewport
once, and restore the original position.
The new host applies these offsets to the composed framebuffer, exports the
same offset for Java text/window drawing, and blocks the script for the step
duration. Layer coordinates never change, so no positional drift persists.

Pause moves the clock; fast-forward and disabled effects finish the shake and
restore zero. Saving remains disabled while the animation is active, preserving
the original save structures. Simplified mode accelerates the steps by 4 for
the four-step modes and 12 for the twelve-step modes. Those factors occur in
native fast paths controlled by global0x562d2c; their exact relationship to the original settings GUI has not
been verified, so applying them to simplified mode is an explicit host policy.
Zero intervals are clamped to1ms to keep a finite, cancellable clock. UI frame
sampling may skip intermediate positions on lower-refresh devices.

## Supplied-game coverage and regression

The 0.2.4 phone error at `yks/story/kaho/kaho_07.yks @1263` was the valid call
`Effect(2,20,1)`. The engine paused because it previously accepted only modes 1
and 5. A complete read-only scan of all supplied `data*.ykc` archives found 175
YKS scripts and 442 literal Effect calls in 67 scripts: 33 mode 1, 6 mode 2,
402 mode 5, and 1 mode 6. All 21 distinct argument combinations are supported.
Mode 6 occurs in `yks/story/Nanako/NANAKO_02.YKS @7979`; modes 3 and 4 do not
occur in this corpus but are implemented from the native dispatch routines.

Unit tests verify every native step for all six modes, odd-amplitude rounding,
blocking duration, defaults, simplified timing, reset, pause, and fast-mode
validation. Unsupported modes and invalid amplitudes are checked before the
fast path. The optional corpus test uses the Rust YKC/YKS decoders and runs
every original Effect call through normal, simplified, and fast validation:

```powershell
$env:YUKA_GAME_DIR = 'path to supplied original game'
cargo test supplied_game_effect_calls_validate_without_fast_path_bypass -- --ignored --nocapture
```

This test reads the input archives only. It does not extract or execute story
content, and it writes no game files or saves.
