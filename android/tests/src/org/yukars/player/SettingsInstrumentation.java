// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

import android.app.Activity;
import android.app.Instrumentation;
import android.content.Intent;
import android.net.Uri;
import android.provider.DocumentsContract;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.os.Bundle;
import android.os.Handler;
import android.os.SystemClock;
import android.view.MotionEvent;
import android.view.View;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.UUID;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.lang.reflect.Field;
import org.json.JSONArray;
import org.json.JSONObject;

/** Original option art, all eight pages, real JNI preferences and native System.dat persistence. */
public final class SettingsInstrumentation extends Instrumentation {
    private Bundle arguments;
    private long session;
    private String stage = "create";
    private int imageChecks;
    private int cropChecks;

    @Override public void onCreate(Bundle arguments) {
        super.onCreate(arguments); this.arguments = arguments == null ? new Bundle() : new Bundle(arguments); start();
    }
    @Override public void onStart() { new Thread(this::runTests, "Yuka settings tests").start(); }

    private void runTests() {
        Bundle result = new Bundle();
        JSONObject summary = new JSONObject();
        File scratch = null;
        int code = Activity.RESULT_CANCELED;
        try {
            String game = arguments.getString("game", "");
            require(!game.isEmpty(), "missing game argument");
            scratch = new File(getTargetContext().getCacheDir(), "settings-" + UUID.randomUUID());
            File saves = new File(scratch, "Save");
            File audio = new File(scratch, "audio");
            GameImporter.mkdir(saves); GameImporter.mkdir(audio);
            summary.put("scratch", scratch.getAbsolutePath());
            session = NativeBridge.create(game, saves.getAbsolutePath(), audio.getAbsolutePath());
            require(session != 0, "native session creation failed");

            stage = "original_option_art";
            OriginalAssets common = OriginalAssets.load(session);
            OriginalSettingsAssets artwork = OriginalSettingsAssets.load(session);
            for (String name : OriginalSettingsAssets.BACKGROUNDS) {
                Bitmap bitmap = artwork.bitmap(name, -1);
                require(bitmap.getWidth() == 528 && bitmap.getHeight() == 495, "incorrect original option background size: " + name);
                imageChecks++;
            }
            checkSheet(artwork, "op_btn01", 51, 17, 150, 28);
            checkSheet(artwork, "op_page", 12, 6, 82, 28);
            checkSheet(artwork, "op_sound_test", 6, 2, 72, 20);
            require(artwork.bitmap("op_var", 0).getWidth() == 19 && artwork.bitmap("op_var", 0).getHeight() == 37,
                "incorrect original slider handle");
            summary.put("original_images_checked", imageChecks).put("sprite_crops_checked", cropChecks);

            stage = "all_original_pages_and_navigation";
            for (int page = 0; page < 8; page++) {
                Recording listener = new Recording();
                OriginalSettingsMenu menu = menu(common, artwork, page, listener);
                Bitmap image = Bitmap.createBitmap(800, 600, Bitmap.Config.ARGB_8888);
                menu.draw(new Canvas(image), -1, -1);
                require(image.getPixel(0, 0) == common.bitmap("op_base", -1).getPixel(0, 0), "option page lost the original background");
                Bitmap source = artwork.bitmap(OriginalSettingsAssets.BACKGROUNDS[page], -1);
                boolean verifiedLabel = false;
                for (int y = 0; y < 40 && !verifiedLabel; y++) for (int x = 0; x < 150; x++) {
                    int pixel = source.getPixel(x, y);
                    if ((pixel >>> 24) == 255 && pixel != 0xffffffff) {
                        require(image.getPixel(56 + x, 65 + y) == pixel, "option page background is offset incorrectly");
                        verifiedLabel = true; break;
                    }
                }
                require(verifiedLabel, "original option page has no opaque heading pixel");
                try (FileOutputStream output = new FileOutputStream(new File(scratch, "settings-page-" + page + ".png"))) {
                    require(image.compress(Bitmap.CompressFormat.PNG, 100, output), "PNG encoding failed");
                } finally { image.recycle(); }
                menu.tap(700, 197); require(listener.action == OriginalSaveMenu.Action.SCREEN, "screen navigation not routed");
                menu.tap(700, 229); require(listener.action == OriginalSaveMenu.Action.TEXT, "text navigation not routed");
                menu.tap(700, 261); require(listener.action == OriginalSaveMenu.Action.SOUND, "sound navigation not routed");
                menu.tap(700, 293); require(listener.action == OriginalSaveMenu.Action.VOICE, "voice navigation not routed");
                listener.action = null;
                menu.tap(700, 470);
                require(menu.confirming() && listener.action == null, "title navigation bypassed original confirmation");
                menu.tap(480, 360); require(!menu.confirming() && listener.action == null, "no button did not cancel navigation");
                menu.tap(700, 470); menu.tap(280, 360);
                require(listener.action == OriginalSaveMenu.Action.TITLE, "confirmed title navigation did not dispatch");
                menu.tap(140, 572);
                require(listener.section == (page == 0 ? 1 : page <= 2 ? 2 : page == 3 ? 3 : 4), "wrong defaults group");
            }
            summary.put("all_eight_pages_navigation_and_confirmation", true);

            stage = "screen_and_text_radio_controls";
            Recording listener = new Recording();
            OriginalSettingsMenu menu = menu(common, artwork, 0, listener);
            menu.tap(300, 130); changed(listener, -1, 0);
            menu.tap(450, 230); changed(listener, 10, 2);
            menu.tap(300, 330); changed(listener, 11, 1);
            menu.tap(450, 430); changed(listener, 15, 2);
            menu = menu(common, artwork, 1, listener);
            menu.tap(300, 230); changed(listener, 3, 1);
            menu.tap(300, 330); changed(listener, 13, 0);
            menu.tap(300, 530); changed(listener, 12, 0);
            menu.tap(570, 50); require(listener.page == 2, "second text page cannot be opened");
            require(menu.begin(138, 128), "text-speed slider did not capture touch");
            menu.drag(468, 128, 0); menu.end(); changed(listener, 4, 0);
            require(menu.begin(138, 425), "auto-speed slider did not capture touch");
            menu.end(); changed(listener, 5, 6600);
            menu = menu(common, artwork, 2, listener);
            menu.tap(300, 130); changed(listener, 2, 1);
            menu.tap(450, 230); changed(listener, 14, 2);
            menu.tap(130, 330); changed(listener, 16, 1);
            summary.put("screen_and_text_controls", true);

            stage = "sound_voice_sliders_mutes_and_test_buttons";
            int[][] flags = {{100, 101}, {102, 103, 104}, {110, 111, 112, 113, 114}, {115, 116, 117, 120}, {118, 119, 121}};
            int[][] samples = {{-1}, {0, 1, 2}, {3, 4, 5, 6, 7}, {8, 9, 10, 11}, {12, 13, 14}};
            List<Integer> actualSamples = new ArrayList<>();
            for (int page = 3; page < 8; page++) {
                menu = menu(common, artwork, page, listener);
                for (int row = 0; row < flags[page - 3].length; row++) {
                    require(menu.begin(138, 128 + 99 * row), "volume slider did not capture touch");
                    menu.drag(468, 128 + 99 * row, 0); menu.end(); changed(listener, flags[page - 3][row], 1000);
                    menu.tap(480, 90 + 99 * row); changed(listener, flags[page - 3][row], 0);
                    if (page > 3 || row > 0) {
                        menu.tap(390, 90 + 99 * row); actualSamples.add(listener.sample);
                    }
                }
                if (page > 3) {
                    for (int index = 0; index < 4; index++) {
                        menu.tap(310 + 84 * index, 50); require(listener.page == index + 4, "voice page tab routing differs");
                    }
                }
            }
            List<Integer> expectedSamples = new ArrayList<>();
            for (int[] pageSamples : samples) for (int sample : pageSamples) expectedSamples.add(sample);
            require(actualSamples.equals(expectedSamples), "voice sample buttons do not match displayed characters");
            summary.put("all_volume_sliders_mutes_and_samples", true);

            stage = "native_settings_persist_without_running_story";
            JSONObject patch = new JSONObject("{\"2\":1,\"3\":1,\"4\":4,\"5\":4200,\"10\":2,\"11\":1,\"12\":0,\"13\":0,\"14\":2,\"15\":2,\"16\":1,\"100\":0,\"101\":300,\"111\":250}");
            NativeBridge.command(session, "settings", patch.toString());
            JSONObject state = new JSONObject(NativeBridge.state(session));
            JSONObject savedFlags = state.getJSONObject("settings").getJSONObject("flags");
            matchesPatch(savedFlags, patch);
            require(state.optBoolean("title") && state.optInt("history_count") == 0, "changing title settings executed story");
            File nativeSystem = new File(saves, "System.dat");
            require(nativeSystem.length() > 100, "original binary System.dat is missing");
            NativeBridge.destroy(session); session = 0;
            session = NativeBridge.create(game, saves.getAbsolutePath(), audio.getAbsolutePath());
            savedFlags = new JSONObject(NativeBridge.state(session)).getJSONObject("settings").getJSONObject("flags");
            matchesPatch(savedFlags, patch);
            NativeBridge.command(session, "settings_reset", "text");
            savedFlags = new JSONObject(NativeBridge.state(session)).getJSONObject("settings").getJSONObject("flags");
            require(savedFlags.getInt("4") == 20 && savedFlags.getInt("5") == 2000 && savedFlags.getInt("100") == 0,
                "text defaults did not reset only their own group");
            summary.put("native_settings_persistence_and_scoped_defaults", true);
            NativeBridge.destroy(session); session = 0;
            stage = "complete_save_import_staging_backup_publish";
            checkSaveDirectoryImport(scratch);
            summary.put("complete_save_import_staging_backup_publish", true);
            stage = "actual_activity_option_controls";
            checkActivity(scratch);
            summary.put("actual_activity_option_controls", true).put("passed", true);
            code = Activity.RESULT_OK;
        } catch (Throwable error) {
            try { summary.put("passed", false).put("stage", stage).put("error", error.toString()); }
            catch (Exception ignored) { }
            result.putString("failure", stage + " - " + error);
        } finally {
            if (session != 0) { try { NativeBridge.destroy(session); } catch (Throwable ignored) { } }
            String report = summary.toString(); result.putString("summary", report);
            if (scratch != null) try { Files.write(new File(scratch, "result.json").toPath(), report.getBytes(StandardCharsets.UTF_8)); }
            catch (IOException error) { result.putString("report_error", error.toString()); }
            finish(code, result);
        }
    }

    private OriginalSettingsMenu menu(OriginalAssets common, OriginalSettingsAssets artwork, int page, Recording listener) throws Exception {
        JSONObject flags = new JSONObject(NativeBridge.state(session)).getJSONObject("settings").getJSONObject("flags");
        return new OriginalSettingsMenu(common, artwork, page, flags, true, listener);
    }

    private void checkSaveDirectoryImport(File scratch) throws Exception {
        GameImporter.Progress progress = new GameImporter.Progress() {
            public void update(String text) { }
            public boolean cancelled() { return false; }
        };
        getTargetContext().getContentResolver().call(Uri.parse("content://org.yukars.player.tests.fixture"), "new_fixture", null, null);
        Uri tree = DocumentsContract.buildTreeDocumentUri("org.yukars.player.tests.documents", "root/input/Save");
        File staged = GameImporter.stageSaves(getTargetContext().getContentResolver(), tree, scratch, progress);
        require(Arrays.equals(Files.readAllBytes(new File(staged, "System.dat").toPath()), new byte[]{0, 17, (byte)254, 3}),
            "SAF staging changed native System.dat bytes");
        require(Arrays.equals(Files.readAllBytes(new File(staged, "Already.dat").toPath()), new byte[]{1, 2, 0, (byte)255, 18}),
            "SAF staging changed Already.dat bytes");
        byte[] incoming = {7, 0, 3, (byte)128, 9, 11};
        require(Arrays.equals(Files.readAllBytes(new File(staged, "savedata.001").toPath()), incoming), "SAF staging changed original slot name/bytes");
        File destination = new File(scratch, "import-destination"); GameImporter.mkdir(destination);
        byte[] previousSystem = {11, 22, 33}, previousAlready = {1, 2, 3, 4}, previousSlot = {5, 6, 7}, retained = {9, 8, 7, 6};
        Files.write(new File(destination, "System.dat").toPath(), previousSystem);
        Files.write(new File(destination, "Already.dat").toPath(), previousAlready);
        File priorSlot = new File(destination, "savedata.001"); Files.write(priorSlot.toPath(), previousSlot);
        require(priorSlot.setLastModified(1600000000000L), "could not set old slot timestamp");
        Files.write(new File(destination, "savedata.044").toPath(), retained);
        Files.write(new File(destination, "unrelated.txt").toPath(), new byte[]{42});
        File backup = GameImporter.backupSaves(destination);
        require(Arrays.equals(Files.readAllBytes(new File(backup, "System.dat").toPath()), previousSystem)
            && Arrays.equals(Files.readAllBytes(new File(backup, "Already.dat").toPath()), previousAlready)
            && Arrays.equals(Files.readAllBytes(new File(backup, "savedata.001").toPath()), previousSlot)
            && Arrays.equals(Files.readAllBytes(new File(backup, "savedata.044").toPath()), retained),
            "old native globals or story slots were not backed up intact");
        require(!new File(backup, "unrelated.txt").exists() && new File(backup, "savedata.001").lastModified() == 1600000000000L,
            "backup included unrelated files or lost the original slot date");
        require(new File(staged, "savedata.001").setLastModified(1700000000000L), "could not set incoming slot timestamp");
        boolean cancelled = false;
        try {
            GameImporter.publishStorySaves(staged, destination, new GameImporter.Progress() {
                public void update(String text) { }
                public boolean cancelled() { return true; }
            });
        } catch (IOException expected) { cancelled = true; }
        require(cancelled && Arrays.equals(Files.readAllBytes(priorSlot.toPath()), previousSlot), "cancelled import replaced an old slot");
        GameImporter.publishStorySaves(staged, destination, progress);
        require(Arrays.equals(Files.readAllBytes(priorSlot.toPath()), incoming) && priorSlot.lastModified() == 1700000000000L,
            "imported slot lost bytes or original timestamp");
        require(!new File(destination, ".savedata.001.import").exists(), "atomic slot publish left its temporary file");
        require(Arrays.equals(Files.readAllBytes(new File(destination, "System.dat").toPath()), previousSystem)
            && Arrays.equals(Files.readAllBytes(new File(destination, "Already.dat").toPath()), previousAlready),
            "story slot publishing unexpectedly replaced globals outside the validated native import transaction");
        require(Arrays.equals(Files.readAllBytes(new File(destination, "savedata.044").toPath()), retained), "unrelated story slot was removed");
        require(Arrays.equals(Files.readAllBytes(new File(backup, "savedata.001").toPath()), previousSlot), "publication changed the recoverable backup");
    }

    private void checkActivity(File scratch) throws Exception {
        MainActivity activity = (MainActivity) startActivitySync(new Intent(Intent.ACTION_MAIN)
            .setClassName(getTargetContext(), "org.yukars.player.MainActivity")
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_CLEAR_TASK));
        View gameView = (View) field(activity, "gameView");
        JSONObject original = null;
        try {
            await(() -> Boolean.TRUE.equals(field(activity, "nativeReady")) && !Boolean.TRUE.equals(field(activity, "busy"))
                && activitySettingsReady(gameView),
                30000, "Activity did not finish opening bundled game");
            original = activityFlags(gameView);
            touch(gameView, 390, 409, 390, 409);
            await(() -> field(activity, "settingsMenu") != null, 5000, "title options did not open original settings");
            touch(gameView, 700, 229, 700, 229);
            await(() -> ((OriginalSettingsMenu) field(activity, "settingsMenu")).page == 1, 5000, "text settings navigation failed");
            touch(gameView, 138, 128, 468, 128);
            await(() -> activityFlags(gameView).optInt("4") == 0, 3000, "text-speed drag lost its final value");
            touch(gameView, 570, 50, 570, 50);
            await(() -> ((OriginalSettingsMenu) field(activity, "settingsMenu")).page == 2, 5000, "second text page failed");
            touch(gameView, 300, 130, 300, 130);
            touch(gameView, 450, 230, 450, 230);
            await(() -> activityFlags(gameView).optInt("2") == 1 && activityFlags(gameView).optInt("14") == 2,
                3000, "font/effect controls did not apply");
            screenshot(gameView, new File(scratch, "activity-text-settings.png"));
            touch(gameView, 700, 293, 700, 293);
            await(() -> ((OriginalSettingsMenu) field(activity, "settingsMenu")).page == 4, 5000, "voice settings navigation failed");
            touch(gameView, 468, 128, 138, 128);
            await(() -> activityFlags(gameView).optInt("102") == 0 && activityFlags(gameView).optInt("111") == 0,
                3000, "master voice slider did not apply to character groups");
            touch(gameView, 400, 50, 400, 50);
            await(() -> ((OriginalSettingsMenu) field(activity, "settingsMenu")).page == 5, 5000, "female voice page failed");
            touch(gameView, 138, 227, 468, 227);
            await(() -> activityFlags(gameView).optInt("111") == 1000 && activityFlags(gameView).optInt("110") == 0,
                3000, "character volume did not stay independent");
            screenshot(gameView, new File(scratch, "activity-voice-settings.png"));
            touch(gameView, 700, 261, 700, 261);
            await(() -> ((OriginalSettingsMenu) field(activity, "settingsMenu")).page == 3, 5000, "sound settings navigation failed");
            touch(gameView, 480, 190, 480, 190);
            await(() -> activityFlags(gameView).optInt("101") == 0, 3000, "SE mute button did not apply");
            touch(gameView, 700, 197, 700, 197);
            await(() -> ((OriginalSettingsMenu) field(activity, "settingsMenu")).page == 0, 5000, "screen settings navigation failed");
            touch(gameView, 450, 430, 450, 430);
            await(() -> activityFlags(gameView).optInt("15") == 2, 3000, "window-opacity control did not apply");
            screenshot(gameView, new File(scratch, "activity-screen-settings.png"));
            touch(gameView, 700, 502, 700, 502);
            await(() -> field(activity, "settingsMenu") == null && "TITLE".equals(String.valueOf(field(activity, "mode"))),
                3000, "return from title settings did not restore the title");
        } finally {
            if (original != null) {
                JSONObject restore = original;
                CountDownLatch restored = new CountDownLatch(1);
                ((Handler) field(activity, "engine")).post(() -> {
                    try { NativeBridge.command(((Long) uncheckedField(activity, "session")).longValue(), "settings", restore.toString()); }
                    finally { restored.countDown(); }
                });
                require(restored.await(5, TimeUnit.SECONDS), "restoring original test settings timed out");
            }
            runOnMainSync(activity::finish);
        }
    }

    private static Object field(Object owner, String name) throws Exception {
        Field field = owner.getClass().getDeclaredField(name); field.setAccessible(true); return field.get(owner);
    }
    private static Object uncheckedField(Object owner, String name) {
        try { return field(owner, name); } catch (Exception error) { throw new IllegalStateException(error); }
    }
    private static JSONObject activityFlags(View gameView) throws Exception {
        synchronized (field(gameView, "frameLock")) {
            return new JSONObject(field(gameView, "state").toString()).getJSONObject("settings").getJSONObject("flags");
        }
    }
    private static boolean activitySettingsReady(View gameView) throws Exception {
        synchronized (field(gameView, "frameLock")) {
            return new JSONObject(field(gameView, "state").toString()).optJSONObject("settings") != null;
        }
    }
    private void touch(View view, float fromX, float fromY, float toX, float toY) {
        waitForIdleSync(); SystemClock.sleep(100);
        runOnMainSync(() -> {
            float scale = Math.min(view.getWidth() / 800f, view.getHeight() / 600f);
            float left = (view.getWidth() - 800 * scale) / 2, top = (view.getHeight() - 600 * scale) / 2;
            long now = SystemClock.uptimeMillis();
            for (int step = 0; step < 3; step++) {
                int action = step == 0 ? MotionEvent.ACTION_DOWN : step == 1 ? MotionEvent.ACTION_MOVE : MotionEvent.ACTION_UP;
                MotionEvent event = MotionEvent.obtain(now, now + step * 20, action,
                    left + (step == 0 ? fromX : toX) * scale, top + (step == 0 ? fromY : toY) * scale, 0);
                try { view.dispatchTouchEvent(event); } finally { event.recycle(); }
            }
        });
    }
    private void screenshot(View view, File destination) throws IOException {
        Bitmap bitmap = Bitmap.createBitmap(800, 600, Bitmap.Config.ARGB_8888);
        runOnMainSync(() -> {
            float scale = Math.min(view.getWidth() / 800f, view.getHeight() / 600f);
            Canvas canvas = new Canvas(bitmap);
            canvas.scale(1 / scale, 1 / scale);
            canvas.translate(-(view.getWidth() - 800 * scale) / 2, -(view.getHeight() - 600 * scale) / 2);
            view.draw(canvas);
        });
        try (FileOutputStream output = new FileOutputStream(destination)) {
            require(bitmap.compress(Bitmap.CompressFormat.PNG, 100, output), "screenshot encoding failed");
        } finally { bitmap.recycle(); }
    }
    private interface Condition { boolean ready() throws Exception; }
    private static void await(Condition condition, long timeout, String message) throws Exception {
        long end = SystemClock.elapsedRealtime() + timeout;
        do { if (condition.ready()) return; Thread.sleep(25); } while (SystemClock.elapsedRealtime() < end);
        throw new IOException(message);
    }

    private void checkSheet(OriginalSettingsAssets artwork, String name, int count, int rows, int width, int height) throws IOException {
        Bitmap sheet = OriginalAssets.decode(NativeBridge.image(session, "ykg\\system\\option\\" + name + ".ykg", -1));
        for (int index = 0; index < count; index++) {
            int x = name.equals("op_btn01") ? index / rows * width : index % rows * width;
            int y = name.equals("op_btn01") ? index % rows * height : index / rows * height;
            Bitmap frame = artwork.bitmap(name, index);
            require(frame.getWidth() == width && frame.getHeight() == height, "sprite dimensions differ: " + name);
            int[] expected = new int[width * height]; int[] actual = new int[expected.length];
            sheet.getPixels(expected, 0, width, x, y, width, height);
            frame.getPixels(actual, 0, width, 0, 0, width, height);
            require(Arrays.equals(expected, actual), "sprite crop differs: " + name + " " + index);
            imageChecks++; cropChecks++;
        }
    }

    private static void changed(Recording listener, int flag, int value) throws IOException {
        require(listener.flag == flag && listener.value == value, "wrong setting callback, expected " + flag + "=" + value);
    }
    private static void matchesPatch(JSONObject flags, JSONObject patch) throws Exception {
        JSONArray names = patch.names();
        for (int index = 0; names != null && index < names.length(); index++) {
            String key = names.getString(index);
            require(flags.getInt(key) == patch.getInt(key), "settings patch was not applied/persisted: " + key);
        }
    }
    private static void require(boolean condition, String message) throws IOException { if (!condition) throw new IOException(message); }
    private static final class Recording implements OriginalSettingsMenu.Listener {
        int flag = -999, value = -999, page = -1, sample = -999, section = -1;
        OriginalSaveMenu.Action action;
        public void change(int flag, int value) { this.flag = flag; this.value = value; }
        public void page(int page) { this.page = page; }
        public void action(OriginalSaveMenu.Action action) { this.action = action; }
        public void sample(int voiceId) { sample = voiceId; }
        public void defaults(int section) { this.section = section; }
    }
}
