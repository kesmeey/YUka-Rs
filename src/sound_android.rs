//! Android MediaPlayer commands. All calls and completion events share the JNI worker thread.
use crate::Result;
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    path::PathBuf,
    time::{Duration, Instant},
};

thread_local! {
    static CACHE: RefCell<PathBuf> = const { RefCell::new(PathBuf::new()) };
    // Generations survive Runtime replacement so stale Java callbacks cannot end new audio.
    static GENERATION: RefCell<u64> = const { RefCell::new(0) };
}

fn default_groups() -> BTreeMap<i32, (i32, i32, i32)> {
    [(0, (1, 9, 650)), (1, (100, 599, 650)), (2, (99, 99, 1000))]
        .into_iter()
        .chain((3..=14).map(|group| (group, (group + 7, group + 7, 1000))))
        .collect()
}

pub fn configure_cache(directory: PathBuf) -> Result<()> {
    std::fs::create_dir_all(&directory)?;
    CACHE.with(|cache| *cache.borrow_mut() = directory);
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SoundState {
    pub path: String,
    pub volume: i32,
    pub looping: bool,
    pub playing: bool,
    #[serde(default)]
    pub streamed: bool,
}

#[derive(Serialize)]
pub struct AudioEvent {
    pub op: &'static str,
    pub slot: i32,
    pub generation: u64,
    pub path: Option<String>,
    #[serde(rename = "loop")]
    pub looping: bool,
    pub volume: f32,
}

struct Fade {
    start: Instant,
    duration: Duration,
    from: i32,
    target: i32,
    delete: bool,
}

pub struct Sound {
    states: BTreeMap<i32, SoundState>,
    generations: BTreeMap<i32, u64>,
    fades: BTreeMap<i32, Fade>,
    events: Vec<AudioEvent>,
    muted: bool,
    paused_at: Option<Instant>,
    groups: BTreeMap<i32, (i32, i32, i32)>,
    auto_duck: bool,
    duck_active: bool,
    pub warning: Option<String>,
    pub decoded_sounds: usize,
}

impl Sound {
    pub fn new(muted: bool) -> Self {
        Self {
            states: BTreeMap::new(),
            generations: BTreeMap::new(),
            fades: BTreeMap::new(),
            events: Vec::new(),
            muted,
            paused_at: None,
            groups: default_groups(),
            auto_duck: false,
            duck_active: false,
            warning: None,
            decoded_sounds: 0,
        }
    }

    fn event(&mut self, op: &'static str, slot: i32, path: Option<String>) {
        let state = self.states.get(&slot);
        self.events.push(AudioEvent {
            op,
            slot,
            generation: self.generations.get(&slot).copied().unwrap_or(0),
            path,
            looping: state.is_some_and(|state| state.looping),
            volume: state.map_or(1.0, |state| state.volume as f32 / 1000.0) * self.group_gain(slot),
        });
    }

    pub fn load(&mut self, slot: i32, path: &str, bytes: Vec<u8>) -> Result<()> {
        if !bytes.starts_with(b"OggS") {
            return Err(format!("invalid Ogg resource: {path}").into());
        }
        let generation = GENERATION.with(|counter| {
            let mut counter = counter.borrow_mut();
            *counter += 1;
            *counter
        });
        // A content-addressed file remains valid while MediaPlayer is preparing an old slot.
        let hash = bytes.iter().fold(0xcbf29ce484222325u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
        let cache = CACHE.with(|cache| cache.borrow().clone());
        if cache.as_os_str().is_empty() {
            return Err("Android audio cache has not been configured".into());
        }
        let file = cache.join(format!("{hash:016x}-{}.ogg", bytes.len()));
        if !file.is_file() || std::fs::metadata(&file)?.len() != bytes.len() as u64 {
            std::fs::write(&file, bytes)?;
        }
        self.stop(slot);
        self.generations.insert(slot, generation);
        self.states.insert(
            slot,
            SoundState {
                path: path.into(),
                volume: 1000,
                looping: false,
                playing: false,
                streamed: false,
            },
        );
        self.decoded_sounds += 1;
        if !self.muted {
            self.event("load", slot, Some(file.to_string_lossy().into_owned()));
        }
        Ok(())
    }

    fn group_gain(&self, slot: i32) -> f32 {
        let gain = self
            .groups
            .values()
            .find(|&&(start, end, _)| (start..=end).contains(&slot))
            .map_or(1.0, |&(_, _, volume)| volume as f32 / 1000.0);
        gain * if self.duck_active && (1..=9).contains(&slot) {
            0.5
        } else {
            1.0
        }
    }

    pub fn set_auto_duck(&mut self, enabled: bool) {
        self.auto_duck = enabled;
        self.update_duck();
    }

    fn update_duck(&mut self) {
        let active = self.auto_duck && (10..=21).any(|slot| self.is_audible(slot));
        if active != self.duck_active {
            self.duck_active = active;
            for slot in self
                .states
                .keys()
                .copied()
                .filter(|slot| (1..=9).contains(slot))
                .collect::<Vec<_>>()
            {
                self.event("volume", slot, None);
            }
        }
    }

    pub fn group_set(&mut self, group: i32, start: i32, end: i32) -> Result<()> {
        if group < 0 || start < 0 || end < start || end > 10000 {
            return Err("invalid sound group".into());
        }
        let volume = self.groups.get(&group).map_or(1000, |entry| entry.2);
        self.groups.insert(group, (start, end, volume));
        Ok(())
    }

    pub fn group_volume(&mut self, group: i32, volume: i32) {
        if let Some(entry) = self.groups.get_mut(&group) {
            entry.2 = volume.clamp(0, 1000);
            let (start, end, _) = *entry;
            for slot in self
                .states
                .keys()
                .copied()
                .filter(|slot| (start..=end).contains(slot))
                .collect::<Vec<_>>()
            {
                self.event("volume", slot, None);
            }
        }
        self.update_duck();
    }

    pub fn is_audible(&self, slot: i32) -> bool {
        self.is_playing(slot)
            && self.group_gain(slot) > 0.0
            && self.states.get(&slot).is_some_and(|state| state.volume > 0)
    }

    pub fn play(&mut self, slot: i32, looping: bool) -> Result<()> {
        let state = self
            .states
            .get_mut(&slot)
            .ok_or_else(|| format!("sound slot {slot} is not loaded"))?;
        state.looping = looping;
        state.playing = true;
        self.fades.remove(&slot);
        if !self.muted {
            self.event("play", slot, None);
            if self.paused_at.is_some() {
                self.event("pause_all", 0, None);
            }
        }
        self.update_duck();
        Ok(())
    }

    pub fn set_streamed(&mut self, slot: i32, streamed: bool) {
        if let Some(state) = self.states.get_mut(&slot) {
            state.streamed = streamed;
        }
    }

    pub fn volume(&mut self, slot: i32, volume: i32) {
        if let Some(state) = self.states.get_mut(&slot) {
            state.volume = volume.clamp(0, 1000);
            self.event("volume", slot, None);
        }
        self.update_duck();
    }

    pub fn fade_delete(&mut self, slot: i32, milliseconds: i32) {
        if milliseconds <= 0 || !self.is_playing(slot) {
            self.delete(slot);
        } else {
            self.begin_fade(slot, 0, milliseconds, true);
        }
    }

    pub fn fade_volume(&mut self, slot: i32, target: i32, milliseconds: i32) {
        self.fades.remove(&slot);
        if milliseconds <= 0 || !self.is_playing(slot) {
            self.volume(slot, target);
        } else {
            self.begin_fade(slot, target, milliseconds, false);
        }
    }

    fn begin_fade(&mut self, slot: i32, target: i32, milliseconds: i32, delete: bool) {
        self.fades.insert(
            slot,
            Fade {
                start: Instant::now(),
                duration: Duration::from_millis(milliseconds as u64),
                from: self.states[&slot].volume,
                target: target.clamp(0, 1000),
                delete,
            },
        );
    }

    pub fn tick(&mut self) {
        if self.paused_at.is_some() {
            return;
        }
        let now = Instant::now();
        let changes: Vec<_> = self
            .fades
            .iter()
            .map(|(&slot, fade)| {
                let progress = (now.duration_since(fade.start).as_secs_f32()
                    / fade.duration.as_secs_f32())
                .min(1.0);
                let volume =
                    (fade.from as f32 + (fade.target - fade.from) as f32 * progress).round() as i32;
                (slot, volume, progress >= 1.0, fade.delete)
            })
            .collect();
        for (slot, volume, finished, delete) in changes {
            self.volume(slot, volume);
            if finished {
                self.fades.remove(&slot);
                if delete {
                    self.delete(slot);
                }
            }
        }
    }

    pub fn stop(&mut self, slot: i32) {
        if let Some(state) = self.states.get_mut(&slot) {
            state.playing = false;
        }
        self.fades.remove(&slot);
        self.event("stop", slot, None);
        self.update_duck();
    }

    pub fn stop_all(&mut self) {
        for slot in self.states.keys().copied().collect::<Vec<_>>() {
            self.stop(slot);
        }
    }

    pub fn delete(&mut self, slot: i32) {
        self.stop(slot);
        self.event("delete", slot, None);
        self.states.remove(&slot);
        self.generations.remove(&slot);
    }

    pub fn is_playing(&self, slot: i32) -> bool {
        !self.muted && self.states.get(&slot).is_some_and(|state| state.playing)
    }
    pub fn enabled(&self) -> bool {
        !self.muted
    }
    pub fn is_stable(&self) -> bool {
        self.fades.is_empty()
    }
    pub fn snapshot(&self) -> Vec<(i32, SoundState)> {
        self.states
            .iter()
            .map(|(&slot, state)| (slot, state.clone()))
            .collect()
    }

    pub fn set_paused(&mut self, paused: bool) {
        if paused && self.paused_at.is_none() {
            self.paused_at = Some(Instant::now());
            self.event("pause_all", 0, None);
        } else if !paused && let Some(start) = self.paused_at.take() {
            for fade in self.fades.values_mut() {
                fade.start += start.elapsed();
            }
            self.event("resume_all", 0, None);
        }
    }

    pub fn completed(&mut self, slot: i32, generation: u64) {
        if self.generations.get(&slot) == Some(&generation)
            && let Some(state) = self.states.get_mut(&slot)
        {
            state.playing = false;
        }
        self.update_duck();
    }

    pub fn drain_events(&mut self) -> Vec<AudioEvent> {
        std::mem::take(&mut self.events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_group_gains_update_audio_without_overwriting_script_volume() {
        let mut sound = Sound::new(false);
        for slot in [1, 100, 10, 11] {
            sound.states.insert(
                slot,
                SoundState {
                    path: "sample.ogg".into(),
                    volume: 500,
                    looping: false,
                    playing: true,
                    streamed: false,
                },
            );
            sound.generations.insert(slot, 1);
            sound.event("play", slot, None);
        }
        let events = sound.drain_events();
        assert!((events[0].volume - 0.325).abs() < 0.0001);
        assert!((events[1].volume - 0.325).abs() < 0.0001);
        assert!((events[2].volume - 0.5).abs() < 0.0001);
        sound.group_volume(3, 0);
        assert!(!sound.is_audible(10));
        assert!(sound.is_audible(11));
        assert_eq!(sound.states[&10].volume, 500);
        let events = sound.drain_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].slot, 10);
        assert_eq!(events[0].volume, 0.0);
    }

    #[test]
    fn voice_duck_restores_bgm_on_valid_completion_and_mute() {
        let mut sound = Sound::new(false);
        for slot in [1, 10] {
            sound.states.insert(
                slot,
                SoundState {
                    path: "sample.ogg".into(),
                    volume: 1000,
                    looping: slot == 1,
                    playing: slot == 1,
                    streamed: false,
                },
            );
            sound.generations.insert(slot, 9);
        }
        sound.set_auto_duck(true);
        sound.play(10, false).unwrap();
        assert!(sound.duck_active);
        assert!((sound.group_gain(1) - 0.325).abs() < 0.0001);
        sound.completed(10, 8);
        assert!(sound.duck_active);
        sound.completed(10, 9);
        assert!(!sound.duck_active);
        assert!((sound.group_gain(1) - 0.65).abs() < 0.0001);
        sound.play(10, false).unwrap();
        assert!(sound.duck_active);
        sound.group_volume(3, 0);
        assert!(!sound.duck_active);
        assert!(
            sound
                .drain_events()
                .iter()
                .any(|event| event.op == "volume" && event.slot == 1)
        );
    }

    #[test]
    fn stale_completions_do_not_stop_new_playback_or_replacement_runtime() {
        let directory =
            std::env::temp_dir().join(format!("yuka-android-audio-{}", std::process::id()));
        configure_cache(directory.clone()).unwrap();
        let mut sound = Sound::new(false);
        sound.load(1, "first.ogg", b"OggSfirst".to_vec()).unwrap();
        sound.play(1, false).unwrap();
        let generation = sound.generations[&1];
        sound.load(1, "second.ogg", b"OggSsecond".to_vec()).unwrap();
        sound.play(1, false).unwrap();
        sound.completed(1, generation);
        assert!(sound.is_playing(1));
        let current = sound.generations[&1];
        sound.completed(1, current);
        assert!(!sound.is_playing(1));
        let mut replacement = Sound::new(false);
        replacement
            .load(1, "first.ogg", b"OggSfirst".to_vec())
            .unwrap();
        replacement.play(1, true).unwrap();
        replacement.completed(1, current);
        assert!(replacement.is_playing(1));
        let events = replacement.drain_events();
        assert!(
            events
                .iter()
                .any(|event| event.op == "play" && event.looping)
        );
        for event in events {
            if let Some(path) = event.path {
                std::fs::remove_file(path).unwrap();
            }
        }
        for event in sound.drain_events() {
            if let Some(path) = event.path {
                let _ = std::fs::remove_file(path);
            }
        }
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn paused_audio_preserves_fade_and_volume_commands() {
        let mut sound = Sound::new(false);
        sound.generations.insert(2, 100);
        sound.states.insert(
            2,
            SoundState {
                path: "bgm.ogg".into(),
                volume: 1000,
                playing: true,
                looping: true,
                streamed: true,
            },
        );
        sound.fade_volume(2, 400, 1000);
        sound.set_paused(true);
        sound.tick();
        assert_eq!(sound.states[&2].volume, 1000);
        assert!(!sound.is_stable());
        sound.set_paused(false);
        sound.volume(2, 400);
        let events = sound.drain_events();
        assert_eq!(
            events.iter().map(|event| event.op).collect::<Vec<_>>(),
            ["pause_all", "resume_all", "volume"]
        );
        assert!((events.last().unwrap().volume - 0.26).abs() < 0.001);
        sound.delete(2);
        assert!(sound.is_stable());
        assert!(!sound.is_playing(2));
    }
}
