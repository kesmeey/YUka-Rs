//! JNI entry points. Session ownership is confined to the Activity's engine thread.
use crate::{
    Result,
    desktop::{HEIGHT, Scene, WIDTH, render_layers},
    resources::Resources,
    runtime::{DOUSEI_ROUTES, PLAY_SCRIPT, Runtime},
    script::TextEncoding,
    sound,
};
use jni::{
    JNIEnv,
    objects::{JClass, JIntArray, JString},
    sys::{jboolean, jint, jintArray, jlong, jstring},
};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    path::{Path, PathBuf},
};

struct Player {
    runtime: Runtime,
    title_scene: Scene,
    dousei: bool,
    title: bool,
    paused: bool,
    hidden: bool,
    pixels: Vec<u32>,
    previous: Vec<u32>,
    argb: Vec<i32>,
    history_cache: PathBuf,
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

fn encode_pixels(env: &mut JNIEnv<'_>, image: Option<&image::RgbaImage>) -> Result<jintArray> {
    let Some(image) = image else {
        return Ok(env.new_int_array(0)?.into_raw());
    };
    let length = image.as_raw().len() / 4 + 2;
    let mut pixels = Vec::with_capacity(length);
    pixels.push(i32::try_from(image.width())?);
    pixels.push(i32::try_from(image.height())?);
    pixels.extend(image.pixels().map(|pixel| {
        let [red, green, blue, alpha] = pixel.0;
        i32::from_be_bytes([alpha, red, green, blue])
    }));
    let array = env.new_int_array(i32::try_from(pixels.len())?)?;
    env.set_int_array_region(&array, 0, &pixels)?;
    Ok(array.into_raw())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_yukars_player_NativeBridge_history(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
) -> jstring {
    let result = with_player(handle, |player| {
        Ok(env
            .new_string(serde_json::to_string(player.runtime.history())?)?
            .into_raw())
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
pub extern "system" fn Java_org_yukars_player_NativeBridge_historyVoice(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    id: jlong,
) -> jstring {
    let result = with_player(handle, |player| {
        let files = player
            .runtime
            .history_voice_files(u64::try_from(id)?, &player.history_cache)?;
        Ok(env.new_string(serde_json::to_string(&files)?)?.into_raw())
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
pub extern "system" fn Java_org_yukars_player_NativeBridge_settingsSample(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    index: jint,
) -> jstring {
    let result = with_player(handle, |player| {
        let sample = player
            .runtime
            .settings_sample(index, &player.history_cache)?;
        Ok(env.new_string(sample.to_string())?.into_raw())
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
pub extern "system" fn Java_org_yukars_player_NativeBridge_historyPlayback(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    id: jlong,
) -> jstring {
    let result = with_player(handle, |player| {
        let entry = player
            .runtime
            .history()
            .iter()
            .find(|entry| entry.id == id as u64)
            .ok_or("dialogue history entry expired")?;
        let volume = entry
            .voices
            .first()
            .map_or(1.0, |path| player.runtime.history_voice_volume(path));
        let files = player
            .runtime
            .history_voice_files(u64::try_from(id)?, &player.history_cache)?;
        Ok(env
            .new_string(serde_json::json!({"paths":files,"volume":volume}).to_string())?
            .into_raw())
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
pub extern "system" fn Java_org_yukars_player_NativeBridge_extras(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
) -> jstring {
    let result = with_player(handle, |player| {
        Ok(env
            .new_string(serde_json::to_string(&player.runtime.extras_info()?)?)?
            .into_raw())
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
pub extern "system" fn Java_org_yukars_player_NativeBridge_extrasImage(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    character: jint,
    item: jint,
    variant: jint,
) -> jintArray {
    let result = with_player(handle, |player| {
        let image = player.runtime.extras_image(
            usize::try_from(character)?,
            usize::try_from(item)?,
            variant,
        )?;
        encode_pixels(&mut env, Some(&image))
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
pub extern "system" fn Java_org_yukars_player_NativeBridge_image(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    path: JString<'_>,
    sequence: jint,
) -> jintArray {
    let result = (|| -> Result<jintArray> {
        let path: String = env.get_string(&path)?.into();
        with_player(handle, |player| {
            let image = player.runtime.ui_image(&path, sequence)?;
            encode_pixels(&mut env, Some(&image))
        })
    })();
    match result {
        Ok(array) => array,
        Err(error) => {
            throw(&mut env, error);
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_yukars_player_NativeBridge_saveInfo(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    path: JString<'_>,
) -> jstring {
    let result = (|| -> Result<jstring> {
        let path: String = env.get_string(&path)?.into();
        with_player(handle, |player| {
            let info = player.runtime.save_info(Path::new(&path))?;
            Ok(env.new_string(serde_json::to_string(&info)?)?.into_raw())
        })
    })();
    match result {
        Ok(value) => value,
        Err(error) => {
            throw(&mut env, error);
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_yukars_player_NativeBridge_saveThumbnail(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
    path: JString<'_>,
) -> jintArray {
    let result = (|| -> Result<jintArray> {
        let path: String = env.get_string(&path)?.into();
        with_player(handle, |player| {
            let image = player.runtime.save_thumbnail(Path::new(&path))?;
            encode_pixels(&mut env, image.as_ref())
        })
    })();
    match result {
        Ok(array) => array,
        Err(error) => {
            throw(&mut env, error);
            std::ptr::null_mut()
        }
    }
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
        let history_cache = Path::new(&cache).join("history");
        sound::configure_cache(cache.into())?;
        std::fs::create_dir_all(&saves)?;
        let directory = Path::new(&game);
        let dousei = {
            let resources = Resources::open(directory, TextEncoding::Gbk)?;
            !resources.contains(PLAY_SCRIPT)
                && DOUSEI_ROUTES.iter().all(|entry| resources.contains(entry))
        };
        let entry = if dousei {
            DOUSEI_ROUTES[0]
        } else {
            PLAY_SCRIPT
        };
        // The title must not execute or mark any story instructions as read.
        let mut runtime =
            Runtime::new_unstarted(directory, entry, TextEncoding::Gbk, false, false, 0)?;
        runtime.configure_save_directory(saves.into());
        runtime.load_native_globals()?;
        let title_scene = if dousei {
            Scene::default()
        } else {
            runtime.title_scene()?
        };
        runtime.set_paused(true);
        let player = Player {
            runtime,
            title_scene,
            dousei,
            title: true,
            paused: true,
            hidden: false,
            pixels: vec![0; WIDTH * HEIGHT],
            previous: vec![0; WIDTH * HEIGHT],
            argb: vec![0; WIDTH * HEIGHT],
            history_cache,
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
pub extern "system" fn Java_org_yukars_player_NativeBridge_isDousei(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    handle: jlong,
) -> jboolean {
    match with_player(handle, |player| Ok(player.dousei)) {
        Ok(dousei) => u8::from(dousei),
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
                    if player.dousei {
                        let (route, variant) = argument
                            .split_once(',')
                            .ok_or("Dousei start requires route,variant")?;
                        player
                            .runtime
                            .restart_dousei(route.parse()?, variant.parse()?)?;
                    } else {
                        player.runtime.restart()?;
                    }
                    player.title = false;
                    player.paused = false;
                    player.hidden = false;
                    player.runtime.set_paused(false);
                }
                "advance" if !player.title => {
                    if player.runtime.global_flag(16) == 1 {
                        player.runtime.stop_current_voice();
                    }
                    player.runtime.set_playback_mode("manual")?;
                    player.runtime.advance()?;
                }
                "reveal" if !player.title => {
                    if player.runtime.global_flag(16) == 1 {
                        player.runtime.stop_current_voice();
                    }
                    player.runtime.reveal_text();
                }
                "settings" => player
                    .runtime
                    .update_settings(serde_json::from_str(&argument)?)?,
                "settings_reset" => player.runtime.reset_settings(&argument)?,
                "import_globals" => player.runtime.import_native_globals(Path::new(&argument))?,
                "player_name" => player
                    .runtime
                    .set_player_name(serde_json::from_str(&argument)?)?,
                "movie_play" => player.runtime.request_menu_movie(&argument)?,
                "movie_finished" => player.runtime.complete_movie(argument.parse()?, None)?,
                "movie_error" => {
                    let (id, error) = argument
                        .split_once(',')
                        .ok_or("movie error requires id,message")?;
                    player.runtime.complete_movie(id.parse()?, Some(error))?;
                }
                "extras_music" => {
                    player.runtime.extras_music(argument.parse()?)?;
                }
                "extras_replay" => {
                    let parts = argument
                        .split(',')
                        .map(str::parse::<usize>)
                        .collect::<std::result::Result<Vec<_>, _>>()?;
                    if parts.len() != 3 {
                        return Err("extras replay requires character,scene,style".into());
                    }
                    player
                        .runtime
                        .extras_replay(parts[0], parts[1], i32::try_from(parts[2])?)?;
                    player.title = false;
                    player.paused = false;
                    player.runtime.set_paused(false);
                }
                "playback" if !player.title => player.runtime.set_playback_mode(&argument)?,
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
                    player.runtime.set_playback_mode("manual")?;
                    player.runtime.flush_native_globals()?;
                    player.title = true;
                    player.paused = true;
                    player.runtime.stop_audio();
                    player.runtime.set_paused(true);
                    player.runtime.play_title_audio(true)?;
                }
                "title_audio" if player.title => player.runtime.play_title_audio(false)?,
                "route_audio" if player.title && player.dousei => {
                    player.runtime.play_route_audio()?
                }
                "ui_sound" => player.runtime.play_ui_sound(argument.parse()?)?,
                "menu_audio" => player.runtime.resume_menu_audio(),
                "save" => player.runtime.save_native(Path::new(&argument))?,
                "save_with_comment" => {
                    #[derive(serde::Deserialize)]
                    struct SaveRequest {
                        path: String,
                        comment: String,
                    }
                    let request: SaveRequest = serde_json::from_str(&argument)?;
                    player
                        .runtime
                        .save_native_with_comment(Path::new(&request.path), &request.comment)?;
                }
                "sync_globals" => player.runtime.flush_native_globals()?,
                "save_selection" => {
                    let (page, row) = argument.split_once(',').ok_or("missing save page or row")?;
                    player
                        .runtime
                        .set_save_selection(page.parse()?, row.parse()?)?;
                }
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
                "advance" | "choose" | "playback" | "reveal" | "title_audio" => {}
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
        // Java draws the original dialogue controls and text together. Keep the
        // runtime layers intact so native exports retain the PC window state.
        let mut scene = scene.clone();
        scene.layers.retain(|slot, _| !(11..=14).contains(slot));
        if let Some(layers) = &mut scene.transition_from {
            layers.retain(|slot, _| !(11..=14).contains(slot));
        }
        render_layers(&scene, &mut player.pixels, &mut player.previous);
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
        let (save_page, save_row) = player.runtime.save_selection();
        let json = serde_json::json!({
            "speaker": scene.speaker, "dialogue": scene.dialogue,
            "dialogue_visible": scene.dialogue_visible && !player.hidden && !player.title,
            "speaker_offset": scene.speaker_offset, "dialogue_offset": scene.dialogue_offset,
            "shake_offset": scene.shake_offset,
            "choices": scene.choices, "finished": scene.finished,
            "can_save": !player.title && !player.runtime.in_recollection() && player.runtime.can_save(),
            "in_recollection":player.runtime.in_recollection(),
            "status": scene.status, "title": player.title, "paused": player.paused,
            "audio_warning": player.runtime.report().audio_warning, "audio": audio,
            "save_page": save_page, "save_row": save_row,
            "playback": player.runtime.playback_mode().as_str(),
            "waiting_key": player.runtime.waiting_key(),
            "current_read": player.runtime.current_read(),
            "voice_playing": player.runtime.voice_playing(),
            "history_count": player.runtime.history().len(),
            "history_revision": player.runtime.history_revision(),
            "visible_chars": player.runtime.visible_chars(), "text_complete": player.runtime.text_complete(),
            "settings": player.runtime.settings(), "player_name": player.runtime.player_name(),
            "extras_enabled": player.runtime.global_flag(200) != 0,
            "end_reason": player.runtime.report().end_reason,
            "movie":player.runtime.movie_request(),
            "keywait_icon":player.runtime.keywait_icon(), "submenu_enabled":player.runtime.submenu_enabled(),
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
