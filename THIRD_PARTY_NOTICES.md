# Sources and licensing

This project is distributed under GPL-3.0-only, with the complete license in
`LICENSE`. The Yuka formats and script semantics are researched using local game
files and the following published reverse-engineering work.

- AtomCrafty, YukaTool 2, GPL-3.0. Format documentation and parser/compiler sources.
  https://github.com/AtomCrafty/yukatool2
- AtomCrafty, original YukaTool. Additional historical format notes.
  https://github.com/AtomCrafty/yukatool
- morkt, GARbro, MIT. `ArcFormats/Yuka/ArcYKC.cs` and
  `ArcFormats/Yuka/ImageYKG.cs` describe archive and image container formats.
  https://github.com/morkt/GARbro/tree/master/ArcFormats/Yuka

The archive and graphics readers use format information from GARbro. Its notice
is retained below.

Copyright (C) 2015 by morkt

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to
deal in the Software without restriction, including without limitation the
rights to use, copy, modify, merge, publish, distribute, sublicense, and/or
sell copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
IN THE SOFTWARE.

`references/` holds locally downloaded research material. `evidence/` holds
local inspection results. Both are ignored by Git along with extracted assets.
Game resources and the Windows executable are supplied separately by the user.

## lzma-rust2 0.25.0

The Android asset importer uses the native Rust `lzma-rust2` decoder, with
bounded streaming buffers and a 100 MiB decoder-memory limit, to prepare the
losslessly compressed game archives. The library is distributed under the
Apache License 2.0. Its complete upstream license is retained in
`android/licenses/lzma-rust2-LICENSE.txt`.
[Project](https://github.com/hasenbanck/lzma-rust2/),
[pinned source](https://docs.rs/crate/lzma-rust2/0.25.0/source/).

## XZ for Java 1.12

Android instrumentation uses XZ for Java 1.12 (`org.tukaani:xz:1.12`) to
create independent test fixtures and benchmark the former decoder. XZ for Java is distributed under the BSD Zero
Clause License (0BSD). The complete upstream license is reproduced below.
[Project](https://tukaani.org/xz/java.html),
[version 1.12 license](https://github.com/tukaani-project/xz-java/blob/v1.12/COPYING).

```text
Copyright (C) The XZ for Java authors and contributors

Permission to use, copy, modify, and/or distribute this
software for any purpose with or without fee is hereby granted.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL
WARRANTIES WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED
WARRANTIES OF MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL
THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT, INDIRECT, OR
CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM
LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT,
NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN
CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
```
