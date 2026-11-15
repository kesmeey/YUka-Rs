use crate::Result;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
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
}

pub struct Sound {
    stream: Option<(OutputStream, OutputStreamHandle)>,
    data: BTreeMap<i32, Arc<[u8]>>,
    sinks: BTreeMap<i32, Sink>,
    volumes: BTreeMap<i32, f32>,
    fades: BTreeMap<i32, Fade>,
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
            warning,
            decoded_sounds: 0,
        }
    }

    pub fn load(&mut self, slot: i32, bytes: Vec<u8>) -> Result<()> {
        let data: Arc<[u8]> = bytes.into();
        let mut decoder = Decoder::new(Cursor::new(data.clone()))?;
        if decoder.next().is_none() {
            return Err("empty decoded audio stream".into());
        }
        self.stop(slot);
        self.volumes.remove(&slot);
        self.decoded_sounds += 1;
        self.data.insert(slot, data);
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
        Ok(())
    }

    pub fn volume(&mut self, slot: i32, volume: i32) {
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
            },
        );
    }

    pub fn tick(&mut self) {
        let now = Instant::now();
        let mut finished = Vec::new();
        for (&slot, fade) in &self.fades {
            let progress = (now.duration_since(fade.start).as_secs_f32()
                / fade.duration.as_secs_f32())
            .min(1.0);
            if let Some(sink) = self.sinks.get(&slot) {
                sink.set_volume(fade.volume * (1.0 - progress));
            }
            if progress >= 1.0 {
                finished.push(slot);
            }
        }
        for slot in finished {
            self.delete(slot);
        }
    }

    pub fn stop(&mut self, slot: i32) {
        self.fades.remove(&slot);
        if let Some(sink) = self.sinks.remove(&slot) {
            sink.stop();
        }
    }
    pub fn stop_all(&mut self) {
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
    }
    pub fn is_playing(&self, slot: i32) -> bool {
        self.sinks.get(&slot).is_some_and(|sink| !sink.empty())
    }
    pub fn enabled(&self) -> bool {
        self.stream.is_some()
    }
}
