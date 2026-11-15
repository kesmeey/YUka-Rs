//! Regression tests against separately supplied, read-only Loverable game data.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use yuka_rs::{
    Result,
    desktop::Scene,
    runtime::{DEFAULT_SCRIPT, PLAY_SCRIPT, Runtime},
    script::TextEncoding,
};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().canonicalize()?;
        let path = root.join(format!(
            "yuka-rs-persistence-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }

    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        // Each test owns a freshly created flat directory.
        if let Ok(entries) = fs::read_dir(&self.0) {
            for entry in entries.flatten() {
                let _ = fs::remove_file(entry.path());
            }
        }
        let _ = fs::remove_dir(&self.0);
    }
}

fn runtime(entry: &str) -> Result<Runtime> {
    let game = std::env::var_os("YUKA_GAME_DIR")
        .ok_or("set YUKA_GAME_DIR to the local Loverable GBK game directory")?;
    Runtime::new(
        &PathBuf::from(game),
        entry,
        TextEncoding::Gbk,
        true,
        true,
        120,
    )
}

fn advance_to(runtime: &mut Runtime, page: usize) -> Result<()> {
    for _ in 0..20_000 {
        if runtime.pages == page {
            return Ok(());
        }
        if runtime.pages > page || runtime.scene.finished {
            return Err(format!(
                "unable to reach page {page}, currently at {}",
                runtime.pages
            )
            .into());
        }
        if runtime.choices().is_empty() {
            runtime.advance()?;
        } else {
            runtime.choose(0)?;
        }
        runtime.tick()?;
    }
    Err("runtime made no progress toward the requested page".into())
}

struct State {
    scene: Scene,
    page: usize,
    pc: u32,
    script: String,
}

fn state(runtime: &Runtime) -> State {
    let report = runtime.report();
    State {
        scene: runtime.scene.clone(),
        page: runtime.pages,
        pc: report.pc,
        script: report.script,
    }
}

fn assert_state(runtime: &Runtime, expected: &State) {
    let report = runtime.report();
    assert_eq!(runtime.pages, expected.page);
    assert_eq!(report.pc, expected.pc);
    assert_eq!(report.script, expected.script);
    assert_eq!(runtime.scene.dialogue, expected.scene.dialogue);
    assert_eq!(runtime.scene.speaker, expected.scene.speaker);
    assert_eq!(
        runtime.scene.dialogue_visible,
        expected.scene.dialogue_visible
    );
    assert_eq!(
        runtime.scene.dialogue_offset,
        expected.scene.dialogue_offset
    );
    assert_eq!(runtime.scene.speaker_offset, expected.scene.speaker_offset);
    assert_eq!(runtime.scene.choices, expected.scene.choices);
    assert_eq!(runtime.scene.finished, expected.scene.finished);
    assert_eq!(
        runtime.scene.layers.keys().collect::<Vec<_>>(),
        expected.scene.layers.keys().collect::<Vec<_>>()
    );
    for (slot, expected) in &expected.scene.layers {
        let actual = &runtime.scene.layers[slot];
        assert_eq!(
            (actual.x, actual.y),
            (expected.x, expected.y),
            "layer {slot} position"
        );
        assert_eq!(actual.opacity, expected.opacity, "layer {slot} opacity");
        assert_eq!(actual.visible, expected.visible, "layer {slot} visibility");
        assert_eq!(
            actual.additive_color, expected.additive_color,
            "layer {slot} additive color"
        );
        assert_eq!(
            actual.source_rect, expected.source_rect,
            "layer {slot} source rectangle"
        );
        assert_eq!(
            actual.sepia_filter, expected.sepia_filter,
            "layer {slot} sepia filter"
        );
        assert_eq!(
            actual.image.dimensions(),
            expected.image.dimensions(),
            "layer {slot} image size"
        );
        assert!(
            actual.image.as_raw() == expected.image.as_raw(),
            "layer {slot} pixels differ"
        );
    }
}

#[test]
#[ignore = "requires local Loverable GBK game data via YUKA_GAME_DIR"]
fn saves_page_40_restores_it_and_replays_to_identical_page_60() -> Result<()> {
    let directory = TestDirectory::new()?;
    let save = directory.file("slot.json");
    let mut runtime = runtime(DEFAULT_SCRIPT)?;
    advance_to(&mut runtime, 40)?;
    assert!(runtime.can_save());
    let page_40 = state(&runtime);
    runtime.save(&save)?;
    advance_to(&mut runtime, 60)?;
    let page_60 = state(&runtime);
    runtime.load(&save)?;
    assert_state(&runtime, &page_40);
    advance_to(&mut runtime, 60)?;
    assert_state(&runtime, &page_60);
    Ok(())
}

#[test]
#[ignore = "requires local Loverable GBK game data via YUKA_GAME_DIR"]
fn invalid_and_foreign_saves_preserve_the_current_session() -> Result<()> {
    let directory = TestDirectory::new()?;
    let save = directory.file("slot.json");
    let bad_path = directory.file("invalid.json");
    let mut runtime = runtime(DEFAULT_SCRIPT)?;
    advance_to(&mut runtime, 40)?;
    runtime.save(&save)?;
    let valid: serde_json::Value = serde_json::from_slice(&fs::read(&save)?)?;
    advance_to(&mut runtime, 60)?;
    let current = state(&runtime);
    let mut variants = Vec::new();
    let mut foreign = valid.clone();
    foreign["archives"][0]["index_fingerprint"] = serde_json::json!("another-game");
    variants.push(("foreign archive identity", foreign));
    let mut bad_pc = valid.clone();
    bad_pc["scripts"][0]["pc"] = serde_json::json!(u32::MAX);
    variants.push(("invalid script PC", bad_pc));
    let mut bad_choices = valid.clone();
    bad_choices["choices"] = serde_json::json!(["fabricated option"]);
    variants.push(("fabricated choices", bad_choices));
    let mut missing_resource = valid;
    missing_resource["layers"][0]["resource"] = serde_json::json!("missing-image.ykg");
    variants.push(("missing resource", missing_resource));
    for (reason, invalid) in variants {
        fs::write(&bad_path, serde_json::to_vec(&invalid)?)?;
        assert!(runtime.load(&bad_path).is_err(), "load accepted {reason}");
        assert_state(&runtime, &current);
    }
    fs::write(&bad_path, b"{broken json")?;
    assert!(runtime.load(&bad_path).is_err());
    assert_state(&runtime, &current);
    runtime.advance()?;
    assert_eq!(runtime.pages, 61);
    Ok(())
}

#[test]
#[ignore = "requires local Loverable GBK game data via YUKA_GAME_DIR"]
fn an_existing_slot_can_be_atomically_replaced_twice() -> Result<()> {
    let directory = TestDirectory::new()?;
    let save = directory.file("slot.json");
    let mut runtime = runtime(DEFAULT_SCRIPT)?;
    advance_to(&mut runtime, 40)?;
    for page in 40..=42 {
        advance_to(&mut runtime, page)?;
        runtime.save(&save)?;
        let content: serde_json::Value = serde_json::from_slice(&fs::read(&save)?)?;
        assert_eq!(content["pages"], page);
        assert_eq!(
            fs::read_dir(&directory.0)?.count(),
            1,
            "temporary save files remain"
        );
    }
    let saved = state(&runtime);
    advance_to(&mut runtime, 60)?;
    runtime.load(&save)?;
    assert_state(&runtime, &saved);
    Ok(())
}

#[test]
#[ignore = "requires local Loverable GBK game data via YUKA_GAME_DIR"]
fn save_restores_pending_external_script_caller_frames() -> Result<()> {
    let directory = TestDirectory::new()?;
    let save = directory.file("slot.json");
    let mut runtime = runtime(PLAY_SCRIPT)?;
    advance_to(&mut runtime, 40)?;
    let before = state(&runtime);
    runtime.save(&save)?;
    let content: serde_json::Value = serde_json::from_slice(&fs::read(&save)?)?;
    assert!(
        content["scripts"].as_array().unwrap().len() >= 2,
        "root_Common should retain a caller frame"
    );
    advance_to(&mut runtime, 60)?;
    let after = state(&runtime);
    runtime.load(&save)?;
    assert_state(&runtime, &before);
    advance_to(&mut runtime, 60)?;
    assert_state(&runtime, &after);
    Ok(())
}
