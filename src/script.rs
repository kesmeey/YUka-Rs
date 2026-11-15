//! YKS001 script sections and symbolic instructions.
//!
//! The format and instruction model follow AtomCrafty's YukaTool 2
//! Disassembler.cs and DataElement.cs. See docs/yks.md for provenance.
use crate::Result;
use encoding_rs::{GBK, SHIFT_JIS};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::ops::Range;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextEncoding {
    #[default]
    ShiftJis,
    Gbk,
    Utf8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Text {
    pub text: String,
    pub bytes: Vec<u8>,
    pub had_errors: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Header {
    pub encryption: u16,
    pub header_size: u32,
    pub unknown1: u32,
    pub code_offset: u32,
    pub code_count: u32,
    pub index_offset: u32,
    pub index_count: u32,
    pub data_offset: u32,
    pub data_length: u32,
    pub max_locals: u32,
    pub unknown2: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Element {
    Function { name: Text },
    Control { name: Text, link: i32 },
    Integer { value: i32 },
    String { value: Text },
    SpecialString { name: Text },
    IntegerVariable { bank: Text, id: i32 },
    StringVariable { bank: Text, id: i32 },
    Local { id: u32 },
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexEntry {
    pub kind: u32,
    pub fields: [u32; 3],
    pub element: Element,
}

#[derive(Debug, Clone, Serialize)]
pub struct Script {
    pub header: Header,
    pub encoding: TextEncoding,
    pub code: Vec<u32>,
    pub entries: Vec<IndexEntry>,
    /// Deobfuscated bytes, retained for exact offsets and later VM work.
    #[serde(skip)]
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Instruction {
    /// Offset in u32 code words, never a byte offset or instruction ordinal.
    pub word_offset: u32,
    pub entry_index: u32,
    pub kind: InstructionKind,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InstructionKind {
    Call { arguments: Vec<u32> },
    Label,
    Target,
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    let end = offset.checked_add(4).ok_or("u32 offset overflow")?;
    let value = bytes
        .get(offset..end)
        .ok_or_else(|| format!("truncated u32 at {offset:#x} in {} bytes", bytes.len()))?;
    Ok(u32::from_le_bytes(value.try_into()?))
}

fn section(
    offset: u32,
    count: u32,
    stride: usize,
    size: usize,
    name: &str,
) -> Result<Range<usize>> {
    let start = offset as usize;
    let length = (count as usize)
        .checked_mul(stride)
        .ok_or("section size overflow")?;
    let end = start.checked_add(length).ok_or("section offset overflow")?;
    if end > size {
        return Err(
            format!("YKS {name} range {start:#x}..{end:#x} exceeds file length {size:#x}").into(),
        );
    }
    Ok(start..end)
}

fn read_text(data: &[u8], offset: u32, encoding: TextEncoding) -> Result<Text> {
    let remaining = data
        .get(offset as usize..)
        .ok_or("string offset exceeds data section")?;
    let length = remaining
        .iter()
        .position(|&b| b == 0)
        .ok_or("unterminated YKS string")?;
    let bytes = &remaining[..length];
    let (text, had_errors) = match encoding {
        TextEncoding::ShiftJis => SHIFT_JIS.decode_without_bom_handling(bytes),
        TextEncoding::Gbk => GBK.decode_without_bom_handling(bytes),
        TextEncoding::Utf8 => (
            String::from_utf8_lossy(bytes),
            std::str::from_utf8(bytes).is_err(),
        ),
    };
    Ok(Text {
        text: text.into_owned(),
        bytes: bytes.to_vec(),
        had_errors,
    })
}

impl Script {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        Self::parse_with_encoding(bytes, TextEncoding::ShiftJis)
    }

    pub fn parse_with_encoding(bytes: &[u8], encoding: TextEncoding) -> Result<Self> {
        if bytes.len() < 48 {
            return Err("YKS header is shorter than 48 bytes".into());
        }
        if &bytes[..6] != b"YKS001" {
            return Err(
                "unsupported YKS signature (only YKS001 has verified script semantics)".into(),
            );
        }
        let header = Header {
            encryption: u16::from_le_bytes(bytes[6..8].try_into()?),
            header_size: read_u32(bytes, 8)?,
            unknown1: read_u32(bytes, 12)?,
            code_offset: read_u32(bytes, 16)?,
            code_count: read_u32(bytes, 20)?,
            index_offset: read_u32(bytes, 24)?,
            index_count: read_u32(bytes, 28)?,
            data_offset: read_u32(bytes, 32)?,
            data_length: read_u32(bytes, 36)?,
            max_locals: read_u32(bytes, 40)?,
            unknown2: read_u32(bytes, 44)?,
        };
        if header.header_size < 48 || header.header_size as usize > bytes.len() {
            return Err("invalid YKS header size".into());
        }
        if header.encryption > 1 {
            return Err(format!("unsupported YKS encryption flag {}", header.encryption).into());
        }
        let code_range = section(
            header.code_offset,
            header.code_count,
            4,
            bytes.len(),
            "code",
        )?;
        let index_range = section(
            header.index_offset,
            header.index_count,
            16,
            bytes.len(),
            "index",
        )?;
        let data_range = section(
            header.data_offset,
            header.data_length,
            1,
            bytes.len(),
            "data",
        )?;
        let ranges = [&code_range, &index_range, &data_range];
        for (i, range) in ranges.iter().enumerate() {
            if range.start < header.header_size as usize {
                return Err("YKS section overlaps header".into());
            }
            for other in ranges.iter().skip(i + 1) {
                if !range.is_empty()
                    && !other.is_empty()
                    && range.start < other.end
                    && other.start < range.end
                {
                    return Err("overlapping YKS sections".into());
                }
            }
        }
        let code = bytes[code_range]
            .chunks_exact(4)
            .map(|word| u32::from_le_bytes(word.try_into().expect("four byte chunk")))
            .collect();
        let mut data = bytes[data_range].to_vec();
        if header.encryption == 1 {
            for byte in &mut data {
                *byte ^= 0xaa;
            }
        }
        let mut entries = Vec::with_capacity(header.index_count as usize);
        for (index, record) in bytes[index_range].chunks_exact(16).enumerate() {
            let kind = read_u32(record, 0)?;
            let fields = [
                read_u32(record, 4)?,
                read_u32(record, 8)?,
                read_u32(record, 12)?,
            ];
            let value = Self::decode_element(kind, fields, &data, encoding)
                .map_err(|e| format!("YKS index entry {index} (kind {kind:#x}): {e}"))?;
            entries.push(IndexEntry {
                kind,
                fields,
                element: value,
            });
        }
        Ok(Self {
            header,
            encoding,
            code,
            entries,
            data,
        })
    }

    fn decode_element(
        kind: u32,
        f: [u32; 3],
        data: &[u8],
        encoding: TextEncoding,
    ) -> Result<Element> {
        Ok(match kind {
            0x00 => Element::Function {
                name: read_text(data, f[0], encoding)?,
            },
            0x01 => Element::Control {
                name: read_text(data, f[0], encoding)?,
                link: read_u32(data, f[1] as usize)? as i32,
            },
            0x04 => Element::Integer {
                value: read_u32(data, f[1] as usize)? as i32,
            },
            0x05 => Element::String {
                value: read_text(data, f[1], encoding)?,
            },
            0x07 => Element::SpecialString {
                name: read_text(data, f[0], encoding)?,
            },
            0x08 => Element::IntegerVariable {
                bank: read_text(data, f[0], encoding)?,
                id: read_u32(data, f[2] as usize)? as i32,
            },
            0x09 => Element::StringVariable {
                bank: read_text(data, f[0], encoding)?,
                id: read_u32(data, f[2] as usize)? as i32,
            },
            0x0a => Element::Local { id: f[1] },
            _ => Element::Unknown,
        })
    }

    pub fn entry(&self, index: u32) -> Result<&IndexEntry> {
        self.entries.get(index as usize).ok_or_else(|| {
            format!(
                "YKS index reference {index} exceeds {} entries",
                self.entries.len()
            )
            .into()
        })
    }

    /// Decode a single instruction using a code-word address.
    pub fn instruction_at(&self, word_offset: u32) -> Result<(Instruction, u32)> {
        let at = word_offset as usize;
        let entry_index = *self
            .code
            .get(at)
            .ok_or("instruction starts beyond code section")?;
        let entry = self.entry(entry_index)?;
        let (kind, next) = match entry.element {
            Element::Function { .. } => {
                let count = *self
                    .code
                    .get(at + 1)
                    .ok_or("missing function argument count")? as usize;
                let args_start = at + 2;
                let args_end = args_start
                    .checked_add(count)
                    .ok_or("argument count overflow")?;
                let args = self
                    .code
                    .get(args_start..args_end)
                    .ok_or("truncated function arguments")?;
                for &arg in args {
                    self.entry(arg)?;
                }
                (
                    InstructionKind::Call {
                        arguments: args.to_vec(),
                    },
                    args_end,
                )
            }
            Element::Control { .. } => (InstructionKind::Label, at + 1),
            Element::Integer { .. }
            | Element::SpecialString { .. }
            | Element::IntegerVariable { .. }
            | Element::StringVariable { .. }
            | Element::Local { .. } => (InstructionKind::Target, at + 1),
            Element::String { .. } => {
                return Err(format!("string constant used as instruction at word {at}").into());
            }
            Element::Unknown => {
                return Err(
                    format!("unsupported index kind {:#x} at word {at}", entry.kind).into(),
                );
            }
        };
        Ok((
            Instruction {
                word_offset,
                entry_index,
                kind,
            },
            u32::try_from(next)?,
        ))
    }

    pub fn disassemble(&self) -> Result<Vec<Instruction>> {
        let mut result = Vec::new();
        let mut offset = 0;
        while (offset as usize) < self.code.len() {
            let (instruction, next) = self.instruction_at(offset)?;
            result.push(instruction);
            offset = next;
        }
        Ok(result)
    }

    /// Unique function symbols from the index. This is static inventory.
    pub fn functions(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter_map(|entry| match &entry.element {
                Element::Function { name } => Some(name.text.clone()),
                _ => None,
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    pub fn format_entry(&self, index: u32) -> String {
        let Ok(entry) = self.entry(index) else {
            return format!("<invalid-index:{index}>");
        };
        match &entry.element {
            Element::Function { name } => name.text.clone(),
            Element::Control { name, link } => format!(":{}[link={link}]", name.text),
            Element::Integer { value } => value.to_string(),
            Element::String { value } => format!("{:?}", value.text),
            Element::SpecialString { name } => name.text.clone(),
            Element::IntegerVariable { bank, id } | Element::StringVariable { bank, id } => {
                format!("{}[{id}]", bank.text)
            }
            Element::Local { id } => format!("${id}"),
            Element::Unknown => format!("<kind={:#x} fields={:?}>", entry.kind, entry.fields),
        }
    }

    pub fn format_instruction(&self, instruction: &Instruction) -> String {
        let name = self.format_entry(instruction.entry_index);
        let text = match &instruction.kind {
            InstructionKind::Call { arguments } => format!(
                "{name}({})",
                arguments
                    .iter()
                    .map(|&a| self.format_entry(a))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            _ => name,
        };
        format!("{:08x}  {text}", instruction.word_offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Small real-format fixture representing Target(GlobalFlag[2]); =(42).
    fn fixture(encrypted: bool) -> Vec<u8> {
        let mut data = b"GlobalFlag\0".to_vec();
        data.extend(2i32.to_le_bytes());
        let name_offset = data.len() as u32;
        data.extend(b"=\0");
        let value_offset = data.len() as u32;
        data.extend(42i32.to_le_bytes());
        let code = [0u32, 1, 1, 2];
        let entries = [
            [8u32, 0, 0, 11],
            [0, name_offset, 0, 0],
            [4, 0, value_offset, 0],
        ];
        let mut bytes = b"YKS001".to_vec();
        bytes.extend((encrypted as u16).to_le_bytes());
        for word in [48u32, 0, 48, 4, 64, 3, 112, data.len() as u32, 0, 0] {
            bytes.extend(word.to_le_bytes());
        }
        for word in code {
            bytes.extend(word.to_le_bytes());
        }
        for entry in entries {
            for word in entry {
                bytes.extend(word.to_le_bytes());
            }
        }
        bytes.extend(
            data.into_iter()
                .map(|b| if encrypted { b ^ 0xaa } else { b }),
        );
        bytes
    }

    #[test]
    fn decodes_same_instructions_with_data_xor() {
        for encrypted in [false, true] {
            let script = Script::parse(&fixture(encrypted)).unwrap();
            let instructions = script.disassemble().unwrap();
            assert_eq!(instructions.len(), 2);
            assert_eq!(
                script.format_instruction(&instructions[0]),
                "00000000  GlobalFlag[2]"
            );
            assert_eq!(
                script.format_instruction(&instructions[1]),
                "00000001  =(42)"
            );
        }
    }

    #[test]
    fn rejects_every_truncation_and_overlapping_sections() {
        let mut bytes = fixture(false);
        for length in 0..bytes.len() {
            assert!(Script::parse(&bytes[..length]).is_err());
        }
        bytes[24..28].copy_from_slice(&48u32.to_le_bytes());
        assert!(
            Script::parse(&bytes)
                .unwrap_err()
                .to_string()
                .contains("overlapping")
        );
    }

    #[test]
    fn rejects_bad_code_reference_and_argument_extent() {
        let mut bytes = fixture(false);
        bytes[60..64].copy_from_slice(&99u32.to_le_bytes());
        assert!(Script::parse(&bytes).unwrap().disassemble().is_err());
        bytes[56..60].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Script::parse(&bytes).unwrap().disassemble().is_err());
    }

    #[test]
    fn preserves_unknown_entries_but_refuses_to_disassemble_them() {
        let mut bytes = fixture(false);
        bytes[64..68].copy_from_slice(&0xbeefu32.to_le_bytes());
        let script = Script::parse(&bytes).unwrap();
        assert!(matches!(script.entries[0].element, Element::Unknown));
        assert!(script.disassemble().is_err());
    }
}
