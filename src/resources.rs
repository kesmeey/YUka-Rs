use crate::{
    Result,
    archive::Archive,
    graphics::Ykg,
    script::{Script, TextEncoding},
};
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path, sync::Arc};

fn key(name: &str) -> String {
    name.replace('\\', "/").to_ascii_lowercase()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveIdentity {
    pub name: String,
    pub bytes: u64,
    pub index_fingerprint: String,
}

fn fingerprint(bytes: impl IntoIterator<Item = u8>) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

pub struct Resources {
    game_identity: std::sync::OnceLock<String>,
    archives: Vec<Archive>,
    members: BTreeMap<String, (usize, usize)>,
    images: BTreeMap<String, Arc<RgbaImage>>,
    scripts: BTreeMap<String, Arc<Script>>,
    encoding: TextEncoding,
    pub loaded_images: usize,
    pub loaded_scripts: usize,
}

impl Resources {
    pub fn manifest(&self) -> Vec<ArchiveIdentity> {
        self.archives
            .iter()
            .map(|archive| {
                let mut bytes = Vec::new();
                for entry in &archive.entries {
                    bytes.extend_from_slice(&entry.name_bytes);
                    bytes.push(0);
                    bytes.extend_from_slice(&entry.offset.to_le_bytes());
                    bytes.extend_from_slice(&entry.size.to_le_bytes());
                }
                ArchiveIdentity {
                    name: archive
                        .path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    bytes: archive.file_size,
                    index_fingerprint: fingerprint(bytes),
                }
            })
            .collect()
    }

    pub fn game_id(&self) -> Result<String> {
        if let Some(identity) = self.game_identity.get() {
            return Ok(identity.clone());
        }
        let identity = fingerprint(serde_json::to_vec(&self.manifest())?);
        let _ = self.game_identity.set(identity.clone());
        Ok(identity)
    }

    pub fn open(directory: &Path, encoding: TextEncoding) -> Result<Self> {
        let mut paths = Vec::new();
        for item in std::fs::read_dir(directory)? {
            let item = item?;
            let name = item.file_name().to_string_lossy().to_ascii_lowercase();
            if item.file_type()?.is_file() && name.starts_with("data") && name.ends_with(".ykc") {
                paths.push(item.path());
            }
        }
        paths.sort();
        if paths.is_empty() {
            return Err("no data*.ykc archives found".into());
        }
        let mut archives = Vec::new();
        let mut members = BTreeMap::new();
        for path in paths {
            let mut archive = Archive::open(path)?;
            let mut local_names = BTreeMap::new();
            for index in 0..archive.entries.len() {
                let entry = &archive.entries[index];
                if entry.name_had_errors {
                    return Err(
                        format!("undecodable resource name in {}", archive.path.display()).into(),
                    );
                }
                let name = key(&entry.name);
                if let Some(&previous) = local_names.get(&name) {
                    // Some shipped archives contain the same image under two
                    // case variants. Collapse only byte-identical members.
                    if !archive.same_payload(previous, index)? {
                        return Err(format!(
                            "ambiguous resource name {}",
                            archive.entries[index].name
                        )
                        .into());
                    }
                    continue;
                }
                local_names.insert(name.clone(), index);
                // Higher-numbered data archives override earlier archives.
                members.insert(name, (archives.len(), index));
            }
            archives.push(archive);
        }
        Ok(Self {
            game_identity: std::sync::OnceLock::new(),
            archives,
            members,
            images: BTreeMap::new(),
            scripts: BTreeMap::new(),
            encoding,
            loaded_images: 0,
            loaded_scripts: 0,
        })
    }

    pub fn contains(&self, name: &str) -> bool {
        self.members.contains_key(&key(name))
    }

    pub fn read(&mut self, name: &str) -> Result<Vec<u8>> {
        let &(archive, index) = self
            .members
            .get(&key(name))
            .ok_or_else(|| format!("resource not found: {name}"))?;
        self.archives[archive].read_entry(index, 256 * 1024 * 1024)
    }

    pub fn script(&mut self, name: &str) -> Result<Arc<Script>> {
        let name_key = key(name);
        if let Some(script) = self.scripts.get(&name_key) {
            return Ok(script.clone());
        }
        let script = Arc::new(Script::parse_with_encoding(
            &self.read(name)?,
            self.encoding,
        )?);
        script.disassemble()?;
        self.loaded_scripts += 1;
        self.scripts.insert(name_key, script.clone());
        Ok(script)
    }

    pub fn script_name(&self, script: &Arc<Script>) -> Option<&str> {
        self.scripts
            .iter()
            .find_map(|(name, loaded)| Arc::ptr_eq(script, loaded).then_some(name.as_str()))
    }

    pub fn image(&mut self, name: &str) -> Result<Arc<RgbaImage>> {
        let name_key = key(name);
        if let Some(image) = self.images.get(&name_key) {
            return Ok(image.clone());
        }
        let bytes = self.read(name)?;
        let ykg = Ykg::parse(&bytes)?;
        let color = ykg.color.as_ref().ok_or("YKG has no color image")?;
        let mut image = image::load_from_memory(&color.normalized_bytes())?.into_rgba8();
        if let Some(alpha) = &ykg.alpha {
            let mask = image::load_from_memory(&alpha.normalized_bytes())?.into_luma8();
            if image.dimensions() != mask.dimensions() {
                return Err("YKG alpha dimensions differ from color".into());
            }
            for (pixel, mask) in image.pixels_mut().zip(mask.pixels()) {
                pixel[3] = mask[0];
            }
        }
        let image = Arc::new(image);
        // Keep a small working set. Scene layers retain their own Arc references.
        if self.images.len() >= 24 {
            self.images.clear();
        }
        self.loaded_images += 1;
        self.images.insert(name_key, image.clone());
        Ok(image)
    }

    /// Full-screen gallery variants must not accumulate in the story image cache.
    pub fn temporary_image(&mut self, name: &str) -> Result<Arc<RgbaImage>> {
        let image = self.image(name)?;
        self.images.remove(&key(name));
        Ok(image)
    }
}
