//! Original unlock-aware CG, scene and music catalogues, evaluated from supplied scripts.
use super::*;
use std::{cell::RefCell, sync::Arc};

const HEROINES: [&str; 5] = ["Tinatu", "Kaho", "Tugumi", "Nanako", "Satuki"];
const DOUSEI_HEROINES: [&str; 5] = ["琴宫千夏", "花穗", "姬野继美", "三枝奈奈子", "柳濑彩月"];
const DOUSEI_ROUTE_ORDER: [usize; 5] = [2, 0, 1, 4, 3];
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
    pub default_track: usize,
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
    music: Vec<String>,
    default_track: usize,
    dousei: bool,
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
    music: Vec<String>,
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
                    "SoundLoadStream" if int(&arguments, 0)? == 1 => {
                        result.music.push(string(&arguments, 1)?.into());
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
                    | "ScriptSkipMode" | "SoundPlayLoop" => {}
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
        let dousei = !self.resources.contains(PLAY_SCRIPT)
            && DOUSEI_ROUTES
                .iter()
                .all(|route| self.resources.contains(route));
        let scene_start = if dousei { 22 } else { 20 };
        let scene_end = if dousei { 25 } else { 24 };
        let image_end = if dousei { 15 } else { 17 };
        let mut pages = Vec::new();
        for (character, name) in HEROINES.iter().enumerate() {
            let mode = self
                .resources
                .script(&format!("yks\\extra\\cg_mode_{name}.yks"))?;
            let snapshot = probe(mode, &[], 1)?;
            let mut page = CatalogPage::default();
            for (&(parent, child), &(x, y, slot)) in &snapshot.sprites {
                if parent != 502 || !(1..=scene_end).contains(&child) {
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
                if child <= image_end {
                    let mut variants = Vec::new();
                    for style in 0..if dousei { 3 } else { 4 } {
                        let mut images = Vec::new();
                        for selection in 1..=if dousei { 1 } else { 2 } {
                            let result = probe(
                                view.clone(),
                                &[
                                    (5000, character as i32),
                                    (5200, child),
                                    (5510, if dousei { style } else { style & 1 }),
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
                } else if child >= scene_start {
                    let id = child - scene_start + 1;
                    let first = match (dousei, character) {
                        (false, 0 | 4) => 3,
                        (false, 3) => 2,
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
        let music = if dousei {
            let script = self.resources.script("yks\\extra\\play_music.yks")?;
            (1..=32)
                .map(|track| {
                    let result = probe(script.clone(), &[(6000, track)], 1)?;
                    match result.music.as_slice() {
                        [path] if self.resources.contains(path) => Ok(path.clone()),
                        _ => Err(format!("FD extras track {track} has no unique source").into()),
                    }
                })
                .collect::<Result<Vec<_>>>()?
        } else {
            (1..=29)
                .map(|track| format!("ogg\\bgm\\lv_bgm{track:02}.ogg"))
                .collect()
        };
        let default_track = music
            .iter()
            .position(|path| path.eq_ignore_ascii_case("ogg\\bgm\\lv_bgm22.ogg"))
            .map_or(1, |index| index + 1);
        let catalog = Arc::new(Catalog {
            pages,
            music,
            default_track,
            dousei,
        });
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
            .zip(if catalog.dousei {
                DOUSEI_HEROINES
            } else {
                HEROINES
            })
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
            music_tracks: catalog.music.len(),
            default_track: catalog.default_track,
        })
    }

    /// item=1..17 is a CG; item=20..24 is a scene thumbnail. Nonnegative variants
    /// encode style*10000+frame; -1 and -2 select normal and hovered thumbnails.
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
        if matches!(variant, -1 | -2) {
            let sequence = i32::from(unlocked && variant == -2);
            return self.ui_image(if unlocked { &entry.thumbnail } else { NO_DATA }, sequence);
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
        if self.extra_global(200) == 0 {
            return Err("extras are not unlocked in original progress".into());
        }
        if track == 0 {
            self.sound.stop(1);
            return Ok(());
        }
        let catalog = self.extras_catalog()?;
        let path = usize::try_from(track)
            .ok()
            .and_then(|id| catalog.music.get(id.checked_sub(1)?))
            .ok_or("invalid extra music track")?;
        self.sound.load(1, path, self.resources.read(path)?)?;
        self.sound.set_streamed(1, true);
        self.sound.play(1, true)?;
        // The gallery freezes the VM but has its own live music player.
        self.sound.set_paused(false);
        Ok(())
    }

    pub fn extras_replay(&mut self, character: usize, scene: usize, style: i32) -> Result<()> {
        let catalog = self.extras_catalog()?;
        if !(0..if catalog.dousei { 3 } else { 4 }).contains(&style) {
            return Err("invalid extra scene style".into());
        }
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
            (5510, if catalog.dousei { style } else { style & 1 }),
            (5520, (style >> 1) & 1),
        ] {
            replacement.stack[0]
                .vm
                .set_variable(flag(id), Value::Integer(value))?;
        }
        replacement.entry = if catalog.dousei {
            DOUSEI_ROUTES[DOUSEI_ROUTE_ORDER[character]].into()
        } else {
            PLAY_SCRIPT.into()
        };
        replacement.drive()?;
        *self = replacement;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires original FD through YUKA_TEST_DOUSEI"]
    fn dousei_extras_catalogue_and_replay_follow_progress() -> Result<()> {
        let game = PathBuf::from(std::env::var("YUKA_TEST_DOUSEI")?);
        let mut runtime =
            Runtime::new_unstarted(&game, DOUSEI_ROUTES[0], TextEncoding::Gbk, true, true, 0)?;
        let catalog = runtime.extras_catalog()?;
        assert!(catalog.dousei);
        assert_eq!(catalog.default_track, 23);
        assert_eq!(catalog.music.len(), 32);
        assert!(catalog.music[18].ends_with("41.ogg"));
        assert!(catalog.music[31].ends_with("00.ogg"));
        assert_eq!(
            catalog
                .pages
                .iter()
                .map(|page| page.images.len())
                .collect::<Vec<_>>(),
            [12, 13, 13, 12, 15]
        );
        assert_eq!(
            catalog
                .pages
                .iter()
                .map(|page| page.scenes.len())
                .collect::<Vec<_>>(),
            [4; 5]
        );
        for page in &catalog.pages {
            for image in &page.images {
                assert_eq!(image.variants.len(), 3);
                assert!(runtime.resources.contains(&image.thumbnail));
                for frames in &image.variants {
                    assert!(!frames.is_empty());
                    assert!(frames.iter().all(|path| runtime.resources.contains(path)));
                }
            }
            for scene in &page.scenes {
                assert!(runtime.resources.contains(&scene.thumbnail));
            }
        }
        let info = runtime.extras_info()?;
        assert!(!info.enabled);
        assert!(runtime.extras_music(1).is_err());
        assert!(runtime.extras_image(1, 1, 20000).is_err());
        let locked_thumbnail = runtime.extras_image(1, 1, -1)?;
        assert_eq!(locked_thumbnail.dimensions(), (100, 75));
        assert_eq!(runtime.extras_image(1, 1, -2)?, locked_thumbnail);
        assert!(runtime.extras_replay(1, 1, 2).is_err());
        let scratch =
            PathBuf::from("evidence/extras-fd-test-io").join(std::process::id().to_string());
        std::fs::create_dir_all(&scratch)?;
        runtime.configure_save_directory(scratch);
        for (id, value) in [(200, 1), (250, 1), (460, 1)] {
            runtime.set_global_flag(id, value)?;
        }
        let info = runtime.extras_info()?;
        assert!(
            info.enabled && info.pages[1].images[0].unlocked && info.pages[1].scenes[0].unlocked
        );
        assert!(!info.pages[1].images[1].unlocked && !info.pages[1].scenes[1].unlocked);
        assert!(runtime.extras_image(1, 2, 0).is_err());
        assert!(runtime.extras_replay(1, 2, 0).is_err());
        let normal_thumbnail = runtime.extras_image(1, 1, -1)?;
        let hovered_thumbnail = runtime.extras_image(1, 1, -2)?;
        assert_eq!(hovered_thumbnail.dimensions(), (100, 75));
        assert_ne!(normal_thumbnail, hovered_thumbnail);
        assert_eq!(
            hovered_thumbnail,
            runtime.ui_image(&catalog.pages[1].images[0].thumbnail, 1)?
        );
        assert_eq!(runtime.extras_image(1, 1, 20000)?.dimensions(), (800, 600));
        runtime.extras_music(32)?;
        assert!(
            runtime
                .sound
                .snapshot()
                .iter()
                .any(|(slot, state)| *slot == 1 && state.path.ends_with("00.ogg"))
        );
        assert!(runtime.extras_music(33).is_err());
        runtime.extras_replay(1, 1, 2)?;
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
        assert_eq!(runtime.entry, DOUSEI_ROUTES[0]);
        assert!(runtime.pages > 10);
        Ok(())
    }

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
