// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

import android.app.Activity;
import android.app.Instrumentation;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.os.Bundle;
import android.os.SystemClock;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.Arrays;
import java.util.UUID;
import org.json.JSONObject;

/** Exercises the original menu artwork, native previews, and comments through JNI. */
public final class OriginalUiInstrumentation extends Instrumentation {
    private Bundle arguments;
    private String stage = "create";
    private long session;
    private int imagesChecked;
    private int cropsChecked;

    @Override public void onCreate(Bundle arguments) {
        super.onCreate(arguments);
        this.arguments = arguments == null ? new Bundle() : new Bundle(arguments);
        start();
    }

    @Override public void onStart() {
        new Thread(this::runTests, "Yuka original UI instrumentation").start();
    }

    private void runTests() {
        Bundle result = new Bundle();
        JSONObject summary = new JSONObject();
        File scratch = null;
        int code = Activity.RESULT_CANCELED;
        try {
            String game = arguments.getString("game", "");
            require(!game.isEmpty(), "missing instrumentation argument game");
            File original = new File(arguments.getString("originalSave", new File(game, "Save/savedata.001").getAbsolutePath()));
            require(original.isFile(), "original PC slot is missing: " + original);
            byte[] originalBytes = Files.readAllBytes(original.toPath());
            long originalModified = original.lastModified();
            scratch = new File(getTargetContext().getCacheDir(), "original-ui-" + UUID.randomUUID());
            File saves = new File(scratch, "Save");
            File audio = new File(scratch, "audio");
            GameImporter.mkdir(saves);
            GameImporter.mkdir(audio);
            summary.put("scratch", scratch.getAbsolutePath());
            session = NativeBridge.create(game, saves.getAbsolutePath(), audio.getAbsolutePath());
            require(session != 0, "native create returned a null session");

            stage = "original_assets";
            OriginalAssets assets = OriginalAssets.load(session);
            checkAssets(assets);
            summary.put("images_checked", imagesChecked);
            summary.put("sprite_crops_checked", cropsChecked);

            stage = "native_pc_preview";
            JSONObject pcInfo = new JSONObject(NativeBridge.saveInfo(session, original.getAbsolutePath()));
            require(pcInfo.has("chapter") && pcInfo.has("caption") && pcInfo.has("window"), "native preview omitted text fields");
            require(pcInfo.getBoolean("has_thumbnail"), "PC slot has no advertised thumbnail");
            require(pcInfo.getLong("modified_ms") == originalModified, "PC preview changed the file timestamp");
            Bitmap pcThumbnail = OriginalAssets.decode(NativeBridge.saveThumbnail(session, original.getAbsolutePath()));
            dimensions(pcThumbnail, 100, 75, "PC save thumbnail");
            require((pcThumbnail.getPixel(50, 37) >>> 24) == 255, "PC thumbnail did not decode to opaque ARGB");
            summary.put("original_pc_preview", true);

            stage = "slot_mapping";
            checkSlotMapping(saves);
            summary.put("slot_mapping", true);

            stage = "save_with_comment";
            NativeBridge.command(session, "new", "");
            NativeBridge.command(session, "pause", "0");
            JSONObject before = waitStable();
            String comment = "中文存档备注：测试\"引号\"和\\路径";
            File saved = OriginalSaveMenu.file(saves, 0, 3);
            JSONObject request = new JSONObject().put("path", saved.getAbsolutePath()).put("comment", comment);
            NativeBridge.command(session, "save_with_comment", request.toString());
            require(saved.isFile() && saved.length() > 64, "save_with_comment did not write a native slot");
            JSONObject savedInfo = new JSONObject(NativeBridge.saveInfo(session, saved.getAbsolutePath()));
            require(comment.equals(savedInfo.getString("caption")), "Chinese comment changed after native encoding round trip");
            require(savedInfo.getBoolean("has_thumbnail"), "new save omitted its thumbnail");
            dimensions(OriginalAssets.decode(NativeBridge.saveThumbnail(session, saved.getAbsolutePath())), 100, 75, "new save thumbnail");
            NativeBridge.command(session, "advance", "");
            waitStable();
            NativeBridge.command(session, "load", saved.getAbsolutePath());
            JSONObject restored = waitStable();
            require(before.optString("dialogue").equals(restored.optString("dialogue")), "commented save changed restored dialogue");
            require(before.optString("speaker").equals(restored.optString("speaker")), "commented save changed restored speaker");
            summary.put("chinese_comment_round_trip", true);

            stage = "damaged_slot_isolation";
            Files.copy(original.toPath(), OriginalSaveMenu.file(saves, 0, 1).toPath(), java.nio.file.StandardCopyOption.REPLACE_EXISTING);
            Files.write(OriginalSaveMenu.file(saves, 0, 2).toPath(), new byte[]{'b', 'a', 'd', 0});
            OriginalSaveMenu.Slot[] slots = new OriginalSaveMenu.Slot[5];
            for (int index = 0; index < slots.length; index++) slots[index] = OriginalSaveMenu.Slot.read(session, saves, 0, index + 1);
            require(slots[0].exists && slots[0].error.isEmpty() && slots[0].thumbnail != null, "copied PC slot preview failed");
            require(slots[1].exists && !slots[1].error.isEmpty(), "damaged slot was not marked unreadable");
            require(slots[2].exists && slots[2].error.isEmpty() && comment.equals(slots[2].caption), "damaged slot prevented reading the next healthy slot");
            require(!slots[3].exists && slots[3].error.isEmpty() && !slots[4].exists, "empty slots were treated as damaged saves");
            summary.put("damaged_slot_isolation", true);

            byte[] badThumbnail = originalBytes.clone();
            int thumbnailOffset = java.nio.ByteBuffer.wrap(badThumbnail).order(java.nio.ByteOrder.LITTLE_ENDIAN).getInt(0x30);
            badThumbnail[thumbnailOffset] = 0;
            File damagedPreview = OriginalSaveMenu.file(saves, 1, 1);
            Files.write(damagedPreview.toPath(), badThumbnail);
            OriginalSaveMenu.Slot playable = OriginalSaveMenu.Slot.read(session, saves, 1, 1);
            require(playable.error.isEmpty() && playable.thumbnail == null && playable.chapter.equals(pcInfo.optString("chapter")),
                "a damaged thumbnail prevented reading healthy slot metadata");
            NativeBridge.command(session, "load", damagedPreview.getAbsolutePath());
            waitStable();
            summary.put("damaged_thumbnail_loadable", true);

            stage = "canvas_and_hit_targets";
            checkMenu(assets, slots, scratch);
            summary.put("canvas_and_hit_targets", true);
            require(originalModified == original.lastModified()
                && Arrays.equals(originalBytes, Files.readAllBytes(original.toPath())), "original PC save changed during UI tests");
            summary.put("original_pc_save_unchanged", true);
            stage = "complete";
            summary.put("passed", true);
            code = Activity.RESULT_OK;
        } catch (Throwable error) {
            try { summary.put("passed", false).put("stage", stage).put("error", error.toString()); }
            catch (Exception ignored) { }
            result.putString("failure", stage + " - " + error);
        } finally {
            if (session != 0) {
                try { NativeBridge.destroy(session); } catch (Throwable ignored) { }
            }
            String text = summary.toString();
            result.putString("summary", text);
            if (scratch != null) {
                try { Files.write(new File(scratch, "result.json").toPath(), text.getBytes(StandardCharsets.UTF_8)); }
                catch (IOException error) { result.putString("report_error", error.toString()); }
            }
            finish(code, result);
        }
    }

    private void checkAssets(OriginalAssets assets) throws IOException {
        checkImages(assets, "op_base", 0, 800, 600);
        checkImages(assets, "op_title", 10, 235, 35);
        checkImages(assets, "op_tab", 33, 144, 25);
        checkImages(assets, "save_item", 2, 502, 89);
        checkImages(assets, "load_item", 2, 502, 89);
        checkImages(assets, "save_page", 40, 40, 40);
        checkImages(assets, "load_page", 40, 40, 40);
        checkImages(assets, "autosave", 0, 100, 75);
        checkImages(assets, "YesNo_Base", 0, 800, 600);
        checkImages(assets, "YesNo_btn", 4, 166, 42);
        for (int index = 1; index <= 7; index++) checkImages(assets, "YesNo_P0" + index, 0, 0, 0);
        checkImages(assets, "t_btn", 10, 0, 0);
        checkImages(assets, "t_btn_msg", 5, 0, 0);
        checkImages(assets, "window", 0, 0, 0);
        checkImages(assets, "name", 0, 0, 0);
        checkImages(assets, "sys_btn01", 15, 0, 0);
        checkImages(assets, "sys_btn02", 12, 0, 0);
        checkImages(assets, "sys_btn_help", 9, 0, 0);
        checkImages(assets, "select", 0, 0, 0);
        Bitmap overlay = assets.bitmap("YesNo_Base", -1);
        int alpha = overlay.getPixel(0, 0) >>> 24;
        require(alpha > 0 && alpha < 255, "confirmation overlay lost its original PNG transparency");

        Bitmap sheet = sheet("op_title");
        for (int sequence = 0; sequence < 10; sequence++) crop(assets.bitmap("op_title", sequence), sheet, 0, sequence * 35);
        sheet = sheet("op_tab");
        for (int sequence = 0; sequence < 33; sequence++) crop(assets.bitmap("op_tab", sequence), sheet, sequence / 11 * 144, sequence % 11 * 25);
        for (String name : new String[]{"save_item", "load_item"}) {
            sheet = sheet(name);
            for (int sequence = 0; sequence < 2; sequence++) crop(assets.bitmap(name, sequence), sheet, 0, sequence * 89);
        }
        for (String name : new String[]{"save_page", "load_page"}) {
            sheet = sheet(name);
            for (int sequence = 0; sequence < 40; sequence++) crop(assets.bitmap(name, sequence), sheet, sequence / 10 * 40, sequence % 10 * 40);
        }
        sheet = sheet("YesNo_btn");
        for (int sequence = 0; sequence < 4; sequence++) crop(assets.bitmap("YesNo_btn", sequence), sheet, sequence % 2 * 166, sequence / 2 * 84);
    }

    private void checkImages(OriginalAssets assets, String name, int sequences, int width, int height) throws IOException {
        if (sequences == 0) {
            dimensions(assets.bitmap(name, -1), width, height, name);
            imagesChecked++;
            return;
        }
        for (int sequence = 0; sequence < sequences; sequence++) {
            dimensions(assets.bitmap(name, sequence), width, height, name + " sequence " + sequence);
            imagesChecked++;
        }
    }

    private Bitmap sheet(String name) {
        return OriginalAssets.decode(NativeBridge.image(session, "ykg\\system\\option\\" + name + ".ykg", -1));
    }

    private void crop(Bitmap frame, Bitmap sheet, int x, int y) throws IOException {
        int[] expected = new int[frame.getWidth() * frame.getHeight()];
        int[] actual = new int[expected.length];
        sheet.getPixels(expected, 0, frame.getWidth(), x, y, frame.getWidth(), frame.getHeight());
        frame.getPixels(actual, 0, frame.getWidth(), 0, 0, frame.getWidth(), frame.getHeight());
        require(Arrays.equals(expected, actual), "native sequence crop differs from the original sheet at " + x + "," + y);
        cropsChecked++;
    }

    private static void checkSlotMapping(File directory) throws IOException {
        require(OriginalSaveMenu.file(directory, 0, 1).getName().equals("savedata.001"), "first slot mapping changed");
        require(OriginalSaveMenu.file(directory, 0, 5).getName().equals("savedata.005"), "first page last slot mapping changed");
        require(OriginalSaveMenu.file(directory, 1, 1).getName().equals("savedata.011"), "second page uses the wrong slot stride");
        require(OriginalSaveMenu.file(directory, 19, 5).getName().equals("savedata.195"), "final slot mapping changed");
        for (int[] invalid : new int[][]{{-1, 1}, {20, 1}, {0, 0}, {0, 6}}) {
            boolean rejected = false;
            try { OriginalSaveMenu.file(directory, invalid[0], invalid[1]); }
            catch (IllegalArgumentException expected) { rejected = true; }
            require(rejected, "out-of-range slot mapping was accepted");
        }
    }

    private static void checkMenu(OriginalAssets assets, OriginalSaveMenu.Slot[] slots, File scratch) throws IOException {
        RecordingListener listener = new RecordingListener();
        OriginalSaveMenu menu = new OriginalSaveMenu(assets, true, 0, 1, listener);
        menu.slots = slots;
        Bitmap frame = Bitmap.createBitmap(800, 600, Bitmap.Config.ARGB_8888);
        Canvas canvas = new Canvas(frame);
        menu.draw(canvas, -1, -1);
        dimensions(frame, 800, 600, "menu canvas");
        pixel(frame, assets.bitmap("op_base", -1), 0, 0, 0, 0);
        pixel(frame, assets.bitmap("save_item", 0), 446, 160, 400, 80);
        pixel(frame, assets.bitmap("autosave", -1), 103, 124, 50, 37);
        pixel(frame, assets.bitmap("save_page", 20), 577, 100, 20, 20);
        pixel(frame, assets.bitmap("op_tab", 27), 726, 338, 70, 0);
        writePng(frame, new File(scratch, "save-menu.png"));
        menu.draw(canvas, 300, 240);
        pixel(frame, assets.bitmap("save_item", 1), 446, 258, 400, 80);
        menu.tap(622, 541);
        require(listener.page == 19, "last-page hit target missed");
        menu.tap(300, 510);
        require(listener.row == 5, "fifth-row hit target missed");
        menu.tap(700, 380);
        require(listener.action == OriginalSaveMenu.Action.LOAD, "load sidebar hit target missed");
        menu.tap(700, 500);
        require(listener.action == OriginalSaveMenu.Action.RETURN, "return sidebar hit target missed");
        menu.tap(700, 438);
        require(listener.action == OriginalSaveMenu.Action.PREVIOUS_CHOICE, "previous choice sidebar hit target missed");

        menu.confirmation = 2;
        menu.draw(canvas, -1, -1);
        pixel(frame, assets.bitmap("YesNo_P02", -1), 109, 280, 100, 50);
        pixel(frame, assets.bitmap("YesNo_btn", 0), 280, 362, 80, 21);
        writePng(frame, new File(scratch, "overwrite-confirmation.png"));
        menu.tap(280, 360);
        menu.tap(500, 360);
        require(listener.confirmations == 1 && listener.cancellations == 1, "confirmation hit targets missed");

        OriginalSaveMenu loading = new OriginalSaveMenu(assets, false, 0, 1, listener);
        loading.slots = slots;
        loading.draw(canvas, -1, -1);
        pixel(frame, assets.bitmap("load_item", 0), 446, 160, 400, 80);
        pixel(frame, assets.bitmap("load_page", 20), 577, 100, 20, 20);
        pixel(frame, assets.bitmap("op_tab", 28), 726, 370, 70, 0);
        writePng(frame, new File(scratch, "load-menu.png"));
    }

    private static void pixel(Bitmap output, Bitmap source, int x, int y, int sourceX, int sourceY) throws IOException {
        int expected = source.getPixel(sourceX, sourceY);
        require((expected >>> 24) == 255, "pixel assertion selected a translucent source pixel at " + sourceX + "," + sourceY);
        require(output.getPixel(x, y) == expected, "menu pixel differs from its original artwork at " + x + "," + y);
    }

    private static void writePng(Bitmap bitmap, File file) throws IOException {
        try (FileOutputStream output = new FileOutputStream(file)) {
            require(bitmap.compress(Bitmap.CompressFormat.PNG, 100, output), "could not encode menu evidence PNG");
        }
    }

    private JSONObject waitStable() throws Exception {
        long deadline = SystemClock.elapsedRealtime() + 30000;
        while (SystemClock.elapsedRealtime() < deadline) {
            NativeBridge.tick(session);
            JSONObject state = new JSONObject(NativeBridge.state(session));
            require(!state.optBoolean("finished"), "runtime finished before a stable saveable state");
            if (state.optBoolean("can_save")) return state;
            Thread.sleep(16);
        }
        throw new IOException("30-second timeout waiting for saveable dialogue");
    }

    private static void dimensions(Bitmap bitmap, int width, int height, String description) throws IOException {
        require(bitmap != null && bitmap.getWidth() > 0 && bitmap.getHeight() > 0, description + " has no bitmap");
        if (width > 0 && height > 0)
            require(bitmap.getWidth() == width && bitmap.getHeight() == height, description + " dimensions changed");
    }

    private static void require(boolean condition, String message) throws IOException {
        if (!condition) throw new IOException(message);
    }

    private static final class RecordingListener implements OriginalSaveMenu.Listener {
        int page = -1;
        int row = -1;
        int confirmations;
        int cancellations;
        OriginalSaveMenu.Action action;
        @Override public void page(int page) { this.page = page; }
        @Override public void slot(int row) { this.row = row; }
        @Override public void confirm() { confirmations++; }
        @Override public void cancel() { cancellations++; }
        @Override public void action(OriginalSaveMenu.Action action) { this.action = action; }
    }
}
