# Screen shake

The original `Effect` dispatcher at VA0x433bb0 passes its integer arguments to
VA0x422630. The supplied story uses modes1 and5. The static native investigation
used the read-only executable with SHA-256
84aef0e66123edfdf265449292abe87b6d46aa268e66fdd219614fb436a329bf.
Local reproduction is `evidence/effect-probe.py` with Capstone and pefile.

Mode1 calls0x4183f0. It alternates vertical `+amplitude,-amplitude` for four
steps, sleeping the second argument in milliseconds per step, then restores the
original image. The negative/default interval is100ms. Mode5 calls0x418e70.
With `s=amplitude/2` and `q=s/2`, its twelve source offsets are:

```
(-s,-s), (s,-s), (-s,s), (s,s), (-q,-q), (s,s),
(s,-s), (-s,-s), (-s,q), (0,-s), (-q,s), (s,-s)
```

Its negative/default interval is50ms. Both native routines block until their
steps finish, capture the viewport once, and restore the original position.
The new host applies these offsets to the composed framebuffer, exports the
same offset for Java text/window drawing, and blocks the script for the step
duration. Layer coordinates never change, so no positional drift persists.

Pause moves the clock; fast-forward and disabled effects finish the shake and
restore zero. Saving remains disabled while the animation is active, preserving
the original save structures. Simplified mode accelerates the steps by4 for
mode1 and12 for mode5. Those factors occur in native fast paths controlled by
global0x562d2c; their exact relationship to the original settings GUI has not
been verified, so applying them to simplified mode is an explicit host policy.
Zero intervals are clamped to1ms to keep a finite, cancellable clock. UI frame
sampling may skip intermediate positions on lower-refresh devices.
