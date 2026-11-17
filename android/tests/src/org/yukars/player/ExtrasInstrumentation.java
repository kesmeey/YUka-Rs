// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

import android.app.Activity;
import android.app.Instrumentation;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.os.Bundle;
import android.os.SystemClock;
import java.io.File;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.Arrays;
import java.util.UUID;
import org.json.JSONArray;
import org.json.JSONObject;

/** Original extras progress gates, catalogue, artwork and music/recollection JNI. */
public final class ExtrasInstrumentation extends Instrumentation {
    private Bundle arguments;
    private long session;
    private String stage = "create";

    @Override public void onCreate(Bundle value) { super.onCreate(value); arguments = value == null ? new Bundle() : new Bundle(value); start(); }
    @Override public void onStart() { new Thread(this::tests, "Yuka extras tests").start(); }

    private void tests() {
        Bundle result = new Bundle();
        JSONObject summary = new JSONObject();
        int code = Activity.RESULT_CANCELED;
        File scratch = new File(getTargetContext().getCacheDir(), "extras-" + UUID.randomUUID());
        try {
            String game = arguments.getString("game", "");
            File profile = new File(arguments.getString("fullProfile", ""));
            require(!game.isEmpty() && profile.isFile(), "game and fullProfile arguments are required");
            byte[] original = Files.readAllBytes(profile.toPath());
            File locked = new File(scratch, "locked");
            File unlocked = new File(scratch, "unlocked");
            File audio = new File(scratch, "audio");
            GameImporter.mkdir(locked); GameImporter.mkdir(unlocked); GameImporter.mkdir(audio);
            session = NativeBridge.create(game, locked.getAbsolutePath(), audio.getAbsolutePath());
            stage = "original_locked_progress";
            JSONObject info = new JSONObject(NativeBridge.extras(session));
            require(!info.optBoolean("enabled"), "new progress unexpectedly unlocked extras");
            int images = 0, scenes = 0;
            JSONArray pages = info.getJSONArray("pages");
            for (int index = 0; index < pages.length(); index++) {
                JSONObject page = pages.getJSONObject(index);
                images += page.getJSONArray("images").length(); scenes += page.getJSONArray("scenes").length();
            }
            require(images == 80 && scenes == 20 && info.optInt("music_tracks") == 29, "original catalogue entry count changed");
            boolean rejected = false;
            try { NativeBridge.extrasImage(session, 0, 1, 0); } catch (IllegalStateException expected) { rejected = true; }
            require(rejected, "locked gallery exposed a full image");
            summary.put("original_locked_progress", true);
            NativeBridge.destroy(session); session = 0;

            stage = "supplied_full_profile_unlocks";
            Files.write(new File(unlocked, "System.dat").toPath(), original);
            session = NativeBridge.create(game, unlocked.getAbsolutePath(), audio.getAbsolutePath());
            info = new JSONObject(NativeBridge.extras(session));
            require(info.optBoolean("enabled") && info.getJSONArray("pages").getJSONObject(0)
                .getJSONArray("images").getJSONObject(0).optBoolean("unlocked"), "supplied full profile did not unlock existing CG");
            Bitmap thumb = OriginalAssets.decode(NativeBridge.extrasImage(session, 0, 1, -1));
            require(thumb.getWidth() == 100 && thumb.getHeight() == 75, "thumbnail did not crop original sequence0");
            thumb.recycle();
            summary.put("supplied_full_profile_unlocks", true);

            stage = "original_gallery_canvas_and_controls";
            Listener listener = new Listener();
            OriginalExtrasMenu menu = OriginalExtrasMenu.load(session, info, listener);
            Bitmap bitmap = Bitmap.createBitmap(800, 600, Bitmap.Config.ARGB_8888);
            Canvas canvas = new Canvas(bitmap);
            menu.draw(canvas, -1, -1);
            require(bitmap.getPixel(0, 0) != 0, "gallery canvas is empty");
            menu.tap(115, 121);
            require(listener.character == 0 && listener.item == 1 && listener.variant == 0, "CG hit target is not mapped to its original item");
            Bitmap picture = OriginalAssets.decode(NativeBridge.extrasImage(session, 0, 1, 0));
            require(picture.getWidth() == 800 && picture.getHeight() == 600, "full CG plane has wrong dimensions");
            menu.showImage(picture, 0, 1, 0);
            menu.draw(canvas, -1, -1);
            menu.tap(500, 300);
            require(listener.variant == 1, "full CG advance did not request the next original variation");
            menu.back();
            menu.tap(585, 536);
            require(listener.track == 29, "last BGM number target is incorrect");
            menu.tap(650, 535);
            require(listener.track == 28, "BGM previous target is incorrect");
            menu.tap(710, 535);
            require(listener.track == 29, "BGM next target is incorrect");
            menu.tap(710, 505);
            require(listener.track == 0, "BGM stop target is incorrect");
            menu.tap(75, 565);
            require(listener.closed, "gallery title-return target is incorrect");
            menu.release(); bitmap.recycle();
            summary.put("original_gallery_canvas_and_controls", true);

            stage = "music_while_vm_paused";
            NativeBridge.command(session, "pause", "1");
            NativeBridge.state(session);
            NativeBridge.command(session, "extras_music", "29");
            JSONObject state = new JSONObject(NativeBridge.state(session));
            require(state.optBoolean("paused") && state.optBoolean("title"), "music playback ran the hidden story VM");
            JSONArray events = state.getJSONArray("audio");
            boolean loaded = false, playing = false, resumed = false;
            for (int index = 0; index < events.length(); index++) {
                JSONObject event = events.getJSONObject(index);
                if (event.optInt("slot") == 1 && event.optString("op").equals("load")) loaded = true;
                if (event.optInt("slot") == 1 && event.optString("op").equals("play") && event.optBoolean("loop")) playing = true;
                if (event.optString("op").equals("resume_all")) resumed = true;
            }
            require(loaded && playing && resumed, "gallery BGM did not request real looped playback independently of the paused VM");
            NativeBridge.command(session, "extras_music", "0");
            summary.put("music_while_vm_paused", true);

            stage = "original_recollection_script";
            NativeBridge.command(session, "extras_replay", "0,3,0");
            long deadline = SystemClock.elapsedRealtime() + 30000;
            do { NativeBridge.tick(session); state = new JSONObject(NativeBridge.state(session)); Thread.sleep(16); }
            while (!state.optBoolean("waiting_key") && SystemClock.elapsedRealtime() < deadline);
            require(!state.optBoolean("title") && state.optBoolean("waiting_key") && !state.optString("dialogue").isEmpty(),
                "unlocked scene did not start its original recollection script");
            require(Arrays.equals(original, Files.readAllBytes(profile.toPath())), "input full profile was modified");
            summary.put("original_recollection_script", true).put("passed", true);
            code = Activity.RESULT_OK;
        } catch (Throwable error) {
            try { summary.put("passed", false).put("stage", stage).put("error", error.toString()); } catch (Exception ignored) { }
            result.putString("failure", stage + " - " + error);
        } finally {
            if (session != 0) try { NativeBridge.destroy(session); } catch (Throwable ignored) { }
            result.putString("summary", summary.toString());
            result.putString("scratch", scratch.getAbsolutePath());
            try { Files.write(new File(scratch, "result.json").toPath(), summary.toString().getBytes(StandardCharsets.UTF_8)); } catch (IOException ignored) { }
            finish(code, result);
        }
    }

    private static final class Listener implements OriginalExtrasMenu.Listener {
        int character = -1, item = -1, variant = -1, track = -1;
        boolean closed;
        public void close() { closed = true; }
        public void image(int character, int item, int variant) { this.character = character; this.item = item; this.variant = variant; }
        public void replay(int character, int scene, int style) { this.character = character; this.item = scene; }
        public void music(int track) { this.track = track; }
    }
    private static void require(boolean value, String message) throws IOException { if (!value) throw new IOException(message); }
}
