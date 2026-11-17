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

    /// Trajectories and blocking step count from native 0x4183f0 / 0x418e70.
    pub(super) fn screen_shake(
        &mut self,
        scene: &mut Scene,
        kind: i32,
        milliseconds: i32,
        amplitude: i32,
        fast: bool,
        simplified: bool,
    ) -> Result<Duration> {
        if !matches!(kind, 1 | 5) {
            return Err(format!("unsupported Effect screen shake mode {kind}").into());
        }
        if !(0..=1024).contains(&amplitude) {
            return Err("Effect amplitude outside 0..1024".into());
        }
        let interval = if milliseconds < 0 {
            if kind == 1 { 100 } else { 50 }
        } else {
            milliseconds.max(1)
        };
        let interval = if simplified {
            (interval / if kind == 1 { 4 } else { 12 }).max(1)
        } else {
            interval
        };
        let half = amplitude / 2;
        let quarter = half / 2;
        let points = if kind == 1 {
            vec![
                (0, amplitude),
                (0, -amplitude),
                (0, amplitude),
                (0, -amplitude),
            ]
        } else {
            vec![
                (-half, -half),
                (half, -half),
                (-half, half),
                (half, half),
                (-quarter, -quarter),
                (half, half),
                (half, -half),
                (-half, -half),
                (-half, quarter),
                (0, -half),
                (-quarter, half),
                (half, -half),
            ]
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
