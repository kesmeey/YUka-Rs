//! Host reading modes and a bounded, session-local dialogue log.
use super::*;
use std::collections::VecDeque;

const HISTORY_LIMIT: usize = 500;
const SKIP_INTERVAL: Duration = Duration::from_millis(45);

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum PlaybackMode {
    #[default]
    Manual,
    Auto,
    SkipRead,
    SkipAll,
}

impl PlaybackMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Auto => "auto",
            Self::SkipRead => "skip_read",
            Self::SkipAll => "skip_all",
        }
    }

    fn skips(self) -> bool {
        matches!(self, Self::SkipRead | Self::SkipAll)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct DialogueEntry {
    pub id: u64,
    pub speaker: String,
    pub dialogue: String,
    pub voices: Vec<String>,
}

pub(super) struct Playback {
    mode: PlaybackMode,
    pub(super) current_read: bool,
    script_skip_allowed: bool,
    due: Option<Instant>,
    voice_finished: Option<Instant>,
    history: VecDeque<DialogueEntry>,
    revision: u64,
    next_id: u64,
    page_started: Instant,
    text_revealed: bool,
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            mode: PlaybackMode::Manual,
            current_read: false,
            script_skip_allowed: true,
            due: None,
            voice_finished: None,
            history: VecDeque::new(),
            revision: 0,
            next_id: 0,
            page_started: Instant::now(),
            text_revealed: false,
        }
    }
}

impl Playback {
    fn set_mode(&mut self, mode: PlaybackMode, now: Instant, chars: usize) {
        self.mode = mode;
        self.due = match mode {
            PlaybackMode::Auto => Some(now + auto_delay(chars)),
            PlaybackMode::SkipAll | PlaybackMode::SkipRead => Some(now + SKIP_INTERVAL),
            PlaybackMode::Manual => None,
        };
        self.voice_finished = None;
    }

    fn record(&mut self, speaker: &str, dialogue: &str, voices: &str) {
        if dialogue.trim().is_empty() {
            return;
        }
        self.next_id += 1;
        self.history.push_back(DialogueEntry {
            id: self.next_id,
            speaker: speaker.into(),
            dialogue: dialogue.into(),
            voices: voices
                .split('\t')
                .filter(|path| !path.is_empty())
                .map(str::to_owned)
                .collect(),
        });
        while self.history.len() > HISTORY_LIMIT {
            self.history.pop_front();
        }
        self.revision += 1;
    }

    fn ready(&mut self, now: Instant, voice_playing: bool) -> bool {
        if self.mode == PlaybackMode::SkipRead && !self.current_read {
            self.set_mode(PlaybackMode::Manual, now, 0);
            return false;
        }
        if self.mode == PlaybackMode::Manual || self.due.is_none_or(|due| now < due) {
            return false;
        }
        if self.mode == PlaybackMode::Auto {
            if voice_playing {
                self.voice_finished = None;
                return false;
            }
            let completed = self.voice_finished.get_or_insert(now);
            return now.saturating_duration_since(*completed) >= Duration::from_millis(300);
        }
        self.script_skip_allowed
    }
}

fn auto_delay(chars: usize) -> Duration {
    // Text is currently shown in full: give silent narration reading time, and
    // independently wait for the actual MediaPlayer voice completion callback.
    Duration::from_millis(900 + chars.min(220) as u64 * 50)
}

#[cfg(any(target_os = "android", test))]
pub(super) fn cache_history_voice(directory: &Path, id: u64, bytes: &[u8]) -> Result<PathBuf> {
    use std::{
        fs::OpenOptions,
        io::Write,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT_EXPORT: AtomicU64 = AtomicU64::new(0);
    loop {
        let sequence = NEXT_EXPORT.fetch_add(1, Ordering::Relaxed);
        let file = directory.join(format!(
            "history-{}-{sequence}-{id}.ogg",
            std::process::id()
        ));
        let mut output = match OpenOptions::new().write(true).create_new(true).open(&file) {
            Ok(output) => output,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        };
        if let Err(error) = output.write_all(bytes) {
            drop(output);
            let _ = std::fs::remove_file(&file);
            return Err(error.into());
        }
        return Ok(file);
    }
}

impl Runtime {
    pub fn set_playback_mode(&mut self, mode: &str) -> Result<()> {
        let mode = match mode {
            "manual" => PlaybackMode::Manual,
            "auto" => PlaybackMode::Auto,
            "skip_read" => PlaybackMode::SkipRead,
            "skip_all" => PlaybackMode::SkipAll,
            _ => return Err(format!("unknown playback mode: {mode}").into()),
        };
        let mode = if self.scene.finished || matches!(self.waiting, Waiting::Choice) {
            PlaybackMode::Manual
        } else {
            mode
        };
        self.playback
            .set_mode(mode, Instant::now(), self.scene.dialogue.chars().count());
        if mode == PlaybackMode::Auto {
            self.playback.due = Some(Instant::now() + self.configured_auto_delay());
        }
        Ok(())
    }

    pub fn playback_mode(&self) -> PlaybackMode {
        self.playback.mode
    }

    pub fn waiting_key(&self) -> bool {
        matches!(self.waiting, Waiting::Key)
    }

    pub fn current_read(&self) -> bool {
        self.waiting_key() && self.playback.current_read
    }

    pub fn voice_playing(&self) -> bool {
        self.sound.voice_playing()
    }

    pub fn visible_chars(&self) -> usize {
        let chars = self.scene.dialogue.chars().count();
        let speed = self.global_flag(4).clamp(0, 66) as u128;
        if self.fast_execution() || self.playback.text_revealed || speed == 0 {
            return chars;
        }
        let now = self.paused_at.unwrap_or_else(Instant::now);
        let elapsed = now
            .saturating_duration_since(self.playback.page_started)
            .as_millis();
        usize::try_from(elapsed / speed)
            .unwrap_or(usize::MAX)
            .min(chars)
    }

    pub fn text_complete(&self) -> bool {
        self.visible_chars() >= self.scene.dialogue.chars().count()
    }

    pub fn reveal_text(&mut self) {
        self.playback.text_revealed = true;
    }

    pub fn stop_current_voice(&mut self) {
        self.sound.stop_voices();
    }

    fn configured_auto_delay(&self) -> Duration {
        let speed = self.global_flag(5).clamp(0, 6600) as u64;
        let chars = if speed == 0 {
            0
        } else {
            self.scene.dialogue.chars().count().min(10000) as u64
        };
        Duration::from_millis(speed + chars * 50)
    }

    pub fn history(&self) -> &VecDeque<DialogueEntry> {
        &self.playback.history
    }

    pub fn history_revision(&self) -> u64 {
        self.playback.revision
    }

    pub(super) fn reset_history(&mut self) {
        self.playback.history.clear();
        self.playback.revision += 1;
    }

    pub(super) fn record_dialogue(&mut self) {
        // KeyWait arms the native "replace previous page voice" bit. Keep the
        // old voice through silent pages unless click-to-stop is enabled, then
        // stop it when the first new voice starts (not every voice in a chorus).
        self.sound.mark_voice_boundary();
        self.playback.page_started = Instant::now();
        self.playback.text_revealed = false;
        self.playback.record(
            &self.scene.speaker,
            &self.scene.dialogue,
            &self.page_voice_paths,
        );
        self.playback.set_mode(
            self.playback.mode,
            Instant::now(),
            self.scene.dialogue.chars().count(),
        );
        if self.playback.mode == PlaybackMode::Auto {
            self.playback.due = Some(Instant::now() + self.configured_auto_delay());
        }
    }

    pub(super) fn shift_playback_time(&mut self, elapsed: Duration) {
        self.playback.page_started += elapsed;
        if let Some(due) = &mut self.playback.due {
            *due += elapsed;
        }
        if let Some(completed) = &mut self.playback.voice_finished {
            *completed += elapsed;
        }
    }

    pub(super) fn fast_execution(&self) -> bool {
        self.fast || (self.playback.mode.skips() && self.playback.script_skip_allowed)
    }

    pub(super) fn script_skip_mode(&mut self, value: i32) -> Result<()> {
        if !(0..=2).contains(&value) {
            return Err("ScriptSkipMode must be 0, 1, or 2".into());
        }
        self.playback.script_skip_allowed = value != 0;
        if value == 0 && self.playback.mode.skips() {
            self.set_playback_mode("manual")?;
        }
        Ok(())
    }

    pub(super) fn reset_script_playback(&mut self) -> Result<()> {
        self.set_playback_mode("manual")?;
        // selectA snapshots modes before stopping them. Its FC_RETURN honors
        // GlobalFlag12/13 and calls setNowMode after the user selects a branch.
        // Keep those snapshots so both "continue after choice" settings work.
        Ok(())
    }

    pub(super) fn tick_playback(&mut self) -> Result<bool> {
        if self.scene.finished || matches!(self.waiting, Waiting::Choice) {
            self.set_playback_mode("manual")?;
            return Ok(false);
        }
        if self.fast_execution() && self.playback.mode.skips() {
            for scroll in self.scrolls.drain(..) {
                if let Some(layer) = self.scene.layers.get_mut(&scroll.slot) {
                    (layer.x, layer.y) = scroll.to;
                }
            }
            self.fade = None;
            self.scene.transition_from = None;
            self.scene.transition_progress = 1.0;
            self.effects.finish_transitions(&mut self.scene);
            if matches!(
                self.waiting,
                Waiting::Timer(_) | Waiting::Sound(_) | Waiting::Voice
            ) {
                self.waiting = Waiting::Ready;
                self.drive()?;
            }
        }
        if self.waiting_key()
            && (self.playback.mode != PlaybackMode::Auto || self.text_complete())
            && self.playback.ready(Instant::now(), self.voice_playing())
        {
            // Voices from the old page must not overlap fast-forwarded text.
            if self.playback.mode.skips() {
                self.stop_current_voice();
            }
            self.advance()?;
            return Ok(true);
        }
        Ok(false)
    }

    #[cfg(target_os = "android")]
    pub(crate) fn history_voice_files(&mut self, id: u64, directory: &Path) -> Result<Vec<String>> {
        let entry = self
            .playback
            .history
            .iter()
            .find(|entry| entry.id == id)
            .ok_or("dialogue history entry expired")?;
        let paths = entry.voices.clone();
        let mut result = Vec::new();
        std::fs::create_dir_all(directory)?;
        let export = (|| -> Result<()> {
            for path in &paths {
                // Only registered original archive members are accepted, never host paths.
                let bytes = self.resources.read(path)?;
                if !bytes.starts_with(b"OggS") {
                    return Err("history voice is not an Ogg resource".into());
                }
                // Each request owns different files, so discarding a stale UI response
                // cannot unlink a newer replay of the same history entry.
                let file = cache_history_voice(directory, id, &bytes)?;
                result.push(file.to_string_lossy().into_owned());
            }
            Ok(())
        })();
        if let Err(error) = export {
            for file in result {
                let _ = std::fs::remove_file(file);
            }
            return Err(error);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skip_read_stops_on_first_unread_boundary() {
        let now = Instant::now();
        let mut playback = Playback {
            current_read: true,
            ..Playback::default()
        };
        playback.set_mode(PlaybackMode::SkipRead, now, 10);
        assert!(!playback.ready(now, false));
        assert!(playback.ready(now + SKIP_INTERVAL, true));
        playback.current_read = false;
        assert!(!playback.ready(now + SKIP_INTERVAL * 2, false));
        assert_eq!(playback.mode, PlaybackMode::Manual);
    }

    #[test]
    fn auto_waits_for_text_then_voice_and_post_voice_delay() {
        let now = Instant::now();
        let mut playback = Playback::default();
        playback.set_mode(PlaybackMode::Auto, now, 20);
        assert!(!playback.ready(now, false));
        let due = now + auto_delay(20);
        assert!(!playback.ready(due, true));
        assert!(!playback.ready(due + Duration::from_secs(10), true));
        assert!(!playback.ready(due + Duration::from_secs(11), false));
        assert!(playback.ready(due + Duration::from_millis(11_300), false));
    }

    #[test]
    fn history_bounds_entries_and_preserves_voice_order() {
        let mut playback = Playback::default();
        playback.record("", "  ", "");
        assert!(playback.history.is_empty());
        for index in 0..HISTORY_LIMIT + 20 {
            playback.record("名字", &format!("第{index}句"), "a.ogg\tb.ogg");
        }
        assert_eq!(playback.history.len(), HISTORY_LIMIT);
        assert_eq!(playback.history.front().unwrap().dialogue, "第20句");
        let last = playback.history.back().unwrap();
        assert_eq!(last.id, 520);
        assert_eq!(last.voices, ["a.ogg", "b.ogg"]);
    }

    #[test]
    fn repeated_history_exports_have_independent_file_ownership() -> Result<()> {
        let directory = std::env::temp_dir();
        let older = cache_history_voice(&directory, 7, b"OggSolder")?;
        let newer = cache_history_voice(&directory, 7, b"OggSnewer")?;
        assert_ne!(older, newer);
        std::fs::remove_file(older)?;
        assert_eq!(std::fs::read(&newer)?, b"OggSnewer");
        std::fs::remove_file(newer)?;
        Ok(())
    }

    #[test]
    #[ignore = "requires the original game through YUKA_TEST_GAME"]
    fn new_story_voice_replaces_previous_character_across_real_dialogue_pages() -> Result<()> {
        let game = PathBuf::from(std::env::var("YUKA_TEST_GAME")?);
        let mut runtime = Runtime::new(
            &game,
            "yks/story/common/CHAPTER1-1.YKS",
            TextEncoding::Gbk,
            true,
            true,
            0,
        )?;
        assert_eq!(runtime.global_flag(16), 0);
        let mut previous_character = None;
        let mut character_changes = 0;
        for _ in 0..160 {
            assert!(runtime.waiting_key());
            let current = runtime.history().back().unwrap();
            if !current.voices.is_empty() {
                let active: Vec<_> = runtime
                    .sound
                    .snapshot()
                    .into_iter()
                    .filter(|(slot, state)| (10..=21).contains(slot) && state.playing)
                    .collect();
                assert!(!active.is_empty());
                for (_, voice) in &active {
                    assert!(
                        current.voices.contains(&voice.path),
                        "voice from a preceding page remains active"
                    );
                }
                let character = active[0].0;
                if previous_character.is_some_and(|previous| previous != character) {
                    character_changes += 1;
                    assert!(
                        runtime
                            .sound
                            .snapshot()
                            .iter()
                            .any(|(slot, state)| *slot == 1 && state.playing)
                    );
                }
                previous_character = Some(character);
                if character_changes >= 3 {
                    println!(
                        "Validated {} real dialogue pages and {character_changes} character handoffs with click-stop disabled",
                        runtime.pages
                    );
                    return Ok(());
                }
            }
            runtime.advance()?;
        }
        Err("did not reach three cross-character voice handoffs".into())
    }

    #[test]
    #[ignore = "requires the original game through YUKA_TEST_GAME"]
    fn original_read_mark_skip_auto_history_and_native_reload() -> Result<()> {
        let game = PathBuf::from(std::env::var("YUKA_TEST_GAME")?);
        let output = PathBuf::from("evidence/playback-test").join(format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let mut runtime = Runtime::new_native(
            &game,
            DEFAULT_SCRIPT,
            TextEncoding::Gbk,
            true,
            true,
            0,
            output.clone(),
        )?;
        assert!(runtime.waiting_key());
        assert!(!runtime.current_read());
        assert_eq!(runtime.history().len(), 1);
        let first = runtime.scene.dialogue.clone();
        let slot = output.join("savedata.002");
        runtime.save_native(&slot)?;
        for _ in 0..3 {
            runtime.advance()?;
        }
        assert_eq!(runtime.history().len(), 4);
        runtime.load_native(&slot)?;
        assert_eq!(runtime.scene.dialogue, first);
        assert!(runtime.current_read());
        assert_eq!(runtime.history().len(), 1);
        runtime.set_playback_mode("skip_read")?;
        for _ in 0..6 {
            runtime.playback.due = Some(Instant::now() - SKIP_INTERVAL);
            runtime.tick()?;
        }
        assert_eq!(runtime.playback_mode(), PlaybackMode::Manual);
        assert!(!runtime.current_read());
        assert_eq!(runtime.history().len(), 5);
        assert_ne!(runtime.scene.dialogue, first);

        runtime.set_playback_mode("auto")?;
        let due = runtime.playback.due.unwrap();
        runtime.set_paused(true);
        runtime.paused_at = Some(Instant::now() - Duration::from_secs(5));
        runtime.set_paused(false);
        assert!(runtime.playback.due.unwrap() >= due + Duration::from_secs(5));
        runtime.playback.due = Some(Instant::now() - Duration::from_secs(1));
        runtime.playback.voice_finished = Some(Instant::now() - Duration::from_secs(1));
        runtime.tick()?;
        assert_eq!(runtime.history().len(), 6);
        assert_eq!(runtime.playback_mode(), PlaybackMode::Auto);

        let mut choice = Runtime::new_unstarted(
            &game,
            "yks/function/selectA.yks",
            TextEncoding::Gbk,
            true,
            true,
            0,
        )?;
        let vm = &mut choice.stack.last_mut().unwrap().vm;
        vm.set_variable(
            Slot::IntegerVariable {
                bank: "Flag".into(),
                id: 101,
            },
            Value::Integer(2),
        )?;
        for id in 1..=2 {
            vm.set_variable(
                Slot::StringVariable {
                    bank: "String".into(),
                    id,
                },
                text_value(&format!("选项{id}")),
            )?;
        }
        choice.drive()?;
        choice.save_native(&output.join("savedata.006"))?;
        runtime.load_native(&output.join("savedata.006"))?;
        assert!(!runtime.choices().is_empty());
        runtime.set_playback_mode("skip_all")?;
        assert_eq!(runtime.playback_mode(), PlaybackMode::Manual);
        runtime.set_playback_mode("auto")?;
        assert_eq!(runtime.playback_mode(), PlaybackMode::Manual);
        runtime.tick()?;
        assert!(!runtime.choices().is_empty());
        Ok(())
    }
}
