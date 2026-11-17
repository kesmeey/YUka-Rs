//! Verified graphic effects used by the continuous common-story scripts.
use super::{Layer, Resources, Result, Scene, Value, int};
use crate::graphics::{AnimationFrame, Ykg};
use image::RgbaImage;
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};

struct OpacityAnimation {
    from: u8,
    to: u8,
    start: Instant,
    duration: Duration,
}

struct SpriteAnimation {
    source_slot: i32,
    frames: Vec<AnimationFrame>,
    period_ms: u128,
    start: Instant,
    frozen: bool,
}

struct ScreenShake {
    points: Vec<(i32, i32)>,
    interval: Duration,
    start: Instant,
}

#[derive(Default)]
pub(super) struct GraphicsEffects {
    opacity: BTreeMap<i32, OpacityAnimation>,
    sprites: BTreeMap<(i32, i32), SpriteAnimation>,
    shake: Option<ScreenShake>,
}

fn opacity(value: i32) -> u8 {
    (((1000 - value.clamp(0, 1000)) * 255 + 500) / 1000) as u8
}

fn rect(frame: &AnimationFrame) -> [u32; 4] {
    [frame.x, frame.y, frame.width, frame.height]
}

pub(super) fn sprite_sequence(
    frames: Vec<AnimationFrame>,
    sequence: usize,
    image: &RgbaImage,
) -> Result<Vec<AnimationFrame>> {
    let sentinel = AnimationFrame {
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        duration: 0,
        flags: 0,
        unknown: 0,
        action: 0,
    };
    let frames = frames
        .split(|frame| frame == &sentinel)
        .nth(sequence)
        .ok_or("sprite animation sequence is out of bounds")?;
    if frames.is_empty() {
        return Err("sprite animation has no display frames".into());
    }
    for frame in frames {
        if frame.width == 0
            || frame.height == 0
            || frame
                .x
                .checked_add(frame.width)
                .is_none_or(|end| end > image.width())
            || frame
                .y
                .checked_add(frame.height)
                .is_none_or(|end| end > image.height())
        {
            return Err("sprite frame lies outside its source image".into());
        }
    }
    Ok(frames.to_vec())
}

fn sprite_frames(
    frames: Vec<AnimationFrame>,
    sequence: usize,
    image: &RgbaImage,
) -> Result<Vec<AnimationFrame>> {
    let frames = sprite_sequence(frames, sequence, image)?;
    if frames.iter().any(|frame| {
        frame.flags != 2 || frame.unknown != 0 || frame.action != 0 || frame.duration == 0
    }) {
        return Err("unsupported sprite frame flags, action, or duration".into());
    }
    Ok(frames)
}

impl GraphicsEffects {
    pub(super) fn finish_transitions(&mut self, scene: &mut Scene) {
        self.shake = None;
        scene.shake_offset = (0, 0);
        for (slot, animation) in std::mem::take(&mut self.opacity) {
            if let Some(layer) = scene.layers.get_mut(&slot) {
                layer.opacity = animation.to;
            }
        }
    }

    pub(super) fn clear_slot(&mut self, slot: i32, scene: &mut Scene) {
        self.opacity.remove(&slot);
        self.sprites.retain(|&(parent, id), sprite| {
            if parent != slot && sprite.source_slot != slot {
                return true;
            }
            if let Some(layer) = scene.layers.get_mut(&parent) {
                layer.children.remove(&id);
            }
            false
        });
    }

    pub(super) fn is_stable(&self) -> bool {
        // Sprite backing-layer state is transient and is not serialized yet.
        self.opacity.is_empty() && self.sprites.is_empty() && self.shake.is_none()
    }

    pub(super) fn deadline(&self, slot: i32) -> Option<Instant> {
        self.opacity
            .get(&slot)
            .map(|animation| animation.start + animation.duration)
    }

    pub(super) fn shift_time(&mut self, duration: Duration) {
        if let Some(shake) = &mut self.shake {
            shake.start += duration;
        }
        for animation in self.opacity.values_mut() {
            animation.start += duration;
        }
        for sprite in self.sprites.values_mut() {
            sprite.start += duration;
        }
    }

    pub(super) fn tick(&mut self, scene: &mut Scene) -> bool {
        self.tick_at(scene, Instant::now())
    }

    fn tick_at(&mut self, scene: &mut Scene, now: Instant) -> bool {
        let mut changed = false;
        if let Some(shake) = &self.shake {
            let frame =
                now.saturating_duration_since(shake.start).as_millis() / shake.interval.as_millis();
            let offset = shake.points.get(frame as usize).copied().unwrap_or((0, 0));
            changed |= scene.shake_offset != offset;
            scene.shake_offset = offset;
            if frame >= shake.points.len() as u128 {
                self.shake = None;
            }
        }
        self.opacity.retain(|slot, animation| {
            let Some(layer) = scene.layers.get_mut(slot) else {
                return false;
            };
            let elapsed = now.saturating_duration_since(animation.start);
            let progress = (elapsed.as_secs_f64() / animation.duration.as_secs_f64()).min(1.0);
            let value = (f64::from(animation.from)
                + (f64::from(animation.to) - f64::from(animation.from)) * progress)
                .round() as u8;
            changed |= layer.opacity != value;
            layer.opacity = value;
            elapsed < animation.duration
        });
        self.sprites.retain(|(parent, id), sprite| {
            let Some(layer) = scene
                .layers
                .get_mut(parent)
                .and_then(|layer| layer.children.get_mut(id))
            else {
                return false;
            };
            if sprite.frozen {
                return true;
            }
            let mut remaining =
                now.saturating_duration_since(sprite.start).as_millis() % sprite.period_ms;
            let mut selected = sprite.frames.len() - 1;
            for (index, frame) in sprite.frames.iter().enumerate() {
                if remaining < u128::from(frame.duration) {
                    selected = index;
                    break;
                }
                remaining -= u128::from(frame.duration);
            }
            let source_rect = Some(rect(&sprite.frames[selected]));
            changed |= layer.source_rect != source_rect;
            layer.source_rect = source_rect;
            true
        });
        changed
    }

    /// Native Effect modes 1..=6 (0x4183f0 through 0x419110).
    pub(super) fn screen_shake(
        &mut self,
        scene: &mut Scene,
        kind: i32,
        milliseconds: i32,
        amplitude: i32,
        fast: bool,
        simplified: bool,
    ) -> Result<Duration> {
        if !(1..=6).contains(&kind) {
            return Err(format!("unsupported Effect screen shake mode {kind}").into());
        }
        if !(0..=1024).contains(&amplitude) {
            return Err("Effect amplitude outside 0..1024".into());
        }
        let interval = if milliseconds < 0 {
            if kind <= 4 { 100 } else { 50 }
        } else {
            milliseconds.max(1)
        };
        let interval = if simplified {
            (interval / if kind <= 4 { 4 } else { 12 }).max(1)
        } else {
            interval
        };
        // Mode 5 halves the supplied amplitude before constructing the same
        // twelve-point trajectory as mode 6. Integer division matches x86's
        // signed truncation (all accepted amplitudes here are nonnegative).
        let scale = if kind == 5 { amplitude / 2 } else { amplitude };
        let half = scale / 2;
        let points = match kind {
            1 => vec![
                (0, amplitude),
                (0, -amplitude),
                (0, amplitude),
                (0, -amplitude),
            ],
            2 => vec![
                (amplitude, 0),
                (-amplitude, 0),
                (amplitude, 0),
                (-amplitude, 0),
            ],
            3 => vec![
                (amplitude, -amplitude),
                (-amplitude, amplitude),
                (amplitude, -amplitude),
                (-amplitude, amplitude),
            ],
            4 => vec![
                (-amplitude, -amplitude),
                (amplitude, amplitude),
                (-amplitude, -amplitude),
                (amplitude, amplitude),
            ],
            5 | 6 => vec![
                (-scale, -scale),
                (scale, -scale),
                (-scale, scale),
                (scale, scale),
                (-half, -half),
                (scale, scale),
                (scale, -scale),
                (-scale, -scale),
                (-scale, half),
                (0, -scale),
                (-half, scale),
                (scale, -scale),
            ],
            _ => unreachable!("Effect mode was validated above"),
        };
        if fast || amplitude == 0 {
            self.shake = None;
            scene.shake_offset = (0, 0);
            return Ok(Duration::ZERO);
        }
        let duration = Duration::from_millis(interval as u64 * points.len() as u64);
        scene.shake_offset = points[0];
        self.shake = Some(ScreenShake {
            points,
            interval: Duration::from_millis(interval as u64),
            start: Instant::now(),
        });
        Ok(duration)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn handle(
        &mut self,
        command: &str,
        args: &[Value],
        scene: &mut Scene,
        resources: &mut Resources,
        sources: &mut BTreeMap<i32, String>,
        fast: bool,
    ) -> Result<bool> {
        match command {
            "GraphicAddColor" => {
                let slot = int(args, 0)?;
                let color = int(args, 1)? as u32;
                if let Some(layer) = scene.layers.get_mut(&slot) {
                    layer.additive_color = [color as u8, (color >> 8) as u8, (color >> 16) as u8];
                }
            }
            "GraphicTransmittance" | "GraphicTransmittanceScroll" => {
                let slot = int(args, 0)?;
                let to = opacity(int(args, 1)?);
                let milliseconds = if command == "GraphicTransmittanceScroll" {
                    int(args, 2)?
                } else {
                    0
                };
                self.opacity.remove(&slot);
                if let Some(layer) = scene.layers.get_mut(&slot) {
                    if fast || milliseconds <= 0 || layer.opacity == to {
                        layer.opacity = to;
                    } else {
                        self.opacity.insert(
                            slot,
                            OpacityAnimation {
                                from: layer.opacity,
                                to,
                                start: Instant::now(),
                                duration: Duration::from_millis(milliseconds as u64),
                            },
                        );
                    }
                }
            }
            "GraphicFilter" => {
                let slot = int(args, 0)?;
                let mode = int(args, 1)?;
                if !matches!(mode, 1..=3) {
                    return Err("only verified GraphicFilter modes 1 to 3 are implemented".into());
                }
                self.clear_slot(slot, scene);
                scene.layers.insert(
                    slot,
                    Layer {
                        image: Arc::new(RgbaImage::new(1, 1)),
                        x: 0,
                        y: 0,
                        opacity: 255,
                        additive_color: [0; 3],
                        source_rect: None,
                        sepia_filter: mode == 1,
                        grayscale_filter: mode == 2,
                        invert_filter: mode == 3,
                        children: BTreeMap::new(),
                        visible: true,
                    },
                );
                sources.insert(slot, String::new());
            }
            "SpriteCreate" | "SpriteCreateHide" => {
                let parent = int(args, 0)?;
                let id = int(args, 1)?;
                let offset = (int(args, 2)?, int(args, 3)?);
                let source_slot = int(args, 4)?;
                let sequence =
                    usize::try_from(int(args, 5)?).map_err(|_| "negative sprite sequence")?;
                if parent == source_slot {
                    return Err("sprite source cannot be its parent layer".into());
                }
                let source_name = sources
                    .get(&source_slot)
                    .ok_or("sprite source is not loaded")?
                    .clone();
                let image = resources.image(&source_name)?;
                let bytes = resources.read(&source_name)?;
                let frames = sprite_frames(Ykg::parse(&bytes)?.frames, sequence, &image)?;
                let source_rect = rect(&frames[0]);
                let period_ms = frames.iter().map(|frame| u128::from(frame.duration)).sum();
                let parent_layer = scene
                    .layers
                    .get_mut(&parent)
                    .ok_or("sprite parent is not loaded")?;
                parent_layer.children.insert(
                    id,
                    Layer {
                        image,
                        x: offset.0,
                        y: offset.1,
                        opacity: 255,
                        additive_color: [0; 3],
                        source_rect: Some(source_rect),
                        sepia_filter: false,
                        grayscale_filter: false,
                        invert_filter: false,
                        children: BTreeMap::new(),
                        visible: command == "SpriteCreate",
                    },
                );
                self.sprites.insert(
                    (parent, id),
                    SpriteAnimation {
                        source_slot,
                        frames,
                        period_ms,
                        start: Instant::now(),
                        frozen: fast,
                    },
                );
            }
            "SpriteDelete" | "SpriteShow" | "SpriteHide" => {
                let parent = int(args, 0)?;
                let id = int(args, 1)?;
                if command == "SpriteDelete" {
                    self.sprites.remove(&(parent, id));
                    if let Some(layer) = scene.layers.get_mut(&parent) {
                        layer.children.remove(&id);
                    }
                } else if let Some(layer) = scene
                    .layers
                    .get_mut(&parent)
                    .and_then(|layer| layer.children.get_mut(&id))
                {
                    layer.visible = command == "SpriteShow";
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_six_native_shake_modes_follow_every_verified_step() -> Result<()> {
        // Independent native fixtures at amplitude 7 also exercise the signed
        // integer truncation in modes 5 and 6, rather than only even inputs.
        let fixtures: [Vec<(i32, i32)>; 6] = [
            vec![(0, 7), (0, -7), (0, 7), (0, -7)],
            vec![(7, 0), (-7, 0), (7, 0), (-7, 0)],
            vec![(7, -7), (-7, 7), (7, -7), (-7, 7)],
            vec![(-7, -7), (7, 7), (-7, -7), (7, 7)],
            vec![
                (-3, -3),
                (3, -3),
                (-3, 3),
                (3, 3),
                (-1, -1),
                (3, 3),
                (3, -3),
                (-3, -3),
                (-3, 1),
                (0, -3),
                (-1, 3),
                (3, -3),
            ],
            vec![
                (-7, -7),
                (7, -7),
                (-7, 7),
                (7, 7),
                (-3, -3),
                (7, 7),
                (7, -7),
                (-7, -7),
                (-7, 3),
                (0, -7),
                (-3, 7),
                (7, -7),
            ],
        ];
        for (index, points) in fixtures.into_iter().enumerate() {
            let mut scene = Scene::default();
            let mut effects = GraphicsEffects::default();
            let duration =
                effects.screen_shake(&mut scene, index as i32 + 1, 20, 7, false, false)?;
            assert_eq!(duration, Duration::from_millis(points.len() as u64 * 20));
            let start = effects.shake.as_ref().unwrap().start;
            for (step, point) in points.iter().enumerate() {
                effects.tick_at(&mut scene, start + Duration::from_millis(step as u64 * 20));
                assert_eq!(scene.shake_offset, *point, "mode {} step {step}", index + 1);
                assert!(!effects.is_stable());
            }
            effects.tick_at(&mut scene, start + duration);
            assert_eq!(scene.shake_offset, (0, 0));
            assert!(effects.is_stable());
        }
        Ok(())
    }

    #[test]
    fn shake_defaults_acceleration_and_fast_validation_cover_all_modes() -> Result<()> {
        let mut scene = Scene::default();
        let mut effects = GraphicsEffects::default();
        for kind in 1..=6 {
            let steps = if kind <= 4 { 4 } else { 12 };
            let default_duration = if kind <= 4 { 400 } else { 600 };
            assert_eq!(
                effects.screen_shake(&mut scene, kind, -1, 7, false, false)?,
                Duration::from_millis(default_duration)
            );
            assert_eq!(
                effects.screen_shake(&mut scene, kind, 48, 7, false, true)?,
                Duration::from_millis(48)
            );
            assert_eq!(
                effects.screen_shake(&mut scene, kind, 0, 7, false, false)?,
                Duration::from_millis(steps)
            );
            assert_eq!(
                effects.screen_shake(&mut scene, kind, 20, 7, true, false)?,
                Duration::ZERO
            );
            assert_eq!(scene.shake_offset, (0, 0));
            assert!(effects.is_stable());
            assert_eq!(
                effects.screen_shake(&mut scene, kind, 20, 0, false, false)?,
                Duration::ZERO
            );
            for fast in [false, true] {
                assert!(
                    effects
                        .screen_shake(&mut scene, kind, 20, -1, fast, false)
                        .is_err()
                );
                assert!(
                    effects
                        .screen_shake(&mut scene, kind, 20, 1025, fast, false)
                        .is_err()
                );
            }
        }
        // The reported kaho_07 @1263 call must run even at amplitude 1.
        assert_eq!(
            effects.screen_shake(&mut scene, 2, 20, 1, false, false)?,
            Duration::from_millis(80)
        );
        assert_eq!(scene.shake_offset, (1, 0));
        for kind in [0, 7, 99] {
            for fast in [false, true] {
                assert!(
                    effects
                        .screen_shake(&mut scene, kind, 20, 7, fast, false)
                        .is_err()
                );
            }
        }
        Ok(())
    }

    #[test]
    #[ignore = "read-only corpus audit; set YUKA_GAME_DIR to supplied original game"]
    fn supplied_game_effect_calls_validate_without_fast_path_bypass() -> Result<()> {
        use crate::{
            archive::Archive,
            script::{Element, InstructionKind, Script, TextEncoding},
        };
        let directory = std::env::var_os("YUKA_GAME_DIR").ok_or("set YUKA_GAME_DIR")?;
        let mut archives = std::fs::read_dir(directory)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        archives.sort();
        let mut scripts = 0;
        let mut calls = 0;
        let mut modes = BTreeMap::<i32, usize>::new();
        for path in archives {
            if !path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("ykc"))
            {
                continue;
            }
            let mut archive = Archive::open(path)?;
            for index in 0..archive.entries.len() {
                let name = archive.entries[index].name.clone();
                if !name.to_ascii_lowercase().ends_with(".yks") {
                    continue;
                }
                scripts += 1;
                let script = Script::parse_with_encoding(
                    &archive.read_entry(index, 16 * 1024 * 1024)?,
                    TextEncoding::Gbk,
                )?;
                for instruction in script.disassemble()? {
                    let Element::Function { name: function } =
                        &script.entry(instruction.entry_index)?.element
                    else {
                        continue;
                    };
                    if function.text != "Effect" {
                        continue;
                    }
                    let InstructionKind::Call { arguments } = instruction.kind else {
                        unreachable!()
                    };
                    let arguments = arguments
                        .into_iter()
                        .map(|argument| match script.entry(argument)?.element {
                            Element::Integer { value } => Ok(value),
                            _ => Err(format!(
                                "{name} @{}: nonliteral Effect argument needs VM audit",
                                instruction.word_offset
                            )
                            .into()),
                        })
                        .collect::<Result<Vec<i32>>>()?;
                    let kind = *arguments.first().ok_or("Effect needs a mode")?;
                    let interval = arguments.get(1).copied().unwrap_or(-1);
                    let amplitude = arguments.get(2).copied().unwrap_or(5);
                    assert!(arguments.len() <= 3, "{name} @{}", instruction.word_offset);
                    for (fast, simplified) in [(false, false), (false, true), (true, false)] {
                        GraphicsEffects::default()
                            .screen_shake(
                                &mut Scene::default(),
                                kind,
                                interval,
                                amplitude,
                                fast,
                                simplified,
                            )
                            .map_err(|error| {
                                format!(
                                    "{name} @{} Effect{arguments:?}: {error}",
                                    instruction.word_offset
                                )
                            })?;
                    }
                    *modes.entry(kind).or_default() += 1;
                    calls += 1;
                }
            }
        }
        assert!(calls > 0, "no Effect calls found in supplied game");
        println!("Audited {scripts} scripts, {calls} Effect calls; modes {modes:?}");
        Ok(())
    }

    #[test]
    fn native_shake_sequences_block_reset_and_pause_without_position_drift() -> Result<()> {
        let mut scene = Scene::default();
        let mut effects = GraphicsEffects::default();
        assert_eq!(
            effects.screen_shake(&mut scene, 1, 20, 8, false, false)?,
            Duration::from_millis(80)
        );
        let start = effects.shake.as_ref().unwrap().start;
        assert_eq!(scene.shake_offset, (0, 8));
        assert!(!effects.is_stable());
        effects.tick_at(&mut scene, start + Duration::from_millis(20));
        assert_eq!(scene.shake_offset, (0, -8));
        effects.tick_at(&mut scene, start + Duration::from_millis(80));
        assert_eq!(scene.shake_offset, (0, 0));
        assert!(effects.is_stable());
        assert_eq!(
            effects.screen_shake(&mut scene, 5, 20, 8, false, false)?,
            Duration::from_millis(240)
        );
        let start = effects.shake.as_ref().unwrap().start;
        assert_eq!(scene.shake_offset, (-4, -4));
        effects.tick_at(&mut scene, start + Duration::from_millis(80));
        assert_eq!(scene.shake_offset, (-2, -2));
        effects.shift_time(Duration::from_secs(1));
        assert!(!effects.tick_at(&mut scene, start + Duration::from_millis(1080)));
        assert_eq!(scene.shake_offset, (-2, -2));
        effects.tick_at(&mut scene, start + Duration::from_millis(1160));
        assert_eq!(scene.shake_offset, (-4, 2));
        effects.finish_transitions(&mut scene);
        assert_eq!(scene.shake_offset, (0, 0));
        assert!(effects.is_stable());
        assert_eq!(
            effects.screen_shake(&mut scene, 5, 20, 8, true, false)?,
            Duration::ZERO
        );
        assert_eq!(scene.shake_offset, (0, 0));
        assert!(
            effects
                .screen_shake(&mut scene, 99, 20, 8, false, false)
                .is_err()
        );
        Ok(())
    }

    fn layer() -> Layer {
        Layer {
            image: Arc::new(RgbaImage::new(2, 1)),
            x: 0,
            y: 0,
            opacity: 255,
            additive_color: [0; 3],
            source_rect: None,
            sepia_filter: false,
            grayscale_filter: false,
            invert_filter: false,
            children: BTreeMap::new(),
            visible: true,
        }
    }

    #[test]
    fn transparency_uses_inverse_thousandths_and_finishes_at_deadline() {
        assert_eq!(opacity(0), 255);
        assert_eq!(opacity(500), 128);
        assert_eq!(opacity(1000), 0);
        let start = Instant::now();
        let mut scene = Scene::default();
        scene.layers.insert(99, layer());
        let mut effects = GraphicsEffects::default();
        effects.opacity.insert(
            99,
            OpacityAnimation {
                from: 255,
                to: 0,
                start,
                duration: Duration::from_millis(1000),
            },
        );
        assert!(!effects.is_stable());
        assert!(effects.tick_at(&mut scene, start + Duration::from_millis(500)));
        assert_eq!(scene.layers[&99].opacity, 128);
        effects.shift_time(Duration::from_secs(2));
        assert!(!effects.tick_at(&mut scene, start + Duration::from_millis(2500)));
        assert_eq!(effects.deadline(99), Some(start + Duration::from_secs(3)));
        effects.tick_at(&mut scene, start + Duration::from_secs(3));
        assert_eq!(scene.layers[&99].opacity, 0);
        assert!(effects.is_stable());
    }

    #[test]
    fn sprite_sequences_loop_and_source_deletion_removes_only_its_children() {
        let frame = AnimationFrame {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            duration: 300,
            flags: 2,
            unknown: 0,
            action: 0,
        };
        let next = AnimationFrame { x: 1, ..frame };
        let end = AnimationFrame {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            duration: 0,
            flags: 0,
            unknown: 0,
            action: 0,
        };
        let base = layer();
        let frames = sprite_frames(vec![frame, next, end], 0, &base.image).unwrap();
        assert!(sprite_frames(vec![frame, end], 1, &base.image).is_err());
        assert_eq!(
            sprite_frames(vec![frame, end, next, end], 1, &base.image).unwrap(),
            vec![next]
        );
        assert!(
            sprite_frames(
                vec![AnimationFrame { width: 3, ..frame }, end],
                0,
                &base.image
            )
            .is_err()
        );
        assert!(
            sprite_frames(
                vec![AnimationFrame { flags: 6, ..frame }, end],
                0,
                &base.image
            )
            .is_err()
        );
        let start = Instant::now();
        let mut effects = GraphicsEffects::default();
        effects.sprites.insert(
            (90, 0),
            SpriteAnimation {
                source_slot: 91,
                frames,
                period_ms: 600,
                start,
                frozen: false,
            },
        );
        effects.sprites.insert(
            (90, 1),
            SpriteAnimation {
                source_slot: 92,
                frames: vec![next, frame],
                period_ms: 600,
                start,
                frozen: false,
            },
        );
        let mut scene = Scene::default();
        let mut parent = base.clone();
        parent.children.insert(0, base);
        parent.children.insert(1, layer());
        scene.layers.insert(90, parent);
        effects.tick_at(&mut scene, start);
        assert_eq!(
            scene.layers[&90].children[&0].source_rect,
            Some([0, 0, 1, 1])
        );
        effects.tick_at(&mut scene, start + Duration::from_millis(300));
        assert_eq!(
            scene.layers[&90].children[&0].source_rect,
            Some([1, 0, 1, 1])
        );
        assert_eq!(
            scene.layers[&90].children[&1].source_rect,
            Some([0, 0, 1, 1])
        );
        effects.tick_at(&mut scene, start + Duration::from_millis(600));
        assert_eq!(
            scene.layers[&90].children[&0].source_rect,
            Some([0, 0, 1, 1])
        );
        assert!(!effects.is_stable());
        effects.shift_time(Duration::from_secs(1));
        assert!(!effects.tick_at(&mut scene, start + Duration::from_millis(1600)));
        effects.clear_slot(91, &mut scene);
        assert_eq!(effects.sprites.len(), 1);
        assert!(!effects.is_stable());
        assert!(!scene.layers[&90].children.contains_key(&0));
        assert!(scene.layers[&90].children.contains_key(&1));
        assert!(scene.layers[&90].source_rect.is_none());
        effects.clear_slot(90, &mut scene);
        assert!(effects.sprites.is_empty());
        assert!(scene.layers[&90].children.is_empty());
        assert!(effects.is_stable());
    }
}
