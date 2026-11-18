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
    /// Explicitly replace profile globals after validating both input codecs.
    /// The selected folder remains read-only; absent Already.dat keeps current marks.
    pub fn import_native_globals(&mut self, directory: &Path) -> Result<()> {
        let mut system = SystemDat::parse(
            &read_optional(directory, "System.dat")?.ok_or("PC存档目录缺少System.dat")?,
        )?;
        let already = read_optional(directory, "Already.dat")?
            .map(|bytes| AlreadyDat::parse(&bytes))
            .transpose()?
            .unwrap_or_else(|| self.native_globals.already.clone());
        let dousei = self.sound.is_voice_slot(25);
        for (id, _) in super::settings::option_defaults(dousei) {
            let value = &mut system.flags[id as usize];
            *value = match id {
                2 | 3 | 11 | 12 | 13 | 16 | 19 => (*value).clamp(0, 1),
                10 | 14 | 15 => (*value).clamp(0, 2),
                4 => (*value).clamp(0, 66),
                5 => (*value).clamp(0, 6600),
                30..=35 => (*value).clamp(0, 255),
                100..=104 | 110..=124 => (*value).clamp(0, 1000),
                _ => *value,
            };
        }
        if system.flags[60] == 0 {
            for (id, value) in super::settings::option_defaults(dousei) {
                system.flags[id as usize] = value;
            }
            system.flags[60] = 1;
        }
        let mut globals = BTreeMap::new();
        for (id, &value) in system
            .flags
            .iter()
            .enumerate()
            .filter(|(_, value)| **value != 0)
        {
            globals.insert(
                Slot::IntegerVariable {
                    bank: "GlobalFlag".into(),
                    id: id as i32,
                },
                Value::Integer(value),
            );
        }
        // A profile with malformed encoded strings is rejected before touching
        // current memory, globals, output files, or the read-only selected folder.
        for (&id, bytes) in &system.strings {
            let text = super::native_persistence::native_text(bytes, self.encoding)?;
            globals.insert(
                Slot::StringVariable {
                    bank: "GlobalString".into(),
                    id: id as i32,
                },
                Value::String(Text {
                    text,
                    bytes: bytes.clone(),
                    had_errors: false,
                }),
            );
        }
        let memories = self
            .stack
            .iter()
            .map(|activation| {
                let mut memory = activation.vm.shared_memory();
                memory.retain(|slot, _| {
                    !matches!(slot,
                Slot::IntegerVariable{bank,..}|Slot::StringVariable{bank,..}
                if bank=="GlobalFlag"||bank=="GlobalString")
                });
                memory.extend(globals.clone());
                memory
            })
            .collect::<Vec<_>>();
        let system_bytes = system.to_bytes()?;
        let already_bytes = already.to_bytes()?;
        let actual = self
            .save_directory
            .as_ref()
            .ok_or("未设置当前游戏存档目录")?;
        if directory.canonicalize()? == actual.canonicalize()? {
            return Err("导入目录必须是独立的PC存档暂存目录".into());
        }
        let system_path = native_file(actual, "System.dat")?;
        let already_path = native_file(actual, "Already.dat")?;
        let previous_already = read_optional(actual, "Already.dat")?;
        super::persistence::write_atomic(&already_path, &already_bytes)?;
        if let Err(error) = super::persistence::write_atomic(&system_path, &system_bytes) {
            let rollback = match previous_already {
                Some(bytes) => super::persistence::write_atomic(&already_path, &bytes),
                None => std::fs::remove_file(&already_path).map_err(Into::into),
            };
            return match rollback {
                Ok(()) => Err(error),
                Err(rollback) => {
                    Err(format!("导入失败: {error}; 已读记录回滚失败: {rollback}").into())
                }
            };
        }
        for (activation, memory) in self.stack.iter_mut().zip(memories) {
            activation.vm.replace_shared_memory(memory)?;
        }
        self.native_globals = NativeGlobals {
            initialized: true,
            system: Some(system),
            already,
        };
        self.initialize_player_name()?;
        self.apply_option_effects();
        Ok(())
    }

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
        runtime.initialize_player_name()?;
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
        self.initialize_options()?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires the original game through YUKA_TEST_GAME"]
    fn explicit_profile_import_validates_every_input_before_publishing() -> Result<()> {
        let game = PathBuf::from(std::env::var("YUKA_TEST_GAME")?);
        let scratch = PathBuf::from("evidence/profile-import-test").join(format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let current = scratch.join("current");
        let incoming = scratch.join("incoming");
        std::fs::create_dir_all(&incoming)?;
        let mut runtime = Runtime::new_native(
            &game,
            PLAY_SCRIPT,
            TextEncoding::Gbk,
            true,
            true,
            0,
            current.clone(),
        )?;
        runtime.update_settings([(100, 321)].into())?;
        runtime.set_global_flag(9998, 77)?;
        runtime.flush_native_globals()?;
        let old_system = std::fs::read(current.join("System.dat"))?;
        let old_already = std::fs::read(current.join("Already.dat"))?;
        assert!(runtime.import_native_globals(&incoming).is_err());
        let mut profile = SystemDat::default();
        for &(id, value) in super::super::settings::OPTION_DEFAULTS {
            profile.flags[id as usize] = value;
        }
        profile.flags[60] = 1;
        profile.flags[200] = 1;
        profile.flags[220] = 1;
        profile.flags[450] = 1;
        profile.flags[100] = 1001; // Host clamps an out-of-range native setting, not progress.
        profile
            .strings
            .insert(20, encoding_rs::GBK.encode("新的").0.into_owned());
        profile
            .strings
            .insert(21, encoding_rs::GBK.encode("姓名").0.into_owned());
        let source_system = profile.to_bytes()?;
        std::fs::write(incoming.join("System.dat"), &source_system)?;
        std::fs::write(incoming.join("Already.dat"), b"truncated")?;
        assert!(runtime.import_native_globals(&incoming).is_err());
        assert_eq!(runtime.global_flag(100), 321);
        assert_eq!(runtime.global_flag(9998), 77);
        assert_eq!(std::fs::read(current.join("System.dat"))?, old_system);
        assert_eq!(std::fs::read(current.join("Already.dat"))?, old_already);
        std::fs::remove_file(incoming.join("Already.dat"))?;
        let mut corrupt = profile.clone();
        corrupt.strings.insert(20, vec![0x81]);
        std::fs::write(incoming.join("System.dat"), corrupt.to_bytes()?)?;
        assert!(runtime.import_native_globals(&incoming).is_err());
        assert_eq!(runtime.global_flag(100), 321);
        assert_eq!(std::fs::read(current.join("System.dat"))?, old_system);
        std::fs::write(incoming.join("System.dat"), &source_system)?;
        runtime.import_native_globals(&incoming)?;
        assert_eq!(runtime.global_flag(100), 1000);
        assert_eq!(runtime.global_flag(9998), 0);
        assert_eq!(runtime.player_name().family, "新的");
        assert_eq!(runtime.player_name().given, "姓名");
        let extras = runtime.extras_info()?;
        assert!(extras.enabled);
        assert!(extras.pages[0].images[0].unlocked);
        assert_eq!(std::fs::read(incoming.join("System.dat"))?, source_system);
        assert_eq!(std::fs::read(current.join("Already.dat"))?, old_already);
        let published = SystemDat::parse(&std::fs::read(current.join("System.dat"))?)?;
        assert_eq!(published.flags[200], 1);
        assert_eq!(published.flags[100], 1000);
        let mut replacement = AlreadyDat::default();
        replacement.entries = vec![AlreadyEntry {
            name: b"test.yks".to_vec(),
            marks: vec![1, 0, 1],
        }];
        let replacement_bytes = replacement.to_bytes()?;
        std::fs::write(incoming.join("Already.dat"), &replacement_bytes)?;
        runtime.import_native_globals(&incoming)?;
        assert_eq!(
            std::fs::read(current.join("Already.dat"))?,
            replacement_bytes
        );
        runtime.stack.last_mut().unwrap().vm.set_variable(
            Slot::IntegerVariable {
                bank: "Flag".into(),
                id: 280,
            },
            Value::Integer(1),
        )?;
        assert!(runtime.in_recollection());
        Ok(())
    }
}
