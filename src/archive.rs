//! YKC archive directory and bounded, streaming member access.
//! Format reference: GARbro ArcFormats/Yuka/ArcYKC.cs (MIT, morkt).
use crate::Result;
use encoding_rs::SHIFT_JIS;
use serde::Serialize;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub name: String,
    pub name_bytes: Vec<u8>,
    pub name_had_errors: bool,
    pub offset: u64,
    pub size: u64,
    pub reserved: u32,
}

#[derive(Debug, Serialize)]
pub struct Archive {
    pub path: PathBuf,
    pub version: u8,
    pub file_size: u64,
    pub header_size: u32,
    pub reserved: u32,
    pub index_offset: u64,
    pub entries: Vec<Entry>,
    #[serde(skip)]
    file: File,
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn check_range(offset: u64, size: u64, file_size: u64, label: &str) -> Result<()> {
    if offset > file_size || size > file_size - offset {
        return Err(format!(
            "{label} range {offset:#x}+{size:#x} exceeds file size {file_size:#x}"
        )
        .into());
    }
    Ok(())
}

impl Archive {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let mut file = File::open(path)?;
        let file_size = file.metadata()?.len();
        let mut header = [0u8; 24];
        file.read_exact(&mut header)?;
        let version = match &header[..8] {
            b"YKC001\0\0" => 1,
            b"YKC002\0\0" => 2,
            _ => return Err("unsupported YKC signature (expected YKC001 or YKC002)".into()),
        };
        let index_offset = u32_at(&header, 16) as u64;
        let index_size = u32_at(&header, 20) as u64;
        if index_offset < 24 || !index_size.is_multiple_of(20) {
            return Err("invalid YKC index offset or record alignment".into());
        }
        check_range(index_offset, index_size, file_size, "index")?;
        let mut entries = Vec::new();
        for i in 0..index_size / 20 {
            file.seek(SeekFrom::Start(index_offset + i * 20))?;
            let mut record = [0u8; 20];
            file.read_exact(&mut record)?;
            let name_offset = u32_at(&record, 0) as u64;
            let name_size = u32_at(&record, 4) as u64;
            let offset = u32_at(&record, 8) as u64;
            let size = u32_at(&record, 12) as u64;
            check_range(name_offset, name_size, file_size, "member name")?;
            check_range(offset, size, file_size, "member data")?;
            // Names are metadata, not payload. Bound allocation for malformed input.
            if name_size == 0 || name_size > 65536 {
                return Err(
                    format!("member {i} name exceeds the 1..65536 byte metadata limit").into(),
                );
            }
            file.seek(SeekFrom::Start(name_offset))?;
            let mut name_bytes = vec![0; name_size as usize];
            file.read_exact(&mut name_bytes)?;
            if name_bytes.last() != Some(&0) {
                return Err(format!("member {i} name is not NUL terminated").into());
            }
            name_bytes.pop();
            if name_bytes.contains(&0) {
                return Err(format!("member {i} name contains an embedded NUL").into());
            }
            let (name, name_had_errors) = if version == 1 {
                let (decoded, errors) = SHIFT_JIS.decode_without_bom_handling(&name_bytes);
                (decoded.into_owned(), errors)
            } else {
                match std::str::from_utf8(&name_bytes) {
                    Ok(name) => (name.to_owned(), false),
                    Err(_) => (String::from_utf8_lossy(&name_bytes).into_owned(), true),
                }
            };
            entries.push(Entry {
                name,
                name_bytes,
                name_had_errors,
                offset,
                size,
                reserved: u32_at(&record, 16),
            });
        }
        Ok(Self {
            path: path.to_owned(),
            version,
            file_size,
            header_size: u32_at(&header, 8),
            reserved: u32_at(&header, 12),
            index_offset,
            entries,
            file,
        })
    }

    pub fn find(&self, name: &str) -> Result<usize> {
        // Windows-era scripts routinely vary the ASCII case of archive names.
        let normalize = |s: &str| s.replace('\\', "/").to_ascii_lowercase();
        let wanted = normalize(name);
        let mut matches = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| normalize(&entry.name) == wanted);
        let (index, entry) = matches
            .next()
            .ok_or_else(|| format!("archive member not found: {name}"))?;
        if entry.name_had_errors || matches.next().is_some() {
            return Err(format!("ambiguous or undecodable member name: {name}").into());
        }
        Ok(index)
    }

    /// Reads one member. The caller supplies an allocation limit appropriate to its decoder.
    pub fn read_entry(&mut self, index: usize, max_bytes: u64) -> Result<Vec<u8>> {
        let entry = self
            .entries
            .get(index)
            .ok_or("archive member index out of range")?;
        if entry.size > max_bytes {
            return Err(format!(
                "{} requires {} bytes, above read limit {max_bytes}",
                entry.name, entry.size
            )
            .into());
        }
        let size = usize::try_from(entry.size)?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(size)?;
        bytes.resize(size, 0);
        self.file.seek(SeekFrom::Start(entry.offset))?;
        self.file.read_exact(&mut bytes)?;
        Ok(bytes)
    }

    pub fn read_prefix(&mut self, index: usize, count: usize) -> Result<Vec<u8>> {
        let entry = self
            .entries
            .get(index)
            .ok_or("archive member index out of range")?;
        let count = usize::try_from(entry.size.min(count as u64))?;
        let mut bytes = vec![0; count];
        self.file.seek(SeekFrom::Start(entry.offset))?;
        self.file.read_exact(&mut bytes)?;
        Ok(bytes)
    }

    /// Copies raw bytes, including any script obfuscation, without loading the member into memory.
    pub fn copy_entry(&mut self, index: usize, output: &mut impl Write) -> Result<u64> {
        let entry = self
            .entries
            .get(index)
            .ok_or("archive member index out of range")?;
        self.file.seek(SeekFrom::Start(entry.offset))?;
        let count = std::io::copy(&mut (&mut self.file).take(entry.size), output)?;
        if count != entry.size {
            return Err("archive was truncated while copying member".into());
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);

    fn fixture(version: u8, name: &[u8], bad_range: bool) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "yuka-archive-{}-{}.ykc",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut bytes = vec![0; 24];
        bytes[..8].copy_from_slice(if version == 1 {
            b"YKC001\0\0"
        } else {
            b"YKC002\0\0"
        });
        bytes[8..12].copy_from_slice(&24u32.to_le_bytes());
        bytes.extend_from_slice(b"payload");
        let name_offset = bytes.len() as u32;
        bytes.extend_from_slice(name);
        bytes.push(0);
        let index_offset = bytes.len() as u32;
        for value in [
            name_offset,
            name.len() as u32 + 1,
            24,
            if bad_range { u32::MAX } else { 7 },
            0,
        ] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes[16..20].copy_from_slice(&index_offset.to_le_bytes());
        bytes[20..24].copy_from_slice(&20u32.to_le_bytes());
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn versions_decode_names_and_preserve_raw_payload() {
        for version in [1, 2] {
            let name = "画像.ykg";
            let encoded = if version == 1 {
                SHIFT_JIS.encode(name).0.into_owned()
            } else {
                name.as_bytes().to_vec()
            };
            let path = fixture(version, &encoded, false);
            let mut archive = Archive::open(&path).unwrap();
            let index = archive.find(name).unwrap();
            assert_eq!(archive.read_entry(index, 7).unwrap(), b"payload");
            assert!(archive.read_entry(index, 6).is_err());
            assert_eq!(archive.entries[index].name_bytes, encoded);
            drop(archive);
            std::fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn rejects_out_of_file_payload() {
        let path = fixture(1, b"a", true);
        assert!(Archive::open(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_partial_index_record() {
        let path = fixture(1, b"a", false);
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[20..24].copy_from_slice(&19u32.to_le_bytes());
        std::fs::write(&path, bytes).unwrap();
        assert!(Archive::open(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn resolves_script_paths_with_windows_ascii_case_and_separators() {
        let path = fixture(1, b"yks\\system\\optionDefault.YKS", false);
        let archive = Archive::open(&path).unwrap();
        assert_eq!(archive.find("yks/system/OptionDefault.yks").unwrap(), 0);
        drop(archive);
        std::fs::remove_file(path).unwrap();
    }
}
