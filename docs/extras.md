# Original Loverable extras

The Android host reads the supplied `cg_mode_*`, `view_cg`, and `play_h` scripts
with the same bounded VM used for the story. Catalogue probing executes only
control flow and metadata commands in a private VM: it never writes game
progress, read bits, files, images, or audio. Unknown probe commands fail with
their code-word position instead of silently discarding behaviour.

The catalogue contains 80 CG groups (17/17/17/17/12 across the five tabs), 20
recollections (3/5/5/4/3), and 29 BGM tracks. Actual availability comes from the
loaded original `System.dat`: GlobalFlag200 enables extras, CG flags start at
220/250/280/310/340, and scene flags start at450/460/470/480/490. The native
image/replay methods check those flags too. Catalogue discovery uses synthetic
completion values only inside the detached metadata VM, never in user memory.

The four original Type combinations select the original image sequences and
scene variables. Nested `selectE` image branches are both available in the CG
viewer's forward/back variation list; their original two-choice overlay is
flattened into that list. Full-screen CG planes are evicted from the resource
cache after transfer so browsing variants cannot retain every decoded image.
Thumbnails use sequence0 of the original 100×150 sprite sheet (100×75 crop).

Scene recollection enters the original `play_h.yks` dispatcher with the selected
character, scene and Type variables. It retains original globals, names and
read records. Its final jump to `cg_main.yks` is returned to the host as
`return_to_extras`. The ordinary new-game entry remains `root_Common`.

Gallery music uses the original slot1 looped Ogg resource and real sound groups.
Only the sound clock resumes in this menu; the story VM remains paused. Foreground
lifecycle handling remains the Android host's responsibility. Original UI
planes are loaded from the supplied archives; no original artwork is checked in.

The OP movie is external to the YKC archives at `avi/lave_op.mpg` in the supplied
game folder; the source file is 243,234,820 bytes. It must be bundled/imported
separately for actual movie playback.
