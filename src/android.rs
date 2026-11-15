//! JNI entry points. Session ownership is confined to the Activity's engine thread.
use crate::{
    Result,
    desktop::{HEIGHT, Scene, WIDTH, render_layers},
    runtime::{PLAY_SCRIPT, Runtime},
    script::TextEncoding,
    sound,
};
use jni::{
    JNIEnv,
    objects::{JClass, JIntArray, JString},
    sys::{jlong, jstring},
};
use std::{cell::RefCell, collections::BTreeMap, path::Path};

struct Player {
    runtime: Runtime,
    title_scene: Scene,
    title: bool,
    paused: bool,
    hidden: bool,
    pixels: Vec<u32>,
    previous: Vec<u32>,
    argb: Vec<i32>,
}

#[derive(Default)]
struct Sessions {
    next: i64,
    players: BTreeMap<i64, Player>,
}

thread_local! {
    static SESSIONS: RefCell<Sessions> = RefCell::new(Sessions::default());
}

fn with_player<T>(handle: jlong, operation: impl FnOnce(&mut Player) -> Result<T>) -> Result<T> {
    SESSIONS.with(|sessions| {
        let mut sessions = sessions.borrow_mut();
        let player = sessions
            .players
            .get_mut(&handle)
            .ok_or("invalid Android session or wrong engine thread")?;
        operation(player)
    })
}

fn throw(env: &mut JNIEnv<'_>, error: impl ToString) {
    let _ = env.throw_new("java/lang/IllegalStateException", error.to_string());
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_yukars_player_NativeBridge_create(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    game_dir: JString<'_>,
    save_dir: JString<'_>,
    audio_cache_dir: JString<'_>,
) -> jlong {
    let result = (|| -> Result<jlong> {
        let game: String = env.get_string(&game_dir)?.into();
        let saves: String = env.get_string(&save_dir)?.into();
        let cache: String = env.get_string(&audio_cache_dir)?.into();
        sound::configure_cache(cache.into())?;
        std::fs::create_dir_all(&saves)?;
        let mut runtime = Runtime::new_native(
            Path::new(&game),
            PLAY_SCRIPT,
            TextEncoding::Gbk,
            false,
            false,
            0,
            saves.into(),
        )?;
        let title_scene = runtime.title_scene()?;
        runtime.set_paused(true);
        let player = Player {
            runtime,
            title_scene,
            title: true,
            paused: true,
            hidden: false,
            pixels: vec![0; WIDTH * HEIGHT],
            previous: vec![0; WIDTH * HEIGHT],
            argb: vec![0; WIDTH * HEIGHT],
        };
        Ok(SESSIONS.with(|sessions| {
            let mut sessions = sessions.borrow_mut();
            sessions.next += 1;
            let handle = sessions.next;
            sessions.players.insert(handle, player);
            handle
        }))
    })();
    match result {
        Ok(handle) => handle,
        Err(error) => {
            throw(&mut env, error);
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_yukars_player_NativeBridge_command(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    operation: JString<'_>,
    argument: JString<'_>,
) {
    let result = (|| -> Result<()> {
        let operation: String = env.get_string(&operation)?.into();
        let argument: String = env.get_string(&argument)?.into();
        with_player(handle, |player| {
            match operation.as_str() {
                "new" => {
                    player.runtime.restart()?;
                    player.title = false;
                    player.paused = false;
                    player.hidden = false;
                    player.runtime.set_paused(false);
                }
                "advance" if !player.title => player.runtime.advance()?,
                "choose" if !player.title && !player.paused => {
                    player.runtime.choose(argument.parse()?)?
                }
                "pause" => {
                    player.paused = argument == "1" || player.title;
                    player.runtime.set_paused(player.paused);
                    if player.paused {
                        player.runtime.flush_native_globals()?;
                    }
                }
                "hide" => player.hidden = argument == "1",
                "title" => {
                    player.runtime.flush_native_globals()?;
                    player.title = true;
                    player.paused = true;
                    player.runtime.stop_audio();
                    player.runtime.set_paused(true);
                }
                "save" => player.runtime.save_native(Path::new(&argument))?,
                "sync_globals" => player.runtime.flush_native_globals()?,
                "load" | "import_save" => {
                    player.runtime.load_native(Path::new(&argument))?;
                    player.title = false;
                    player.paused = false;
                    player.hidden = false;
                    player.runtime.set_paused(false);
                }
                "audio_finished" | "audio_error" => {
                    let mut parts = argument.splitn(3, ',');
                    let slot = parts.next().ok_or("missing audio slot")?.parse()?;
                    let generation = parts.next().ok_or("missing audio generation")?.parse()?;
                    let error = (operation == "audio_error")
                        .then(|| parts.next().unwrap_or("Android audio error").to_owned());
                    player.runtime.audio_completed(slot, generation, error);
                }
                "advance" | "choose" => {}
                _ => return Err(format!("unknown Android command: {operation}").into()),
            }
            Ok(())
        })
    })();
    if let Err(error) = result {
        throw(&mut env, error);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_yukars_player_NativeBridge_tick(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
) {
    let result = with_player(handle, |player| {
        player.runtime.tick()?;
        if player.runtime.scene.finished && !player.title {
            player.runtime.flush_native_globals()?;
            player.title = true;
            player.paused = true;
            player.runtime.set_paused(true);
        }
        Ok(())
    });
    if let Err(error) = result {
        throw(&mut env, error);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_yukars_player_NativeBridge_render(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    pixels: JIntArray<'_>,
) {
    let result = with_player(handle, |player| {
        if env.get_array_length(&pixels)? as usize != WIDTH * HEIGHT {
            return Err("framebuffer must contain 800×600 pixels".into());
        }
        let scene = if player.title {
            &player.title_scene
        } else {
            &player.runtime.scene
        };
        render_layers(scene, &mut player.pixels, &mut player.previous);
        for (argb, rgb) in player.argb.iter_mut().zip(&player.pixels) {
            *argb = (rgb | 0xff000000) as i32;
        }
        env.set_int_array_region(&pixels, 0, &player.argb)?;
        Ok(())
    });
    if let Err(error) = result {
        throw(&mut env, error);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_yukars_player_NativeBridge_state(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
) -> jstring {
    let result = with_player(handle, |player| {
        let audio = player.runtime.drain_audio_events();
        let scene = &player.runtime.scene;
        let json = serde_json::json!({
            "speaker": scene.speaker, "dialogue": scene.dialogue,
            "dialogue_visible": scene.dialogue_visible && !player.hidden && !player.title,
            "speaker_offset": scene.speaker_offset, "dialogue_offset": scene.dialogue_offset,
            "choices": scene.choices, "finished": scene.finished,
            "can_save": !player.title && player.runtime.can_save(),
            "status": scene.status, "title": player.title, "paused": player.paused,
            "audio_warning": player.runtime.report().audio_warning, "audio": audio,
        });
        Ok(env.new_string(json.to_string())?.into_raw())
    });
    match result {
        Ok(value) => value,
        Err(error) => {
            throw(&mut env, error);
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_yukars_player_NativeBridge_destroy(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
) {
    let removed = SESSIONS.with(|sessions| sessions.borrow_mut().players.remove(&handle));
    match removed {
        Some(player) => {
            if let Err(error) = player.runtime.flush_native_globals() {
                throw(&mut env, error);
            }
        }
        None => throw(&mut env, "invalid Android session or wrong engine thread"),
    }
}
