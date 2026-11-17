//! Original System.dat globals and Already.dat code-word read markers.
use super::*;
use crate::native_save::{AlreadyDat, AlreadyEntry, SystemDat};
use std::io::Read;

#[derive(Clone, Default)]
pub(super) struct NativeGlobals {
    pub(super) initialized: bool,
    system: Option<SystemDat>,
    already: AlreadyDat,
}

fn normalize(name: &[u8]) -> Vec<u8> {
    name.iter()
        .map(|byte| {
            if *byte == b'/' {
                b'\\'
            } else {
                byte.to_ascii_lowercase()
            }
        })
        .collect()
}

impl NativeGlobals {
    pub(super) fn contains(&self, script: &str, pc: u32) -> bool {
        let name = normalize(script.as_bytes());
        self.already.entries.iter().any(|entry| {
            normalize(&entry.name) == name
                && entry.marks.get(pc as usize).is_some_and(|mark| *mark != 0)
        })
    }

    // Native dispatcher 0x442483..0x4424bb marks the current code word before execution.
    pub(super) fn mark(&mut self, script: &str, words: usize, pc: u32) {
        let name = normalize(script.as_bytes());
        let entry = match self
            .already
            .entries
            .iter_mut()
            .find(|entry| normalize(&entry.name) == name)
        {
            Some(entry) => entry,
            None => {
                self.already.entries.push(AlreadyEntry {
                    name,
                    marks: vec![0; words],
                });
                self.already.entries.last_mut().unwrap()
            }
        };
        entry.marks.resize(words, 0);
        if let Some(mark) = entry.marks.get_mut(pc as usize) {
            *mark = 1;
        }
    }
}

fn native_file(directory: &Path, name: &str) -> Result<PathBuf> {
    if !directory.is_dir() {
        return Ok(directory.join(name));
    }
    let mut path = None;
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        if entry
            .file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case(name)
        {
            if path.is_some() {
                return Err(format!("ambiguous native global file {name}").into());
            }
            path = Some(entry.path());
        }
    }
    Ok(path.unwrap_or_else(|| directory.join(name)))
}

fn read_optional(directory: &Path, name: &str) -> Result<Option<Vec<u8>>> {
    let path = native_file(directory, name)?;
    if !path.try_exists()? {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(format!("native global file too large: {name}").into());
    }
    Ok(Some(bytes))
}

impl Runtime {
    pub fn new_native(
        directory: &Path,
        entry: &str,
        encoding: TextEncoding,
        fast: bool,
        muted: bool,
        pages: usize,
        save_directory: PathBuf,
    ) -> Result<Self> {
        let mut runtime = Self::new_unstarted(directory, entry, encoding, fast, muted, pages)?;
        runtime.configure_save_directory(save_directory);
        runtime.load_native_globals()?;
        runtime.drive()?;
        Ok(runtime)
    }

    pub fn load_native_globals(&mut self) -> Result<()> {
        if let Some(directory) = self.save_directory.clone() {
            self.load_native_globals_from(&directory)?;
        }
        Ok(())
    }

    pub fn flush_native_globals(&self) -> Result<()> {
        if let Some(directory) = &self.save_directory {
            self.save_native_globals_to(directory)?;
        }
        Ok(())
    }

    pub(super) fn load_native_globals_from(&mut self, directory: &Path) -> Result<()> {
        let system = read_optional(directory, "System.dat")?
            .map(|bytes| SystemDat::parse(&bytes))
            .transpose()?;
        let already = read_optional(directory, "Already.dat")?
            .map(|bytes| AlreadyDat::parse(&bytes))
            .transpose()?
            .unwrap_or_default();
        if let Some(system) = &system {
            let encoding = match self.encoding {
                TextEncoding::Gbk => encoding_rs::GBK,
                TextEncoding::ShiftJis => encoding_rs::SHIFT_JIS,
                TextEncoding::Utf8 => encoding_rs::UTF_8,
            };
            for activation in &mut self.stack {
                let mut memory = activation.vm.shared_memory();
                memory.retain(|slot, _| {
                    !matches!(slot,
                    Slot::IntegerVariable { bank, .. } | Slot::StringVariable { bank, .. }
                    if bank == "GlobalFlag" || bank == "GlobalString")
                });
                for (id, &value) in system
                    .flags
                    .iter()
                    .enumerate()
                    .filter(|(_, value)| **value != 0)
                {
                    memory.insert(
                        Slot::IntegerVariable {
                            bank: "GlobalFlag".into(),
                            id: id as i32,
                        },
                        Value::Integer(value),
                    );
                }
                for (&id, bytes) in &system.strings {
                    let (decoded, _, had_errors) = encoding.decode(bytes);
                    memory.insert(
                        Slot::StringVariable {
                            bank: "GlobalString".into(),
                            id: id as i32,
                        },
                        Value::String(Text {
                            text: decoded.into_owned(),
                            bytes: bytes.clone(),
                            had_errors,
                        }),
                    );
                }
                activation.vm.replace_shared_memory(memory)?;
            }
        }
        self.native_globals = NativeGlobals {
            initialized: true,
            system,
            already,
        };
        Ok(())
    }

    pub(super) fn save_native_globals_to(&self, directory: &Path) -> Result<()> {
        let mut system = self.native_globals.system.clone().unwrap_or_default();
        if let Some(activation) = self.stack.last() {
            system.flags.fill(0);
            system.strings.clear();
            for (slot, value) in activation.vm.memory() {
                match (slot, value) {
                    (Slot::IntegerVariable { bank, id }, Value::Integer(value))
                        if bank == "GlobalFlag" =>
                    {
                        *system
                            .flags
                            .get_mut(*id as usize)
                            .ok_or("GlobalFlag outside native range")? = *value;
                    }
                    (Slot::StringVariable { bank, id }, Value::String(value))
                        if bank == "GlobalString" =>
                    {
                        if !(0..10000).contains(id) {
                            return Err("GlobalString outside native range".into());
                        }
                        system.strings.insert(*id as u32, value.bytes.clone());
                    }
                    _ => {}
                }
            }
        }
        let system_bytes = system.to_bytes()?;
        let already_bytes = self.native_globals.already.to_bytes()?;
        let system_path = native_file(directory, "System.dat")?;
        let already_path = native_file(directory, "Already.dat")?;
        super::persistence::write_atomic(&system_path, &system_bytes)?;
        super::persistence::write_atomic(&already_path, &already_bytes)
    }
}
