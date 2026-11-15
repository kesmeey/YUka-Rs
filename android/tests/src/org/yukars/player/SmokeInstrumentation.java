// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

import android.app.Activity;
import android.app.Instrumentation;
import android.content.ContentResolver;
import android.database.Cursor;
import android.net.Uri;
import android.os.Bundle;
import android.os.SystemClock;
import android.provider.DocumentsContract;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.Arrays;
import java.util.HashMap;
import java.util.Map;
import java.util.UUID;
import org.json.JSONArray;
import org.json.JSONObject;

/** Packaged in a separate instrumentation APK. Executes the real JNI runtime. */
public final class SmokeInstrumentation extends Instrumentation {
    private Bundle arguments;
    private String stage = "create";
    private long session;
    private int loads;
    private int plays;
    private int frames;
    private final int[] pixels = new int[800 * 600];

    @Override public void onCreate(Bundle arguments) {
        super.onCreate(arguments);
        this.arguments = arguments == null ? new Bundle() : new Bundle(arguments);
        start();
    }

    @Override public void onStart() {
        new Thread(this::runSmoke, "Yuka instrumentation").start();
    }

    private void runSmoke() {
        Bundle result = new Bundle();
        JSONObject summary = new JSONObject();
        File output = null;
        int code = Activity.RESULT_CANCELED;
        try {
            stage = "saf_import_export";
            testSafCopy();
            summary.put("saf_import_export", true);
            stage = "create";
            String game = required("game");
            String saves = required("save");
            String audio = required("audio");
            new File(saves).mkdirs();
            new File(audio).mkdirs();
            output = new File(saves, "android-smoke-result.json");
            session = NativeBridge.create(game, saves, audio);
            require(session != 0, "native create returned a null session");

            stage = "title";
            NativeBridge.command(session, "title", "");
            JSONObject title = state();
            require(title.optBoolean("title"), "title command did not enter title state");
            renderVisible();

            stage = "new_game";
            NativeBridge.command(session, "new", "");
            NativeBridge.command(session, "pause", "0");
            JSONObject current = waitStable();
            require(!current.optString("dialogue").isEmpty(), "new game has no first dialogue");
            renderVisible();
            for (int index = 0; index < 3; index++) {
                stage = "advance_" + (index + 1);
                String before = current.optString("dialogue");
                NativeBridge.command(session, "advance", "");
                current = waitStable();
                require(!before.equals(current.optString("dialogue")), "advance did not change dialogue");
            }

            stage = "native_save";
            String dialogue = current.optString("dialogue");
            String speaker = current.optString("speaker");
            File save = new File(saves, "smoke.savedata.001");
            NativeBridge.command(session, "save", save.getAbsolutePath());
            require(save.isFile() && save.length() > 64, "native save was not written");
            stage = "advance_after_save";
            NativeBridge.command(session, "advance", "");
            waitStable();
            stage = "native_load";
            NativeBridge.command(session, "load", save.getAbsolutePath());
            current = waitStable();
            require(dialogue.equals(current.optString("dialogue")), "saved dialogue was not restored");
            require(speaker.equals(current.optString("speaker")), "saved speaker was not restored");
            renderVisible();
            summary.put("native_round_trip", true);
            summary.put("native_save_bytes", save.length());

            String original = arguments.getString("originalSave", "");
            if (!original.isEmpty()) {
                stage = "original_pc_load";
                NativeBridge.command(session, "load", original);
                JSONObject imported = waitStable(true);
                require(imported.optBoolean("can_save"), "original PC save did not restore a saveable runtime");
                NativeBridge.render(session, pixels);
                int originalImageHash = Arrays.hashCode(pixels);
                frames++;
                String originalText = imported.optString("dialogue");
                String originalSpeaker = imported.optString("speaker");
                String originalChoices = imported.optJSONArray("choices").toString();
                File reexported = new File(saves, "smoke.savedata.pc-roundtrip");
                stage = "original_pc_resave";
                NativeBridge.command(session, "save", reexported.getAbsolutePath());
                require(reexported.isFile() && reexported.length() > 64, "resaved PC data missing");
                NativeBridge.command(session, "new", "");
                stage = "original_pc_round_trip";
                NativeBridge.command(session, "load", reexported.getAbsolutePath());
                JSONObject restored = waitStable(true);
                require(originalText.equals(restored.optString("dialogue")), "PC imported dialogue changed after native round trip");
                require(originalSpeaker.equals(restored.optString("speaker")), "PC imported speaker changed after native round trip");
                require(originalChoices.equals(restored.optJSONArray("choices").toString()), "PC imported choice changed after native round trip");
                NativeBridge.render(session, pixels);
                require(originalImageHash == Arrays.hashCode(pixels), "PC imported image changed after native round trip");
                frames++;
                summary.put("original_pc_round_trip", true);
                int choices = restored.optJSONArray("choices").length();
                if (choices > 0) {
                    stage = "restored_choice_branches";
                    int branches = Math.min(2, choices);
                    for (int choice = 0; choice < branches; choice++) {
                        NativeBridge.command(session, "load", reexported.getAbsolutePath());
                        waitStable(true);
                        NativeBridge.command(session, "choose", Integer.toString(choice));
                        JSONObject branch = waitStable();
                        require(branch.optJSONArray("choices").length() == 0, "chosen branch did not leave its selection");
                        renderVisible();
                    }
                    summary.put("restored_choice_branches", branches);
                } else {
                    stage = "original_pc_continue";
                    NativeBridge.command(session, "advance", "");
                    JSONObject continued = waitStable();
                    require(!originalText.equals(continued.optString("dialogue")), "PC imported dialogue did not advance after round trip");
                    NativeBridge.render(session, pixels);
                    for (int pixel : pixels) require((pixel >>> 24) == 255, "continued frame has invalid ARGB alpha");
                    frames++;
                    summary.put("original_pc_continued", true);
                }
            }

            stage = "audio_events";
            require(loads > 0 && plays > 0, "runtime produced no usable load/play audio events");
            stage = "pause_resume";
            NativeBridge.command(session, "pause", "1");
            require(state().optBoolean("paused"), "pause state missing");
            NativeBridge.command(session, "pause", "0");
            require(!state().optBoolean("paused"), "resume state missing");
            stage = "complete";
            summary.put("passed", true);
            summary.put("audio_load_events", loads);
            summary.put("audio_play_events", plays);
            summary.put("frames_checked", frames);
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
            if (output != null) {
                try { Files.write(output.toPath(), text.getBytes(StandardCharsets.UTF_8)); }
                catch (IOException error) { result.putString("report_error", error.toString()); }
            }
            finish(code, result);
        }
    }

    private JSONObject waitStable() throws Exception {
        return waitStable(false);
    }

    private void testSafCopy() throws Exception {
        ContentResolver resolver = getTargetContext().getContentResolver();
        String authority = FixtureDocumentsProvider.AUTHORITY;
        resolver.call(Uri.parse("content://org.yukars.player.tests.fixture"), "new_fixture", null, null);
        Uri input = DocumentsContract.buildTreeDocumentUri(authority, "root/input");
        Uri output = DocumentsContract.buildTreeDocumentUri(authority, "root/output");
        File privateDirectory = new File(getTargetContext().getCacheDir(), "saf-smoke-" + UUID.randomUUID());
        GameImporter.mkdir(privateDirectory);
        File existingSave = new File(privateDirectory, "game-" + UUID.randomUUID() + "/Save/savedata.004");
        GameImporter.mkdir(existingSave.getParentFile());
        byte[] existingBytes = new byte[]{11, 12, 13, 14};
        Files.write(existingSave.toPath(), existingBytes);
        File interrupted = new File(privateDirectory, ".import-" + UUID.randomUUID());
        GameImporter.mkdir(interrupted);
        Files.write(new File(interrupted, "data01.ykc").toPath(), new byte[]{1, 2});
        GameImporter.Progress progress = new GameImporter.Progress() {
            public void update(String text) { }
            public boolean cancelled() { return false; }
        };
        Map<String, byte[]> expected = new HashMap<>();
        expected.put("System.dat", new byte[]{0, 17, (byte) 254, 3});
        expected.put("Already.dat", new byte[]{1, 2, 0, (byte) 255, 18});
        expected.put("savedata.001", new byte[]{7, 0, 3, (byte) 128, 9, 11});
        File imported = GameImporter.importGame(resolver, input, privateDirectory, progress);
        require(!interrupted.exists(), "interrupted import staging was not cleaned on retry");
        require(Arrays.equals(Files.readAllBytes(new File(imported, "data01.ykc").toPath()),
            new byte[]{'Y', 'K', 'C', '0', '0', '1', 0, (byte) 255}), "SAF archive bytes changed");
        File saves = new File(imported, "Save");
        for (Map.Entry<String, byte[]> item : expected.entrySet())
            require(Arrays.equals(item.getValue(), Files.readAllBytes(new File(saves, item.getKey()).toPath())), "SAF import changed " + item.getKey());
        Uri exported = GameImporter.exportSaves(resolver, output, saves, progress);
        Uri children = DocumentsContract.buildChildDocumentsUriUsingTree(output, DocumentsContract.getDocumentId(exported));
        int count = 0;
        try (Cursor cursor = resolver.query(children, new String[]{DocumentsContract.Document.COLUMN_DOCUMENT_ID,
                DocumentsContract.Document.COLUMN_DISPLAY_NAME}, null, null, null)) {
            require(cursor != null, "SAF export cannot be enumerated");
            while (cursor.moveToNext()) {
                String name = cursor.getString(1);
                require(expected.containsKey(name), "SAF export changed a native filename: " + name);
                Uri document = DocumentsContract.buildDocumentUriUsingTree(output, cursor.getString(0));
                require(Arrays.equals(expected.get(name), readDocument(resolver, document)), "SAF export changed " + name);
                count++;
            }
        }
        require(count == expected.size(), "SAF export omitted System.dat, Already.dat, or a save slot");
        Uri originalSlot = DocumentsContract.buildDocumentUriUsingTree(input, "root/input/Save/savedata.001");
        File single = GameImporter.importSave(resolver, originalSlot, saves, progress);
        require(Arrays.equals(expected.get("savedata.001"), Files.readAllBytes(single.toPath())), "single PC save import changed bytes");
        GameImporter.Progress cancelled = new GameImporter.Progress() {
            public void update(String text) { }
            public boolean cancelled() { return true; }
        };
        verifyRejectedImport(resolver, input, privateDirectory, cancelled, "cancelled");
        resolver.call(Uri.parse("content://org.yukars.player.tests.fixture"), "new_fixture", "duplicate", null);
        verifyRejectedImport(resolver, input, privateDirectory, progress, "duplicate filename");
        require(Arrays.equals(existingBytes, Files.readAllBytes(existingSave.toPath())), "failed import changed an existing game's save");
        require(Arrays.equals(expected.get("System.dat"), Files.readAllBytes(new File(saves, "System.dat").toPath())),
            "failed import changed the previously imported game");
        GameImporter.removePrivateTree(privateDirectory, getTargetContext().getCacheDir());
    }

    private static void verifyRejectedImport(ContentResolver resolver, Uri input, File directory,
                                            GameImporter.Progress progress, String cause) throws Exception {
        boolean rejected = false;
        try { GameImporter.importGame(resolver, input, directory, progress); }
        catch (IOException expected) { rejected = true; }
        require(rejected, cause + " import unexpectedly succeeded");
        File[] staging = directory.listFiles(file -> file.getName().startsWith(".import-"));
        require(staging != null && staging.length == 0, cause + " import left partial archive files");
    }

    private static byte[] readDocument(ContentResolver resolver, Uri uri) throws IOException {
        try (InputStream input = resolver.openInputStream(uri); ByteArrayOutputStream output = new ByteArrayOutputStream()) {
            require(input != null, "exported document could not be opened");
            byte[] buffer = new byte[1024];
            int count;
            while ((count = input.read(buffer)) != -1) output.write(buffer, 0, count);
            return output.toByteArray();
        }
    }

    private JSONObject waitStable(boolean allowChoice) throws Exception {
        long deadline = SystemClock.elapsedRealtime() + 30000;
        JSONObject latest = null;
        while (SystemClock.elapsedRealtime() < deadline) {
            NativeBridge.tick(session);
            latest = state();
            JSONArray choices = latest.optJSONArray("choices");
            require(allowChoice || choices == null || choices.length() == 0, "unexpected choice in opening smoke path");
            require(!latest.optBoolean("finished"), "runtime finished before expected stable dialogue");
            if (latest.optBoolean("can_save")) return latest;
            Thread.sleep(16);
        }
        throw new IOException("30-second timeout waiting for stable dialogue, status="
            + (latest == null ? "unavailable" : latest.optString("status")));
    }

    private JSONObject state() throws Exception {
        JSONObject state = new JSONObject(NativeBridge.state(session));
        JSONArray events = state.optJSONArray("audio");
        if (events != null) for (int index = 0; index < events.length(); index++) {
            JSONObject event = events.getJSONObject(index);
            String operation = event.getString("op");
            if (operation.equals("load")) {
                require(event.has("slot") && event.has("generation"), "audio load lacks slot/generation");
                require(new File(event.getString("path")).isFile(), "audio event points to missing Ogg cache");
                loads++;
            } else if (operation.equals("play")) {
                require(event.has("slot") && event.has("generation"), "audio play lacks slot/generation");
                plays++;
            }
        }
        return state;
    }

    private void renderVisible() throws IOException {
        NativeBridge.render(session, pixels);
        int visible = 0;
        for (int pixel : pixels) if ((pixel & 0x00ffffff) != 0) visible++;
        require(visible > 0, "native framebuffer is entirely black");
        frames++;
    }

    private String required(String key) throws IOException {
        String value = arguments.getString(key, "");
        require(!value.isEmpty(), "missing instrumentation argument " + key);
        return value;
    }

    private static void require(boolean condition, String message) throws IOException {
        if (!condition) throw new IOException(message);
    }
}
