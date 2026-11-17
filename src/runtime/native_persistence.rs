//! Original-engine slot persistence, using the verified native record layout.
use super::*;
use crate::{
    native_save::{NativeDrawable, NativeLayer, NativeSave, NativeSound, read_u32, write_u32},
    script::Element,
};
use image::RgbaImage;
use std::{fs, sync::Arc};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct NativeSelection {
    mode: i32,
    colors: [i32; 4],
    mode_arguments: [i32; 2],
    position: [i32; 5],
}

impl Default for NativeSelection {
    fn default() -> Self {
        Self {
            mode: 0,
            colors: [0x030303, 0xffffff, -1, -1],
            mode_arguments: [0; 2],
            position: [0, 0, 0, 0, 30],
        }
    }
}

impl NativeSelection {
    pub(super) fn handle(&mut self, command: &str, args: &[Value]) -> Result<()> {
        if command == "SelectPosition" {
            if !args.is_empty() {
                self.position[0] = int(args, 0)?;
            }
            self.position[1] = if args.len() >= 3 { int(args, 1)? } else { 0 };
            self.position[2] = if args.len() >= 3 { int(args, 2)? } else { 0 };
            self.position[3] = if args.len() >= 5 { int(args, 3)? } else { 0 };
            self.position[4] = if args.len() >= 5 { int(args, 4)? } else { 30 };
            return Ok(());
        }
        self.mode = int(args, 0)?;
        match self.mode {
            0 => {
                self.colors[0] = if args.len() >= 3 { int(args, 2)? } else { -1 };
                self.colors[1] = if args.len() >= 2 {
                    int(args, 1)?
                } else {
                    0xf2d51f
                };
                self.colors[2] = if args.len() >= 5 { int(args, 4)? } else { -1 };
                self.colors[3] = if args.len() >= 4 { int(args, 3)? } else { -1 };
            }
            1 => {}
            2 if args.len() >= 3 => self.mode_arguments = [int(args, 1)?, int(args, 2)?],
            _ => return Err("unsupported native SelectMode arguments".into()),
        }
        Ok(())
    }

    fn write(&self, header: &mut [u8]) -> Result<()> {
        for (offset, value) in [
            (0x98, self.colors[0]),
            (0x9c, self.colors[1]),
            (0xa0, self.mode),
            (0xa4, self.mode_arguments[0]),
            (0xa8, self.mode_arguments[1]),
            (0x130, self.colors[2]),
            (0x134, self.colors[3]),
        ] {
            write_u32(header, offset, value as u32)?;
        }
        for (index, &value) in self.position.iter().enumerate() {
            write_u32(header, 0xac + index * 4, value as u32)?;
        }
        Ok(())
    }

    fn read(header: &[u8]) -> Result<Self> {
        let mut position = [0; 5];
        for (index, value) in position.iter_mut().enumerate() {
            *value = signed(header, 0xac + index * 4)?;
        }
        Ok(Self {
            mode: signed(header, 0xa0)?,
            colors: [
                signed(header, 0x98)?,
                signed(header, 0x9c)?,
                signed(header, 0x130)?,
                signed(header, 0x134)?,
            ],
            mode_arguments: [signed(header, 0xa4)?, signed(header, 0xa8)?],
            position,
        })
    }
}

pub(super) fn native_text(bytes: &[u8], encoding: TextEncoding) -> Result<String> {
    let (text, errors) = match encoding {
        TextEncoding::Gbk => encoding_rs::GBK.decode_without_bom_handling(bytes),
        TextEncoding::ShiftJis => encoding_rs::SHIFT_JIS.decode_without_bom_handling(bytes),
        TextEncoding::Utf8 => {
            return Ok(std::str::from_utf8(bytes)?.to_owned());
        }
    };
    if errors {
        return Err("native save contains invalid text for the selected encoding".into());
    }
    Ok(text.into_owned())
}

fn encode_native(text: &str, encoding: TextEncoding) -> Result<Vec<u8>> {
    if text.contains('\0') {
        return Err("native strings cannot contain a NUL character".into());
    }
    let (bytes, _, errors) = match encoding {
        TextEncoding::Gbk => encoding_rs::GBK.encode(text),
        TextEncoding::ShiftJis => encoding_rs::SHIFT_JIS.encode(text),
        TextEncoding::Utf8 => return Ok(text.as_bytes().to_vec()),
    };
    if errors {
        return Err("text cannot be represented in the original game's encoding".into());
    }
    Ok(bytes.into_owned())
}

fn signed(header: &[u8], offset: usize) -> Result<i32> {
    Ok(read_u32(header, offset)? as i32)
}

fn default_layer(slot: i32) -> Result<NativeLayer> {
    let mut layer = NativeLayer::default();
    write_u32(&mut layer.header, 8, slot as u32)?;
    write_u32(&mut layer.header, 0x0c, 1)?;
    write_u32(&mut layer.header, 0x28, 1)?;
    write_u32(&mut layer.header, 0x30, 0xffffff)?;
    for offset in [0x80, 0x84, 0x124, 0x128] {
        write_u32(&mut layer.header, offset, u32::MAX)?;
    }
    Ok(layer)
}

fn layer_kind(layer: &Layer) -> u32 {
    if layer.sepia_filter {
        10001
    } else if layer.grayscale_filter {
        10002
    } else if layer.invert_filter {
        10003
    } else {
        1
    }
}

/// Recreate the supplied game's createText/createMemu setup for fresh sessions.
fn complete_dialogue_ui(save: &mut NativeSave, visible: bool) -> Result<()> {
    for (slot, path, x, y, shown, transparency) in [
        (11, "window\\window", 3, 473, visible, 300),
        (12, "window\\name", 0, 437, visible, 300),
        (13, "damy", 0, 0, visible, 0),
        (14, "damy", 0, 0, visible, 0),
        (101, "window\\sys_btn01", 0, 0, false, 0),
        (102, "window\\sys_btn02", 0, 0, false, 0),
        (103, "window\\sys_btn_help", 0, 0, false, 0),
    ] {
        if save
            .layers
            .iter()
            .any(|layer| signed(&layer.header, 8).ok() == Some(slot))
        {
            continue;
        }
        let mut layer = default_layer(slot)?;
        layer.image_path = format!("ykg\\system\\{path}.ykg").into_bytes();
        for (offset, value) in [
            (0x0c, 2),
            (0x20, x),
            (0x24, y),
            (0x28, u32::from(shown)),
            (0x74, transparency),
        ] {
            write_u32(&mut layer.header, offset, value)?;
        }
        save.layers.push(layer);
    }
    let text = save
        .layers
        .iter_mut()
        .find(|layer| signed(&layer.header, 8).ok() == Some(13))
        .unwrap();
    if text
        .items
        .iter()
        .any(|item| read_u32(&item.header, 8).ok() == Some(1))
    {
        return Ok(());
    }
    for (offset, value) in [(0x4c, 730), (0x50, 555), (0x54, 1)] {
        write_u32(&mut text.header, offset, value)?;
    }
    for (id, source, sequence, x, y, sx, sy, width, height) in [
        (0, 101, 0, 388, 580, 0, 0, 74, 19),
        (1, 101, 1, 462, 580, 74, 0, 74, 19),
        (2, 101, 2, 536, 580, 148, 0, 74, 19),
        (3, 101, 3, 610, 580, 222, 0, 74, 19),
        (4, 101, 4, 684, 580, 296, 0, 74, 19),
        (5, 102, 0, 761, 497, 0, 0, 39, 21),
        (6, 102, 1, 761, 520, 0, 21, 39, 21),
        (7, 102, 2, 761, 543, 0, 42, 39, 21),
        (8, 102, 3, 761, 566, 0, 63, 39, 21),
    ] {
        let mut item = NativeDrawable::default();
        for (offset, value) in [
            (8, 1),
            (0x0c, 1),
            (0x10, x),
            (0x14, y),
            (0x18, width),
            (0x1c, height),
            (0x20, sx),
            (0x24, sy),
            (0x28, sx + width),
            (0x2c, sy + height),
            (0x30, i32::MAX as u32),
            (0x34, source),
            (0x38, 1),
            (0x48, sequence),
            (0x5c, id),
            (0x60, 1),
        ] {
            write_u32(&mut item.header, offset, value)?;
        }
        item.extra = [sx, sy, width, height, 0, 1, 0, 0]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
        text.items.push(item);
    }
    Ok(())
}

fn text_from_layer(layer: &NativeLayer, encoding: TextEncoding) -> Result<String> {
    let mut result = String::new();
    let mut previous_y = None;
    let mut previous_end = 0;
    let right = signed(&layer.header, 0x44)?;
    for item in &layer.items {
        if read_u32(&item.header, 8)? != 3 || item.header[0x0c] == 0 {
            continue;
        }
        let y = signed(&item.header, 0x14)?;
        let width = signed(&item.header, 0x18)?;
        if previous_y.is_some_and(|previous| y > previous) && previous_end + width <= right {
            result.push('\n');
        }
        previous_y = Some(y);
        previous_end = signed(&item.header, 0x10)? + width;
        if !item.text.is_empty() {
            result.push_str(&native_text(&item.text, encoding)?);
        } else {
            let length = if item.header[0x3d] == 0 { 1 } else { 2 };
            let bytes = &item.header[0x3c..0x3c + length];
            if bytes[0] != 0 {
                result.push_str(&native_text(bytes, encoding)?);
            }
        }
    }
    Ok(result)
}

fn put_text(
    layer: &mut NativeLayer,
    text: &str,
    encoding: TextEncoding,
    name: bool,
    offset: (i32, i32),
) -> Result<()> {
    layer
        .items
        .retain(|item| read_u32(&item.header, 8).ok() != Some(3));
    write_u32(&mut layer.header, 0x0c, 2)?;
    write_u32(&mut layer.header, 0x2c, u32::from(name))?;
    write_u32(&mut layer.header, 0x84, 0)?;
    let origin = if name { (0, 442) } else { (98, 482) };
    let (mut x, mut y) = (origin.0 + offset.0, origin.1 + offset.1);
    let width = if name { 220 } else { 614 };
    for character in text.chars() {
        if character == '\n' {
            x = origin.0 + offset.0;
            y += 32;
            continue;
        }
        let bytes = encode_native(&character.to_string(), encoding)?;
        if bytes.is_empty() || bytes.len() > 2 {
            return Err("original text drawables require one- or two-byte characters".into());
        }
        let advance = if bytes.len() == 1 { 13 } else { 25 };
        if x + advance > origin.0 + offset.0 + width {
            x = origin.0 + offset.0;
            y += 32;
        }
        let mut item = NativeDrawable::default();
        write_u32(&mut item.header, 8, 3)?;
        write_u32(&mut item.header, 0x0c, 1)?;
        write_u32(&mut item.header, 0x10, x as u32)?;
        write_u32(&mut item.header, 0x14, y as u32)?;
        write_u32(&mut item.header, 0x18, (advance + 4) as u32)?;
        write_u32(&mut item.header, 0x1c, 28)?;
        write_u32(&mut item.header, 0x28, (advance + 4) as u32)?;
        write_u32(&mut item.header, 0x2c, 28)?;
        write_u32(&mut item.header, 0x34, u32::MAX)?;
        item.header[0x3c..0x3c + bytes.len()].copy_from_slice(&bytes);
        write_u32(&mut item.header, 0x40, u32::from(name))?;
        write_u32(&mut item.header, 0x44, 0xffffff)?;
        write_u32(&mut item.header, 0x68, u32::MAX)?;
        layer.items.push(item);
        x += advance;
    }
    write_u32(&mut layer.header, 0x34, x as u32)?;
    write_u32(&mut layer.header, 0x38, y as u32)?;
    write_u32(&mut layer.header, 0x3c, (origin.0 + offset.0) as u32)?;
    write_u32(&mut layer.header, 0x40, (origin.1 + offset.1) as u32)?;
    write_u32(
        &mut layer.header,
        0x44,
        (origin.0 + offset.0 + width) as u32,
    )?;
    write_u32(&mut layer.header, 0x48, if name { 40 } else { 600 })?;
    Ok(())
}

impl Runtime {
    pub fn save_native(&self, path: &Path) -> Result<()> {
        self.save_native_with_comment(path, "")
    }

    pub fn save_native_with_comment(&self, path: &Path, comment: &str) -> Result<()> {
        if !self.can_save() {
            return Err("native saves require a stable dialogue or choice boundary".into());
        }
        let comment = encode_native(comment, self.encoding)?;
        let mut save = self.native_template.clone().unwrap_or_default();
        if self.native_template.is_none() {
            for (offset, value) in [(0x34, 1), (0x38, 1), (0xc0, 255), (0x154, 0x7ff)] {
                write_u32(&mut save.state_header, offset, value)?;
            }
            for offset in (0x68..0x90).step_by(4).chain([0x130, 0x134]) {
                write_u32(&mut save.state_header, offset, u32::MAX)?;
            }
        }
        save.frames.clear();
        for activation in &self.stack {
            save.frames
                .extend(activation.vm.export_native_frames(|script| {
                    self.resources
                        .script_name(script)
                        .map(str::to_owned)
                        .ok_or_else(|| "native save script has no resource name".into())
                })?);
        }
        if matches!(self.waiting, Waiting::Key) {
            let vm = &self.stack.last().ok_or("empty script stack")?.vm;
            let script = vm.current_script();
            let instruction = script
                .disassemble()?
                .into_iter()
                .take_while(|instruction| instruction.word_offset < vm.pc)
                .last()
                .ok_or("native save has no preceding dialogue boundary")?;
            if !matches!(&script.entries[instruction.entry_index as usize].element,
                Element::Function { name } if name.text == "KeyWait")
            {
                return Err("native save requires the preceding KeyWait instruction".into());
            }
            let frame = save.frames.last_mut().ok_or("native save has no frame")?;
            write_u32(&mut frame.header, 0x28, instruction.word_offset)?;
            write_u32(&mut frame.header, 0x2c, vm.pc)?;
        }
        save.flags.fill(0);
        save.strings.clear();
        for (slot, value) in self
            .stack
            .last()
            .ok_or("empty script stack")?
            .vm
            .shared_memory()
        {
            match (slot, value) {
                (Slot::IntegerVariable { bank, id }, Value::Integer(value)) if bank == "Flag" => {
                    *save
                        .flags
                        .get_mut(id as usize)
                        .ok_or("native Flag index out of bounds")? = value;
                }
                (Slot::StringVariable { bank, id }, Value::String(value)) if bank == "String" => {
                    save.strings.insert(id as u32, value.bytes);
                }
                _ => {}
            }
        }
        let old_layers = std::mem::take(&mut save.layers)
            .into_iter()
            .map(|layer| Ok((signed(&layer.header, 8)?, layer)))
            .collect::<Result<BTreeMap<_, _>>>()?;
        for (&slot, layer) in &self.scene.layers {
            if layer.source_rect.is_some() || !layer.children.is_empty() {
                return Err(
                    "native export of active cropped or child layers is unsupported".into(),
                );
            }
            let mut native = old_layers
                .get(&slot)
                .cloned()
                .unwrap_or(default_layer(slot)?);
            native.image_path = encode_native(
                self.layer_sources
                    .get(&slot)
                    .ok_or("native layer has no source resource")?,
                self.encoding,
            )?;
            native.mask_path.clear();
            if !matches!(slot, 13 | 14) {
                native.items.clear();
            }
            write_u32(&mut native.header, 0x0c, layer_kind(layer))?;
            write_u32(&mut native.header, 0x20, layer.x as u32)?;
            write_u32(&mut native.header, 0x24, layer.y as u32)?;
            write_u32(
                &mut native.header,
                0x28,
                u32::from(
                    *self
                        .native_selection_visibility
                        .get(&slot)
                        .unwrap_or(&layer.visible),
                ),
            )?;
            write_u32(
                &mut native.header,
                0x74,
                ((255 - u32::from(layer.opacity)) * 1000 + 127) / 255,
            )?;
            let [red, green, blue] = layer.additive_color;
            let color = u32::from(red) | u32::from(green) << 8 | u32::from(blue) << 16;
            write_u32(
                &mut native.header,
                0x124,
                if color == 0 { u32::MAX } else { color },
            )?;
            save.layers.push(native);
        }
        for (slot, text, name, offset) in [
            (
                13,
                self.scene.dialogue.as_str(),
                false,
                self.scene.dialogue_offset,
            ),
            (
                14,
                self.scene.speaker.as_str(),
                true,
                self.scene.speaker_offset,
            ),
        ] {
            if !save
                .layers
                .iter()
                .any(|layer| signed(&layer.header, 8).ok() == Some(slot))
            {
                let mut layer = old_layers
                    .get(&slot)
                    .cloned()
                    .unwrap_or(default_layer(slot)?);
                layer.image_path = b"ykg\\system\\damy.ykg".to_vec();
                save.layers.push(layer);
            }
            let layer = save
                .layers
                .iter_mut()
                .find(|layer| signed(&layer.header, 8).ok() == Some(slot))
                .unwrap();
            write_u32(
                &mut layer.header,
                0x28,
                u32::from(self.scene.dialogue_visible),
            )?;
            let local_offset = (
                offset.0 - signed(&layer.header, 0x20)?,
                offset.1 - signed(&layer.header, 0x24)?,
            );
            put_text(layer, text, self.encoding, name, local_offset)?;
        }
        complete_dialogue_ui(&mut save, self.scene.dialogue_visible)?;
        // Native menu settings apply to both freshly synthesized and restored UI.
        let transparency = match self.global_flag(15) {
            0 => 600,
            2 => 0,
            _ => 150,
        };
        let rgb = self.settings().text_color;
        let color = u32::from(rgb[0]) | u32::from(rgb[1]) << 8 | u32::from(rgb[2]) << 16;
        for layer in &mut save.layers {
            match signed(&layer.header, 8)? {
                11 | 12 => write_u32(&mut layer.header, 0x74, transparency)?,
                13 | 14 => {
                    write_u32(&mut layer.header, 0x30, color)?;
                    for item in &mut layer.items {
                        if read_u32(&item.header, 8)? == 3 {
                            write_u32(&mut item.header, 0x44, color)?;
                        }
                    }
                }
                _ => {}
            }
        }
        save.layers
            .sort_by_key(|layer| signed(&layer.header, 8).unwrap_or_default());
        save.sounds.clear();
        for (slot, state) in self.sound.snapshot() {
            let mut sound = NativeSound {
                path: encode_native(&state.path, self.encoding)?,
                ..Default::default()
            };
            write_u32(&mut sound.header, 8, slot as u32)?;
            sound.header[0x0c] = u8::from(state.playing);
            sound.header[0x0d] = u8::from(state.looping);
            sound.header[0x0e] = u8::from(state.streamed);
            write_u32(&mut sound.header, 0x18, state.volume as u32)?;
            save.sounds.push(sound);
        }
        save.chapter = encode_native(&self.chapter, self.encoding)?;
        save.window = encode_native(&self.window_title, self.encoding)?;
        save.state_extra_strings[1] = encode_native(&self.page_voice_paths, self.encoding)?;
        self.native_selection.write(&mut save.state_header)?;
        save.comment = comment;
        save.thumbnail = super::native_thumbnail::thumbnail(&self.scene);
        let timer_now = self.paused_at.unwrap_or_else(Instant::now);
        for id in 0..16 {
            let elapsed = self.timers.get(&id).map_or(0, |start| {
                timer_now.saturating_duration_since(*start).as_millis() as u32
            });
            write_u32(
                &mut save.state_header,
                0xc8 + id as usize * 4,
                elapsed.wrapping_neg(),
            )?;
        }
        write_u32(&mut save.state_header, 8, 13)?;
        let bytes = save.to_bytes()?;
        let directory = path
            .parent()
            .ok_or("native save requires a parent directory")?;
        self.save_native_globals_to(directory)?;
        super::persistence::write_atomic(path, &bytes)
    }

    pub fn load_native(&mut self, path: &Path) -> Result<()> {
        if fs::metadata(path)?.len() > 128 * 1024 * 1024 {
            return Err("native slot exceeds 128 MiB".into());
        }
        let save = NativeSave::parse(&fs::read(path)?)?;
        if read_u32(&save.state_header, 0x64)? != 0 {
            return Err("native particle animation state is unsupported".into());
        }
        if save.frames.is_empty() || save.frames.len() > 64 {
            return Err("native slot has an invalid script stack".into());
        }
        let mut replacement = Self::new_unstarted(
            &self.directory,
            &self.entry,
            self.encoding,
            self.fast,
            self.muted,
            self.page_limit,
        )?;
        replacement.stack.clear();
        let last = save.frames.len() - 1;
        for (index, frame) in save.frames.iter().enumerate() {
            let vm = Vm::from_native_frame(
                frame,
                self.encoding,
                |name| replacement.resources.script(name),
                index != last,
            )?;
            replacement.stack.push(Activation {
                name: native_text(&frame.name, self.encoding)?,
                vm,
            });
        }
        let vm = &mut replacement.stack.last_mut().unwrap().vm;
        for (id, &value) in save
            .flags
            .iter()
            .enumerate()
            .filter(|(_, value)| **value != 0)
        {
            vm.set_variable(
                Slot::IntegerVariable {
                    bank: "Flag".into(),
                    id: id as i32,
                },
                Value::Integer(value),
            )?;
        }
        for (&id, bytes) in &save.strings {
            vm.set_variable(
                Slot::StringVariable {
                    bank: "String".into(),
                    id: id as i32,
                },
                Value::String(Text {
                    text: native_text(bytes, self.encoding)?,
                    bytes: bytes.clone(),
                    had_errors: false,
                }),
            )?;
        }
        replacement.save_directory = self.save_directory.clone();
        if self.native_globals.initialized {
            replacement.native_globals = self.native_globals.clone();
            let globals = self.stack.last().map(|activation| activation.vm.shared_memory())
                .unwrap_or_default().into_iter().filter(|(slot, _)| {
                    matches!(slot, Slot::IntegerVariable { bank, .. } | Slot::StringVariable { bank, .. }
                        if bank == "GlobalFlag" || bank == "GlobalString")
                }).collect::<BTreeMap<_, _>>();
            for activation in &mut replacement.stack {
                let mut memory = activation.vm.shared_memory();
                memory.retain(|slot, _| {
                    !matches!(slot,
                    Slot::IntegerVariable { bank, .. } | Slot::StringVariable { bank, .. }
                        if bank == "GlobalFlag" || bank == "GlobalString")
                });
                memory.extend(globals.clone());
                activation.vm.replace_shared_memory(memory)?;
            }
        } else {
            replacement.load_native_globals_from(
                path.parent().ok_or("native slot has no parent directory")?,
            )?;
        }
        for native in &save.layers {
            let slot = signed(&native.header, 8)?;
            let kind = read_u32(&native.header, 0x0c)?;
            if !matches!(kind, 0..=2 | 10001..=10003) {
                return Err(format!("native layer {slot} uses unsupported type {kind}").into());
            }
            // Native tick 0x41560e skips scroll velocity when mode +0x88 is zero.
            // The writer retains both velocity doubles after a scroll completes.
            if native.header[0x88..0x90].iter().any(|&byte| byte != 0)
                || native.header[0xa0..0x10c].iter().any(|&byte| byte != 0)
                || native.header[0x12c..0x12f].iter().any(|&byte| byte != 0)
            {
                return Err(
                    format!("native layer {slot} contains an unsupported transform").into(),
                );
            }
            if !matches!(signed(&native.header, 0x128)?, -1 | 0) {
                return Err(
                    format!("native layer {slot} has unsupported subtractive color").into(),
                );
            }
            if !native.items.is_empty() && !matches!(slot, 13 | 14) {
                return Err(format!("native layer {slot} has unsupported drawables").into());
            }
            let path_bytes = if native.mask_path.is_empty() {
                &native.image_path
            } else {
                &native.mask_path
            };
            let resource = native_text(path_bytes, self.encoding)?;
            if !native.mask_path.is_empty() && !resource.to_ascii_lowercase().ends_with(".ykg") {
                return Err("only verified native YKG external images are supported".into());
            }
            let image = if resource.is_empty() {
                Arc::new(RgbaImage::new(1, 1))
            } else {
                replacement.resources.image(&resource)?
            };
            let color = read_u32(&native.header, 0x124)?;
            let additive_color = if color == u32::MAX {
                [0; 3]
            } else {
                [color as u8, (color >> 8) as u8, (color >> 16) as u8]
            };
            let transparency = signed(&native.header, 0x74)?;
            if !(0..=1000).contains(&transparency) {
                return Err("native transparency lies outside 0..1000".into());
            }
            replacement.scene.layers.insert(
                slot,
                Layer {
                    image,
                    x: signed(&native.header, 0x20)?,
                    y: signed(&native.header, 0x24)?,
                    opacity: (((1000 - transparency) * 255 + 500) / 1000) as u8,
                    additive_color,
                    source_rect: None,
                    sepia_filter: kind == 10001,
                    grayscale_filter: kind == 10002,
                    invert_filter: kind == 10003,
                    children: BTreeMap::new(),
                    visible: native.header[0x28] != 0,
                },
            );
            replacement.layer_sources.insert(slot, resource);
            if slot == 13 {
                replacement.scene.dialogue = text_from_layer(native, self.encoding)?;
                replacement.scene.dialogue_visible = native.header[0x28] != 0;
                replacement.scene.dialogue_offset = (
                    signed(&native.header, 0x3c)? - 98 + signed(&native.header, 0x20)?,
                    signed(&native.header, 0x40)? - 482 + signed(&native.header, 0x24)?,
                );
            } else if slot == 14 {
                replacement.scene.speaker = text_from_layer(native, self.encoding)?;
                replacement.scene.speaker_offset = (
                    signed(&native.header, 0x3c)? + signed(&native.header, 0x20)?,
                    signed(&native.header, 0x40)? - 442 + signed(&native.header, 0x24)?,
                );
            }
        }
        replacement.sound.set_paused(true);
        for sound in &save.sounds {
            let slot = signed(&sound.header, 8)?;
            let name = native_text(&sound.path, self.encoding)?;
            let volume = signed(&sound.header, 0x18)?;
            if !(0..=1000).contains(&volume) || read_u32(&sound.header, 0x1c)? != 0 {
                return Err("native audio uses unsupported volume or stream start offset".into());
            }
            replacement
                .sound
                .load(slot, &name, replacement.resources.read(&name)?)?;
            replacement
                .sound
                .set_streamed(slot, sound.header[0x0e] != 0);
            replacement.sound.volume(slot, volume);
            if sound.header[0x0c] != 0 && sound.header[0x0d] != 0 {
                replacement.sound.play(slot, true)?;
            }
        }
        replacement.chapter = native_text(&save.chapter, self.encoding)?;
        replacement.window_title = native_text(&save.window, self.encoding)?;
        replacement.page_voice_paths = native_text(&save.state_extra_strings[1], self.encoding)?;
        replacement.native_selection = NativeSelection::read(&save.state_header)?;
        let now = Instant::now();
        for id in 0..16 {
            let elapsed = read_u32(&save.state_header, 0xc8 + id * 4)?.wrapping_neg();
            let start = now
                .checked_sub(Duration::from_millis(u64::from(elapsed)))
                .ok_or("native timer exceeds this platform's monotonic clock range")?;
            replacement.timers.insert(id as i32, start);
        }
        replacement.native_template = Some(save);
        replacement.apply_option_effects();
        replacement.drive()?;
        replacement.paused_at = self.paused_at.map(|_| Instant::now());
        replacement
            .sound
            .set_paused(replacement.paused_at.is_some());
        self.sound.stop_all();
        *self = replacement;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_native_dialogue_ui_preserves_windows_and_menu_sprites() -> Result<()> {
        let mut save = NativeSave::default();
        complete_dialogue_ui(&mut save, true)?;
        assert_eq!(save.layers.len(), 7);
        for (slot, x, y, visibility, transparency) in [
            (11, 3, 473, 1, 300),
            (12, 0, 437, 1, 300),
            (101, 0, 0, 0, 0),
            (102, 0, 0, 0, 0),
            (103, 0, 0, 0, 0),
        ] {
            let layer = save
                .layers
                .iter()
                .find(|layer| signed(&layer.header, 8).ok() == Some(slot))
                .unwrap();
            for (offset, expected) in [
                (0x20, x),
                (0x24, y),
                (0x28, visibility),
                (0x74, transparency),
            ] {
                assert_eq!(read_u32(&layer.header, offset)?, expected);
            }
        }
        let text = save
            .layers
            .iter_mut()
            .find(|layer| signed(&layer.header, 8).ok() == Some(13))
            .unwrap();
        assert_eq!(text.items.len(), 9);
        for (id, item) in text.items.iter().enumerate() {
            assert_eq!(read_u32(&item.header, 8)?, 1);
            assert_eq!(read_u32(&item.header, 0x5c)?, id as u32);
            assert_eq!(item.extra.len(), 32);
            assert_eq!(read_u32(&item.extra, 20)?, 1);
        }
        put_text(text, "总有一天\n中文", TextEncoding::Gbk, false, (0, 0))?;
        assert_eq!(text.items.len(), 15);
        assert_eq!(text_from_layer(text, TextEncoding::Gbk)?, "总有一天\n中文");
        let glyph = &text.items[9];
        assert!(glyph.text.is_empty());
        assert_eq!(
            &glyph.header[0x3c..0x3e],
            encoding_rs::GBK.encode("总").0.as_ref()
        );
        assert_eq!(read_u32(&glyph.header, 0x18)?, 29);
        assert_eq!(read_u32(&glyph.header, 0x1c)?, 28);
        for offset in [0x30, 0x64, 0x6c] {
            assert_eq!(read_u32(&glyph.header, offset)?, 0);
        }
        let name = save
            .layers
            .iter_mut()
            .find(|layer| signed(&layer.header, 8).ok() == Some(14))
            .unwrap();
        put_text(name, "名字", TextEncoding::Gbk, true, (0, 0))?;
        assert_eq!(read_u32(&name.header, 0x48)?, 40);
        let original = save.to_bytes()?;
        complete_dialogue_ui(&mut save, false)?;
        assert_eq!(save.to_bytes()?, original);
        let restored = NativeSave::parse(&original)?;
        assert_eq!(
            text_from_layer(&restored.layers[2], TextEncoding::Gbk)?,
            "总有一天\n中文"
        );
        Ok(())
    }

    #[test]
    #[ignore = "requires the original game through YUKA_TEST_GAME"]
    fn original_slot_resumes_and_exports_native_state() -> Result<()> {
        let directory = PathBuf::from(std::env::var("YUKA_TEST_GAME")?);
        let output = PathBuf::from("evidence/native-slot-test");
        fs::create_dir_all(&output)?;
        let mut runtime =
            Runtime::new_unstarted(&directory, PLAY_SCRIPT, TextEncoding::Gbk, true, true, 0)?;
        runtime.load_native(&directory.join("Save/savedata.001"))?;
        assert!(matches!(runtime.waiting, Waiting::Key));
        let destination = output.join("savedata.002");
        runtime.save_native(&destination)?;
        let exported = NativeSave::parse(&fs::read(&destination)?)?;
        assert!(!exported.layers.is_empty());
        assert!(!exported.frames.is_empty());
        let first_pc = runtime.report().pc;
        let mut restored =
            Runtime::new_unstarted(&directory, PLAY_SCRIPT, TextEncoding::Gbk, true, true, 0)?;
        restored.load_native(&destination)?;
        assert_eq!(restored.report().pc, first_pc);
        assert_eq!(restored.scene.dialogue, runtime.scene.dialogue);
        for _ in 0..40 {
            runtime.advance()?;
            restored.advance()?;
            assert_eq!(restored.report().pc, runtime.report().pc);
            assert_eq!(restored.scene.dialogue, runtime.scene.dialogue);
        }
        runtime.save_native(&destination)?;
        restored.load_native(&destination)?;
        assert_eq!(restored.report().pc, runtime.report().pc);
        assert_eq!(restored.scene.dialogue, runtime.scene.dialogue);
        let fresh = Runtime::new(&directory, PLAY_SCRIPT, TextEncoding::Gbk, true, true, 0)?;
        let fresh_destination = output.join("savedata.003");
        fresh.save_native(&fresh_destination)?;
        restored.load_native(&fresh_destination)?;
        assert_eq!(restored.report().pc, fresh.report().pc);
        assert_eq!(restored.scene.dialogue, fresh.scene.dialogue);
        let mut completed_scroll = NativeSave::parse(&fs::read(&fresh_destination)?)?;
        let layer = completed_scroll
            .layers
            .iter_mut()
            .find(|layer| signed(&layer.header, 8).ok() == Some(0))
            .ok_or("first scene has no background layer")?;
        write_u32(&mut layer.header, 0x88, 0)?;
        layer.header[0x98..0xa0].copy_from_slice(&0.1125_f64.to_le_bytes());
        let completed_path = output.join("completed-scroll.savedata");
        fs::write(&completed_path, completed_scroll.to_bytes()?)?;
        restored.load_native(&completed_path)?;
        assert_eq!(restored.report().pc, fresh.report().pc);
        assert_eq!(restored.scene.layers[&0].x, fresh.scene.layers[&0].x);
        assert_eq!(restored.scene.layers[&0].y, fresh.scene.layers[&0].y);
        let layer = completed_scroll
            .layers
            .iter_mut()
            .find(|layer| signed(&layer.header, 8).ok() == Some(0))
            .unwrap();
        write_u32(&mut layer.header, 0x88, 1)?;
        fs::write(&completed_path, completed_scroll.to_bytes()?)?;
        assert!(restored.load_native(&completed_path).is_err());
        Ok(())
    }

    #[test]
    #[ignore = "requires the original game through YUKA_TEST_GAME"]
    fn native_choice_resumes_original_case_body() -> Result<()> {
        let directory = PathBuf::from(std::env::var("YUKA_TEST_GAME")?);
        let mut runtime = Runtime::new_unstarted(
            &directory,
            "yks/function/selectA.yks",
            TextEncoding::Gbk,
            true,
            true,
            0,
        )?;
        let vm = &mut runtime.stack.last_mut().unwrap().vm;
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
                text_value(&format!("Choice {id}")),
            )?;
        }
        runtime.drive()?;
        assert_eq!(runtime.choices().len(), 2);
        let destination = PathBuf::from("evidence/native-choice-test/savedata.004");
        runtime.save_native(&destination)?;
        let mut restored =
            Runtime::new_unstarted(&directory, PLAY_SCRIPT, TextEncoding::Gbk, true, true, 0)?;
        restored.load_native(&destination)?;
        assert_eq!(runtime.choices(), restored.choices());
        runtime.choose(1)?;
        restored.choose(1)?;
        assert_eq!(runtime.report().end_reason, restored.report().end_reason);
        assert!(restored.scene.finished);
        Ok(())
    }

    #[test]
    #[ignore = "requires the original game through YUKA_TEST_GAME"]
    fn native_globals_survive_slot_load_restart_and_failed_load() -> Result<()> {
        use crate::native_save::{AlreadyDat, SystemDat};
        let directory = PathBuf::from(std::env::var("YUKA_TEST_GAME")?);
        let output = PathBuf::from("evidence/native-globals-runtime-test");
        let mut runtime = Runtime::new_native(
            &directory,
            PLAY_SCRIPT,
            TextEncoding::Gbk,
            true,
            true,
            0,
            output.clone(),
        )?;
        let flag = Slot::IntegerVariable {
            bank: "GlobalFlag".into(),
            id: 9998,
        };
        let name = Slot::StringVariable {
            bank: "GlobalString".into(),
            id: 9998,
        };
        runtime
            .stack
            .last_mut()
            .unwrap()
            .vm
            .set_variable(flag.clone(), Value::Integer(3))?;
        runtime
            .stack
            .last_mut()
            .unwrap()
            .vm
            .set_variable(name.clone(), text_value("中文存档"))?;
        let slot = output.join("savedata.005");
        runtime.save_native(&slot)?;
        runtime
            .stack
            .last_mut()
            .unwrap()
            .vm
            .set_variable(flag.clone(), Value::Integer(7))?;
        runtime
            .stack
            .last_mut()
            .unwrap()
            .vm
            .set_variable(name.clone(), text_value("中文进度"))?;
        runtime.load_native(&slot)?;
        assert!(matches!(
            runtime.stack.last().unwrap().vm.memory().get(&flag),
            Some(Value::Integer(7))
        ));
        assert!(
            matches!(runtime.stack.last().unwrap().vm.memory().get(&name), Some(Value::String(value)) if value.text == "中文进度")
        );
        runtime.flush_native_globals()?;
        let system = SystemDat::parse(&fs::read(output.join("System.dat"))?)?;
        assert_eq!(system.flags[9998], 7);
        assert_eq!(
            system.strings[&9998],
            encode_native("中文进度", TextEncoding::Gbk)?
        );
        let already = AlreadyDat::parse(&fs::read(output.join("Already.dat"))?)?;
        let root = already
            .entries
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(PLAY_SCRIPT.as_bytes()))
            .ok_or("root script has no read markers")?;
        assert_eq!(root.marks[0], 1);
        let pc = runtime.report().pc;
        let text = runtime.scene.dialogue.clone();
        fs::write(output.join("broken-slot"), b"truncated")?;
        assert!(runtime.load_native(&output.join("broken-slot")).is_err());
        assert_eq!(runtime.report().pc, pc);
        assert_eq!(runtime.scene.dialogue, text);
        runtime.restart()?;
        assert!(matches!(
            runtime.stack.last().unwrap().vm.memory().get(&flag),
            Some(Value::Integer(7))
        ));
        assert!(
            matches!(runtime.stack.last().unwrap().vm.memory().get(&name), Some(Value::String(value)) if value.text == "中文进度")
        );
        Ok(())
    }

    #[test]
    #[ignore = "requires the original game through YUKA_TEST_GAME"]
    fn native_story_choice_retains_caller_and_both_branches() -> Result<()> {
        let directory = PathBuf::from(std::env::var("YUKA_TEST_GAME")?);
        let mut runtime = Runtime::new(
            &directory,
            "yks/story/common/CHAPTER3-3.YKS",
            TextEncoding::Gbk,
            true,
            true,
            0,
        )?;
        for _ in 0..100 {
            if !runtime.choices().is_empty() {
                break;
            }
            runtime.advance()?;
        }
        assert_eq!(runtime.choices().len(), 2);
        assert!(runtime.stack.len() >= 2);
        let choices = runtime.choices().to_vec();
        let destination = PathBuf::from("evidence/native-choice-test/savedata.006");
        runtime.save_native(&destination)?;
        let save = NativeSave::parse(&fs::read(&destination)?)?;
        assert_eq!(read_u32(&save.state_header, 0xac)?, 45);
        for layer in &save.layers {
            if matches!(signed(&layer.header, 8)?, 40 | 41) {
                assert_ne!(layer.header[0x28], 0);
            }
        }
        let mut branches = Vec::new();
        for choice in 0..2 {
            runtime.load_native(&destination)?;
            assert_eq!(runtime.choices(), choices);
            runtime.choose(choice)?;
            assert!(runtime.choices().is_empty());
            assert!(!runtime.scene.finished);
            assert!(runtime.report().script.ends_with("chapter3-3.yks"));
            branches.push(runtime.report().pc);
            for _ in 0..5 {
                runtime.advance()?;
            }
        }
        assert_ne!(branches[0], branches[1]);
        Ok(())
    }
}
