//! Original menu artwork and bounded native-slot previews for platform hosts.
use super::{
    Result, Runtime, Slot, Value, graphics_host::sprite_sequence, native_persistence::native_text,
};
use crate::{graphics::Ykg, native_save::NativeSavePreview};
use image::{ImageFormat, ImageReader, Limits, RgbaImage};
use serde::Serialize;
use std::{fs::File, io::Cursor, path::Path, sync::Arc, time::UNIX_EPOCH};

#[derive(Debug, Serialize)]
pub struct SaveInfo {
    pub caption: String,
    pub chapter: String,
    pub window: String,
    pub modified_ms: u64,
    pub has_thumbnail: bool,
}

impl Runtime {
    pub fn save_selection(&self) -> (i32, i32) {
        let values = [85, 86].map(|id| {
            match self.stack.last().and_then(|activation| {
                activation.vm.memory().get(&Slot::IntegerVariable {
                    bank: "GlobalFlag".into(),
                    id,
                })
            }) {
                Some(Value::Integer(value)) => *value,
                _ => 0,
            }
        });
        (values[0].clamp(0, 19), values[1].clamp(1, 5))
    }

    pub fn set_save_selection(&mut self, page: i32, row: i32) -> Result<()> {
        if !(0..20).contains(&page) || !(1..=5).contains(&row) {
            return Err("native save selection is outside pages 0..19 and rows 1..5".into());
        }
        for activation in &mut self.stack {
            for (id, value) in [(85, page), (86, row)] {
                activation.vm.set_variable(
                    Slot::IntegerVariable {
                        bank: "GlobalFlag".into(),
                        id,
                    },
                    Value::Integer(value),
                )?;
            }
        }
        self.flush_native_globals()
    }

    /// UI button states use the first frame of their original YKG sequence.
    pub fn ui_image(&mut self, path: &str, sequence: i32) -> Result<Arc<RgbaImage>> {
        let result = (|| -> Result<Arc<RgbaImage>> {
            let image = self.resources.image(path)?;
            if sequence == -1 {
                return Ok(image);
            }
            let sequence = usize::try_from(sequence).map_err(|_| "invalid UI sprite sequence")?;
            let bytes = self.resources.read(path)?;
            let frames = sprite_sequence(Ykg::parse(&bytes)?.frames, sequence, &image)?;
            let frame = &frames[0];
            if frame.unknown != 0
                || frame.action != 0
                || !matches!((frame.flags, frame.duration), (1, 0) | (2 | 4, 1..))
            {
                return Err("unsupported UI sprite frame flags, action, or duration".into());
            }
            Ok(Arc::new(
                image::imageops::crop_imm(&*image, frame.x, frame.y, frame.width, frame.height)
                    .to_image(),
            ))
        })();
        result.map_err(|error| format!("UI image {path} sequence {sequence}: {error}").into())
    }

    pub fn save_info(&self, path: &Path) -> Result<SaveInfo> {
        let mut file = File::open(path)?;
        let preview = NativeSavePreview::read(&mut file, false)?;
        let modified = file.metadata()?.modified()?.duration_since(UNIX_EPOCH)?;
        Ok(SaveInfo {
            caption: native_text(&preview.comment, self.encoding)?,
            chapter: native_text(&preview.chapter, self.encoding)?,
            window: native_text(&preview.window, self.encoding)?,
            modified_ms: u64::try_from(modified.as_millis())?,
            has_thumbnail: preview.has_thumbnail,
        })
    }

    pub fn save_thumbnail(&self, path: &Path) -> Result<Option<RgbaImage>> {
        let mut file = File::open(path)?;
        let preview = NativeSavePreview::read(&mut file, true)?;
        if !preview.has_thumbnail {
            return Ok(None);
        }
        let mut reader = ImageReader::with_format(Cursor::new(preview.thumbnail), ImageFormat::Bmp);
        let mut limits = Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(64 * 1024 * 1024);
        reader.limits(limits);
        Ok(Some(reader.decode()?.into_rgba8()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        native_save::{NativeSave, SystemDat},
        runtime::PLAY_SCRIPT,
        script::TextEncoding,
    };
    use std::{fs, path::PathBuf};

    #[test]
    #[ignore = "requires the original game through YUKA_TEST_GAME"]
    fn original_menu_artwork_and_slot_previews() -> Result<()> {
        let game = PathBuf::from(std::env::var("YUKA_TEST_GAME")?);
        let output = PathBuf::from("evidence/native-ui-test");
        let mut runtime = Runtime::new_native(
            &game,
            PLAY_SCRIPT,
            TextEncoding::Gbk,
            true,
            true,
            0,
            output.clone(),
        )?;
        runtime.set_paused(true);
        assert_eq!(
            runtime.ui_image("ykg\\title\\base.ykg", -1)?.dimensions(),
            (800, 600)
        );
        let tab_path = "ykg\\system\\option\\op_tab.ykg";
        let sheet = runtime.ui_image(tab_path, -1)?;
        assert_eq!(sheet.dimensions(), (432, 275));
        for (sequence, x) in [(5, 0), (16, 144), (27, 288)] {
            let button = runtime.ui_image(tab_path, sequence)?;
            assert_eq!(button.dimensions(), (144, 25));
            assert_eq!(
                *button,
                image::imageops::crop_imm(&*sheet, x, 125, 144, 25).to_image()
            );
        }
        for sequence in [-2, 100] {
            let error = runtime
                .ui_image(tab_path, sequence)
                .unwrap_err()
                .to_string();
            assert!(error.contains(tab_path));
            assert!(error.contains(&format!("sequence {sequence}")));
        }
        let help_path = "ykg\\title\\t_btn_msg.ykg";
        let help_sheet = runtime.ui_image(help_path, -1)?;
        for sequence in 0..5 {
            let help = runtime.ui_image(help_path, sequence)?;
            assert_eq!(help.dimensions(), (310, 55));
            assert_eq!(
                *help,
                image::imageops::crop_imm(&*help_sheet, 0, sequence as u32 * 55, 310, 55)
                    .to_image()
            );
        }
        for sequence in [0, 1] {
            assert_eq!(
                runtime
                    .ui_image("ykg\\system\\option\\save_item.ykg", sequence)?
                    .dimensions(),
                (502, 89)
            );
        }

        let original_path = game.join("Save/savedata.001");
        let mut original = NativeSave::parse(&fs::read(&original_path)?)?;
        let info = runtime.save_info(&original_path)?;
        assert_eq!(
            info.caption,
            native_text(&original.comment, TextEncoding::Gbk)?
        );
        assert_eq!(
            info.chapter,
            native_text(&original.chapter, TextEncoding::Gbk)?
        );
        assert_eq!(
            info.window,
            native_text(&original.window, TextEncoding::Gbk)?
        );
        assert!(info.has_thumbnail);
        assert_eq!(
            info.modified_ms,
            fs::metadata(&original_path)?
                .modified()?
                .duration_since(UNIX_EPOCH)?
                .as_millis() as u64
        );
        assert_eq!(
            runtime
                .save_thumbnail(&original_path)?
                .unwrap()
                .dimensions(),
            (100, 75)
        );

        runtime.set_save_selection(3, 4)?;
        assert_eq!(runtime.save_selection(), (3, 4));
        for activation in &runtime.stack {
            for (id, value) in [(85, 3), (86, 4)] {
                assert_eq!(
                    activation.vm.memory().get(&Slot::IntegerVariable {
                        bank: "GlobalFlag".into(),
                        id,
                    }),
                    Some(&Value::Integer(value))
                );
            }
        }
        let system = SystemDat::parse(&fs::read(output.join("System.dat"))?)?;
        assert_eq!((system.flags[85], system.flags[86]), (3, 4));
        for (page, row) in [(-1, 1), (20, 1), (0, 0), (0, 6)] {
            assert!(runtime.set_save_selection(page, row).is_err());
            assert_eq!(runtime.save_selection(), (3, 4));
        }
        runtime.restart()?;
        assert_eq!(runtime.save_selection(), (3, 4));

        let slot = output.join("savedata.034");
        runtime.save_native_with_comment(&slot, "中文存档备注")?;
        assert_eq!(runtime.save_info(&slot)?.caption, "中文存档备注");
        let saved_bytes = fs::read(&slot)?;
        assert!(runtime.save_native_with_comment(&slot, "\0").is_err());
        assert!(runtime.save_native_with_comment(&slot, "🙂").is_err());
        assert_eq!(fs::read(&slot)?, saved_bytes);
        runtime.save_native(&slot)?;
        assert!(runtime.save_info(&slot)?.caption.is_empty());

        let invalid_thumbnail = output.join("invalid-thumbnail.savedata");
        original.thumbnail = b"invalid BMP".to_vec();
        fs::write(&invalid_thumbnail, original.to_bytes()?)?;
        assert!(runtime.save_info(&invalid_thumbnail)?.has_thumbnail);
        assert!(runtime.save_thumbnail(&invalid_thumbnail).is_err());
        let empty_thumbnail = output.join("empty-thumbnail.savedata");
        original.thumbnail.clear();
        fs::write(&empty_thumbnail, original.to_bytes()?)?;
        assert!(!runtime.save_info(&empty_thumbnail)?.has_thumbnail);
        assert!(runtime.save_thumbnail(&empty_thumbnail)?.is_none());
        Ok(())
    }
}
