//! The supplied game's optionDefault / m_screen / m_text / vo_volume globals.
use super::*;
use serde::Deserialize;

pub(super) const OPTION_DEFAULTS: &[(i32, i32)] = &[
    (2, 0),
    (3, 0),
    (4, 20),
    (5, 2000),
    (10, 1),
    (11, 0),
    (12, 1),
    (13, 1),
    (14, 1),
    (15, 1),
    (16, 0),
    (19, 0),
    (30, 255),
    (31, 255),
    (32, 255),
    (33, 0),
    (34, 0),
    (35, 0),
    (100, 650),
    (101, 650),
    (102, 1000),
    (103, 1000),
    (104, 1000),
    (110, 1000),
    (111, 1000),
    (112, 1000),
    (113, 1000),
    (114, 1000),
    (115, 1000),
    (116, 1000),
    (117, 1000),
    (118, 1000),
    (119, 1000),
    (120, 1000),
    (121, 1000),
];

fn global_slot(id: i32) -> Slot {
    Slot::IntegerVariable {
        bank: "GlobalFlag".into(),
        id,
    }
}

fn validate(id: i32, value: i32) -> Result<()> {
    let valid = match id {
        2 | 3 | 11 | 12 | 13 | 16 | 19 => (0..=1).contains(&value),
        10 | 14 | 15 => (0..=2).contains(&value),
        4 => (0..=66).contains(&value),
        5 => (0..=6600).contains(&value),
        30..=35 => (0..=255).contains(&value),
        100..=104 | 110..=121 => (0..=1000).contains(&value),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(format!("unsupported option GlobalFlag[{id}]={value}").into())
    }
}

#[derive(Serialize)]
pub struct Settings {
    pub flags: BTreeMap<i32, i32>,
    pub text_speed_ms: i32,
    pub auto_delay_ms: i32,
    pub font_family: &'static str,
    /// 0 plain, 1 outline, 2 shadow (the original menu's order).
    pub font_effect: i32,
    pub window_alpha: u8,
    pub text_color: [u8; 3],
    pub border_color: [u8; 3],
}

#[derive(Serialize, Deserialize)]
pub struct PlayerName {
    pub family: String,
    pub given: String,
}

impl Runtime {
    pub fn in_recollection(&self) -> bool {
        matches!(
            self.stack
                .last()
                .and_then(
                    |activation| activation.vm.memory().get(&Slot::IntegerVariable {
                        bank: "Flag".into(),
                        id: 280
                    })
                ),
            Some(Value::Integer(1))
        )
    }
    pub fn keywait_icon(&self) -> bool {
        self.keywait_icon
    }
    pub fn submenu_enabled(&self) -> bool {
        self.submenu_enabled
    }
    pub fn global_flag(&self, id: i32) -> i32 {
        match self
            .stack
            .last()
            .and_then(|activation| activation.vm.memory().get(&global_slot(id)))
        {
            Some(Value::Integer(value)) => *value,
            _ => 0,
        }
    }

    pub(super) fn set_global_flag(&mut self, id: i32, value: i32) -> Result<()> {
        for activation in &mut self.stack {
            activation
                .vm
                .set_variable(global_slot(id), Value::Integer(value))?;
        }
        Ok(())
    }

    pub(super) fn initialize_options(&mut self) -> Result<()> {
        // Start.yks SETUP_FORMAT runs OptionDefault only when GlobalFlag60 is zero.
        // This also initializes old yuka-rs saves that never ran native setup.
        if self.global_flag(60) == 0 {
            for &(id, value) in OPTION_DEFAULTS {
                self.set_global_flag(id, value)?;
            }
            if self.sound.is_voice_slot(25) {
                for id in 122..=124 {
                    self.set_global_flag(id, 1000)?;
                }
            }
            self.set_global_flag(60, 1)?;
        }
        self.apply_option_effects();
        Ok(())
    }

    pub fn settings(&self) -> Settings {
        let flags = OPTION_DEFAULTS
            .iter()
            .map(|&(id, _)| (id, self.global_flag(id)))
            .collect();
        let rgb =
            |base| [0, 1, 2].map(|offset| self.global_flag(base + offset).clamp(0, 255) as u8);
        Settings {
            flags,
            text_speed_ms: self.global_flag(4).clamp(0, 66),
            auto_delay_ms: self.global_flag(5).clamp(0, 6600),
            font_family: if self.global_flag(2) == 0 {
                "sans-serif"
            } else {
                "serif"
            },
            font_effect: self.global_flag(14).clamp(0, 2),
            window_alpha: match self.global_flag(15) {
                0 => 102,
                2 => 255,
                _ => 217,
            },
            text_color: rgb(30),
            border_color: rgb(33),
        }
    }

    pub fn update_settings(&mut self, patch: BTreeMap<i32, i32>) -> Result<()> {
        for (&id, &value) in &patch {
            validate(id, value)?;
        }
        let mut expanded = patch.clone();
        // The original aggregate sliders assign their children's stored volumes;
        // they are not extra multipliers on top of character volume.
        for (&id, &value) in &patch {
            let children: &[i32] = match id {
                102 => &[
                    103, 104, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121,
                ],
                103 => &[110, 111, 112, 113, 114, 115, 116, 117, 120],
                104 => &[118, 119, 121],
                _ => &[],
            };
            for child in children {
                expanded.insert(*child, value);
            }
            if self.sound.is_voice_slot(25) {
                let dousei_children: &[i32] = match id {
                    102 => &[122, 123, 124],
                    103 => &[122, 123],
                    104 => &[124],
                    _ => &[],
                };
                for child in dousei_children {
                    expanded.insert(*child, value);
                }
            }
        }
        // Explicit per-character values in a bulk patch take precedence.
        expanded.extend(patch);
        for (id, value) in expanded {
            self.set_global_flag(id, value)?;
        }
        self.apply_option_effects();
        self.flush_native_globals()
    }

    pub fn reset_settings(&mut self, section: &str) -> Result<()> {
        let included = |id| match section {
            "all" | "0" => true,
            "screen" | "1" => matches!(id, 10 | 11 | 15),
            "text" | "2" => matches!(id, 2..=5 | 12..=14 | 16 | 30..=35),
            "sound" | "3" => matches!(id, 19 | 100 | 101),
            "voice" | "4" => matches!(id, 102..=104 | 110..=121),
            _ => false,
        };
        let patch = OPTION_DEFAULTS
            .iter()
            .copied()
            .filter(|&(id, _)| included(id))
            .collect::<BTreeMap<_, _>>();
        if patch.is_empty() {
            return Err("unknown settings reset section".into());
        }
        self.update_settings(patch)
    }

    pub(super) fn apply_option_effects(&mut self) {
        for (group, id) in [(0, 100), (1, 101), (2, 102)]
            .into_iter()
            .chain((3..=17).map(|group| (group, group + 107)))
        {
            self.sound
                .group_volume(group, self.global_flag(id).clamp(0, 1000));
        }
        self.sound.set_auto_duck(self.global_flag(19) != 0);
        let opacity = self.settings().window_alpha;
        for id in [11, 12] {
            if let Some(layer) = self.scene.layers.get_mut(&id) {
                layer.opacity = opacity;
            }
        }
    }

    pub fn history_voice_volume(&self, path: &str) -> f32 {
        let path = path.replace('\\', "/").to_ascii_lowercase();
        let bank = [
            ("tinatu", 110),
            ("kaho", 111),
            ("tugumi", 112),
            ("nanako", 113),
            ("satuki", 114),
            ("huyumi", 115),
            ("nao", 116),
            ("kiina", 117),
            ("tentyou", 118),
            ("ryuusei", 119),
            ("extra_w", 120),
            ("extra_m", 121),
        ]
        .into_iter()
        .find(|(name, _)| path.contains(&format!("/voice/{name}/")))
        .map_or(102, |(_, id)| id);
        self.global_flag(bank).clamp(0, 1000) as f32 / 1000.0
    }

    #[cfg(target_os = "android")]
    pub(crate) fn resume_menu_audio(&mut self) {
        self.sound.set_paused(false);
    }

    #[cfg(target_os = "android")]
    pub(crate) fn play_title_audio(&mut self, restart: bool) -> Result<()> {
        // Keep the story VM paused: only title audio resumes, so entering the
        // title does not execute or mark unseen story instructions as read.
        self.sound.set_paused(false);
        let playing = self
            .sound
            .snapshot()
            .iter()
            .any(|(id, state)| *id == 1 && state.looping && state.playing);
        if restart || !playing {
            let path = "ogg\\bgm\\lv_bgm13.ogg";
            self.sound.load(1, path, self.resources.read(path)?)?;
            self.sound.set_streamed(1, true);
            self.sound.play(1, true)?;
        }
        if restart {
            self.random_state ^= self.random_state << 13;
            self.random_state ^= self.random_state >> 7;
            self.random_state ^= self.random_state << 17;
            let character = ["KA", "NA", "SA", "TI", "TU"][(self.random_state % 5) as usize];
            let path = format!("ogg\\system\\{character}_LOVE.ogg");
            self.sound.load(99, &path, self.resources.read(&path)?)?;
            self.sound.play(99, false)?;
        }
        Ok(())
    }

    pub fn player_name(&self) -> PlayerName {
        let value = |id, default: &str| match self.stack.last().and_then(|activation| {
            activation.vm.memory().get(&Slot::StringVariable {
                bank: "GlobalString".into(),
                id,
            })
        }) {
            Some(Value::String(value)) if !value.text.is_empty() => value.text.clone(),
            _ => default.into(),
        };
        PlayerName {
            family: value(20, "爱泽"),
            given: value(21, "晴树"),
        }
    }

    pub fn set_player_name(&mut self, name: PlayerName) -> Result<()> {
        for value in [&name.family, &name.given] {
            if value.trim().is_empty()
                || value.chars().count() > 8
                || value.chars().any(char::is_control)
            {
                return Err("姓和名须各为1至8个可显示字符".into());
            }
            let (_, _, errors) = encoding_rs::GBK.encode(value);
            if errors {
                return Err("姓名含原版GBK存档不支持的字符".into());
            }
        }
        for activation in &mut self.stack {
            for (id, value) in [(20, &name.family), (21, &name.given)] {
                activation.vm.set_variable(
                    Slot::StringVariable {
                        bank: "GlobalString".into(),
                        id,
                    },
                    text_value(value),
                )?;
            }
        }
        self.initialize_player_name()?;
        self.flush_native_globals()
    }

    pub(super) fn initialize_player_name(&mut self) -> Result<()> {
        let name = self.player_name();
        let full = format!("{}{}", name.family, name.given);
        for activation in &mut self.stack {
            for (id, value) in [
                (20, name.family.as_str()),
                (21, name.given.as_str()),
                (22, full.as_str()),
            ] {
                activation.vm.set_variable(
                    Slot::StringVariable {
                        bank: "String".into(),
                        id,
                    },
                    text_value(value),
                )?;
            }
            activation.vm.set_variable(
                Slot::IntegerVariable {
                    bank: "Flag".into(),
                    id: 285,
                },
                Value::Integer(i32::from(full != "爱泽晴树")),
            )?;
        }
        Ok(())
    }

    #[cfg(target_os = "android")]
    pub(crate) fn settings_sample(
        &mut self,
        index: i32,
        directory: &Path,
    ) -> Result<serde_json::Value> {
        let (path, flag) = match index {
            0 => ("ogg\\system\\TI_LOVE.ogg", 102),
            1 => ("ogg\\voice\\Kaho\\CHAPTER3-3_KA_025.ogg", 103),
            2 => ("ogg\\voice\\Tentyou\\CHAPTER2-1_TN_001.ogg", 104),
            3 => ("ogg\\voice\\Tinatu\\CHAPTER4-3_TI_001.ogg", 110),
            4 => ("ogg\\voice\\Kaho\\PROLOGUE1_KA_032.ogg", 111),
            5 => ("ogg\\voice\\Tugumi\\TUGUMI_07_TU_039.ogg", 112),
            6 => ("ogg\\voice\\Nanako\\NANAKO_05_NA_012.ogg", 113),
            7 => ("ogg\\voice\\Satuki\\CHAPTER4-1_SA_025.ogg", 114),
            8 => ("ogg\\voice\\Huyumi\\KAHO_02_HU_001.ogg", 115),
            9 => ("ogg\\voice\\Nao\\CHAPTER1-3_NO_003.ogg", 116),
            10 => ("ogg\\voice\\Kiina\\KAHO_05_KI_012.ogg", 117),
            11 => ("ogg\\voice\\extra_W\\KAHO_04_W08_005.ogg", 120),
            12 => ("ogg\\voice\\Tentyou\\CHAPTER5-2_TN_006.ogg", 118),
            13 => ("ogg\\voice\\Ryuusei\\CHAPTER1-3_RY_002.ogg", 119),
            14 => ("ogg\\voice\\extra_M\\CHAPTER2-1_M22_006.ogg", 121),
            15 => ("ogg\\system\\click1.ogg", 101),
            _ => return Err("unsupported settings sample".into()),
        };
        let bytes = self.resources.read(path)?;
        if !bytes.starts_with(b"OggS") {
            return Err("invalid original sound sample".into());
        }
        std::fs::create_dir_all(directory)?;
        let file = super::playback::cache_history_voice(directory, index as u64, &bytes)?;
        Ok(
            serde_json::json!({"paths":[file.to_string_lossy()],"volume":self.global_flag(flag).clamp(0,1000) as f32/1000.0}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setting_ranges_reject_unknown_flags_and_bad_values() {
        for &(id, value) in OPTION_DEFAULTS {
            assert!(validate(id, value).is_ok());
        }
        for (id, value) in [
            (4, -1),
            (4, 67),
            (5, 6601),
            (14, 3),
            (30, 256),
            (100, 1001),
            (110, -1),
            (9998, 1),
            (-1, 1),
        ] {
            assert!(validate(id, value).is_err());
        }
    }

    #[test]
    #[ignore = "requires the original game through YUKA_TEST_GAME"]
    fn options_and_names_preserve_native_globals_after_reload_and_restart() -> Result<()> {
        use crate::native_save::{NativeSave, SystemDat, read_u32};
        let game = PathBuf::from(std::env::var("YUKA_TEST_GAME")?);
        let output = PathBuf::from("evidence/settings-test").join(format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let mut runtime = Runtime::new_native(
            &game,
            PLAY_SCRIPT,
            TextEncoding::Gbk,
            true,
            true,
            0,
            output.clone(),
        )?;
        assert_eq!(runtime.global_flag(60), 1);
        assert_eq!(runtime.settings().text_speed_ms, 20);
        assert_eq!(runtime.settings().window_alpha, 217);
        let slot = output.join("savedata.002");
        runtime.save_native(&slot)?;
        runtime.update_settings(
            [
                (102, 400),
                (103, 600),
                (111, 0),
                (4, 0),
                (5, 6600),
                (14, 2),
                (15, 0),
                (30, 18),
                (31, 52),
                (32, 86),
            ]
            .into(),
        )?;
        assert_eq!(runtime.global_flag(110), 600);
        assert_eq!(runtime.global_flag(111), 0);
        assert_eq!(runtime.global_flag(118), 400);
        assert_eq!(
            runtime.history_voice_volume("ogg\\voice\\Kaho\\test.ogg"),
            0.0
        );
        assert_eq!(
            runtime.history_voice_volume("ogg\\voice\\Tentyou\\test.ogg"),
            0.4
        );
        assert_eq!(runtime.settings().text_color, [18, 52, 86]);
        assert!(runtime.text_complete());
        let before = serde_json::to_string(&runtime.settings())?;
        assert!(
            runtime
                .update_settings([(100, 500), (9998, 1)].into())
                .is_err()
        );
        assert_eq!(before, serde_json::to_string(&runtime.settings())?);
        runtime.set_player_name(PlayerName {
            family: "测试".into(),
            given: "名字".into(),
        })?;
        assert!(
            runtime
                .set_player_name(PlayerName {
                    family: "😀".into(),
                    given: "名字".into()
                })
                .is_err()
        );
        runtime.load_native(&slot)?;
        assert_eq!(runtime.global_flag(110), 600);
        assert_eq!(runtime.global_flag(15), 0);
        runtime.save_native(&slot)?;
        let save = NativeSave::parse(&std::fs::read(&slot)?)?;
        let window = save
            .layers
            .iter()
            .find(|layer| read_u32(&layer.header, 8).ok() == Some(11))
            .unwrap();
        assert_eq!(read_u32(&window.header, 0x74)?, 600);
        let text = save
            .layers
            .iter()
            .find(|layer| read_u32(&layer.header, 8).ok() == Some(13))
            .unwrap();
        assert_eq!(read_u32(&text.header, 0x30)?, 0x563412);
        runtime.restart()?;
        assert_eq!(runtime.player_name().family, "测试");
        assert_eq!(runtime.player_name().given, "名字");
        assert!(
            matches!(runtime.stack.last().unwrap().vm.memory().get(&Slot::StringVariable{bank:"String".into(),id:22}),
            Some(Value::String(value))if value.text=="测试名字")
        );
        assert!(matches!(
            runtime
                .stack
                .last()
                .unwrap()
                .vm
                .memory()
                .get(&Slot::IntegerVariable {
                    bank: "Flag".into(),
                    id: 285
                }),
            Some(Value::Integer(1))
        ));
        let system = SystemDat::parse(&std::fs::read(output.join("System.dat"))?)?;
        assert_eq!(system.flags[110], 600);
        assert_eq!(system.flags[111], 0);
        assert_eq!(system.flags[118], 400);
        runtime.reset_settings("voice")?;
        assert_eq!(runtime.global_flag(110), 1000);
        assert_eq!(runtime.global_flag(15), 0);
        Ok(())
    }
}
