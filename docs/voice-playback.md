# Dialogue voice replacement

The supplied game assigns individual voice groups to slots 10 through 21;
BGM uses slots 1 through 9 and effects use slots 100 through 599. Previously,
loading a new voice stopped only its own slot. Advancing quickly from one
character to another with click-to-stop disabled could leave both voices playing.

## Verified original behavior

Static inspection uses the read-only `Loverable_cn.exe` with SHA-256
`84aef0e66123edfdf265449292abe87b6d46aa268e66fdd219614fb436a329bf`.
`voiceSetting.YKS` registers voice groups 3 through 14 as slots 10 through 21
and calls `SoundGroupAutomodeSkipSet(group, 1)` for each.

`KeyWait` at 0x441540 calls 0x43f6f0, which sets the engine's byte at +0x80138
to 1 at 0x43f774. In the two-argument `SoundPlay(slot,path)` handler at
0x441ff0, 0x44209c checks the new slot's eligible group via 0x458ef0.
0x4420aa tests the boundary byte; if set, 0x4420b6 calls 0x459050 and
0x4420bb clears it. The helper visits playing sounds and stops eligible voice
groups. The single-argument `SoundPlay(slot)` branch at 0x44221a and
`SoundPlayLoop` at 0x43a940 do not consume that byte.

The runtime marks the boundary when recording a `KeyWait` dialogue page,
including a page reconstructed by loading a save. Only the first subsequent
two-argument nonlooping voice play consumes it. Both audio backends stop the
preceding page's playing voice slots before starting the new voice. BGM and
sound effects do not consume the boundary and are not stopped. Multiple voices
started before the next dialogue wait can still form an intentional chorus.

GlobalFlag16 retains its existing click behavior: value 1 stops voice on a
manual reveal/advance; value 0 lets the preceding voice continue through silent
narration until another voice replaces it. Repeated silent pages do not clear
the pending replacement. Fast-forward retains its explicit voice-stop policy.
No fields were added to `SoundState`, original saves, or global profile files.

## Android events and validation

The Android backend queues old-slot `stop` events before the new `play` event.
`AudioHost.apply` processes them on the same HandlerThread as MediaPlayer
callbacks. A stop sets `requested=false` even while a player is preparing;
a later prepared callback therefore cannot start that old voice. Slot-object
identity and generation checks continue to reject callbacks from replaced
players. No Java behavior change is needed for this handoff.

The Android backend unit regressions verify cross-character stop/play event
order, multiple old voices, BGM/effect preservation, same-page chorus, silent
pages, muted old voices, missing-slot rejection, stale completion, and the
single-argument/loop exceptions. The optional original-data regression advances
`CHAPTER1-1.YKS` through three different-character handoffs with GlobalFlag16=0
and checks that active voice paths belong to the current page while BGM remains
playing. It runs with output muted; it validates script/audio state, not audible
device output:

```powershell
$env:YUKA_TEST_GAME = 'path to supplied original game'
cargo test --lib new_story_voice_replaces_previous_character -- --ignored --nocapture
```

This change was validated without interacting with a phone or starting an
Android emulator. Actual MediaPlayer timing and audible output require device
testing of the resulting APK.
