//! Original unlock-aware CG, scene and music catalogues, evaluated from supplied scripts.
use super::*;
use std::{cell::RefCell, sync::Arc};

const HEROINES: [&str; 5] = ["Tinatu", "Kaho", "Tugumi", "Nanako", "Satuki"];
const CG_FLAGS: [i32; 5] = [220, 250, 280, 310, 340];
const SCENE_FLAGS: [i32; 5] = [450, 460, 470, 480, 490];
const NO_DATA: &str = "ykg\\extra\\NoData.ykg";

#[derive(Clone, Debug, Serialize)]
pub struct ExtraItem {
    pub id: usize,
    pub x: i32,
    pub y: i32,
    pub unlocked: bool,
    pub variants: Vec<usize>,
}

#[derive(Debug, Serialize)]
pub struct ExtraPage {
    pub name: String,
    pub images: Vec<ExtraItem>,
    pub scenes: Vec<ExtraItem>,
}

#[derive(Debug, Serialize)]
pub struct ExtrasInfo {
    pub enabled: bool,
    pub pages: Vec<ExtraPage>,
    pub music_tracks: usize,
}

#[derive(Clone)]
struct Item {
    id: usize,
    x: i32,
    y: i32,
    unlock: i32,
    thumbnail: String,
    variants: Vec<Vec<String>>,
}

#[derive(Clone, Default)]
struct CatalogPage {
    images: Vec<Item>,
    scenes: Vec<Item>,
}

struct Catalog {
    pages: Vec<CatalogPage>,
}

thread_local! {
    static CATALOGS: RefCell<BTreeMap<String, Arc<Catalog>>> = const { RefCell::new(BTreeMap::new()) };
}

#[derive(Default)]
struct Probe {
    graphics: BTreeMap<i32, String>,
    sprites: BTreeMap<(i32, i32), (i32, i32, i32)>,
    images: Vec<String>,
    stories: Vec<String>,
}

fn flag(id: i32) -> Slot {
    Slot::IntegerVariable {
        bank: "Flag".into(),
        id,
    }
}

/// Run only the catalogue scripts. No image/audio data, game globals or read bits are written.
fn probe(
    script: Arc<crate::script::Script>,
    seeds: &[(i32, i32)],
    selection: i32,
) -> Result<Probe> {
    let mut vm = Vm::new_owned(script);
    vm.initialize_default_banks();
    // Catalogue discovery uses a private VM. These synthetic values never reach Runtime or disk.
    for id in 200..500 {
        vm.set_variable(
            Slot::IntegerVariable {
                bank: "GlobalFlag".into(),
                id,
            },
            Value::Integer(1),
        )?;
    }
    for &(id, value) in seeds {
        vm.set_variable(flag(id), Value::Integer(value))?;
    }
    let mut result = Probe::default();
    for _ in 0..100_000 {
        match vm.step()? {
            Event::Halted
            | Event::Returned {
                destination: None, ..
            } => return Ok(result),
            Event::HostCall {
                function,
                arguments,
                word_offset,
                ..
            } => {
                match function.as_str() {
                    "GraphicLoad" | "GraphicLoadHide" => {
                        let slot = int(&arguments, 0)?;
                        let path = string(&arguments, 1)?.to_owned();
                        if slot == 599 {
                            result.images.push(path.clone());
                        }
                        result.graphics.insert(slot, path);
                    }
                    "SpriteCreate" => {
                        result.sprites.insert(
                            (int(&arguments, 0)?, int(&arguments, 1)?),
                            (
                                int(&arguments, 2)?,
                                int(&arguments, 3)?,
                                int(&arguments, 4)?,
                            ),
                        );
                    }
                    "SpriteDelete" => {
                        result
                            .sprites
                            .remove(&(int(&arguments, 0)?, int(&arguments, 1)?));
                    }
                    "ScriptCall" => {
                        let path = string(&arguments, 0)?
                            .replace('/', "\\")
                            .to_ascii_lowercase();
                        if path == "yks\\extra\\cg_cancel.yks" {
                            vm.set_variable(flag(100), Value::Integer(1))?;
                        } else if path == "yks\\extra\\selecte.yks" {
                            vm.set_variable(flag(100), Value::Integer(selection))?;
                        } else if path.starts_with("yks\\story\\") {
                            result.stories.push(string(&arguments, 0)?.into());
                        } else if path != "yks\\extra\\cg_main_e.yks"
                            && path != "yks\\system\\clear_main.yks"
                        {
                            return Err(format!(
                                "extra catalogue @{word_offset}: unsupported script {path}"
                            )
                            .into());
                        }
                    }
                    "ScriptJump"
                        if string(&arguments, 0)?
                            .replace('/', "\\")
                            .eq_ignore_ascii_case("yks\\extra\\cg_main.yks") =>
                    {
                        return Ok(result);
                    }
                    "DrawStop" | "Transition" | "WindowNameSet" | "HistoryReset"
                    | "GraphicDelete" | "SoundStopALL" | "KeyWaitIcon" | "SubMenuEnableMode"
                    | "ScriptSkipMode" => {}
                    _ => {
                        return Err(format!(
                            "extra catalogue @{word_offset}: unsupported host {function}"
                        )
                        .into());
                    }
                }
                if function == "ScriptCall" {
                    vm.resume_script_call(None)?;
                } else {
                    vm.resume_host(None)?;
                }
            }
            _ => {}
        }
    }
    Err("extra catalogue exceeded its instruction budget".into())
}

impl Runtime {
    fn extras_catalog(&mut self) -> Result<Arc<Catalog>> {
        let key = self.resources.game_id()?;
        if let Some(catalog) = CATALOGS.with(|cache| cache.borrow().get(&key).cloned()) {
            return Ok(catalog);
        }
        let view = self.resources.script("yks\\extra\\view_cg.yks")?;
        let mut pages = Vec::new();
        for (character, name) in HEROINES.iter().enumerate() {
            let mode = self
                .resources
                .script(&format!("yks\\extra\\cg_mode_{name}.yks"))?;
            let snapshot = probe(mode, &[], 1)?;
            let mut page = CatalogPage::default();
            for (&(parent, child), &(x, y, slot)) in &snapshot.sprites {
                if parent != 502 || !(1..=24).contains(&child) {
                    continue;
                }
                let thumbnail = snapshot
                    .graphics
                    .get(&slot)
                    .ok_or("extra sprite references missing image")?
                    .clone();
                if thumbnail.eq_ignore_ascii_case(NO_DATA) {
                    continue;
                }
                if child <= 17 {
                    let mut variants = Vec::new();
                    for style in 0..4 {
                        let mut images = Vec::new();
                        for selection in 1..=2 {
                            let result = probe(
                                view.clone(),
                                &[
                                    (5000, character as i32),
                                    (5200, child),
                                    (5510, style & 1),
                                    (5520, (style >> 1) & 1),
                                ],
                                selection,
                            )?;
                            for path in result.images {
                                if !images.contains(&path) {
                                    images.push(path);
                                }
                            }
                        }
                        variants.push(images);
                    }
                    page.images.push(Item {
                        id: child as usize,
                        x,
                        y,
                        unlock: CG_FLAGS[character] + child - 1,
                        thumbnail,
                        variants,
                    });
                } else if child >= 20 {
                    let id = child - 19;
                    let first = match character {
                        0 | 4 => 3,
                        3 => 2,
                        _ => 1,
                    };
                    page.scenes.push(Item {
                        id: id as usize,
                        x,
                        y,
                        unlock: SCENE_FLAGS[character] + id - first,
                        thumbnail,
                        variants: Vec::new(),
                    });
                }
            }
            pages.push(page);
        }
        let catalog = Arc::new(Catalog { pages });
        CATALOGS.with(|cache| {
            let mut cache = cache.borrow_mut();
            if cache.len() >= 2 {
                cache.clear();
            }
            cache.insert(key, catalog.clone());
        });
        Ok(catalog)
    }

    fn extra_global(&self, id: i32) -> i32 {
        match self.stack.last().and_then(|activation| {
            activation.vm.memory().get(&Slot::IntegerVariable {
                bank: "GlobalFlag".into(),
                id,
            })
        }) {
            Some(Value::Integer(value)) => *value,
            _ => 0,
        }
    }

    pub fn extras_info(&mut self) -> Result<ExtrasInfo> {
        let catalog = self.extras_catalog()?;
        let enabled = self.extra_global(200) != 0;
        let pages = catalog
            .pages
            .iter()
            .zip(HEROINES)
            .map(|(page, name)| {
                let item = |item: &Item| ExtraItem {
                    id: item.id,
                    x: item.x,
                    y: item.y,
                    unlocked: enabled && self.extra_global(item.unlock) == 1,
                    variants: item.variants.iter().map(Vec::len).collect(),
                };
                ExtraPage {
                    name: name.into(),
                    images: page.images.iter().map(item).collect(),
                    scenes: page.scenes.iter().map(item).collect(),
                }
            })
            .collect();
        Ok(ExtrasInfo {
            enabled,
            pages,
            music_tracks: 29,
        })
    }

    /// item=1..17 is a CG; item=20..24 is a scene thumbnail. variant encodes style*10000+frame.
    pub fn extras_image(
        &mut self,
        character: usize,
        item: usize,
        variant: i32,
    ) -> Result<Arc<image::RgbaImage>> {
        let catalog = self.extras_catalog()?;
        let page = catalog
            .pages
            .get(character)
            .ok_or("invalid extra character")?;
        let entry = if item >= 20 {
            page.scenes.iter().find(|entry| entry.id + 19 == item)
        } else {
            page.images.iter().find(|entry| entry.id == item)
        }
        .ok_or("invalid extra item")?;
        let unlocked = self.extra_global(200) != 0 && self.extra_global(entry.unlock) == 1;
        if variant == -1 {
            return self.ui_image(if unlocked { &entry.thumbnail } else { NO_DATA }, 0);
        }
        if !unlocked {
            return Err("extra image is locked by the original progress flags".into());
        }
        let index = usize::try_from(variant)?;
        let path = entry
            .variants
            .get(index / 10000)
            .and_then(|frames| frames.get(index % 10000))
            .ok_or("invalid extra image variant")?;
        self.resources.temporary_image(path)
    }

    pub fn extras_music(&mut self, track: i32) -> Result<()> {
        if !(0..=29).contains(&track) {
            return Err("extra music track must be 0..29".into());
        }
        if self.extra_global(200) == 0 {
            return Err("extras are not unlocked in original progress".into());
        }
        if track == 0 {
            self.sound.stop(1);
            return Ok(());
        }
        let path = format!("ogg\\bgm\\lv_bgm{track:02}.ogg");
        self.sound.load(1, &path, self.resources.read(&path)?)?;
        self.sound.set_streamed(1, true);
        self.sound.play(1, true)?;
        // The gallery freezes the VM but has its own live music player.
        self.sound.set_paused(false);
        Ok(())
    }

    pub fn extras_replay(&mut self, character: usize, scene: usize, style: i32) -> Result<()> {
        if !(0..4).contains(&style) {
            return Err("invalid extra scene style".into());
        }
        let catalog = self.extras_catalog()?;
        let entry = catalog
            .pages
            .get(character)
            .and_then(|page| page.scenes.iter().find(|entry| entry.id == scene))
            .ok_or("invalid extra scene")?;
        if self.extra_global(200) == 0 || self.extra_global(entry.unlock) != 1 {
            return Err("extra scene is locked by original progress".into());
        }
        self.flush_native_globals()?;
        let mut replacement = Self::new_unstarted(
            &self.directory,
            "yks\\extra\\play_h.yks",
            self.encoding,
            self.fast,
            self.muted,
            0,
        )?;
        replacement.save_directory = self.save_directory.clone();
        replacement.load_native_globals()?;
        replacement.initialize_player_name()?;
        for (id, value) in [
            (5000, character as i32),
            (5210, scene as i32),
            (5510, style & 1),
            (5520, (style >> 1) & 1),
        ] {
            replacement.stack[0]
                .vm
                .set_variable(flag(id), Value::Integer(value))?;
        }
        replacement.entry = PLAY_SCRIPT.into();
        replacement.drive()?;
        *self = replacement;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires original game through YUKA_TEST_GAME"]
    fn original_extras_catalog_unlocks_and_variants() -> Result<()> {
        let game = PathBuf::from(std::env::var("YUKA_TEST_GAME")?);
        let mut runtime =
            Runtime::new_unstarted(&game, PLAY_SCRIPT, TextEncoding::Gbk, true, true, 0)?;
        let info = runtime.extras_info()?;
        assert!(!info.enabled);
        assert_eq!(
            info.pages
                .iter()
                .map(|page| page.images.len())
                .collect::<Vec<_>>(),
            [17, 17, 17, 17, 12]
        );
        assert_eq!(
            info.pages
                .iter()
                .map(|page| page.scenes.len())
                .collect::<Vec<_>>(),
            [3, 5, 5, 4, 3]
        );
        assert!(runtime.extras_image(0, 1, 0).is_err());
        assert!(runtime.extras_music(1).is_err());
        for (id, value) in [(200, 1), (220, 1)] {
            runtime.stack[0].vm.set_variable(
                Slot::IntegerVariable {
                    bank: "GlobalFlag".into(),
                    id,
                },
                Value::Integer(value),
            )?;
        }
        let info = runtime.extras_info()?;
        assert!(info.enabled && info.pages[0].images[0].unlocked);
        assert_eq!(info.pages[0].images[0].variants, [3, 3, 3, 3]);
        assert!(!info.pages[0].images[1].unlocked);
        assert_eq!(runtime.extras_image(0, 1, -1)?.dimensions(), (100, 75));
        assert_eq!(runtime.extras_image(0, 1, 0)?.dimensions(), (800, 600));
        assert!(runtime.extras_image(0, 1, 3).is_err());
        assert!(runtime.extras_replay(0, 3, 0).is_err());
        // Only the isolated test copy receives synthetic completion flags.
        let scratch =
            PathBuf::from("evidence/extras-test-io").join(format!("{}", std::process::id()));
        std::fs::create_dir_all(&scratch)?;
        runtime.configure_save_directory(scratch.clone());
        runtime.stack[0].vm.set_variable(
            Slot::IntegerVariable {
                bank: "GlobalFlag".into(),
                id: 450,
            },
            Value::Integer(1),
        )?;
        runtime.extras_replay(0, 3, 0)?;
        for _ in 0..3000 {
            if runtime.scene.finished {
                break;
            }
            if matches!(runtime.waiting, Waiting::Choice) {
                runtime.choose(0)?;
            } else {
                runtime.advance()?;
            }
        }
        assert!(runtime.scene.finished);
        assert_eq!(runtime.end_reason.as_deref(), Some("return_to_extras"));
        assert_eq!(runtime.entry, PLAY_SCRIPT);
        assert!(runtime.pages > 10);
        Ok(())
    }
}
