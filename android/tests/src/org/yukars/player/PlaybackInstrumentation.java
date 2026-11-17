// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

import android.app.Activity;
import android.app.Instrumentation;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.os.Bundle;
import android.os.SystemClock;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import org.json.JSONArray;
import org.json.JSONObject;

/** Real-game JNI regression for reading modes, original read records, and history UI. */
public final class PlaybackInstrumentation extends Instrumentation {
    private Bundle arguments;
    private long session;
    private String stage = "create";
    private final Map<Integer, Long> voices = new HashMap<>();

    @Override public void onCreate(Bundle arguments) {
        super.onCreate(arguments);
        this.arguments = arguments == null ? new Bundle() : new Bundle(arguments);
        start();
    }

    @Override public void onStart() { new Thread(this::runTests, "Yuka playback tests").start(); }

    private void runTests() {
        Bundle result = new Bundle();
        JSONObject summary = new JSONObject();
        File scratch = null;
        int code = Activity.RESULT_CANCELED;
        try {
            String game = arguments.getString("game", "");
            require(!game.isEmpty(), "missing game argument");
            scratch = new File(getTargetContext().getCacheDir(), "playback-" + UUID.randomUUID());
            File saves = new File(scratch, "Save");
            File audio = new File(scratch, "audio");
            GameImporter.mkdir(saves); GameImporter.mkdir(audio);
            summary.put("scratch", scratch.getAbsolutePath());
            session = NativeBridge.create(game, saves.getAbsolutePath(), audio.getAbsolutePath());
            require(session != 0, "null native session");
            JSONObject title = state();
            require(title.optBoolean("title") && title.optInt("history_count") == 0,
                "title executed unseen story or added dialogue history");
            NativeBridge.command(session, "new", "");
            JSONObject current = stable();
            require(!current.optBoolean("current_read"), "first new-game dialogue was incorrectly marked already read");
            String first = current.optString("dialogue");
            require(!first.isEmpty(), "first dialogue is empty");
            File checkpoint = new File(saves, "playback.savedata.002");
            NativeBridge.command(session, "save", checkpoint.getAbsolutePath());

            stage = "skip_stops_unread";
            NativeBridge.command(session, "playback", "skip_read");
            current = manual(3000);
            require(first.equals(current.optString("dialogue")), "read-only skip advanced unseen text");
            summary.put("skip_stops_unread", true);

            stage = "skip_known_then_stops_first_unread";
            List<String> read = new ArrayList<>();
            read.add(first);
            for (int index = 0; index < 4; index++) {
                NativeBridge.command(session, "advance", "");
                current = stable();
                read.add(current.optString("dialogue"));
            }
            NativeBridge.command(session, "load", checkpoint.getAbsolutePath());
            current = stable();
            require(current.optBoolean("current_read"), "load lost the visited-page read marker");
            require(current.optInt("history_count") == 1, "load retained history from an unrelated timeline");
            NativeBridge.command(session, "playback", "skip_read");
            current = manual(10000);
            require(!current.optBoolean("current_read") && !read.contains(current.optString("dialogue")),
                "read skip failed to stop on the first unseen page");
            require(current.optInt("history_count") == 6, "read skip skipped or duplicated a history boundary");
            summary.put("skip_known_then_stops_first_unread", true);

            stage = "read_records_survive_session_restart";
            NativeBridge.command(session, "sync_globals", "");
            require(new File(saves, "Already.dat").length() > 4, "original Already.dat was not persisted");
            NativeBridge.destroy(session); session = 0; voices.clear();
            session = NativeBridge.create(game, saves.getAbsolutePath(), audio.getAbsolutePath());
            NativeBridge.command(session, "load", checkpoint.getAbsolutePath());
            current = stable();
            require(current.optBoolean("current_read") && first.equals(current.optString("dialogue")),
                "persisted original read records did not restore after creating a new session");
            summary.put("read_records_survive_session_restart", true);

            stage = "skip_all_and_manual_cancel";
            int initialCount = current.optInt("history_count");
            NativeBridge.command(session, "playback", "skip_all");
            long deadline = SystemClock.elapsedRealtime() + 10000;
            do {
                NativeBridge.tick(session); current = state();
                Thread.sleep(8);
            } while (current.optInt("history_count") < initialCount + 3 && SystemClock.elapsedRealtime() < deadline);
            require(current.optInt("history_count") >= initialCount + 3, "all-text skip did not advance multiple pages");
            NativeBridge.command(session, "playback", "manual");
            current = stable();
            String cancelled = current.optString("dialogue");
            tickFor(200);
            require(cancelled.equals(state().optString("dialogue")), "manual cancellation allowed another skip page");
            summary.put("skip_all_and_manual_cancel", true);

            stage = "find_voiced_dialogue";
            for (int index = 0; index < 80 && !current.optBoolean("voice_playing"); index++) {
                NativeBridge.command(session, "advance", "");
                current = stable();
            }
            require(current.optBoolean("voice_playing") && !voices.isEmpty(), "opening path supplied no voiced dialogue");
            String voicedText = current.optString("dialogue");
            int voicedCount = current.optInt("history_count");

            stage = "history_voice_is_independent";
            NativeBridge.command(session, "pause", "1");
            state(); // Drain the pause event, which belongs to the game audio rather than history replay.
            int[] before = new int[800 * 600];
            int[] after = new int[before.length];
            NativeBridge.render(session, before);
            JSONArray history = new JSONArray(NativeBridge.history(session));
            require(history.length() == voicedCount, "JNI history count differs from state");
            JSONObject last = history.getJSONObject(history.length() - 1);
            require(voicedText.equals(last.optString("dialogue")), "latest history omitted current dialogue");
            JSONArray files = new JSONArray(NativeBridge.historyVoice(session, last.getLong("id")));
            require(files.length() > 0, "voiced history exported no audio");
            for (int index = 0; index < files.length(); index++) {
                File file = new File(files.getString(index));
                byte[] header = new byte[4];
                try (FileInputStream input = new FileInputStream(file)) {
                    require(input.read(header) == 4 && Arrays.equals(header, new byte[]{'O', 'g', 'g', 'S'}),
                        "history audio is not an original Ogg resource");
                }
            }
            NativeBridge.render(session, after);
            require(Arrays.equals(before, after), "reading history altered the current scene");
            current = state();
            require(voicedText.equals(current.optString("dialogue")) && current.optInt("history_count") == voicedCount,
                "history replay changed script progress");
            require(current.getJSONArray("audio").length() == 0, "history replay modified the main audio channels");
            summary.put("history_voice_is_independent", true);

            NativeBridge.command(session, "pause", "0");

            stage = "auto_waits_for_voice_completion";
            NativeBridge.command(session, "playback", "auto");
            long textDelay = 900 + Math.min(220, voicedText.codePointCount(0, voicedText.length())) * 50L;
            tickFor(textDelay + 450);
            current = state();
            require(voicedText.equals(current.optString("dialogue")) && current.optBoolean("voice_playing"),
                "auto advanced while the voice had no completion callback");
            // A late callback from a replaced player must also be ignored.
            Map.Entry<Integer, Long> active = voices.entrySet().iterator().next();
            NativeBridge.command(session, "audio_finished", active.getKey() + "," + (active.getValue() - 1));
            tickFor(100);
            require(voicedText.equals(state().optString("dialogue")), "stale voice completion released auto reading");
            for (Map.Entry<Integer, Long> voice : new HashMap<>(voices).entrySet())
                NativeBridge.command(session, "audio_finished", voice.getKey() + "," + voice.getValue());
            voices.clear();
            tickFor(180);
            require(voicedText.equals(state().optString("dialogue")), "auto omitted the post-voice delay");
            deadline = SystemClock.elapsedRealtime() + 15000;
            do {
                NativeBridge.tick(session); current = state();
                Thread.sleep(16);
            } while (current.optInt("history_count") == voicedCount && SystemClock.elapsedRealtime() < deadline);
            require(current.optInt("history_count") > voicedCount, "auto failed to advance after voice completion");
            NativeBridge.command(session, "playback", "manual");
            summary.put("auto_waits_for_voice_completion", true);

            stage = "history_canvas_scrolling_and_buttons";
            NativeBridge.command(session, "pause", "1");
            OriginalAssets assets = OriginalAssets.load(session);
            checkHistoryMenu(assets, scratch);
            summary.put("history_canvas_scrolling_and_buttons", true);
            NativeBridge.command(session, "pause", "0");

            stage = "choice_never_auto_selects";
            String original = arguments.getString("originalSave", "");
            require(!original.isEmpty() && new File(original).isFile(), "originalSave must identify a real PC choice slot");
            byte[] originalBytes = Files.readAllBytes(new File(original).toPath());
            NativeBridge.command(session, "load", original);
            current = stable();
            String choices = current.getJSONArray("choices").toString();
            require(current.getJSONArray("choices").length() > 0, "provided PC slot is not a pending choice");
            for (String mode : new String[]{"auto", "skip_read", "skip_all"}) {
                NativeBridge.command(session, "playback", mode);
                tickFor(150);
                current = state();
                require(current.optString("playback").equals("manual") && choices.equals(current.getJSONArray("choices").toString()),
                    mode + " advanced a pending choice");
            }
            require(Arrays.equals(originalBytes, Files.readAllBytes(new File(original).toPath())), "test mutated the PC choice input");
            summary.put("choice_never_auto_selects", true);
            summary.put("passed", true);
            stage = "complete";
            code = Activity.RESULT_OK;
        } catch (Throwable error) {
            try { summary.put("passed", false).put("stage", stage).put("error", error.toString()); }
            catch (Exception ignored) { }
            result.putString("failure", stage + " - " + error);
        } finally {
            if (session != 0) {
                try { NativeBridge.destroy(session); } catch (Throwable ignored) { }
            }
            String report = summary.toString();
            result.putString("summary", report);
            if (scratch != null) {
                try { Files.write(new File(scratch, "result.json").toPath(), report.getBytes(StandardCharsets.UTF_8)); }
                catch (IOException error) { result.putString("report_error", error.toString()); }
            }
            finish(code, result);
        }
    }

    private JSONObject state() throws Exception {
        JSONObject result = new JSONObject(NativeBridge.state(session));
        JSONArray audio = result.optJSONArray("audio");
        if (audio != null) for (int index = 0; index < audio.length(); index++) {
            JSONObject event = audio.getJSONObject(index);
            int slot = event.getInt("slot");
            if (slot < 10 || slot > 21) continue;
            String operation = event.optString("op");
            if (operation.equals("play")) voices.put(slot, event.getLong("generation"));
            else if (operation.equals("stop") || operation.equals("delete")) voices.remove(slot);
        }
        return result;
    }

    private JSONObject stable() throws Exception {
        long deadline = SystemClock.elapsedRealtime() + 30000;
        do {
            NativeBridge.tick(session);
            JSONObject current = state();
            require(!current.optBoolean("finished"), "story ended while waiting for a stable page");
            if (current.optBoolean("can_save")) return current;
            Thread.sleep(16);
        } while (SystemClock.elapsedRealtime() < deadline);
        throw new IOException("stable page timeout");
    }

    private JSONObject manual(long timeout) throws Exception {
        long deadline = SystemClock.elapsedRealtime() + timeout;
        do {
            NativeBridge.tick(session);
            JSONObject current = state();
            if (current.optString("playback").equals("manual") && current.optBoolean("waiting_key")) return current;
            Thread.sleep(8);
        } while (SystemClock.elapsedRealtime() < deadline);
        throw new IOException("reading mode did not stop at a manual page");
    }

    private void tickFor(long milliseconds) throws Exception {
        long deadline = SystemClock.elapsedRealtime() + milliseconds;
        do { NativeBridge.tick(session); state(); Thread.sleep(16); }
        while (SystemClock.elapsedRealtime() < deadline);
    }

    private static void checkHistoryMenu(OriginalAssets assets, File scratch) throws Exception {
        JSONArray fixture = new JSONArray();
        for (int index = 0; index < 18; index++) fixture.put(new JSONObject().put("id", index + 101)
            .put("speaker", "角色" + index).put("dialogue", "这是第" + index + "句对话履历，用于验证原版窗口、滚动与语音按钮。")
            .put("voices", new JSONArray().put("test.ogg")));
        HistoryListener listener = new HistoryListener();
        OriginalHistoryMenu menu = new OriginalHistoryMenu(assets, fixture, listener);
        require(menu.size() == 18 && menu.maximumScroll() > 450, "history text did not lay out as scrollable pages");
        require(menu.scrollPosition() == menu.maximumScroll(), "history did not open at the newest dialogue");
        Bitmap frame = Bitmap.createBitmap(800, 600, Bitmap.Config.ARGB_8888);
        menu.draw(new Canvas(frame), -1, -1);
        require(frame.getPixel(0, 0) == assets.bitmap("his_base", -1).getPixel(0, 0), "history did not use original background");
        try (FileOutputStream output = new FileOutputStream(new File(scratch, "history-menu.png"))) {
            require(frame.compress(Bitmap.CompressFormat.PNG, 100, output), "history PNG encoding failed");
        }
        menu.tap(777, 80);
        require(menu.scrollPosition() < menu.maximumScroll(), "history up button missed");
        menu.tap(777, 560);
        require(menu.scrollPosition() == menu.maximumScroll(), "history down button missed");
        require(menu.begin(770, 130), "scrollbar drag target missed");
        require(menu.scrollPosition() == 0, "scrollbar did not reach oldest dialogue");
        menu.drag(770, 501, 0); menu.end();
        require(menu.scrollPosition() == menu.maximumScroll(), "scrollbar did not reach newest dialogue");
        menu.move(-Float.MAX_VALUE);
        require(menu.scrollPosition() == 0, "negative history scroll escaped bounds");
        menu.tap(32, 80);
        require(listener.voice == 101, "history voice button selected the wrong entry");
        require(!menu.begin(400, 200), "text drag was confused with scrollbar drag");
        menu.drag(400, 100, -100); menu.end();
        require(menu.scrollPosition() == 100, "text swipe did not scroll by gesture distance");
        menu.move(Float.MAX_VALUE);
        require(menu.scrollPosition() == menu.maximumScroll(), "positive history scroll escaped bounds");
        menu.tap(777, 35);
        require(listener.closed == 1, "history close button missed");
        OriginalHistoryMenu empty = new OriginalHistoryMenu(assets, new JSONArray(), listener);
        empty.draw(new Canvas(frame), -1, -1);
        empty.move(100);
        require(empty.size() == 0 && empty.scrollPosition() == 0, "empty history was not safe to render or scroll");
    }

    private static final class HistoryListener implements OriginalHistoryMenu.Listener {
        long voice = -1;
        int closed;
        @Override public void close() { closed++; }
        @Override public void voice(long id) { voice = id; }
    }

    private static void require(boolean condition, String message) throws IOException {
        if (!condition) throw new IOException(message);
    }
}
