use super::*;
use crate::{resources::ArchiveIdentity, sound::SoundState, vm::VmCheckpoint};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Serialize, Deserialize)]
struct SavedLayer {
    slot: i32,
    resource: String,
    x: i32,
    y: i32,
    opacity: u8,
    visible: bool,
    additive_color: [u8; 3],
    source_rect: Option<[u32; 4]>,
    sepia_filter: bool,
    #[serde(default)]
    invert_filter: bool,
    #[serde(default)]
    grayscale_filter: bool,
}

#[derive(Serialize, Deserialize)]
struct Save {
    version: u32,
    archives: Vec<ArchiveIdentity>,
    encoding: TextEncoding,
    entry: String,
    pages: usize,
    page_limit: usize,
    chapter: String,
    #[serde(default)]
    window_title: String,
    #[serde(default)]
    page_voice_paths: String,
    #[serde(default)]
    native_selection: super::native_persistence::NativeSelection,
    #[serde(default)]
    native_selection_visibility: BTreeMap<i32, bool>,
    random_state: u64,
    scripts: Vec<VmCheckpoint>,
    layers: Vec<SavedLayer>,
    sounds: Vec<(i32, SoundState)>,
    speaker: String,
    dialogue: String,
    dialogue_visible: bool,
    speaker_offset: (i32, i32),
    dialogue_offset: (i32, i32),
    choices: Vec<String>,
    #[serde(default)]
    timers: BTreeMap<i32, u64>,
}

impl Runtime {
    pub fn game_id(&self) -> Result<String> {
        self.resources.game_id()
    }

    pub fn configure_save_directory(&mut self, directory: PathBuf) {
        self.save_directory = Some(directory);
    }

    pub fn can_save(&self) -> bool {
        matches!(self.waiting, Waiting::Key | Waiting::Choice)
            && !self.scene.finished
            && self.fade.is_none()
            && self.scrolls.is_empty()
            && self.draw_before.is_none()
            && self.sound.is_stable()
            && self.effects.is_stable()
    }

    pub fn restart(&mut self) -> Result<()> {
        self.flush_native_globals()?;
        let mut replacement = Self::new_unstarted(
            &self.directory,
            &self.entry,
            self.encoding,
            self.fast,
            self.muted,
            self.page_limit,
        )?;
        replacement.save_directory = self.save_directory.clone();
        replacement.load_native_globals()?;
        replacement.drive()?;
        *self = replacement;
        Ok(())
    }

    pub fn set_paused(&mut self, paused: bool) {
        if paused {
            if self.paused_at.is_none() {
                self.paused_at = Some(Instant::now());
            }
        } else if let Some(start) = self.paused_at.take() {
            let elapsed = start.elapsed();
            if let Waiting::Timer(deadline) = &mut self.waiting {
                *deadline += elapsed;
            }
            for scroll in &mut self.scrolls {
                scroll.start += elapsed;
            }
            if let Some(fade) = &mut self.fade {
                fade.start += elapsed;
            }
            self.effects.shift_time(elapsed);
            for timer in self.timers.values_mut() {
                *timer += elapsed;
            }
            self.shift_playback_time(elapsed);
        }
        self.sound.set_paused(paused);
    }

    pub fn title_scene(&mut self) -> Result<Scene> {
        let mut scene = Scene {
            dialogue_visible: false,
            ..Scene::default()
        };
        let background = self.resources.image("ykg\\title\\base.ykg")?;
        scene.layers.insert(
            0,
            Layer {
                image: background,
                x: 0,
                y: 0,
                opacity: 255,
                visible: true,
                additive_color: [0; 3],
                source_rect: None,
                sepia_filter: false,
                invert_filter: false,
                grayscale_filter: false,
                children: BTreeMap::new(),
            },
        );
        let logo = self.resources.image("ykg\\title\\logo.ykg")?;
        if logo.width() >= 1608 && logo.height() >= 1920 {
            let crop = image::imageops::crop_imm(logo.as_ref(), 1072, 1728, 536, 192).to_image();
            scene.layers.insert(
                1,
                Layer {
                    image: std::sync::Arc::new(crop),
                    x: 132,
                    y: 57,
                    opacity: 255,
                    visible: true,
                    additive_color: [0; 3],
                    source_rect: None,
                    sepia_filter: false,
                    invert_filter: false,
                    grayscale_filter: false,
                    children: BTreeMap::new(),
                },
            );
        }
        Ok(scene)
    }

    pub fn choices(&self) -> &[String] {
        &self.scene.choices
    }

    pub fn choose(&mut self, index: usize) -> Result<()> {
        if !matches!(self.waiting, Waiting::Choice) {
            return Err("no pending choice".into());
        }
        self.stack
            .last_mut()
            .ok_or("empty script stack")?
            .vm
            .resume_selection(index)?;
        self.scene.choices.clear();
        for (slot, visible) in std::mem::take(&mut self.native_selection_visibility) {
            if let Some(layer) = self.scene.layers.get_mut(&slot) {
                layer.visible = visible;
            }
        }
        self.waiting = Waiting::Ready;
        self.drive()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if !self.can_save() {
            return Err("请在场景变化结束、等待推进对白或选择时存档".into());
        }
        let mut scripts = Vec::new();
        for activation in &self.stack {
            scripts.push(activation.vm.checkpoint(|script| {
                self.resources
                    .script_name(script)
                    .map(str::to_owned)
                    .ok_or_else(|| "save script is not registered".into())
            })?);
        }
        let mut layers = Vec::new();
        for (&slot, layer) in &self.scene.layers {
            layers.push(SavedLayer {
                slot,
                resource: self
                    .layer_sources
                    .get(&slot)
                    .ok_or("layer has no source resource")?
                    .clone(),
                x: layer.x,
                y: layer.y,
                opacity: layer.opacity,
                visible: layer.visible,
                additive_color: layer.additive_color,
                source_rect: layer.source_rect,
                sepia_filter: layer.sepia_filter,
                invert_filter: layer.invert_filter,
                grayscale_filter: layer.grayscale_filter,
            });
        }
        let save = Save {
            version: 1,
            archives: self.resources.manifest(),
            encoding: self.encoding,
            entry: self.entry.clone(),
            pages: self.pages,
            page_limit: self.page_limit,
            chapter: self.chapter.clone(),
            window_title: self.window_title.clone(),
            page_voice_paths: self.page_voice_paths.clone(),
            native_selection: self.native_selection.clone(),
            native_selection_visibility: self.native_selection_visibility.clone(),
            scripts,
            layers,
            random_state: self.random_state,
            sounds: self.sound.snapshot(),
            speaker: self.scene.speaker.clone(),
            dialogue: self.scene.dialogue.clone(),
            dialogue_visible: self.scene.dialogue_visible,
            speaker_offset: self.scene.speaker_offset,
            dialogue_offset: self.scene.dialogue_offset,
            choices: self.scene.choices.clone(),
            timers: self
                .timers
                .iter()
                .map(|(&id, start)| {
                    (
                        id,
                        self.paused_at
                            .unwrap_or_else(Instant::now)
                            .saturating_duration_since(*start)
                            .as_millis()
                            .min(u128::from(u64::MAX)) as u64,
                    )
                })
                .collect(),
        };
        write_atomic(path, &serde_json::to_vec(&save)?)
    }

    pub fn load(&mut self, path: &Path) -> Result<()> {
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err("save exceeds 16 MiB".into());
        }
        let save: Save = serde_json::from_slice(&bytes)?;
        if save.version != 1 {
            return Err("unsupported save version".into());
        }
        if save.archives != self.resources.manifest() || save.encoding != self.encoding {
            return Err("存档与当前游戏资源或文字编码不匹配".into());
        }
        if save.scripts.is_empty()
            || save.scripts.len() > 64
            || save.layers.len() > 4096
            || save.sounds.len() > 4096
        {
            return Err("invalid save collection size".into());
        }
        // Build the complete replacement before touching the current game session.
        let mut resources = Resources::open(&self.directory, self.encoding)?;
        let mut stack = Vec::new();
        for checkpoint in save.scripts {
            let vm = Vm::restore(checkpoint, |name| resources.script(name))?;
            let name = resources
                .script_name(vm.current_script())
                .ok_or("restored script is not registered")?
                .to_owned();
            stack.push(Activation { name, vm });
        }
        for activation in &stack[..stack.len() - 1] {
            if !matches!(activation.vm.pending_host_call(), Some(Event::HostCall { function, .. }) if function == "ScriptCall")
            {
                return Err("saved caller is not waiting for ScriptCall".into());
            }
        }
        let top = &stack.last().unwrap().vm;
        if save.choices.is_empty() {
            if top.pending_host_call().is_some() {
                return Err("saved input state does not match VM host call".into());
            }
        } else {
            let actual: Vec<_> = top
                .selection_options()?
                .into_iter()
                .map(|option| option.label)
                .collect();
            if save.choices != actual {
                return Err("saved choices do not match script".into());
            }
        }
        let mut scene = Scene {
            speaker: save.speaker,
            dialogue: save.dialogue,
            dialogue_visible: save.dialogue_visible,
            speaker_offset: save.speaker_offset,
            dialogue_offset: save.dialogue_offset,
            choices: save.choices,
            ..Scene::default()
        };
        let mut layer_sources = BTreeMap::new();
        for layer in save.layers {
            if layer_sources
                .insert(layer.slot, layer.resource.clone())
                .is_some()
            {
                return Err("duplicate saved layer".into());
            }
            let image = if layer.sepia_filter || layer.invert_filter || layer.grayscale_filter {
                std::sync::Arc::new(image::RgbaImage::new(0, 0))
            } else {
                resources.image(&layer.resource)?
            };
            scene.layers.insert(
                layer.slot,
                Layer {
                    image,
                    x: layer.x,
                    y: layer.y,
                    opacity: layer.opacity,
                    visible: layer.visible,
                    additive_color: layer.additive_color,
                    source_rect: layer.source_rect,
                    sepia_filter: layer.sepia_filter,
                    invert_filter: layer.invert_filter,
                    grayscale_filter: layer.grayscale_filter,
                    children: BTreeMap::new(),
                },
            );
        }
        let mut sound = Sound::new(self.muted);
        sound.set_paused(true);
        let mut slots = BTreeSet::new();
        for (slot, state) in save.sounds {
            if !slots.insert(slot) || !(0..=1000).contains(&state.volume) {
                return Err("invalid saved sound".into());
            }
            sound.load(slot, &state.path, resources.read(&state.path)?)?;
            sound.set_streamed(slot, state.streamed);
            sound.volume(slot, state.volume);
            if state.playing {
                sound.play(slot, state.looping)?;
            }
        }
        let mut timers = BTreeMap::new();
        for (id, elapsed) in save.timers {
            timers.insert(
                id,
                Instant::now()
                    .checked_sub(Duration::from_millis(elapsed))
                    .ok_or("invalid saved timer")?,
            );
        }
        self.timers = timers;
        self.stack = stack;
        self.scene = scene;
        self.resources = resources;
        self.sound = sound;
        self.layer_sources = layer_sources;
        self.entry = save.entry;
        self.pages = save.pages;
        self.page_limit = save.page_limit;
        self.chapter = save.chapter;
        self.window_title = save.window_title;
        self.page_voice_paths = save.page_voice_paths;
        self.native_selection = save.native_selection;
        self.native_selection_visibility = save.native_selection_visibility;
        self.random_state = save.random_state;
        self.waiting = if self.scene.choices.is_empty() {
            Waiting::Key
        } else {
            Waiting::Choice
        };
        self.autosave_requested = false;
        self.end_reason = None;
        self.paused_at = self.paused_at.map(|_| Instant::now());
        self.scrolls.clear();
        self.effects = GraphicsEffects::default();
        self.fade = None;
        self.draw_before = None;
        self.sound.set_paused(self.paused_at.is_some());
        self.playback = super::playback::Playback::default();
        self.record_dialogue();
        self.approximations
            .insert("Audio restarts at the beginning after loading a save".into());
        self.status();
        Ok(())
    }
}

pub(super) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".yuka-save-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = (|| -> Result<()> {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
