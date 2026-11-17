// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

import android.app.Activity;
import android.app.Instrumentation;
import android.content.Intent;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.graphics.Rect;
import android.os.Bundle;
import android.os.SystemClock;
import android.view.MotionEvent;
import android.view.InputDevice;
import android.view.View;
import android.view.accessibility.AccessibilityNodeInfo;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.lang.reflect.Field;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.List;
import java.util.UUID;
import org.json.JSONObject;

/** Taps the real Activity controls; JNI state is observed through the rendered UI snapshot. */
public final class ActivityControlsInstrumentation extends Instrumentation {
    private MainActivity activity;
    private View gameView;
    private String stage = "launch";

    @Override public void onCreate(Bundle arguments) { super.onCreate(arguments); start(); }
    @Override public void onStart() { new Thread(this::runTests, "Yuka Activity controls tests").start(); }

    private void runTests() {
        Bundle result = new Bundle();
        JSONObject summary = new JSONObject();
        File scratch = new File(getTargetContext().getCacheDir(), "activity-controls-" + UUID.randomUUID());
        int code = Activity.RESULT_CANCELED;
        try {
            GameImporter.mkdir(scratch);
            getUiAutomation(); // Connect before timing any dialog accessibility query.
            Intent intent = new Intent(Intent.ACTION_MAIN).setClassName(getTargetContext(), "org.yukars.player.MainActivity")
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_CLEAR_TASK);
            activity = (MainActivity) startActivitySync(intent);
            gameView = (View) field(activity, "gameView");
            await(() -> Boolean.TRUE.equals(field(activity, "nativeReady"))
                && !Boolean.TRUE.equals(field(activity, "busy")), 30000, "bundled game did not reach its title");
            require(snapshot().optBoolean("title"), "Activity did not show the title");
            tap(390, 296);
            await(() -> Boolean.TRUE.equals(field(activity, "nameEntry")), 3000, "new game did not open original name input");
            tap(280, 410);
            await(() -> snapshot().optBoolean("waiting_key") && snapshot().optBoolean("can_save")
                && "PLAYING".equals(String.valueOf(field(activity, "mode"))), 30000, "new-game title button did not start dialogue");

            stage = "auto_button_and_scene_cancel";
            tap(780, 530);
            await(() -> snapshot().optString("playback").equals("auto"), 3000, "original auto button did not enable auto reading");
            tap(400, 300);
            await(() -> snapshot().optString("playback").equals("manual"), 3000, "scene tap did not cancel auto reading");
            summary.put("auto_button_and_scene_cancel", true);

            stage = "all_text_skip_dialog_and_button";
            int before = snapshot().optInt("history_count");
            hold(780, 553);
            await(() -> clickText("全部快进（包括未读对话）"), 3000, "original skip button did not expose its explicit all-text choice");
            await(() -> snapshot().optInt("history_count") >= before + 3, 10000, "all-text skip did not advance several real pages");
            tap(400, 300);
            await(() -> snapshot().optString("playback").equals("manual") && snapshot().optBoolean("can_save"),
                10000, "scene tap did not stop fast-forward");
            summary.put("all_text_skip_dialog_and_button", true);

            stage = "original_history_button_and_return";
            String dialogue = snapshot().optString("dialogue");
            int count = snapshot().optInt("history_count");
            tap(780, 576);
            await(() -> field(activity, "historyMenu") != null && snapshot().optBoolean("paused"),
                3000, "original history button did not open and pause the game");
            Thread.sleep(350);
            require(dialogue.equals(snapshot().optString("dialogue")) && count == snapshot().optInt("history_count"),
                "history overlay advanced the paused dialogue");
            screenshot(new File(scratch, "activity-history.png"));
            tap(777, 35);
            await(() -> field(activity, "historyMenu") == null && !snapshot().optBoolean("paused")
                && "PLAYING".equals(String.valueOf(field(activity, "mode"))), 3000, "history return did not resume gameplay");
            require(dialogue.equals(snapshot().optString("dialogue")), "closing history changed the current dialogue");
            summary.put("original_history_button_and_return", true);

            stage = "background_resume_stops_auto";
            tap(780, 530);
            await(() -> snapshot().optString("playback").equals("auto"), 3000, "auto could not restart before lifecycle test");
            runOnMainSync(() -> activity.moveTaskToBack(true));
            await(() -> Boolean.TRUE.equals(field(activity, "background")), 3000, "Activity did not receive a real background pause");
            Thread.sleep(350);
            getTargetContext().startActivity(new Intent(Intent.ACTION_MAIN).setClassName(getTargetContext(), "org.yukars.player.MainActivity")
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_REORDER_TO_FRONT));
            await(() -> !Boolean.TRUE.equals(field(activity, "background"))
                && snapshot().optString("playback").equals("manual") && !snapshot().optBoolean("paused"),
                5000, "resuming the Activity left auto reading or pause active");
            screenshot(new File(scratch, "activity-reading-controls.png"));
            summary.put("background_resume_stops_auto", true);
            summary.put("scratch", scratch.getAbsolutePath()).put("passed", true);
            code = Activity.RESULT_OK;
            stage = "complete";
        } catch (Throwable error) {
            try {
                summary.put("passed", false).put("stage", stage).put("error", error.toString());
                AccessibilityNodeInfo root = getUiAutomation().getRootInActiveWindow();
                if (root != null) summary.put("window", root.toString());
            }
            catch (Exception ignored) { }
            result.putString("failure", stage + " - " + error);
        } finally {
            if (activity != null) runOnMainSync(() -> activity.finish());
            String report = summary.toString();
            result.putString("summary", report);
            try { Files.write(new File(scratch, "result.json").toPath(), report.getBytes(StandardCharsets.UTF_8)); }
            catch (IOException error) { result.putString("report_error", error.toString()); }
            finish(code, result);
        }
    }

    private static Object field(Object owner, String name) throws Exception {
        Field field = owner.getClass().getDeclaredField(name);
        field.setAccessible(true);
        return field.get(owner);
    }

    private JSONObject snapshot() throws Exception {
        synchronized (field(gameView, "frameLock")) {
            return new JSONObject(field(gameView, "state").toString());
        }
    }

    private void tap(float x, float y) {
        waitForIdleSync();
        SystemClock.sleep(80); // Let the next frame rebuild the current screen's hit targets.
        runOnMainSync(() -> {
            float scale = Math.min(gameView.getWidth() / 800f, gameView.getHeight() / 600f);
            float screenX = (gameView.getWidth() - scale * 800) / 2 + x * scale;
            float screenY = (gameView.getHeight() - scale * 600) / 2 + y * scale;
            long now = SystemClock.uptimeMillis();
            MotionEvent down = MotionEvent.obtain(now, now, MotionEvent.ACTION_DOWN, screenX, screenY, 0);
            MotionEvent up = MotionEvent.obtain(now, now + 30, MotionEvent.ACTION_UP, screenX, screenY, 0);
            try { gameView.dispatchTouchEvent(down); gameView.dispatchTouchEvent(up); }
            finally { down.recycle(); up.recycle(); }
        });
    }

    private void hold(float x, float y) throws InterruptedException {
        long now = SystemClock.uptimeMillis();
        runOnMainSync(() -> touch(x, y, MotionEvent.ACTION_DOWN, now));
        Thread.sleep(700);
        runOnMainSync(() -> touch(x, y, MotionEvent.ACTION_UP, now));
    }

    private void touch(float x, float y, int action, long downTime) {
        float scale = Math.min(gameView.getWidth() / 800f, gameView.getHeight() / 600f);
        MotionEvent event = MotionEvent.obtain(downTime, SystemClock.uptimeMillis(), action,
            (gameView.getWidth() - scale * 800) / 2 + x * scale,
            (gameView.getHeight() - scale * 600) / 2 + y * scale, 0);
        try { gameView.dispatchTouchEvent(event); } finally { event.recycle(); }
    }

    private boolean clickText(String text) {
        AccessibilityNodeInfo root = getUiAutomation().getRootInActiveWindow();
        if (root == null) return false;
        List<AccessibilityNodeInfo> nodes = root.findAccessibilityNodeInfosByText(text);
        for (AccessibilityNodeInfo node : nodes) {
            if (!text.contentEquals(node.getText())) continue;
            Rect bounds = new Rect();
            node.getBoundsInScreen(bounds);
            long now = SystemClock.uptimeMillis();
            MotionEvent down = MotionEvent.obtain(now, now, MotionEvent.ACTION_DOWN, bounds.centerX(), bounds.centerY(), 0);
            MotionEvent up = MotionEvent.obtain(now, now + 30, MotionEvent.ACTION_UP, bounds.centerX(), bounds.centerY(), 0);
            down.setSource(InputDevice.SOURCE_TOUCHSCREEN);
            up.setSource(InputDevice.SOURCE_TOUCHSCREEN);
            try { return getUiAutomation().injectInputEvent(down, true) && getUiAutomation().injectInputEvent(up, true); }
            finally { down.recycle(); up.recycle(); }
        }
        return false;
    }

    private void screenshot(File destination) throws IOException {
        Bitmap image = Bitmap.createBitmap(800, 600, Bitmap.Config.ARGB_8888);
        runOnMainSync(() -> {
            Canvas canvas = new Canvas(image);
            float scale = Math.min(gameView.getWidth() / 800f, gameView.getHeight() / 600f);
            canvas.scale(1 / scale, 1 / scale);
            canvas.translate(-(gameView.getWidth() - 800 * scale) / 2, -(gameView.getHeight() - 600 * scale) / 2);
            gameView.draw(canvas);
        });
        try (FileOutputStream output = new FileOutputStream(destination)) {
            require(image.compress(Bitmap.CompressFormat.PNG, 100, output), "Activity screenshot encoding failed");
        } finally { image.recycle(); }
    }

    private interface Condition { boolean ready() throws Exception; }
    private static void await(Condition condition, long timeout, String message) throws Exception {
        long deadline = SystemClock.elapsedRealtime() + timeout;
        do { if (condition.ready()) return; Thread.sleep(25); }
        while (SystemClock.elapsedRealtime() < deadline);
        throw new IOException(message);
    }

    private static void require(boolean condition, String message) throws IOException {
        if (!condition) throw new IOException(message);
    }
}
