# YKC archives

All integer fields below are little-endian. The archive reader accepts the
YKC001 and YKC002 directory formats documented by GARbro. Real-game validation
currently covers the five YKC001 archives of the supplied Lover Able Chinese
edition. YKC002 has synthetic directory coverage only.

| Offset | Size | Meaning |
| --- | --- | --- |
| 0x00 | 8 | `YKC001\0\0` or `YKC002\0\0` |
| 0x08 | 4 | Header size, observed 24 |
| 0x0c | 4 | Reserved, retained |
| 0x10 | 4 | Absolute directory offset |
| 0x14 | 4 | Directory size in bytes |

Each directory record is 20 bytes.

| Offset within record | Size | Meaning |
| --- | --- | --- |
| 0x00 | 4 | Absolute name offset |
| 0x04 | 4 | Name length including terminating NUL |
| 0x08 | 4 | Absolute payload offset |
| 0x0c | 4 | Payload length |
| 0x10 | 4 | Reserved, retained |

Version 1 names use Shift-JIS-compatible decoding through `encoding_rs`.
Version 2 names use UTF-8. Raw name bytes and decoding-error flags are retained
to prevent display text from silently replacing original identity. The reader
rejects ambiguous names when selecting an entry by name. Lookup normalizes
directory separators and ASCII letter case. Real scripts use paths such as
`yks\\system\\OptionDefault.yks` for a member named `yks\\system\\optionDefault.YKS`.
Non-ASCII name bytes remain preserved, and ambiguous folded names are refused.

Archive opening validates directory alignment and name/payload ranges. Names
are limited to 64 KiB of metadata per record. Large payloads are copied in a
stream. The caller supplies a size limit when requesting an in-memory payload.

The `extract` CLI copies one raw entry into an explicitly selected new file.
It uses `create_new`, and deletes its own incomplete file if copying fails.
Archive paths are never used as filesystem destinations. It preserves YKS
obfuscation bytes. The script decoder handles obfuscation separately.

Reference

https://github.com/morkt/GARbro/blob/master/ArcFormats/Yuka/ArcYKC.cs
