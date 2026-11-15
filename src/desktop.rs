//! Desktop framebuffer host and software rendering for the Yuka demo.
//! The same Renderer can produce deterministic screenshots without a window.

#[cfg(not(target_os = "android"))]
use crate::Result;
#[cfg(not(target_os = "android"))]
use fontdue::{Font, FontSettings, Metrics};
#[cfg(not(target_os = "android"))]
use image::ImageEncoder;
use image::RgbaImage;
#[cfg(not(target_os = "android"))]
use minifb::{Key, KeyRepeat, MouseButton, MouseMode, ScaleMode, Window, WindowOptions};
use std::collections::BTreeMap;
#[cfg(not(target_os = "android"))]
use std::collections::HashMap;
#[cfg(not(target_os = "android"))]
use std::fs::OpenOptions;
#[cfg(not(target_os = "android"))]
use std::io::{BufWriter, Write};
#[cfg(not(target_os = "android"))]
use std::path::{Path, PathBuf};
use std::sync::Arc;
#[cfg(not(target_os = "android"))]
use std::time::{Duration, Instant};

pub const WIDTH: usize = 800;
pub const HEIGHT: usize = 600;

#[derive(Clone)]
pub struct Layer {
    pub image: Arc<RgbaImage>,
    pub x: i32,
    pub y: i32,
    pub opacity: u8,
    pub additive_color: [u8; 3],
    pub source_rect: Option<[u32; 4]>,
    pub sepia_filter: bool,
    pub grayscale_filter: bool,
    pub invert_filter: bool,
    pub children: BTreeMap<i32, Layer>,
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
    pub choices: Vec<String>,
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
            choices: Vec::new(),
        }
    }
}

#[cfg(not(target_os = "android"))]
struct Glyph {
    metrics: Metrics,
    pixels: Vec<u8>,
}

#[cfg(not(target_os = "android"))]
pub struct Renderer {
    font: Font,
    glyphs: HashMap<(char, u16), Glyph>,
    pixels: Vec<u32>,
    previous: Vec<u32>,
}

#[cfg(not(target_os = "android"))]
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
        self.render_selected(scene, hide_dialogue, 0)
    }

    fn render_selected(&mut self, scene: &Scene, hide_dialogue: bool, selected: usize) -> &[u32] {
        render_layers(scene, &mut self.pixels, &mut self.previous);
        if scene.dialogue_visible && !hide_dialogue && !scene.dialogue.is_empty() {
            self.dialogue(scene);
        }
        if !hide_dialogue {
            self.footer(scene);
        }
        if !scene.choices.is_empty() {
            self.choice_menu(&scene.choices, selected);
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

    fn menu(&mut self, title: &str, items: &[MenuItem], selected: usize) {
        self.rect(0, 0, WIDTH, HEIGHT, 0x06101e, 150);
        let top = menu_top(items.len());
        self.rect(125, top - 52, 550, items.len() * 48 + 66, 0x111e30, 244);
        self.text(title, 149, top - 42, 25, 0xf7e4b8);
        for (index, item) in items.iter().enumerate() {
            let row = top + index as i32 * 48;
            let (fill, ink) = match (item.enabled, selected == index) {
                (false, _) => (0x172236, 0x788598),
                (true, true) => (0x365970, 0xffffff),
                (true, false) => (0x1c3048, 0xe0e9ef),
            };
            self.rect(139, row, 522, 43, fill, 255);
            let label = self.ellipsize(&item.label, 22, 480.0);
            self.text(&label, 159, row + 6, 22, ink);
        }
        self.rect(0, 575, WIDTH, 25, 0x08111b, 245);
        self.text(
            "方向键选择 · Enter / 点击确认 · Esc 返回",
            18,
            578,
            15,
            0xe0e9ef,
        );
    }

    fn choice_menu(&mut self, choices: &[String], selected: usize) {
        let items: Vec<_> = choices
            .iter()
            .map(|label| MenuItem {
                label: label.clone(),
                enabled: true,
                action: MenuAction::Resume,
            })
            .collect();
        self.menu("请选择", &items, selected);
    }

    fn notice(&mut self, message: &str) {
        let lines = self.wrap(message, 17, 700.0);
        let height = (lines.len().min(5) * 22 + 20).max(44);
        self.rect(35, 12, 730, height, 0x101c2c, 250);
        for (index, line) in lines.iter().take(5).enumerate() {
            self.text(line, 50, 20 + index as i32 * 22, 17, 0xf7e4b8);
        }
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

/// Render the shared 800×600 scene. Android draws text using its system fonts.
pub fn render_layers(scene: &Scene, pixels: &mut [u32], previous: &mut [u32]) {
    assert_eq!(pixels.len(), WIDTH * HEIGHT);
    assert_eq!(previous.len(), WIDTH * HEIGHT);
    pixels.fill(0);
    composite_layers(pixels, &scene.layers);
    if let Some(layers) = &scene.transition_from {
        let progress = scene.transition_progress.clamp(0.0, 1.0);
        if progress < 1.0 {
            previous.fill(0);
            composite_layers(previous, layers);
            let alpha = (progress * 255.0).round() as u8;
            for (current, previous) in pixels.iter_mut().zip(previous) {
                *current = blend(*previous, *current, alpha);
            }
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
    for layer in layers.values() {
        composite_layer(frame, layer, (0, 0), 255, [0; 3]);
    }
}

fn composite_layer(
    frame: &mut [u32],
    layer: &Layer,
    parent_position: (i64, i64),
    parent_opacity: u8,
    parent_color: [u8; 3],
) {
    if !layer.visible || layer.opacity == 0 {
        return;
    }
    let opacity = ((u16::from(layer.opacity) * u16::from(parent_opacity) + 127) / 255) as u8;
    if opacity == 0 {
        return;
    }
    let position = (
        parent_position.0.saturating_add(i64::from(layer.x)),
        parent_position.1.saturating_add(i64::from(layer.y)),
    );
    let color = [
        parent_color[0].saturating_add(layer.additive_color[0]),
        parent_color[1].saturating_add(layer.additive_color[1]),
        parent_color[2].saturating_add(layer.additive_color[2]),
    ];
    if layer.invert_filter {
        for pixel in frame.iter_mut() {
            *pixel ^= 0x00ff_ffff;
        }
    } else if layer.grayscale_filter {
        for pixel in frame.iter_mut() {
            let gray =
                (3 * ((*pixel >> 16) & 255) + 6 * ((*pixel >> 8) & 255) + (*pixel & 255)) / 10;
            *pixel = (gray << 16) | (gray << 8) | gray;
        }
    } else if layer.sepia_filter {
        for pixel in frame.iter_mut() {
            let luminance = f64::from((*pixel >> 16) & 255) * 0.299
                + f64::from((*pixel >> 8) & 255) * 0.587
                + f64::from(*pixel & 255) * 0.114;
            let red = (luminance + 20.02056).clamp(0.0, 255.0) as u32;
            let green = (luminance - 2.2134).clamp(0.0, 255.0) as u32;
            let blue = (luminance - 41.11926).clamp(0.0, 255.0) as u32;
            *pixel = (red << 16) | (green << 8) | blue;
        }
    } else {
        let rect = layer
            .source_rect
            .unwrap_or([0, 0, layer.image.width(), layer.image.height()]);
        let source_left = rect[0].min(layer.image.width());
        let source_top = rect[1].min(layer.image.height());
        let source_width = i64::from(rect[2].min(layer.image.width() - source_left));
        let source_height = i64::from(rect[3].min(layer.image.height() - source_top));
        let (x, y) = position;
        let left = x.saturating_neg().clamp(0, source_width);
        let right = (WIDTH as i64).saturating_sub(x).clamp(0, source_width);
        let top = y.saturating_neg().clamp(0, source_height);
        let bottom = (HEIGHT as i64).saturating_sub(y).clamp(0, source_height);
        for source_y in top..bottom {
            for source_x in left..right {
                let rgba = layer
                    .image
                    .get_pixel(source_left + source_x as u32, source_top + source_y as u32)
                    .0;
                let alpha = ((u16::from(rgba[3]) * u16::from(opacity) + 127) / 255) as u8;
                let foreground = (u32::from(rgba[0].saturating_add(color[0])) << 16)
                    | (u32::from(rgba[1].saturating_add(color[1])) << 8)
                    | u32::from(rgba[2].saturating_add(color[2]));
                let pixel = &mut frame[(source_y + y) as usize * WIDTH + (source_x + x) as usize];
                *pixel = blend(*pixel, foreground, alpha);
            }
        }
    }
    for child in layer.children.values() {
        composite_layer(frame, child, position, opacity, color);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg(not(target_os = "android"))]
enum Screen {
    Title,
    Playing,
    Paused,
    SaveSlots,
    LoadSlots,
}

#[derive(Clone, Copy)]
#[cfg(not(target_os = "android"))]
enum MenuAction {
    NewGame,
    Continue,
    Resume,
    QuickSave,
    QuickLoad,
    AutoLoad,
    SaveMenu,
    LoadMenu,
    SaveSlot(usize),
    LoadSlot(usize),
    Title,
    Back,
    Quit,
}

#[cfg(not(target_os = "android"))]
struct MenuItem {
    label: String,
    enabled: bool,
    action: MenuAction,
}

#[cfg(not(target_os = "android"))]
impl MenuItem {
    fn new(label: &str, enabled: bool, action: MenuAction) -> Self {
        Self {
            label: label.into(),
            enabled,
            action,
        }
    }
}

#[cfg(not(target_os = "android"))]
struct PlayUi {
    screen: Screen,
    parent: Screen,
    selected: usize,
    choice_selected: usize,
    hide_dialogue: bool,
    notice: Option<(String, Instant)>,
    failure: Option<String>,
}

#[cfg(not(target_os = "android"))]
impl PlayUi {
    fn enter(&mut self, screen: Screen) {
        self.screen = screen;
        self.selected = 0;
        self.hide_dialogue = false;
    }

    fn inform(&mut self, message: impl Into<String>) {
        self.notice = Some((message.into(), Instant::now()));
    }

    fn items(&self, runtime: &crate::runtime::Runtime, directory: &Path) -> Vec<MenuItem> {
        let can_save = self.failure.is_none() && runtime.can_save();
        let quick = directory.join("quick.json").is_file();
        match self.screen {
            Screen::Title => vec![
                MenuItem::new("开始游戏", true, MenuAction::NewGame),
                MenuItem::new(
                    "继续上次存档",
                    latest_save(directory).is_some(),
                    MenuAction::Continue,
                ),
                MenuItem::new("读取存档", true, MenuAction::LoadMenu),
                MenuItem::new("退出", true, MenuAction::Quit),
            ],
            Screen::Paused => vec![
                MenuItem::new(
                    "继续游戏",
                    self.failure.is_none() && !runtime.scene.finished,
                    MenuAction::Resume,
                ),
                MenuItem::new("快速存档  F5", can_save, MenuAction::QuickSave),
                MenuItem::new("读取快速存档  F9", quick, MenuAction::QuickLoad),
                MenuItem::new("保存到存档槽", can_save, MenuAction::SaveMenu),
                MenuItem::new("读取存档", true, MenuAction::LoadMenu),
                MenuItem::new("返回标题", true, MenuAction::Title),
                MenuItem::new("退出", true, MenuAction::Quit),
            ],
            Screen::SaveSlots | Screen::LoadSlots => {
                let saving = self.screen == Screen::SaveSlots;
                let mut items = Vec::new();
                if !saving {
                    items.push(MenuItem::new(
                        "读取自动存档",
                        directory.join("auto.json").is_file(),
                        MenuAction::AutoLoad,
                    ));
                }
                for slot in 1..=3 {
                    let exists = directory.join(format!("slot{slot}.json")).is_file();
                    let state = if exists { "已有存档" } else { "空槽" };
                    items.push(MenuItem {
                        label: format!("存档 {slot} · {state}"),
                        enabled: if saving { can_save } else { exists },
                        action: if saving {
                            MenuAction::SaveSlot(slot)
                        } else {
                            MenuAction::LoadSlot(slot)
                        },
                    });
                }
                items.push(MenuItem::new("返回", true, MenuAction::Back));
                items
            }
            Screen::Playing => Vec::new(),
        }
    }

    fn execute(
        &mut self,
        action: MenuAction,
        runtime: &mut crate::runtime::Runtime,
        directory: &Path,
    ) -> Result<bool> {
        match action {
            MenuAction::NewGame => {
                runtime.restart()?;
                self.failure = None;
                self.choice_selected = 0;
                self.enter(Screen::Playing);
                runtime.set_paused(false);
            }
            MenuAction::Resume => {
                self.enter(Screen::Playing);
                runtime.set_paused(false);
            }
            MenuAction::QuickSave | MenuAction::SaveSlot(_) => {
                let filename = match action {
                    MenuAction::SaveSlot(slot) => format!("slot{slot}.json"),
                    _ => "quick.json".into(),
                };
                runtime.save(&directory.join(filename))?;
                self.inform("存档已保存");
                if self.screen == Screen::SaveSlots {
                    self.enter(Screen::Paused);
                }
            }
            MenuAction::Continue => {
                let path = latest_save(directory).ok_or("没有可读取的存档")?;
                runtime.load(&path)?;
                self.failure = None;
                self.choice_selected = 0;
                self.enter(Screen::Playing);
                runtime.set_paused(false);
                self.inform("存档已读取");
            }
            MenuAction::QuickLoad | MenuAction::AutoLoad | MenuAction::LoadSlot(_) => {
                let filename = match action {
                    MenuAction::LoadSlot(slot) => format!("slot{slot}.json"),
                    MenuAction::AutoLoad => "auto.json".into(),
                    _ => "quick.json".into(),
                };
                runtime.load(&directory.join(filename))?;
                self.failure = None;
                self.choice_selected = 0;
                self.enter(Screen::Playing);
                runtime.set_paused(false);
                self.inform("存档已读取");
            }
            MenuAction::SaveMenu => {
                self.parent = self.screen;
                self.enter(Screen::SaveSlots);
            }
            MenuAction::LoadMenu => {
                self.parent = self.screen;
                self.enter(Screen::LoadSlots);
            }
            MenuAction::Title => {
                runtime.set_paused(true);
                self.enter(Screen::Title);
            }
            MenuAction::Back => self.enter(self.parent),
            MenuAction::Quit => return Ok(true),
        }
        Ok(false)
    }

    fn playback_error(&mut self, runtime: &mut crate::runtime::Runtime, error: impl ToString) {
        let message = error.to_string();
        self.failure = Some(message.clone());
        self.inform(message);
        runtime.set_paused(true);
        self.enter(Screen::Paused);
    }
}

#[cfg(not(target_os = "android"))]
fn menu_top(count: usize) -> i32 {
    ((HEIGHT as i32 - count as i32 * 48) / 2 + 80).max(65)
}

#[cfg(not(target_os = "android"))]
fn latest_save(directory: &Path) -> Option<PathBuf> {
    [
        "quick.json",
        "auto.json",
        "slot1.json",
        "slot2.json",
        "slot3.json",
    ]
    .into_iter()
    .filter_map(|name| {
        let path = directory.join(name);
        let metadata = std::fs::metadata(&path).ok()?;
        metadata
            .is_file()
            .then_some((metadata.modified().ok()?, path))
    })
    .max_by_key(|(modified, _)| *modified)
    .map(|(_, path)| path)
}

#[cfg(not(target_os = "android"))]
fn menu_hit(position: Option<(f32, f32)>, count: usize) -> Option<usize> {
    let (x, y) = position?;
    if !(139.0..661.0).contains(&x) {
        return None;
    }
    let relative_y = y - menu_top(count) as f32;
    if relative_y < 0.0 {
        return None;
    }
    let index = (relative_y / 48.0) as usize;
    (index < count && relative_y % 48.0 < 43.0).then_some(index)
}

#[cfg(not(target_os = "android"))]
fn framebuffer_position(position: Option<(f32, f32)>, size: (usize, usize)) -> Option<(f32, f32)> {
    let (x, y) = position?;
    let scale = (size.0 as f32 / WIDTH as f32).min(size.1 as f32 / HEIGHT as f32);
    if scale <= 0.0 {
        return None;
    }
    let left = (size.0 as f32 - WIDTH as f32 * scale) / 2.0;
    let top = (size.1 as f32 - HEIGHT as f32 * scale) / 2.0;
    let point = ((x - left) / scale, (y - top) / scale);
    ((0.0..WIDTH as f32).contains(&point.0) && (0.0..HEIGHT as f32).contains(&point.1))
        .then_some(point)
}

/// Interactive player with title, pause, choices and independent Rust-format save slots.
#[cfg(not(target_os = "android"))]
pub fn play(
    runtime: &mut crate::runtime::Runtime,
    font_path: &Path,
    save_dir: &Path,
) -> Result<()> {
    let mut renderer = Renderer::new(std::fs::read(font_path)?)?;
    runtime.set_paused(true);
    let title_scene = runtime.title_scene()?;
    let mut window = Window::new(
        "Yuka RS",
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
    let mut ui = PlayUi {
        screen: Screen::Title,
        parent: Screen::Title,
        selected: 0,
        choice_selected: 0,
        hide_dialogue: false,
        notice: None,
        failure: None,
    };
    let mut mouse_was_down = false;
    let mut previous_position = None;
    let mut dirty = true;

    while window.is_open() {
        let active = window.is_active();
        let position = framebuffer_position(
            window.get_unscaled_mouse_pos(MouseMode::Discard),
            window.get_size(),
        );
        let mouse_down = window.get_mouse_down(MouseButton::Left);
        let click = active && mouse_down && !mouse_was_down;
        mouse_was_down = mouse_down;
        let moved = position != previous_position;
        previous_position = position;
        let confirm = active
            && (window.is_key_pressed(Key::Enter, KeyRepeat::No)
                || window.is_key_pressed(Key::Space, KeyRepeat::No));
        let escape = active && window.is_key_pressed(Key::Escape, KeyRepeat::No);
        let up = active && window.is_key_pressed(Key::Up, KeyRepeat::Yes);
        let down = active && window.is_key_pressed(Key::Down, KeyRepeat::Yes);
        let quick_save = active && window.is_key_pressed(Key::F5, KeyRepeat::No);
        let quick_load = active && window.is_key_pressed(Key::F9, KeyRepeat::No);

        if ui
            .notice
            .as_ref()
            .is_some_and(|(_, time)| time.elapsed() >= Duration::from_secs(8))
        {
            ui.notice = None;
            dirty = true;
        }
        if escape {
            match ui.screen {
                Screen::Playing => {
                    runtime.set_paused(true);
                    ui.enter(Screen::Paused);
                }
                Screen::Paused if ui.failure.is_none() && !runtime.scene.finished => {
                    ui.execute(MenuAction::Resume, runtime, save_dir)?;
                }
                Screen::SaveSlots | Screen::LoadSlots => ui.enter(ui.parent),
                _ => {}
            }
            dirty = true;
        } else if quick_save || quick_load {
            let action = if quick_save {
                MenuAction::QuickSave
            } else {
                MenuAction::QuickLoad
            };
            if quick_save
                && (ui.screen == Screen::Title
                    || (ui.screen == Screen::LoadSlots && ui.parent == Screen::Title)
                    || !runtime.can_save()
                    || ui.failure.is_some())
            {
                ui.inform("请在对白或选择支停顿时存档");
            } else if let Err(error) = ui.execute(action, runtime, save_dir) {
                ui.inform(format!("存读档失败 · {error}"));
            }
            dirty = true;
        } else if ui.screen == Screen::Playing {
            if active && window.is_key_pressed(Key::H, KeyRepeat::No) {
                ui.hide_dialogue = !ui.hide_dialogue;
                dirty = true;
            }
            let count = runtime.choices().len();
            if count > 0 {
                ui.choice_selected = ui.choice_selected.min(count - 1);
                if up {
                    ui.choice_selected = (ui.choice_selected + count - 1) % count;
                    dirty = true;
                }
                if down {
                    ui.choice_selected = (ui.choice_selected + 1) % count;
                    dirty = true;
                }
                let hovered = menu_hit(position, count);
                if active
                    && moved
                    && let Some(index) = hovered
                {
                    ui.choice_selected = index;
                    dirty = true;
                }
                if confirm || (click && hovered.is_some()) {
                    if click && let Some(index) = hovered {
                        ui.choice_selected = index;
                    }
                    if let Err(error) = runtime.choose(ui.choice_selected) {
                        ui.playback_error(runtime, error);
                    }
                    ui.choice_selected = 0;
                    dirty = true;
                }
            } else if confirm || (click && position.is_some()) {
                if ui.hide_dialogue {
                    ui.hide_dialogue = false;
                } else if let Err(error) = runtime.advance() {
                    ui.playback_error(runtime, error);
                }
                dirty = true;
            }
        } else if active && (up || down || moved || confirm || click) {
            let items = ui.items(runtime, save_dir);
            let count = items.len();
            if up {
                ui.selected = (ui.selected + count - 1) % count;
                dirty = true;
            }
            if down {
                ui.selected = (ui.selected + 1) % count;
                dirty = true;
            }
            let hovered = menu_hit(position, count);
            if active
                && moved
                && let Some(index) = hovered
            {
                ui.selected = index;
                dirty = true;
            }
            if confirm || (click && hovered.is_some()) {
                if click && let Some(index) = hovered {
                    ui.selected = index;
                }
                if items[ui.selected].enabled {
                    match ui.execute(items[ui.selected].action, runtime, save_dir) {
                        Ok(true) => break,
                        Ok(false) => {}
                        Err(error) => ui.inform(format!("操作失败 · {error}")),
                    }
                }
                dirty = true;
            }
        }
        if ui.screen == Screen::Playing && ui.failure.is_none() {
            match runtime.tick() {
                Ok(changed) => dirty |= changed,
                Err(error) => {
                    ui.playback_error(runtime, error);
                    dirty = true;
                }
            }
        }
        if ui.screen == Screen::Playing && runtime.scene.finished {
            runtime.set_paused(true);
            ui.enter(Screen::Title);
            ui.inform("本次路线播放结束");
            dirty = true;
        }
        if dirty {
            if ui.screen == Screen::Title
                || (ui.screen == Screen::LoadSlots && ui.parent == Screen::Title)
            {
                renderer.render(&title_scene, true);
            } else {
                renderer.render_selected(&runtime.scene, ui.hide_dialogue, ui.choice_selected);
            }
            if ui.screen == Screen::Playing {
                if runtime.choices().is_empty() && !ui.hide_dialogue {
                    renderer.rect(0, 575, WIDTH, 25, 0x08111b, 255);
                    renderer.text(
                        "点击 / 空格继续 · H 隐藏 · Esc 菜单 · F5 存档 · F9 读档",
                        12,
                        579,
                        13,
                        0xe0e9ef,
                    );
                    let status = renderer.ellipsize(&runtime.scene.status, 13, 245.0);
                    let width = renderer.text_width(&status, 13);
                    renderer.text(&status, (788.0 - width) as i32, 579, 13, 0xa6b6c5);
                }
            } else {
                let title = match ui.screen {
                    Screen::Title => "Yuka RS",
                    Screen::Paused => "游戏已暂停",
                    Screen::SaveSlots => "保存游戏",
                    _ => "读取存档",
                };
                renderer.menu(title, &ui.items(runtime, save_dir), ui.selected);
            }
            if let Some((message, _)) = &ui.notice {
                renderer.notice(message);
            }
            dirty = false;
        }
        window.update_with_buffer(renderer.pixels(), WIDTH, HEIGHT)?;
    }
    runtime.set_paused(true);
    if let Some(error) = ui.failure {
        return Err(error.into());
    }
    Ok(())
}

/// The runtime should be initialized to its first yield before entering the window loop.
#[cfg(not(target_os = "android"))]
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
    fn pointer_mapping_preserves_letterboxed_menu_coordinates() {
        assert_eq!(
            framebuffer_position(Some((200.0, 200.0)), (800, 800)),
            Some((200.0, 100.0))
        );
        assert_eq!(framebuffer_position(Some((200.0, 50.0)), (800, 800)), None);
        assert_eq!(
            framebuffer_position(Some((600.0, 300.0)), (1200, 600)),
            Some((400.0, 300.0))
        );
        assert_eq!(framebuffer_position(Some((5.0, 5.0)), (0, 0)), None);
        let top = menu_top(4) as f32;
        assert_eq!(menu_hit(Some((150.0, top + 50.0)), 4), Some(1));
        assert_eq!(menu_hit(Some((150.0, top + 45.0)), 4), None);
        assert_eq!(menu_hit(Some((120.0, top)), 4), None);
    }

    #[test]
    fn entering_menu_resets_selection_and_hidden_dialogue() {
        let mut ui = PlayUi {
            screen: Screen::Playing,
            parent: Screen::Title,
            selected: 4,
            choice_selected: 1,
            hide_dialogue: true,
            notice: None,
            failure: None,
        };
        ui.enter(Screen::Paused);
        assert!(ui.screen == Screen::Paused);
        assert_eq!(ui.selected, 0);
        assert!(!ui.hide_dialogue);
        assert_eq!(ui.choice_selected, 1);
    }

    #[test]
    fn sepia_filter_affects_only_preceding_layers() {
        let image = Arc::new(RgbaImage::from_pixel(1, 1, image::Rgba([0, 255, 0, 255])));
        let layers = BTreeMap::from([
            (
                10,
                Layer {
                    image: image.clone(),
                    x: 0,
                    y: 0,
                    opacity: 255,
                    additive_color: [0; 3],
                    source_rect: None,
                    sepia_filter: true,
                    grayscale_filter: false,
                    invert_filter: false,
                    children: BTreeMap::new(),
                    visible: true,
                },
            ),
            (
                11,
                Layer {
                    image,
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
                },
            ),
        ]);
        let mut frame = vec![0xffffff; WIDTH * HEIGHT];
        composite_layers(&mut frame, &layers);
        assert_eq!(frame[0], 0x00ff00);
        assert_eq!(frame[1], 0xfffcd5);
    }

    #[test]
    fn invert_filter_preserves_following_layers_and_obeys_visibility() {
        let image = Arc::new(RgbaImage::from_pixel(1, 1, image::Rgba([0, 255, 0, 255])));
        let mut layers = BTreeMap::from([
            (
                10,
                Layer {
                    image: image.clone(),
                    x: 0,
                    y: 0,
                    opacity: 255,
                    additive_color: [0; 3],
                    source_rect: None,
                    sepia_filter: false,
                    grayscale_filter: false,
                    invert_filter: true,
                    children: BTreeMap::new(),
                    visible: true,
                },
            ),
            (
                11,
                Layer {
                    image,
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
                },
            ),
        ]);
        let mut frame = vec![0x123456; WIDTH * HEIGHT];
        composite_layers(&mut frame, &layers);
        assert_eq!(frame[0], 0x00ff00);
        assert_eq!(frame[1], 0xedcba9);
        layers.get_mut(&10).unwrap().visible = false;
        frame.fill(0x123456);
        composite_layers(&mut frame, &layers);
        assert_eq!(frame[1], 0x123456);
    }

    #[test]
    fn grayscale_filter_uses_integer_weights_before_following_layers() {
        let image = Arc::new(RgbaImage::from_pixel(1, 1, image::Rgba([0, 255, 0, 255])));
        let mut layers = BTreeMap::from([
            (
                10,
                Layer {
                    image: image.clone(),
                    x: 0,
                    y: 0,
                    opacity: 255,
                    additive_color: [0; 3],
                    source_rect: None,
                    sepia_filter: false,
                    grayscale_filter: true,
                    invert_filter: false,
                    visible: true,
                    children: BTreeMap::new(),
                },
            ),
            (
                11,
                Layer {
                    image,
                    x: 0,
                    y: 0,
                    opacity: 255,
                    additive_color: [0; 3],
                    source_rect: None,
                    sepia_filter: false,
                    grayscale_filter: false,
                    invert_filter: false,
                    visible: true,
                    children: BTreeMap::new(),
                },
            ),
        ]);
        let mut frame = vec![0x123456; WIDTH * HEIGHT];
        composite_layers(&mut frame, &layers);
        assert_eq!(frame[0], 0x00ff00);
        assert_eq!(frame[1], 0x2d2d2d);
        layers.get_mut(&10).unwrap().visible = false;
        frame.fill(0x123456);
        composite_layers(&mut frame, &layers);
        assert_eq!(frame[1], 0x123456);
    }

    #[test]
    fn child_sprites_inherit_parent_position_opacity_color_and_visibility() {
        let child = Layer {
            image: Arc::new(RgbaImage::from_pixel(1, 1, image::Rgba([200, 20, 40, 255]))),
            x: 2,
            y: 1,
            opacity: 128,
            additive_color: [0, 20, 0],
            source_rect: None,
            sepia_filter: false,
            grayscale_filter: false,
            invert_filter: false,
            visible: true,
            children: BTreeMap::new(),
        };
        let parent = Layer {
            image: Arc::new(RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 0, 0]))),
            x: 3,
            y: 4,
            opacity: 128,
            additive_color: [20, 0, 0],
            source_rect: None,
            sepia_filter: false,
            grayscale_filter: false,
            invert_filter: false,
            visible: true,
            children: BTreeMap::from([(1, child)]),
        };
        let mut layers = BTreeMap::from([(7, parent)]);
        let mut frame = vec![0; WIDTH * HEIGHT];
        composite_layers(&mut frame, &layers);
        assert_eq!(frame[5 * WIDTH + 5], 0x370a0a);
        assert_eq!(frame[WIDTH + 2], 0);
        assert_eq!(frame.iter().filter(|&&pixel| pixel != 0).count(), 1);
        layers.get_mut(&7).unwrap().visible = false;
        frame.fill(0);
        composite_layers(&mut frame, &layers);
        assert!(frame.iter().all(|&pixel| pixel == 0));
    }

    #[test]
    fn sprite_rect_and_additive_color_preserve_alpha_and_bounds() {
        let mut image = RgbaImage::from_pixel(3, 2, image::Rgba([10, 20, 30, 255]));
        image.put_pixel(1, 1, image::Rgba([200, 100, 50, 128]));
        let mut layers = BTreeMap::from([(
            5,
            Layer {
                image: Arc::new(image),
                x: 0,
                y: 0,
                opacity: 255,
                additive_color: [100, 20, 0],
                source_rect: Some([1, 1, 1, 1]),
                sepia_filter: false,
                grayscale_filter: false,
                invert_filter: false,
                children: BTreeMap::new(),
                visible: true,
            },
        )]);
        let mut frame = vec![0; WIDTH * HEIGHT];
        composite_layers(&mut frame, &layers);
        assert_eq!(frame[0], 0x803c19);
        assert_eq!(frame[1], 0);
        layers.get_mut(&5).unwrap().source_rect = Some([u32::MAX, u32::MAX, u32::MAX, u32::MAX]);
        frame.fill(0);
        composite_layers(&mut frame, &layers);
        assert!(frame.iter().all(|&pixel| pixel == 0));
    }

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
                additive_color: [0; 3],
                source_rect: None,
                sepia_filter: false,
                grayscale_filter: false,
                invert_filter: false,
                children: BTreeMap::new(),
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
                    additive_color: [0; 3],
                    source_rect: None,
                    sepia_filter: false,
                    grayscale_filter: false,
                    invert_filter: false,
                    children: BTreeMap::new(),
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
                    additive_color: [0; 3],
                    source_rect: None,
                    sepia_filter: false,
                    grayscale_filter: false,
                    invert_filter: false,
                    children: BTreeMap::new(),
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
