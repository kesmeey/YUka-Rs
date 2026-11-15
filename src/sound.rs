use crate::Result;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::Cursor,
    sync::Arc,
    time::{Duration, Instant},
};

struct Fade {
    start: Instant,
    duration: Duration,
    volume: f32,
    target: f32,
    delete: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SoundState {
    pub path: String,
    pub volume: i32,
    pub looping: bool,
    pub playing: bool,
}

pub struct Sound {
    stream: Option<(OutputStream, OutputStreamHandle)>,
    data: BTreeMap<i32, Arc<[u8]>>,
    sinks: BTreeMap<i32, Sink>,
    volumes: BTreeMap<i32, f32>,
    fades: BTreeMap<i32, Fade>,
    states: BTreeMap<i32, SoundState>,
    paused_at: Option<Instant>,
    pub warning: Option<String>,
    pub decoded_sounds: usize,
}

impl Sound {
    pub fn new(muted: bool) -> Self {
        let (stream, warning) = if muted {
            (None, None)
        } else {
            match OutputStream::try_default() {
                Ok(stream) => (Some(stream), None),
                Err(error) => (None, Some(format!("音频设备不可用，已静音 ({error})"))),
            }
        };
        Self {
            stream,
            data: BTreeMap::new(),
            sinks: BTreeMap::new(),
            volumes: BTreeMap::new(),
            fades: BTreeMap::new(),
            states: BTreeMap::new(),
            paused_at: None,
            warning,
            decoded_sounds: 0,
        }
    }

    pub fn load(&mut self, slot: i32, path: &str, bytes: Vec<u8>) -> Result<()> {
        let data: Arc<[u8]> = bytes.into();
        let mut decoder = Decoder::new(Cursor::new(data.clone()))?;
        if decoder.next().is_none() {
            return Err("empty decoded audio stream".into());
        }
        self.stop(slot);
        self.volumes.remove(&slot);
        self.decoded_sounds += 1;
        self.data.insert(slot, data);
        self.states.insert(
            slot,
            SoundState {
                path: path.into(),
                volume: 1000,
                looping: false,
                playing: false,
            },
        );
        Ok(())
    }

    pub fn play(&mut self, slot: i32, looping: bool) -> Result<()> {
        let data = self
            .data
            .get(&slot)
            .ok_or_else(|| format!("sound slot {slot} is not loaded"))?;
        let decoder = Decoder::new(Cursor::new(data.clone()))?;
        self.fades.remove(&slot);
        if let Some((_, handle)) = &self.stream {
            let sink = Sink::try_new(handle)?;
            sink.set_volume(*self.volumes.get(&slot).unwrap_or(&1.0) * 0.65);
            if looping {
                sink.append(decoder.repeat_infinite());
            } else {
                sink.append(decoder);
            }
            self.sinks.insert(slot, sink);
        }
        if let Some(state) = self.states.get_mut(&slot) {
            state.looping = looping;
            state.playing = true;
        }
        if self.paused_at.is_some() {
            for sink in self.sinks.values() {
                sink.pause();
            }
        }
        Ok(())
    }

    pub fn volume(&mut self, slot: i32, volume: i32) {
        if let Some(state) = self.states.get_mut(&slot) {
            state.volume = volume.clamp(0, 1000);
        }
        let volume = volume.clamp(0, 1000) as f32 / 1000.0;
        self.volumes.insert(slot, volume);
        if let Some(sink) = self.sinks.get(&slot) {
            sink.set_volume(volume * 0.65);
        }
    }

    pub fn fade_delete(&mut self, slot: i32, milliseconds: i32) {
        if milliseconds <= 0 || !self.is_playing(slot) {
            self.delete(slot);
            return;
        }
        let volume = self.sinks[&slot].volume();
        self.fades.insert(
            slot,
            Fade {
                start: Instant::now(),
                duration: Duration::from_millis(milliseconds as u64),
                volume,
                target: 0.0,
                delete: true,
            },
        );
    }

    pub fn fade_volume(&mut self, slot: i32, target: i32, milliseconds: i32) {
        self.fades.remove(&slot);
        if milliseconds <= 0 || !self.is_playing(slot) {
            self.volume(slot, target);
            return;
        }
        self.fades.insert(
            slot,
            Fade {
                start: Instant::now(),
                duration: Duration::from_millis(milliseconds as u64),
                volume: self.sinks[&slot].volume(),
                target: target.clamp(0, 1000) as f32 / 1000.0 * 0.65,
                delete: false,
            },
        );
    }

    pub fn tick(&mut self) {
        if self.paused_at.is_some() {
            return;
        }
        let now = Instant::now();
        let mut finished = Vec::new();
        for (&slot, fade) in &self.fades {
            let progress = (now.duration_since(fade.start).as_secs_f32()
                / fade.duration.as_secs_f32())
            .min(1.0);
            if let Some(sink) = self.sinks.get(&slot) {
                sink.set_volume(fade.volume + (fade.target - fade.volume) * progress);
            }
            if progress >= 1.0 {
                finished.push((slot, fade.delete, fade.target));
            }
        }
        for (slot, delete, target) in finished {
            self.fades.remove(&slot);
            if delete {
                self.delete(slot);
            } else {
                self.volume(slot, (target / 0.65 * 1000.0).round() as i32);
            }
        }
    }

    pub fn stop(&mut self, slot: i32) {
        if let Some(state) = self.states.get_mut(&slot) {
            state.playing = false;
        }
        self.fades.remove(&slot);
        if let Some(sink) = self.sinks.remove(&slot) {
            sink.stop();
        }
    }
    pub fn stop_all(&mut self) {
        for state in self.states.values_mut() {
            state.playing = false;
        }
        for sink in self.sinks.values() {
            sink.stop();
        }
        self.sinks.clear();
        self.fades.clear();
    }
    pub fn delete(&mut self, slot: i32) {
        self.stop(slot);
        self.data.remove(&slot);
        self.volumes.remove(&slot);
        self.states.remove(&slot);
    }
    pub fn is_playing(&self, slot: i32) -> bool {
        self.sinks.get(&slot).is_some_and(|sink| !sink.empty())
    }
    pub fn enabled(&self) -> bool {
        self.stream.is_some()
    }

    pub fn is_stable(&self) -> bool {
        self.fades.is_empty()
    }

    pub fn snapshot(&self) -> Vec<(i32, SoundState)> {
        self.states
            .iter()
            .map(|(&slot, state)| {
                let mut state = state.clone();
                if self.stream.is_some() {
                    state.playing = self.is_playing(slot);
                }
                (slot, state)
            })
            .collect()
    }

    pub fn set_paused(&mut self, paused: bool) {
        if paused {
            if self.paused_at.is_none() {
                self.paused_at = Some(Instant::now());
            }
            for sink in self.sinks.values() {
                sink.pause();
            }
        } else if let Some(start) = self.paused_at.take() {
            let elapsed = start.elapsed();
            for fade in self.fades.values_mut() {
                fade.start += elapsed;
            }
            for sink in self.sinks.values() {
                sink.play();
            }
        }
    }
}
