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
