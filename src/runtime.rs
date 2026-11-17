//! A bounded desktop host for Yuka's ordinary opening-story commands.
use crate::{
    Result,
    desktop::{Layer, Scene},
    resources::Resources,
    script::{Text, TextEncoding},
    sound::Sound,
    vm::{Event, Slot, Value, Vm},
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub const DEFAULT_SCRIPT: &str = "yks\\story\\common\\PROLOGUE1.YKS";
pub const PLAY_SCRIPT: &str = "yks\\root\\root_Common.yks";

mod graphics_host;
mod native_globals;
mod native_persistence;
mod native_thumbnail;
mod persistence;
mod playback;
mod ui;
use graphics_host::GraphicsEffects;
pub use playback::{DialogueEntry, PlaybackMode};
pub use ui::SaveInfo;

struct Activation {
    name: String,
    vm: Vm,
}
enum Waiting {
    Ready,
    Key,
    Timer(Instant),
    Sound(i32),
    Choice,
}
struct Scroll {
    slot: i32,
    from: (i32, i32),
    to: (i32, i32),
    start: Instant,
    duration: Duration,
}
struct Fade {
    start: Instant,
    duration: Duration,
}

#[derive(Serialize)]
pub struct DemoReport {
    pub pages: usize,
    pub instructions: usize,
    pub images_decoded: usize,
    pub scripts_loaded: usize,
    pub sounds_decoded: usize,
    pub audio_enabled: bool,
    pub audio_warning: Option<String>,
    pub script: String,
    pub pc: u32,
    pub finished: bool,
    pub end_reason: Option<String>,
    pub host_commands: BTreeMap<String, usize>,
    pub approximations: BTreeSet<String>,
    pub choices: Vec<String>,
}

pub struct Runtime {
    pub scene: Scene,
    resources: Resources,
    sound: Sound,
    stack: Vec<Activation>,
    waiting: Waiting,
    scrolls: Vec<Scroll>,
    fade: Option<Fade>,
    draw_before: Option<BTreeMap<i32, Layer>>,
    fast: bool,
    page_limit: usize,
    pub pages: usize,
    instructions: usize,
    commands: BTreeMap<String, usize>,
    approximations: BTreeSet<String>,
    directory: PathBuf,
    entry: String,
    encoding: TextEncoding,
    muted: bool,
    paused_at: Option<Instant>,
    layer_sources: BTreeMap<i32, String>,
    chapter: String,
    window_title: String,
    page_voice_paths: String,
    native_selection: native_persistence::NativeSelection,
    native_selection_visibility: BTreeMap<i32, bool>,
    save_directory: Option<PathBuf>,
    autosave_requested: bool,
    random_state: u64,
    effects: GraphicsEffects,
    timers: BTreeMap<i32, Instant>,
    end_reason: Option<String>,
    native_globals: native_globals::NativeGlobals,
    native_template: Option<crate::native_save::NativeSave>,
    playback: playback::Playback,
}

fn int(arguments: &[Value], index: usize) -> Result<i32> {
    match arguments.get(index) {
        Some(Value::Integer(value)) => Ok(*value),
        _ => Err(format!("argument {index} must be an integer").into()),
    }
}
fn string(arguments: &[Value], index: usize) -> Result<&str> {
    match arguments.get(index) {
        Some(Value::String(value)) => Ok(&value.text),
        _ => Err(format!("argument {index} must be a string").into()),
    }
}
fn duration(value: i32) -> Duration {
    Duration::from_millis(value.max(0) as u64)
}
fn text_value(text: &str) -> Value {
    Value::String(Text {
        text: text.to_string(),
        bytes: encoding_rs::GBK.encode(text).0.into_owned(),
        had_errors: false,
    })
}

impl Runtime {
    #[cfg(target_os = "android")]
    pub(crate) fn drain_audio_events(&mut self) -> Vec<crate::sound::AudioEvent> {
        self.sound.drain_events()
    }

    #[cfg(target_os = "android")]
    pub(crate) fn audio_completed(&mut self, slot: i32, generation: u64, error: Option<String>) {
        self.sound.completed(slot, generation);
        if let Some(error) = error {
            self.sound.warning = Some(error);
        }
    }

    #[cfg(target_os = "android")]
    pub(crate) fn stop_audio(&mut self) {
        self.sound.stop_all();
    }

    pub fn new(
        directory: &Path,
        entry: &str,
        encoding: TextEncoding,
        fast: bool,
        muted: bool,
        pages: usize,
    ) -> Result<Self> {
        let mut runtime = Self::new_unstarted(directory, entry, encoding, fast, muted, pages)?;
        runtime.drive()?;
        Ok(runtime)
    }

    pub(crate) fn new_unstarted(
        directory: &Path,
        entry: &str,
        encoding: TextEncoding,
        fast: bool,
        muted: bool,
        pages: usize,
    ) -> Result<Self> {
        let mut resources = Resources::open(directory, encoding)?;
        let mut vm = Vm::new_owned(resources.script(entry)?);
        vm.initialize_default_banks();
        // The demo enters the ordinary story directly, with the game's default protagonist name.
        for (id, name) in [(20, "爱泽"), (21, "晴树"), (22, "爱泽晴树")] {
            vm.set_variable(
                Slot::StringVariable {
                    bank: "String".into(),
                    id,
                },
                text_value(name),
            )?;
            vm.set_variable(
                Slot::StringVariable {
                    bank: "GlobalString".into(),
                    id,
                },
                text_value(name),
            )?;
        }
        let runtime = Self {
            scene: Scene::default(),
            resources,
            sound: Sound::new(muted),
            stack: vec![Activation {
                name: entry.into(),
                vm,
            }],
            waiting: Waiting::Ready,
            scrolls: Vec::new(),
            fade: None,
            draw_before: None,
            fast,
            page_limit: pages,
            pages: 0,
            instructions: 0,
            commands: BTreeMap::new(),
            approximations: BTreeSet::new(),
            directory: directory.into(),
            entry: entry.into(),
            encoding,
            muted,
            paused_at: None,
            layer_sources: BTreeMap::new(),
            chapter: String::new(),
            window_title: "Yuka RS".into(),
            page_voice_paths: String::new(),
            native_selection: native_persistence::NativeSelection::default(),
            native_selection_visibility: BTreeMap::new(),
            save_directory: None,
            autosave_requested: false,
            random_state: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64
                | 1,
            effects: GraphicsEffects::default(),
            timers: BTreeMap::new(),
            end_reason: None,
            native_globals: native_globals::NativeGlobals::default(),
            native_template: None,
            playback: playback::Playback::default(),
        };
        Ok(runtime)
    }

    pub fn report(&self) -> DemoReport {
        DemoReport {
            pages: self.pages,
            instructions: self.instructions,
            images_decoded: self.resources.loaded_images,
            scripts_loaded: self.resources.loaded_scripts,
            sounds_decoded: self.sound.decoded_sounds,
            audio_enabled: self.sound.enabled(),
            audio_warning: self.sound.warning.clone(),
            script: self.stack.last().map_or(String::new(), |a| a.name.clone()),
            pc: self.stack.last().map_or(0, |a| a.vm.pc),
            finished: self.scene.finished,
            end_reason: self.end_reason.clone(),
            host_commands: self.commands.clone(),
            approximations: self.approximations.clone(),
            choices: self.scene.choices.clone(),
        }
    }

    fn status(&mut self) {
        self.scene.status = if self.scene.finished {
            format!("Demo 已结束 · 共 {} 页", self.pages)
        } else if self.page_limit == 0 {
            format!("第 {} 页", self.pages)
        } else {
            format!("第 {} / {} 页", self.pages, self.page_limit)
        };
        if self.sound.warning.is_some() {
            self.scene.status.push_str(" · 静音（音频设备不可用）");
        }
    }

    pub fn advance(&mut self) -> Result<()> {
        if self.paused_at.is_some() {
            return Ok(());
        }
        if self.scene.finished {
            self.set_playback_mode("manual")?;
            return Ok(());
        }
        if matches!(self.waiting, Waiting::Key) {
            if self.page_limit != 0 && self.pages >= self.page_limit {
                self.scene.finished = true;
                self.end_reason = Some("page_limit".into());
                self.sound.stop_all();
                self.status();
                return Ok(());
            }
            self.waiting = Waiting::Ready;
            self.drive()?;
        }
        Ok(())
    }

    pub fn tick(&mut self) -> Result<bool> {
        if self.paused_at.is_some() {
            return Ok(false);
        }
        if self.scene.finished {
            return Ok(false);
        }
        self.sound.tick();
        let now = Instant::now();
        let mut changed = self.effects.tick(&mut self.scene);
        for scroll in &self.scrolls {
            let t = (now.duration_since(scroll.start).as_secs_f32()
                / scroll.duration.as_secs_f32().max(0.001))
            .min(1.0);
            if let Some(layer) = self.scene.layers.get_mut(&scroll.slot) {
                layer.x = (scroll.from.0 as f32 + (scroll.to.0 - scroll.from.0) as f32 * t) as i32;
                layer.y = (scroll.from.1 as f32 + (scroll.to.1 - scroll.from.1) as f32 * t) as i32;
                changed = true;
            }
        }
        self.scrolls
            .retain(|scroll| now.duration_since(scroll.start) < scroll.duration);
        if let Some(fade) = &self.fade {
            let t = (now.duration_since(fade.start).as_secs_f32()
                / fade.duration.as_secs_f32().max(0.001))
            .min(1.0);
            self.scene.transition_progress = t;
            changed = true;
            if t >= 1.0 {
                self.fade = None;
                self.scene.transition_from = None;
            }
        }
        let ready = match self.waiting {
            Waiting::Timer(deadline) => now >= deadline,
            Waiting::Sound(slot) => !self.sound.is_playing(slot),
            _ => false,
        };
        if ready {
            self.waiting = Waiting::Ready;
            self.drive()?;
            changed = true;
        }
        changed |= self.tick_playback()?;
        Ok(changed)
    }

    fn finish_script(&mut self, value: Option<Value>) -> Result<()> {
        let completed = self.stack.pop().ok_or("empty script stack")?;
        if let Some(caller) = self.stack.last_mut() {
            caller
                .vm
                .replace_shared_memory(completed.vm.shared_memory())?;
            caller.vm.resume_script_call(value)?;
        } else {
            self.scene.finished = true;
            self.end_reason = Some("script_return".into());
            self.sound.stop_all();
        }
        Ok(())
    }

    fn drive(&mut self) -> Result<()> {
        for _ in 0..20000 {
            if self.scene.finished || !matches!(self.waiting, Waiting::Ready) {
                if self.scene.finished || matches!(self.waiting, Waiting::Choice) {
                    self.set_playback_mode("manual")?;
                }
                self.status();
                if self.autosave_requested
                    && self.can_save()
                    && let Some(directory) = &self.save_directory
                {
                    #[cfg(target_os = "android")]
                    let result = self
                        .save_native_with_comment(&directory.join("savedata.001"), "[AUTO SAVE]");
                    #[cfg(not(target_os = "android"))]
                    let result = self.save(&directory.join("auto.json"));
                    self.autosave_requested = false;
                    if let Err(error) = result {
                        self.scene.status = format!("自动存档失败 · {error}");
                    }
                }
                return Ok(());
            }
            let current = self.stack.last_mut().ok_or("empty script stack")?;
            if let Some(name) = self.resources.script_name(current.vm.current_script()) {
                current.name = name.to_owned();
            }
            let was_read = self.native_globals.contains(&current.name, current.vm.pc);
            self.native_globals.mark(
                &current.name,
                current.vm.current_script().code.len(),
                current.vm.pc,
            );
            let event = current
                .vm
                .step()
                .map_err(|e| format!("{} @{}: {e}", current.name, current.vm.pc))?;
            self.instructions += 1;
            match event {
                Event::Halted => self.finish_script(None)?,
                Event::Returned {
                    destination: None,
                    value,
                    ..
                } => self.finish_script(value)?,
                Event::HostCall {
                    function,
                    arguments,
                    word_offset,
                    ..
                } => {
                    *self.commands.entry(function.clone()).or_default() += 1;
                    if function == "Select" {
                        self.set_playback_mode("manual")?;
                        self.scene.choices = self
                            .stack
                            .last()
                            .unwrap()
                            .vm
                            .selection_options()?
                            .into_iter()
                            .map(|option| option.label)
                            .collect();
                        // The player draws its own selectable rows. Suppress the game's
                        // empty button artwork so only the active hit targets are visible.
                        for (&slot, source) in &self.layer_sources {
                            if source
                                .replace('\\', "/")
                                .to_ascii_lowercase()
                                .starts_with("ykg/system/select/")
                                && let Some(layer) = self.scene.layers.get_mut(&slot)
                            {
                                self.native_selection_visibility.insert(slot, layer.visible);
                                layer.visible = false;
                            }
                        }
                        self.waiting = Waiting::Choice;
                        continue;
                    }
                    if matches!(function.as_str(), "ScriptCall" | "ScriptJump") {
                        let name = string(&arguments, 0)?.to_owned();
                        if arguments.len() != 1 {
                            return Err(
                                "file script calls with arguments are not implemented in this demo"
                                    .into(),
                            );
                        }
                        if function == "ScriptJump"
                            && name
                                .replace('\\', "/")
                                .eq_ignore_ascii_case("yks/title/title_main.yks")
                        {
                            self.scene.finished = true;
                            self.end_reason = Some("return_to_title".into());
                            self.sound.stop_all();
                            continue;
                        }
                        let script = self.resources.script(&name)?;
                        if function == "ScriptJump" {
                            let current = self.stack.last_mut().unwrap();
                            current.vm.resume_script_jump(script)?;
                            current.name = name;
                            continue;
                        }
                        if self.stack.len() >= 64 {
                            return Err("script call depth exceeds 64".into());
                        }
                        let memory = self.stack.last().unwrap().vm.shared_memory();
                        let mut vm = Vm::new_owned(script);
                        vm.initialize_default_banks();
                        vm.replace_shared_memory(memory)?;
                        self.stack.push(Activation { name, vm });
                    } else {
                        if function == "KeyWait" {
                            self.playback.current_read = was_read;
                        }
                        let result = self.host(&function, &arguments).map_err(|e| {
                            format!(
                                "{} @{word_offset} {function}: {e}",
                                self.stack.last().unwrap().name
                            )
                        })?;
                        self.stack.last_mut().unwrap().vm.resume_host(result)?;
                    }
                }
                _ => {}
            }
        }
        Err("script exceeded 20000 instructions without an input/timing boundary".into())
    }

    fn wait(&mut self, milliseconds: i32) {
        if !self.fast_execution() && milliseconds > 0 {
            self.waiting = Waiting::Timer(Instant::now() + duration(milliseconds));
        }
    }

    fn transition(&mut self, milliseconds: i32) {
        if !self.fast_execution() && milliseconds > 0 {
            self.scene.transition_from = self.draw_before.take();
            self.scene.transition_progress = 0.0;
            self.fade = Some(Fade {
                start: Instant::now(),
                duration: duration(milliseconds),
            });
            self.wait(milliseconds);
        } else {
            self.draw_before = None;
            self.scene.transition_from = None;
            self.scene.transition_progress = 1.0;
        }
    }

    fn host(&mut self, function: &str, args: &[Value]) -> Result<Option<Value>> {
        let fast = self.fast_execution();
        if self.effects.handle(
            function,
            args,
            &mut self.scene,
            &mut self.resources,
            &mut self.layer_sources,
            fast,
        )? {
            return Ok(None);
        }
        match function {
            "Index" => self.chapter = string(args, 0)?.to_owned(),
            "WindowNameSet" => {
                self.window_title = string(args, 0)?.to_owned();
            }
            "SaveDataSave" => self.autosave_requested = true,
            "SystemDataWrite" | "AlreadyDataWrite" => {
                self.flush_native_globals()?;
                return Ok(Some(Value::Integer(1)));
            }
            "RGB" => {
                let color = if args.len() == 1 {
                    let rgb = int(args, 0)? as u32;
                    ((rgb & 255) << 16) | (rgb & 0xff00) | ((rgb >> 16) & 255)
                } else {
                    (int(args, 0)? as u32 & 255)
                        | ((int(args, 1)? as u32 & 255) << 8)
                        | ((int(args, 2)? as u32 & 255) << 16)
                };
                return Ok(Some(Value::Integer(color as i32)));
            }
            "Random" => {
                let limit = int(args, 0)?;
                if limit < 0 {
                    return Err("Random requires a nonnegative bound".into());
                }
                self.random_state ^= self.random_state << 13;
                self.random_state ^= self.random_state >> 7;
                self.random_state ^= self.random_state << 17;
                return Ok(Some(Value::Integer(
                    (self.random_state % (limit as u64 + 1)) as i32,
                )));
            }
            "DrawStart" => {
                self.draw_before = None;
            }
            "SoundStopALL" => self.sound.stop_all(),
            "HistoryReset" => self.reset_history(),
            "ScriptSkipReset" => self.reset_script_playback()?,
            "ScriptSkipMode" => self.script_skip_mode(int(args, 0)?)?,
            "SkipModeSet" => self.set_playback_mode("skip_all")?,
            "AlreadySkipModeSet" => self.set_playback_mode("skip_read")?,
            "AutoModeSet" => self.set_playback_mode("auto")?,
            "SakuraEffect" if args.is_empty() => {}
            "TimerStart" => {
                self.timers.insert(int(args, 0)?, Instant::now());
            }
            "TimerWait" => {
                let id = int(args, 0)?;
                let start = self
                    .timers
                    .get(&id)
                    .ok_or("TimerWait references an unstarted timer")?;
                let deadline = *start + duration(int(args, 1)?);
                if !fast && deadline > Instant::now() {
                    self.waiting = Waiting::Timer(deadline);
                }
            }
            "SelectMode" | "SelectPosition" => {
                self.native_selection.handle(function, args)?;
                self.approximations
                    .insert("Choice layout and text borders use the runtime menu".into());
            }
            "TextBorderColor" => {
                self.approximations
                    .insert("Choice layout and text borders use the runtime menu".into());
            }
            "MovieVolumeSet" => {
                let _ = int(args, 0)?;
            }
            "MoviePlay" => {
                let _ = string(args, 0)?;
                self.approximations
                    .insert("Movie playback is skipped".into());
            }
            "LF" => self.scene.dialogue.push('\n'),
            "SkipModeCheck" | "AlreadySkipModeCheck" | "AutoModeCheck" => {
                let expected = match function {
                    "SkipModeCheck" => PlaybackMode::SkipAll,
                    "AlreadySkipModeCheck" => PlaybackMode::SkipRead,
                    _ => PlaybackMode::Auto,
                };
                return Ok(Some(Value::Integer(i32::from(
                    self.playback_mode() == expected,
                ))));
            }
            "DrawStop" => self.draw_before = Some(self.scene.layers.clone()),
            "GraphicLoad" | "GraphicLoadHide" => {
                let slot = int(args, 0)?;
                let (name, coordinates) = match args.len() {
                    2 => (string(args, 1)?, None),
                    4 => (string(args, 1)?, Some((int(args, 2)?, int(args, 3)?))),
                    5 => {
                        let mask = string(args, 2)?;
                        if !mask.to_ascii_lowercase().ends_with(".ykg") {
                            return Err("external PNG/BMP alpha import is not implemented".into());
                        }
                        // This executable's YKG mask branch calls the ordinary YKG loader,
                        // replacing the color image. See docs/playback-semantics.md.
                        (mask, Some((int(args, 3)?, int(args, 4)?)))
                    }
                    _ => return Err("unsupported GraphicLoad argument count".into()),
                };
                let image = self.resources.image(name)?;
                self.effects.clear_slot(slot, &mut self.scene);
                self.layer_sources.insert(slot, name.into());
                self.scrolls.retain(|scroll| scroll.slot != slot);
                let (old_x, old_y, opacity) = self
                    .scene
                    .layers
                    .get(&slot)
                    .map_or((0, 0, 255), |layer| (layer.x, layer.y, layer.opacity));
                self.scene.layers.insert(
                    slot,
                    Layer {
                        image,
                        x: coordinates.map_or(old_x, |xy| xy.0),
                        y: coordinates.map_or(old_y, |xy| xy.1),
                        opacity,
                        visible: function == "GraphicLoad",
                        additive_color: [0; 3],
                        source_rect: None,
                        sepia_filter: false,
                        invert_filter: false,
                        grayscale_filter: false,
                        children: BTreeMap::new(),
                    },
                );
            }
            "GraphicDelete" => {
                let slot = int(args, 0)?;
                self.effects.clear_slot(slot, &mut self.scene);
                self.scene.layers.remove(&slot);
                self.layer_sources.remove(&slot);
                self.scrolls.retain(|scroll| scroll.slot != slot);
            }
            "GraphicMove" => {
                let slot = int(args, 0)?;
                let (x, y) = (int(args, 1)?, int(args, 2)?);
                self.scrolls.retain(|scroll| scroll.slot != slot);
                match slot {
                    11 => self.scene.dialogue_offset = (x - 3, y - 473),
                    12 => self.scene.speaker_offset = (x, y - 437),
                    _ => {
                        let Some(layer) = self.scene.layers.get_mut(&slot) else {
                            return Ok(None);
                        };
                        layer.x = x;
                        layer.y = y;
                    }
                }
            }
            "GraphicShow" | "GraphicHide" => {
                let visible = function == "GraphicShow";
                for index in 0..args.len() {
                    let slot = int(args, index)?;
                    if (11..=14).contains(&slot) {
                        self.scene.dialogue_visible = visible;
                    }
                    if let Some(layer) = self.scene.layers.get_mut(&slot) {
                        layer.visible = visible;
                    }
                }
            }
            "Transition" => {
                self.approximations
                    .insert("Transition modes use crossfade".into());
                self.transition(int(args, 1)?);
            }
            "UserTransition" => {
                self.approximations
                    .insert("UserTransition masks use crossfade".into());
                self.transition(int(args, 2)?);
            }
            "Wait" => self.wait(int(args, 0)?),
            "GraphicScroll" => {
                let slot = int(args, 0)?;
                let milliseconds = int(args, 1)?;
                let to = (int(args, 2)?, int(args, 3)?);
                let Some(layer) = self.scene.layers.get_mut(&slot) else {
                    return Ok(None);
                };
                self.scrolls.retain(|scroll| scroll.slot != slot);
                if fast || milliseconds <= 0 {
                    layer.x = to.0;
                    layer.y = to.1;
                } else {
                    self.scrolls.push(Scroll {
                        slot,
                        from: (layer.x, layer.y),
                        to,
                        start: Instant::now(),
                        duration: duration(milliseconds),
                    });
                }
            }
            "GraphicScrollWait" => {
                let slot = int(args, 0)?;
                if let Some(scroll) = self.scrolls.iter().find(|s| s.slot == slot) {
                    self.waiting = Waiting::Timer(scroll.start + scroll.duration);
                }
            }
            "GraphicTransmittanceScrollWait" => {
                if let Some(deadline) = self.effects.deadline(int(args, 0)?) {
                    self.waiting = Waiting::Timer(deadline);
                }
            }
            "StrOut" | "StrOutNW" => self.scene.dialogue.push_str(string(args, 0)?),
            "StrOutNWC" => match int(args, 0)? {
                14 => self.scene.speaker = string(args, 1)?.to_owned(),
                13 => self.scene.dialogue.push_str(string(args, 1)?),
                layer => return Err(format!("StrOutNWC layer {layer} unsupported").into()),
            },
            "NameOut" => self.scene.speaker = string(args, 0)?.to_owned(),
            "KeyWait" => {
                self.pages += 1;
                self.waiting = Waiting::Key;
                self.record_dialogue();
            }
            "PF" => {
                self.page_voice_paths.clear();
                if args.is_empty() {
                    self.scene.dialogue.clear();
                    self.scene.speaker.clear();
                }
                for index in 0..args.len() {
                    match int(args, index)? {
                        13 => self.scene.dialogue.clear(),
                        14 => self.scene.speaker.clear(),
                        _ => {}
                    }
                }
            }
            "SoundLoad" | "SoundLoadStream" => {
                let bytes = self.resources.read(string(args, 1)?)?;
                self.sound.load(int(args, 0)?, string(args, 1)?, bytes)?;
                self.sound
                    .set_streamed(int(args, 0)?, function == "SoundLoadStream");
            }
            "SoundPlay" | "SoundPlayLoop" => {
                let slot = int(args, 0)?;
                if args.len() >= 2 {
                    self.sound.load(
                        slot,
                        string(args, 1)?,
                        self.resources.read(string(args, 1)?)?,
                    )?;
                }
                self.sound.play(slot, function == "SoundPlayLoop")?;
                // The supplied voiceSetting/createHistory scripts map voice groups to slots 10..21.
                if (10..=21).contains(&slot)
                    && let Some((_, state)) = self
                        .sound
                        .snapshot()
                        .into_iter()
                        .find(|(id, _)| *id == slot)
                {
                    if !self.page_voice_paths.is_empty() {
                        self.page_voice_paths.push('\t');
                    }
                    self.page_voice_paths.push_str(&state.path);
                }
            }
            "SoundStop" => self.sound.stop(int(args, 0)?),
            "SoundDelete" => self.sound.delete(int(args, 0)?),
            "SoundDeleteToFade" => {
                let milliseconds = if args.len() > 1 { int(args, 1)? } else { 1000 };
                self.sound
                    .fade_delete(int(args, 0)?, if fast { 0 } else { milliseconds });
            }
            "SoundVolume" => self.sound.volume(int(args, 0)?, int(args, 1)?),
            "SoundVolumeToFade" => {
                let milliseconds = if args.len() >= 3 { int(args, 2)? } else { 1000 };
                self.sound.fade_volume(
                    int(args, 0)?,
                    int(args, 1)?,
                    if fast { 0 } else { milliseconds },
                );
            }
            "SoundPlayWait" => {
                if !fast && self.sound.is_playing(int(args, 0)?) {
                    self.waiting = Waiting::Sound(int(args, 0)?);
                }
            }
            "FontSet" => {
                self.approximations
                    .insert("FontSet uses the demo's common Chinese font".into());
            }
            "Effect" => {
                self.approximations
                    .insert("Effect screen shake is omitted".into());
            }
            _ => return Err(format!("unimplemented host command {function}").into()),
        }
        Ok(None)
    }
}
