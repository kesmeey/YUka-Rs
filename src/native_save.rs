//! Original Yuka save containers and their absolute-offset binary records.
//! The address profile and field evidence are recorded in docs/native-save.md.
use crate::Result;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Seek, SeekFrom},
};

const SLOT_HEADER: usize = 0x48;
const STATE_HEADER: usize = 0x15c;
const FRAME_HEADER: usize = 0x50;
const LAYER_HEADER: usize = 0x130;
const DRAWABLE_HEADER: usize = 0x78;
const SOUND_HEADER: usize = 0x20;
const LOCAL_COUNT: usize = 65536;
const GLOBAL_COUNT: usize = 10000;

#[derive(Clone, Debug)]
struct Original {
    bytes: Vec<u8>,
    canonical: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct NativeValue {
    pub kind: u32,
    pub integer: i32,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct NativeFrame {
    pub header: Vec<u8>,
    pub name: Vec<u8>,
    pub code: Vec<u32>,
    pub index: Vec<[u32; 4]>,
    pub data: Vec<u8>,
    pub locals: Vec<NativeValue>,
}

impl Default for NativeFrame {
    fn default() -> Self {
        Self {
            header: sized_header(FRAME_HEADER),
            name: Vec::new(),
            code: Vec::new(),
            index: Vec::new(),
            data: Vec::new(),
            locals: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct NativeSound {
    pub header: Vec<u8>,
    pub path: Vec<u8>,
}
impl Default for NativeSound {
    fn default() -> Self {
        Self {
            header: sized_header(SOUND_HEADER),
            path: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct NativeDrawable {
    pub header: Vec<u8>,
    pub text: Vec<u8>,
    pub extra: Vec<u8>,
}
impl Default for NativeDrawable {
    fn default() -> Self {
        Self {
            header: sized_header(DRAWABLE_HEADER),
            text: Vec::new(),
            extra: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct NativeLayer {
    pub header: Vec<u8>,
    pub image_path: Vec<u8>,
    pub mask_path: Vec<u8>,
    pub items: Vec<NativeDrawable>,
}
impl Default for NativeLayer {
    fn default() -> Self {
        Self {
            header: sized_header(LAYER_HEADER),
            image_path: Vec::new(),
            mask_path: Vec::new(),
            items: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct NativeSave {
    pub header: Vec<u8>,
    pub state_header: Vec<u8>,
    pub comment: Vec<u8>,
    pub chapter: Vec<u8>,
    pub window: Vec<u8>,
    pub thumbnail: Vec<u8>,
    pub flags: Vec<i32>,
    pub strings: BTreeMap<u32, Vec<u8>>,
    pub frames: Vec<NativeFrame>,
    pub layers: Vec<NativeLayer>,
    pub sounds: Vec<NativeSound>,
    pub state_strings: [Vec<u8>; 2],
    pub state_extra_strings: [Vec<u8>; 2],
    pub auxiliary: [Vec<u8>; 2],
    original: Option<Original>,
}

/// Menu fields read without loading the serialized VM, graphics or audio state.
#[derive(Debug)]
pub struct NativeSavePreview {
    pub comment: Vec<u8>,
    pub chapter: Vec<u8>,
    pub window: Vec<u8>,
    pub thumbnail: Vec<u8>,
    pub has_thumbnail: bool,
}

impl NativeSavePreview {
    pub fn read(reader: &mut (impl Read + Seek), include_thumbnail: bool) -> Result<Self> {
        let file_size = reader.seek(SeekFrom::End(0))?;
        reader.seek(SeekFrom::Start(0))?;
        let mut header = [0; SLOT_HEADER];
        reader.read_exact(&mut header)?;
        if header.get(..7) != Some(b"Yuka  \0") || read_u32(&header, 8)? != SLOT_HEADER as u32 {
            return Err("unsupported original Yuka slot header".into());
        }
        // Validate container ranges, including the skipped state, without reading them.
        for field in [0x10, 0x18, 0x20, 0x28, 0x30, 0x38, 0x40] {
            let offset = u64::from(read_u32(&header, field)?);
            let length = u64::from(read_u32(&header, field + 4)?);
            if offset > file_size || length > file_size - offset {
                return Err("native preview section outside file".into());
            }
        }
        let comment = cstring(&read_preview_section(reader, &header, 0x10, 65536)?)?;
        let chapter = cstring(&read_preview_section(reader, &header, 0x18, 65536)?)?;
        let window = cstring(&read_preview_section(reader, &header, 0x40, 65536)?)?;
        let thumbnail = if include_thumbnail {
            read_preview_section(reader, &header, 0x30, 16 * 1024 * 1024)?
        } else {
            Vec::new()
        };
        Ok(Self {
            comment,
            chapter,
            window,
            thumbnail,
            has_thumbnail: read_u32(&header, 0x34)? != 0,
        })
    }
}

fn read_preview_section(
    reader: &mut (impl Read + Seek),
    header: &[u8],
    field: usize,
    maximum: usize,
) -> Result<Vec<u8>> {
    let length = read_u32(header, field + 4)? as usize;
    if length > maximum {
        return Err("native preview section exceeds its allocation limit".into());
    }
    let mut bytes = vec![0; length];
    reader.seek(SeekFrom::Start(u64::from(read_u32(header, field)?)))?;
    reader.read_exact(&mut bytes)?;
    Ok(bytes)
}

impl Default for NativeSave {
    fn default() -> Self {
        let mut header = vec![0; SLOT_HEADER];
        header[..8].copy_from_slice(b"Yuka  \0\0");
        header[8..12].copy_from_slice(&(SLOT_HEADER as u32).to_le_bytes());
        Self {
            header,
            state_header: sized_header(STATE_HEADER),
            comment: Vec::new(),
            chapter: Vec::new(),
            window: Vec::new(),
            thumbnail: Vec::new(),
            flags: vec![0; LOCAL_COUNT],
            strings: BTreeMap::new(),
            frames: Vec::new(),
            layers: Vec::new(),
            sounds: Vec::new(),
            state_strings: Default::default(),
            state_extra_strings: Default::default(),
            auxiliary: Default::default(),
            original: None,
        }
    }
}

fn sized_header(size: usize) -> Vec<u8> {
    let mut header = vec![0; size];
    header[..4].copy_from_slice(&(size as u32).to_le_bytes());
    header
}

pub fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    let value = bytes
        .get(offset..offset.checked_add(4).ok_or("native word offset overflow")?)
        .ok_or("truncated native word")?;
    Ok(u32::from_le_bytes(value.try_into()?))
}

pub fn write_u32(bytes: &mut [u8], offset: usize, value: u32) -> Result<()> {
    let target = bytes
        .get_mut(offset..offset.checked_add(4).ok_or("native word offset overflow")?)
        .ok_or("native word outside header")?;
    target.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn span(bytes: &[u8], offset: u32, length: u32, stride: usize) -> Result<&[u8]> {
    let size = (length as usize)
        .checked_mul(stride)
        .ok_or("native section size overflow")?;
    let end = (offset as usize)
        .checked_add(size)
        .ok_or("native section offset overflow")?;
    bytes
        .get(offset as usize..end)
        .ok_or_else(|| "native section outside file".into())
}

fn pair<'a>(bytes: &'a [u8], header: &[u8], offset: usize) -> Result<&'a [u8]> {
    span(
        bytes,
        read_u32(header, offset)?,
        read_u32(header, offset + 4)?,
        1,
    )
}

fn cstring(bytes: &[u8]) -> Result<Vec<u8>> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let end = bytes
        .iter()
        .position(|&byte| byte == 0)
        .ok_or("unterminated native string")?;
    Ok(bytes[..end].to_vec())
}

fn checked_header(bytes: &[u8], offset: u32, size: usize) -> Result<Vec<u8>> {
    let header = span(bytes, offset, size as u32, 1)?;
    if read_u32(header, 0)? != size as u32 {
        return Err(format!("unsupported native header size at {offset:#x}").into());
    }
    Ok(header.to_vec())
}

fn append(bytes: &mut Vec<u8>, value: &[u8]) -> Result<(u32, u32)> {
    let offset = u32::try_from(bytes.len())?;
    let length = u32::try_from(value.len())?;
    offset
        .checked_add(length)
        .ok_or("native file exceeds 4 GiB")?;
    bytes.extend_from_slice(value);
    Ok((offset, length))
}

fn append_string(bytes: &mut Vec<u8>, value: &[u8], terminators: usize) -> Result<(u32, u32)> {
    if value.contains(&0) {
        return Err("native logical string contains an embedded NUL".into());
    }
    let (offset, length) = append(bytes, value)?;
    bytes.resize(
        bytes
            .len()
            .checked_add(terminators)
            .ok_or("native string size overflow")?,
        0,
    );
    Ok((
        offset,
        length
            .checked_add(terminators as u32)
            .ok_or("native string exceeds u32")?,
    ))
}

fn write_pair(header: &mut [u8], at: usize, value: (u32, u32)) -> Result<()> {
    write_u32(header, at, value.0)?;
    write_u32(header, at + 4, value.1)
}

fn append_optional_string(bytes: &mut Vec<u8>, value: &[u8]) -> Result<(u32, u32)> {
    if value.is_empty() {
        Ok((0, 0))
    } else {
        append_string(bytes, value, 1)
    }
}

fn parse_flags(bytes: &[u8], header: &[u8], at: usize, count: usize) -> Result<Vec<i32>> {
    let raw = pair(bytes, header, at)?;
    if raw.len() != count * 4 {
        return Err("native flag bank has an unsupported length".into());
    }
    Ok(raw
        .chunks_exact(4)
        .map(|word| i32::from_le_bytes(word.try_into().expect("four bytes")))
        .collect())
}

fn parse_strings(
    bytes: &[u8],
    header: &[u8],
    at: usize,
    count: usize,
) -> Result<BTreeMap<u32, Vec<u8>>> {
    let table = pair(bytes, header, at)?;
    if table.len() != count * 8 {
        return Err("native string bank has an unsupported length".into());
    }
    let mut strings = BTreeMap::new();
    for (id, entry) in table.chunks_exact(8).enumerate() {
        let offset = read_u32(entry, 0)?;
        let length = read_u32(entry, 4)?;
        if length != 0 {
            strings.insert(id as u32, cstring(span(bytes, offset, length, 1)?)?);
        }
    }
    Ok(strings)
}

fn write_banks(
    bytes: &mut Vec<u8>,
    flags: &[i32],
    strings: &BTreeMap<u32, Vec<u8>>,
    count: usize,
) -> Result<((u32, u32), (u32, u32))> {
    if flags.len() != count || strings.keys().any(|&id| id as usize >= count) {
        return Err("native variable bank dimensions differ".into());
    }
    let flag_start = u32::try_from(bytes.len())?;
    for value in flags {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    let mut table = vec![0; count * 8];
    for (&id, value) in strings {
        write_pair(&mut table, id as usize * 8, append_string(bytes, value, 2)?)?;
    }
    let string_index = append(bytes, &table)?;
    Ok(((flag_start, (count * 4) as u32), string_index))
}

fn chain(bytes: &[u8], offset: u32, count: u32, size: usize) -> Result<Vec<u32>> {
    if count as usize > bytes.len() / size {
        return Err("native record count exceeds file bounds".into());
    }
    let mut records = Vec::new();
    let mut visited = BTreeSet::new();
    let mut position = offset;
    for _ in 0..count {
        if !visited.insert(position) {
            return Err("cyclic native record chain".into());
        }
        let header = checked_header(bytes, position, size)?;
        records.push(position);
        position = read_u32(&header, 4)?;
    }
    Ok(records)
}

impl NativeFrame {
    fn parse(bytes: &[u8], offset: u32) -> Result<Self> {
        let header = checked_header(bytes, offset, FRAME_HEADER)?;
        let name = cstring(pair(bytes, &header, 8)?)?;
        let code = span(bytes, read_u32(&header, 0x10)?, read_u32(&header, 0x14)?, 4)?
            .chunks_exact(4)
            .map(|v| u32::from_le_bytes(v.try_into().expect("word")))
            .collect();
        let index = span(
            bytes,
            read_u32(&header, 0x18)?,
            read_u32(&header, 0x1c)?,
            16,
        )?
        .chunks_exact(16)
        .map(|v| {
            Ok([
                read_u32(v, 0)?,
                read_u32(v, 4)?,
                read_u32(v, 8)?,
                read_u32(v, 12)?,
            ])
        })
        .collect::<Result<_>>()?;
        let data = pair(bytes, &header, 0x20)?.to_vec();
        let raw_values = span(
            bytes,
            read_u32(&header, 0x40)?,
            read_u32(&header, 0x44)?,
            12,
        )?;
        let mut locals = Vec::new();
        for raw in raw_values.chunks_exact(12) {
            let kind = read_u32(raw, 0)?;
            let integer = read_u32(raw, 4)? as i32;
            let value = if kind == 2 {
                cstring(span(bytes, integer as u32, read_u32(raw, 8)?, 1)?)?
            } else {
                Vec::new()
            };
            if kind > 2 {
                return Err(format!("unsupported native register kind {kind}").into());
            }
            locals.push(NativeValue {
                kind,
                integer,
                bytes: value,
            });
        }
        Ok(Self {
            header,
            name,
            code,
            index,
            data,
            locals,
        })
    }

    fn write(&self, bytes: &mut Vec<u8>) -> Result<()> {
        if self.header.len() != FRAME_HEADER {
            return Err("invalid native frame header size".into());
        }
        let start = bytes.len();
        bytes.extend_from_slice(&self.header);
        let mut header = self.header.clone();
        write_pair(&mut header, 8, append_string(bytes, &self.name, 1)?)?;
        let code_offset = u32::try_from(bytes.len())?;
        for word in &self.code {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        write_pair(
            &mut header,
            0x10,
            (code_offset, u32::try_from(self.code.len())?),
        )?;
        let index_offset = u32::try_from(bytes.len())?;
        for entry in &self.index {
            for word in entry {
                bytes.extend_from_slice(&word.to_le_bytes());
            }
        }
        write_pair(
            &mut header,
            0x18,
            (index_offset, u32::try_from(self.index.len())?),
        )?;
        write_pair(&mut header, 0x20, append(bytes, &self.data)?)?;
        let mut values = Vec::new();
        for value in &self.locals {
            let (word, length) = match value.kind {
                0 | 1 => (value.integer as u32, 0),
                2 => append_string(bytes, &value.bytes, 2)?,
                _ => return Err("unsupported native register type".into()),
            };
            values.extend_from_slice(&value.kind.to_le_bytes());
            values.extend_from_slice(&word.to_le_bytes());
            values.extend_from_slice(&length.to_le_bytes());
        }
        let (offset, _) = append(bytes, &values)?;
        write_pair(
            &mut header,
            0x40,
            (offset, u32::try_from(self.locals.len())?),
        )?;
        write_u32(&mut header, 4, u32::try_from(bytes.len())?)?;
        bytes[start..start + FRAME_HEADER].copy_from_slice(&header);
        Ok(())
    }
}

impl NativeSound {
    fn parse(bytes: &[u8], offset: u32) -> Result<Self> {
        let header = checked_header(bytes, offset, SOUND_HEADER)?;
        let path = cstring(pair(bytes, &header, 0x10)?)?;
        Ok(Self { header, path })
    }
    fn write(&self, bytes: &mut Vec<u8>) -> Result<()> {
        if self.header.len() != SOUND_HEADER {
            return Err("invalid native sound header size".into());
        }
        let start = bytes.len();
        bytes.extend_from_slice(&self.header);
        let mut header = self.header.clone();
        write_pair(&mut header, 0x10, append_string(bytes, &self.path, 1)?)?;
        write_u32(&mut header, 4, u32::try_from(bytes.len())?)?;
        bytes[start..start + SOUND_HEADER].copy_from_slice(&header);
        Ok(())
    }
}

impl NativeLayer {
    fn parse(bytes: &[u8], offset: u32) -> Result<Self> {
        let header = checked_header(bytes, offset, LAYER_HEADER)?;
        let image_path = cstring(pair(bytes, &header, 0x5c)?)?;
        let mask_path = cstring(pair(bytes, &header, 0x64)?)?;
        let raw_items = span(
            bytes,
            read_u32(&header, 0x6c)?,
            read_u32(&header, 0x70)?,
            DRAWABLE_HEADER,
        )?;
        let mut items = Vec::new();
        for raw in raw_items.chunks_exact(DRAWABLE_HEADER) {
            let extra = pair(bytes, raw, 0x70)?.to_vec();
            items.push(NativeDrawable {
                header: raw.to_vec(),
                text: cstring(pair(bytes, raw, 0x54)?)?,
                extra,
            });
        }
        Ok(Self {
            header,
            image_path,
            mask_path,
            items,
        })
    }
    fn write(&self, bytes: &mut Vec<u8>) -> Result<()> {
        if self.header.len() != LAYER_HEADER {
            return Err("invalid native layer header size".into());
        }
        let start = bytes.len();
        bytes.extend_from_slice(&self.header);
        let mut header = self.header.clone();
        write_pair(
            &mut header,
            0x5c,
            append_optional_string(bytes, &self.image_path)?,
        )?;
        write_pair(
            &mut header,
            0x64,
            append_optional_string(bytes, &self.mask_path)?,
        )?;
        let mut item_headers = Vec::new();
        for item in &self.items {
            if item.header.len() != DRAWABLE_HEADER {
                return Err("invalid native drawable header size".into());
            }
            let mut record = item.header.clone();
            write_pair(
                &mut record,
                0x54,
                append_optional_string(bytes, &item.text)?,
            )?;
            write_pair(&mut record, 0x70, append(bytes, &item.extra)?)?;
            item_headers.extend_from_slice(&record);
        }
        let (offset, _) = append(bytes, &item_headers)?;
        write_pair(
            &mut header,
            0x6c,
            (offset, u32::try_from(self.items.len())?),
        )?;
        write_u32(&mut header, 4, u32::try_from(bytes.len())?)?;
        bytes[start..start + LAYER_HEADER].copy_from_slice(&header);
        Ok(())
    }
}

impl NativeSave {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.get(..7) != Some(b"Yuka  \0") || read_u32(bytes, 8)? != SLOT_HEADER as u32 {
            return Err("unsupported original Yuka slot header".into());
        }
        let header = bytes
            .get(..SLOT_HEADER)
            .ok_or("truncated native slot header")?
            .to_vec();
        let state_offset = read_u32(&header, 0x20)?;
        pair(bytes, &header, 0x20)?;
        let state_header = checked_header(bytes, state_offset, STATE_HEADER)?;
        let sounds = chain(
            bytes,
            read_u32(&state_header, 0x1c)?,
            read_u32(&state_header, 0x20)?,
            SOUND_HEADER,
        )?
        .into_iter()
        .map(|at| NativeSound::parse(bytes, at))
        .collect::<Result<_>>()?;
        let layers = chain(
            bytes,
            read_u32(&state_header, 0x24)?,
            read_u32(&state_header, 0x28)?,
            LAYER_HEADER,
        )?
        .into_iter()
        .map(|at| NativeLayer::parse(bytes, at))
        .collect::<Result<_>>()?;
        let frames = chain(
            bytes,
            read_u32(&state_header, 0x2c)?,
            read_u32(&state_header, 0x30)?,
            FRAME_HEADER,
        )?
        .into_iter()
        .map(|at| NativeFrame::parse(bytes, at))
        .collect::<Result<_>>()?;
        let mut save = Self {
            comment: cstring(pair(bytes, &header, 0x10)?)?,
            chapter: cstring(pair(bytes, &header, 0x18)?)?,
            window: cstring(pair(bytes, &header, 0x40)?)?,
            thumbnail: pair(bytes, &header, 0x30)?.to_vec(),
            flags: parse_flags(bytes, &state_header, 0x14, LOCAL_COUNT)?,
            strings: parse_strings(bytes, &state_header, 0x110, LOCAL_COUNT)?,
            frames,
            sounds,
            layers,
            state_strings: [
                cstring(pair(bytes, &state_header, 0x13c)?)?,
                cstring(pair(bytes, &state_header, 0x144)?)?,
            ],
            state_extra_strings: [
                cstring(pair(bytes, &state_header, 0x4c)?)?,
                cstring(pair(bytes, &state_header, 0x124)?)?,
            ],
            auxiliary: [
                pair(bytes, &header, 0x28)?.to_vec(),
                pair(bytes, &header, 0x38)?.to_vec(),
            ],
            header,
            state_header,
            original: None,
        };
        save.original = Some(Original {
            bytes: bytes.to_vec(),
            canonical: save.encode()?,
        });
        Ok(save)
    }

    fn encode(&self) -> Result<Vec<u8>> {
        if self.header.len() != SLOT_HEADER || self.state_header.len() != STATE_HEADER {
            return Err("invalid native save header size".into());
        }
        let mut bytes = self.header.clone();
        let mut header = self.header.clone();
        header[..8].copy_from_slice(b"Yuka  \0\0");
        write_u32(&mut header, 8, SLOT_HEADER as u32)?;
        write_pair(
            &mut header,
            0x10,
            append_string(&mut bytes, &self.comment, 1)?,
        )?;
        write_pair(
            &mut header,
            0x18,
            append_string(&mut bytes, &self.chapter, 1)?,
        )?;
        write_pair(
            &mut header,
            0x40,
            append_string(&mut bytes, &self.window, 1)?,
        )?;
        write_pair(&mut header, 0x30, append(&mut bytes, &self.thumbnail)?)?;
        let state_start = bytes.len();
        bytes.extend_from_slice(&self.state_header);
        let mut state = self.state_header.clone();
        write_u32(&mut state, 0, STATE_HEADER as u32)?;
        let (flags, strings) = write_banks(&mut bytes, &self.flags, &self.strings, LOCAL_COUNT)?;
        write_pair(&mut state, 0x14, flags)?;
        write_pair(&mut state, 0x110, strings)?;
        write_pair(
            &mut state,
            0x1c,
            (
                u32::try_from(bytes.len())?,
                u32::try_from(self.sounds.len())?,
            ),
        )?;
        for sound in &self.sounds {
            sound.write(&mut bytes)?;
        }
        write_pair(
            &mut state,
            0x24,
            (
                u32::try_from(bytes.len())?,
                u32::try_from(self.layers.len())?,
            ),
        )?;
        for layer in &self.layers {
            layer.write(&mut bytes)?;
        }
        write_pair(
            &mut state,
            0x2c,
            (
                u32::try_from(bytes.len())?,
                u32::try_from(self.frames.len())?,
            ),
        )?;
        for frame in &self.frames {
            frame.write(&mut bytes)?;
        }
        write_pair(
            &mut state,
            0x4c,
            append_optional_string(&mut bytes, &self.state_extra_strings[0])?,
        )?;
        write_pair(
            &mut state,
            0x124,
            append_optional_string(&mut bytes, &self.state_extra_strings[1])?,
        )?;
        write_pair(
            &mut state,
            0x13c,
            append_string(&mut bytes, &self.state_strings[0], 1)?,
        )?;
        write_pair(
            &mut state,
            0x144,
            append_string(&mut bytes, &self.state_strings[1], 1)?,
        )?;
        bytes[state_start..state_start + STATE_HEADER].copy_from_slice(&state);
        write_pair(
            &mut header,
            0x20,
            (
                u32::try_from(state_start)?,
                u32::try_from(bytes.len() - state_start)?,
            ),
        )?;
        write_pair(&mut header, 0x28, append(&mut bytes, &self.auxiliary[0])?)?;
        write_pair(&mut header, 0x38, append(&mut bytes, &self.auxiliary[1])?)?;
        bytes[..SLOT_HEADER].copy_from_slice(&header);
        Ok(bytes)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let encoded = self.encode()?;
        if let Some(original) = &self.original
            && encoded == original.canonical
        {
            return Ok(original.bytes.clone());
        }
        Ok(encoded)
    }
}

#[derive(Clone, Debug)]
pub struct SystemDat {
    pub header: Vec<u8>,
    pub flags: Vec<i32>,
    pub strings: BTreeMap<u32, Vec<u8>>,
    pub fonts: Vec<u8>,
    pub tail: Vec<u8>,
    original: Option<Original>,
}

impl Default for SystemDat {
    fn default() -> Self {
        let mut header = vec![0; 0x7c];
        header[0x10..0x14].copy_from_slice(&0x7cu32.to_le_bytes());
        header[0x14..0x18].copy_from_slice(b"YKAL");
        header[0x68..0x6c].copy_from_slice(&800u32.to_le_bytes());
        header[0x6c..0x70].copy_from_slice(&600u32.to_le_bytes());
        Self {
            header,
            flags: vec![0; GLOBAL_COUNT],
            strings: BTreeMap::new(),
            fonts: Vec::new(),
            tail: vec![0],
            original: None,
        }
    }
}

impl SystemDat {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if read_u32(bytes, 0x10)? != 0x7c {
            return Err("unsupported native System.dat header".into());
        }
        let header = bytes
            .get(..0x7c)
            .ok_or("truncated System.dat header")?
            .to_vec();
        let mut system = Self {
            flags: parse_flags(bytes, &header, 0x18, GLOBAL_COUNT)?,
            strings: parse_strings(bytes, &header, 0x50, GLOBAL_COUNT)?,
            fonts: pair(bytes, &header, 0x20)?.to_vec(),
            tail: span(bytes, read_u32(&header, 0x44)?, read_u32(&header, 0x48)?, 1)?.to_vec(),
            header,
            original: None,
        };
        system.original = Some(Original {
            bytes: bytes.to_vec(),
            canonical: system.encode()?,
        });
        Ok(system)
    }
    fn encode(&self) -> Result<Vec<u8>> {
        if self.header.len() != 0x7c {
            return Err("invalid native System.dat header size".into());
        }
        let mut header = self.header.clone();
        let mut bytes = header.clone();
        let (flags, strings) = write_banks(&mut bytes, &self.flags, &self.strings, GLOBAL_COUNT)?;
        write_pair(&mut header, 0x18, flags)?;
        write_pair(&mut header, 0x50, strings)?;
        write_pair(&mut header, 0x20, append(&mut bytes, &self.fonts)?)?;
        write_pair(&mut header, 0x44, append(&mut bytes, &self.tail)?)?;
        bytes[..0x7c].copy_from_slice(&header);
        Ok(bytes)
    }
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let encoded = self.encode()?;
        if let Some(original) = &self.original
            && encoded == original.canonical
        {
            return Ok(original.bytes.clone());
        }
        Ok(encoded)
    }
}

#[derive(Clone, Debug, Default)]
pub struct AlreadyEntry {
    pub name: Vec<u8>,
    pub marks: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct AlreadyDat {
    pub entries: Vec<AlreadyEntry>,
    original: Option<Original>,
}

impl AlreadyDat {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.get(..12) != Some(b"YKALREADY01\0") {
            return Err("unsupported native Already.dat header".into());
        }
        let table = span(bytes, read_u32(bytes, 16)?, read_u32(bytes, 12)?, 16)?;
        let mut entries = Vec::new();
        for entry in table.chunks_exact(16) {
            entries.push(AlreadyEntry {
                name: cstring(pair(bytes, entry, 0)?)?,
                marks: span(bytes, read_u32(entry, 12)?, read_u32(entry, 8)?, 1)?.to_vec(),
            });
        }
        let mut already = Self {
            entries,
            original: None,
        };
        already.original = Some(Original {
            bytes: bytes.to_vec(),
            canonical: already.encode()?,
        });
        Ok(already)
    }
    fn encode(&self) -> Result<Vec<u8>> {
        let mut bytes = b"YKALREADY01\0".to_vec();
        bytes.resize(20, 0);
        let mut table = Vec::new();
        for entry in &self.entries {
            let (name, name_len) = append_string(&mut bytes, &entry.name, 1)?;
            let (marks, marks_len) = append(&mut bytes, &entry.marks)?;
            for value in [name, name_len, marks_len, marks] {
                table.extend_from_slice(&value.to_le_bytes());
            }
        }
        let (offset, _) = append(&mut bytes, &table)?;
        write_u32(&mut bytes, 12, u32::try_from(self.entries.len())?)?;
        write_u32(&mut bytes, 16, offset)?;
        Ok(bytes)
    }
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let encoded = self.encode()?;
        if let Some(original) = &self.original
            && encoded == original.canonical
        {
            return Ok(original.bytes.clone());
        }
        Ok(encoded)
    }
}
