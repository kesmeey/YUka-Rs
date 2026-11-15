//! Desktop framebuffer host and software rendering for the Yuka demo.
//! The same Renderer can produce deterministic screenshots without a window.

use crate::Result;
use fontdue::{Font, FontSettings, Metrics};
use image::{ImageEncoder, RgbaImage};
use minifb::{Key, KeyRepeat, MouseButton, MouseMode, ScaleMode, Window, WindowOptions};
use std::collections::{BTreeMap, HashMap};
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::Arc;

pub const WIDTH: usize = 800;
pub const HEIGHT: usize = 600;

#[derive(Clone)]
pub struct Layer {
    pub image: Arc<RgbaImage>,
    pub x: i32,
    pub y: i32,
    pub opacity: u8,
    pub visible: bool,
}

#[derive(Clone)]
pub struct Scene {
    pub layers: BTreeMap<i32, Layer>,
    pub transition_from: Option<BTreeMap<i32, Layer>>,
    pub transition_progress: f32,
    pub speaker: String,
    pub dialogue: String,
    pub dialogue_offset: (i32, i32),
    pub speaker_offset: (i32, i32),
    pub dialogue_visible: bool,
    pub status: String,
    pub finished: bool,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            layers: BTreeMap::new(),
            transition_from: None,
            transition_progress: 1.0,
            speaker: String::new(),
            dialogue: String::new(),
            dialogue_offset: (0, 0),
            speaker_offset: (0, 0),
            dialogue_visible: true,
            status: String::new(),
            finished: false,
        }
    }
}

struct Glyph {
    metrics: Metrics,
    pixels: Vec<u8>,
}

pub struct Renderer {
    font: Font,
    glyphs: HashMap<(char, u16), Glyph>,
    pixels: Vec<u32>,
    previous: Vec<u32>,
}

impl Renderer {
    pub fn new(font_bytes: Vec<u8>) -> Result<Self> {
        let font = Font::from_bytes(font_bytes, FontSettings::default())
            .map_err(|error| format!("unable to read font: {error}"))?;
        if !font.has_glyph('中') {
            return Err(
                "selected font does not contain Chinese glyphs; select a CJK font with --font"
                    .into(),
            );
        }
        Ok(Self {
            font,
            glyphs: HashMap::new(),
            pixels: vec![0; WIDTH * HEIGHT],
            previous: vec![0; WIDTH * HEIGHT],
        })
    }

    pub fn render(&mut self, scene: &Scene, hide_dialogue: bool) -> &[u32] {
        self.pixels.fill(0);
        composite_layers(&mut self.pixels, &scene.layers);
        if let Some(layers) = &scene.transition_from {
            let progress = scene.transition_progress.clamp(0.0, 1.0);
            if progress < 1.0 {
                self.previous.fill(0);
                composite_layers(&mut self.previous, layers);
                let alpha = (progress * 255.0).round() as u8;
                for (current, previous) in self.pixels.iter_mut().zip(&self.previous) {
                    *current = blend(*previous, *current, alpha);
                }
            }
        }
        if scene.dialogue_visible && !hide_dialogue && !scene.dialogue.is_empty() {
            self.dialogue(scene);
        }
        if !hide_dialogue {
            self.footer(scene);
        }
        &self.pixels
    }

    pub fn pixels(&self) -> &[u32] {
        &self.pixels
    }

    pub fn save_png(&self, path: &Path) -> Result<()> {
        let mut bytes = Vec::with_capacity(WIDTH * HEIGHT * 3);
        for pixel in &self.pixels {
            bytes.extend_from_slice(&[(pixel >> 16) as u8, (pixel >> 8) as u8, *pixel as u8]);
        }
        let file = OpenOptions::new().write(true).create_new(true).open(path)?;
        let mut output = BufWriter::new(file);
        image::codecs::png::PngEncoder::new(&mut output).write_image(
            &bytes,
            WIDTH as u32,
            HEIGHT as u32,
            image::ExtendedColorType::Rgb8,
        )?;
        output.flush()?;
        Ok(())
    }

    fn dialogue(&mut self, scene: &Scene) {
        let mut size = 24;
        let mut lines = self.wrap(&scene.dialogue, size, 714.0);
        while size > 16 && lines.len() * (usize::from(size) + 5) + 18 > 520 {
            size -= 1;
            lines = self.wrap(&scene.dialogue, size, 714.0);
        }
        let line_height = usize::from(size) + 5;
        let panel_height = (lines.len() * line_height + 18).max(137);
        let panel_y = 567_i32.saturating_sub(panel_height as i32);
        let (dialogue_x, dialogue_y) = scene.dialogue_offset;
        let dialogue_top = panel_y.saturating_add(dialogue_y);
        self.rect(
            20_i32.saturating_add(dialogue_x),
            dialogue_top,
            760,
            panel_height,
            0x101b2b,
            226,
        );
        self.rect(
            20_i32.saturating_add(dialogue_x),
            dialogue_top,
            760,
            2,
            0xa7c1cf,
            230,
        );
        if !scene.speaker.is_empty() {
            let (speaker_x, speaker_y) = scene.speaker_offset;
            let width = (self.text_width(&scene.speaker, 23) + 38.0).ceil() as usize;
            self.rect(
                20_i32.saturating_add(speaker_x),
                panel_y.saturating_sub(35).saturating_add(speaker_y),
                width.clamp(110, 710),
                35,
                0x233951,
                242,
            );
            self.text(
                &scene.speaker,
                38_i32.saturating_add(speaker_x),
                panel_y.saturating_sub(31).saturating_add(speaker_y),
                23,
                0xf7e4b8,
            );
        }
        for (index, line) in lines.iter().enumerate() {
            self.text(
                line,
                42_i32.saturating_add(dialogue_x),
                dialogue_top.saturating_add(10 + (index * line_height) as i32),
                size,
                0xffffff,
            );
        }
    }

    fn footer(&mut self, scene: &Scene) {
        self.rect(0, 575, WIDTH, 25, 0x08111b, 238);
        let controls = if scene.finished {
            "演示结束 · Esc 退出"
        } else {
            "点击 / 空格继续 · H 隐藏文字 · Esc 退出"
        };
        self.text(controls, 18, 578, 15, 0xe0e9ef);
        let status = self.ellipsize(&scene.status, 14, 360.0);
        let width = self.text_width(&status, 14);
        self.text(&status, (782.0 - width).round() as i32, 579, 14, 0xa6b6c5);
    }

    fn error(&mut self, message: &str) {
        self.rect(45, 185, 710, 220, 0x151c29, 248);
        self.rect(45, 185, 710, 3, 0xda9a81, 255);
        self.text("播放暂停", 69, 205, 25, 0xf7e4b8);
        for (index, line) in self.wrap(message, 17, 660.0).iter().take(6).enumerate() {
            self.text(line, 69, 246 + index as i32 * 22, 17, 0xffffff);
        }
        self.text("Esc 退出", 644, 373, 16, 0xa6b6c5);
    }

    fn rect(&mut self, x: i32, y: i32, width: usize, height: usize, color: u32, alpha: u8) {
        let left = x.max(0) as usize;
        let top = y.max(0) as usize;
        let right = (i64::from(x) + width as i64).clamp(0, WIDTH as i64) as usize;
        let bottom = (i64::from(y) + height as i64).clamp(0, HEIGHT as i64) as usize;
        for row in top..bottom {
            for column in left..right {
                let pixel = &mut self.pixels[row * WIDTH + column];
                *pixel = blend(*pixel, color, alpha);
            }
        }
    }

    fn text_width(&self, text: &str, size: u16) -> f32 {
        text.chars()
            .map(|ch| self.font.metrics(ch, size as f32).advance_width)
            .sum()
    }

    fn wrap(&self, text: &str, size: u16, max_width: f32) -> Vec<String> {
        let mut lines = Vec::new();
        let mut line = String::new();
        let mut width = 0.0;
        for ch in text.chars().filter(|&ch| ch != '\r') {
            let advance = self.font.metrics(ch, size as f32).advance_width;
            if ch == '\n' || (!line.is_empty() && width + advance > max_width) {
                lines.push(std::mem::take(&mut line));
                width = 0.0;
            }
            if ch != '\n' {
                line.push(ch);
                width += advance;
            }
        }
        lines.push(line);
        lines
    }

    fn ellipsize(&self, text: &str, size: u16, max_width: f32) -> String {
        if self.text_width(text, size) <= max_width {
            return text.to_owned();
        }
        let mut result = String::new();
        let mut width = self.font.metrics('…', size as f32).advance_width;
        for ch in text.chars() {
            width += self.font.metrics(ch, size as f32).advance_width;
            if width > max_width {
                break;
            }
            result.push(ch);
        }
        result.push('…');
        result
    }

    fn text(&mut self, text: &str, x: i32, top: i32, size: u16, color: u32) {
        let mut pen = x as f32;
        let baseline = top + size as i32;
        for ch in text.chars() {
            let glyph = self.glyphs.entry((ch, size)).or_insert_with(|| {
                let (metrics, pixels) = self.font.rasterize(ch, size as f32);
                Glyph { metrics, pixels }
            });
            let left = pen.round() as i32 + glyph.metrics.xmin;
            let top = baseline - glyph.metrics.height as i32 - glyph.metrics.ymin;
            for row in 0..glyph.metrics.height {
                let screen_y = top + row as i32;
                if !(0..HEIGHT as i32).contains(&screen_y) {
                    continue;
                }
                for column in 0..glyph.metrics.width {
                    let screen_x = left + column as i32;
                    if !(0..WIDTH as i32).contains(&screen_x) {
                        continue;
                    }
                    let alpha = glyph.pixels[row * glyph.metrics.width + column];
                    let pixel = &mut self.pixels[screen_y as usize * WIDTH + screen_x as usize];
                    *pixel = blend(*pixel, color, alpha);
                }
            }
            pen += glyph.metrics.advance_width;
        }
    }
}

fn blend(background: u32, foreground: u32, alpha: u8) -> u32 {
    let alpha = u32::from(alpha);
    let inverse = 255 - alpha;
    let channel = |shift: u32| -> u32 {
        ((((foreground >> shift) & 255) * alpha + ((background >> shift) & 255) * inverse + 127)
            / 255)
            << shift
    };
    channel(16) | channel(8) | channel(0)
}

fn composite_layers(frame: &mut [u32], layers: &BTreeMap<i32, Layer>) {
    for layer in layers
        .values()
        .filter(|layer| layer.visible && layer.opacity > 0)
    {
        let source_width = i64::from(layer.image.width());
        let source_height = i64::from(layer.image.height());
        let x = i64::from(layer.x);
        let y = i64::from(layer.y);
        let left = (-x).clamp(0, source_width);
        let right = (WIDTH as i64 - x).clamp(0, source_width);
        let top = (-y).clamp(0, source_height);
        let bottom = (HEIGHT as i64 - y).clamp(0, source_height);
        for source_y in top..bottom {
            for source_x in left..right {
                let rgba = layer.image.get_pixel(source_x as u32, source_y as u32).0;
                let alpha = ((u16::from(rgba[3]) * u16::from(layer.opacity) + 127) / 255) as u8;
                let foreground =
                    (u32::from(rgba[0]) << 16) | (u32::from(rgba[1]) << 8) | u32::from(rgba[2]);
                let pixel = &mut frame[(source_y + y) as usize * WIDTH + (source_x + x) as usize];
                *pixel = blend(*pixel, foreground, alpha);
            }
        }
    }
}

/// The runtime should be initialized to its first yield before entering the window loop.
pub fn run(runtime: &mut crate::runtime::Runtime, font_path: &Path) -> Result<()> {
    let mut renderer = Renderer::new(std::fs::read(font_path)?)?;
    let mut window = Window::new(
        "Yuka RS Demo",
        WIDTH,
        HEIGHT,
        WindowOptions {
            resize: true,
            scale_mode: ScaleMode::AspectRatioStretch,
            ..WindowOptions::default()
        },
    )?;
    window.set_target_fps(60);
    window.set_background_color(8, 12, 18);
    window.set_cursor_visibility(true);
    let mut mouse_was_down = false;
    let mut hide_dialogue = false;
    let mut dirty = true;
    let mut failure = None;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let mouse_down = window.get_mouse_down(MouseButton::Left);
        let click =
            mouse_down && !mouse_was_down && window.get_mouse_pos(MouseMode::Discard).is_some();
        mouse_was_down = mouse_down;
        let active = window.is_active();
        if active && window.is_key_pressed(Key::H, KeyRepeat::No) {
            hide_dialogue = !hide_dialogue;
            dirty = true;
        }
        let advance = click
            || window.is_key_pressed(Key::Space, KeyRepeat::No)
            || window.is_key_pressed(Key::Enter, KeyRepeat::No);
        if active && advance && failure.is_none() {
            if hide_dialogue {
                hide_dialogue = false;
            } else if let Err(error) = runtime.advance() {
                failure = Some(error.to_string());
            }
            dirty = true;
        }
        if failure.is_none() {
            match runtime.tick() {
                Ok(changed) => dirty |= changed,
                Err(error) => {
                    failure = Some(error.to_string());
                    dirty = true;
                }
            }
        }
        if dirty {
            renderer.render(&runtime.scene, hide_dialogue);
            if let Some(message) = &failure {
                renderer.error(message);
            }
            dirty = false;
        }
        window.update_with_buffer(renderer.pixels(), WIDTH, HEIGHT)?;
    }
    if let Some(error) = failure {
        return Err(error.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composites_alpha_and_clips_negative_positions() {
        let mut frame = vec![0x0000ff; WIDTH * HEIGHT];
        let image = Arc::new(RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 128])));
        let layers = BTreeMap::from([(
            0,
            Layer {
                image,
                x: -1,
                y: -1,
                opacity: 255,
                visible: true,
            },
        )]);
        composite_layers(&mut frame, &layers);
        assert_eq!(frame[0], 0x80007f);
        assert_eq!(frame[1], 0x0000ff);
        assert_eq!(frame[WIDTH], 0x0000ff);
    }

    #[test]
    fn layer_order_and_visibility_follow_scene_state() {
        let red = Arc::new(RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255])));
        let green = Arc::new(RgbaImage::from_pixel(1, 1, image::Rgba([0, 255, 0, 255])));
        let mut layers = BTreeMap::from([
            (
                5,
                Layer {
                    image: red,
                    x: 0,
                    y: 0,
                    opacity: 255,
                    visible: true,
                },
            ),
            (
                10,
                Layer {
                    image: green,
                    x: 0,
                    y: 0,
                    opacity: 255,
                    visible: true,
                },
            ),
        ]);
        let mut frame = vec![0; WIDTH * HEIGHT];
        composite_layers(&mut frame, &layers);
        assert_eq!(frame[0], 0x00ff00);
        layers.get_mut(&10).unwrap().visible = false;
        frame.fill(0);
        composite_layers(&mut frame, &layers);
        assert_eq!(frame[0], 0xff0000);
    }
}
