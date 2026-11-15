// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

import android.app.Activity;
import android.app.Instrumentation;
import android.content.Intent;
import android.media.AudioManager;
import android.os.Bundle;
import android.os.Handler;
import android.os.HandlerThread;
import android.os.SystemClock;
import java.io.File;
import java.io.IOException;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.nio.file.Files;
import java.util.UUID;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;
import org.json.JSONArray;
import org.json.JSONObject;

/** Runs separately from the headless JNI smoke because Android 15 requires a foreground Activity. */
public final class AudioFocusInstrumentation extends Instrumentation {
    private Bundle arguments;
    private HandlerThread audioThread;
    private Handler handler;
    private AudioHost host;
    private final AtomicReference<String> audioError = new AtomicReference<>();
    private interface AudioAction { void run() throws Exception; }

    @Override public void onCreate(Bundle arguments) {
        super.onCreate(arguments);
        this.arguments = arguments == null ? new Bundle() : new Bundle(arguments);
        start();
    }

    @Override public void onStart() { new Thread(this::runTest, "Yuka focus regression").start(); }

    private void runTest() {
        Bundle result = new Bundle();
        Activity activity = null;
        File sample = null;
        String stage = "foreground_activity";
        int code = Activity.RESULT_CANCELED;
        try {
            Intent intent = new Intent(getTargetContext(), MainActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            activity = startActivitySync(intent);
            long deadline = SystemClock.elapsedRealtime() + 10000;
            while (!activity.hasWindowFocus() && SystemClock.elapsedRealtime() < deadline) Thread.sleep(25);
            require(activity.hasWindowFocus(), "player Activity did not become foreground");
            stage = "audio_sample";
            String directory = arguments.getString("audio", "");
            require(!directory.isEmpty(), "missing audio cache argument");
            File[] samples = new File(directory).listFiles(file -> file.isFile() && file.getName().endsWith(".ogg"));
            require(samples != null && samples.length > 0, "run JNI smoke first to create an Ogg cache sample");
            sample = new File(getTargetContext().getCacheDir(), "focus-test-" + UUID.randomUUID() + ".ogg");
            Files.copy(samples[0].toPath(), sample.toPath());
            final String samplePath = sample.getAbsolutePath();
            audioThread = new HandlerThread("Yuka focus audio");
            audioThread.start();
            handler = new Handler(audioThread.getLooper());
            stage = "initial_focus";
            onAudio(() -> {
                host = new AudioHost(getTargetContext(), handler, (operation, argument) -> {
                    if (operation.equals("audio_error")) audioError.set(argument);
                });
                JSONArray events = new JSONArray();
                events.put(new JSONObject().put("op", "load").put("slot", 9).put("generation", 1).put("path", samplePath));
                events.put(new JSONObject().put("op", "play").put("slot", 9).put("generation", 1).put("loop", true).put("volume", 0));
                host.apply(events);
                require(ownsFocus(), "foreground player did not obtain initial audio focus");
            });
            stage = "resume_engine_after_focus_loss";
            onAudio(() -> {
                loseFocus();
                host.setLifecyclePaused(true);
                host.apply(new JSONArray().put(new JSONObject().put("op", "pause_all")));
                host.setLifecyclePaused(false);
                require(!ownsFocus(), "engine pause should keep audio focus suspended");
                host.apply(new JSONArray().put(new JSONObject().put("op", "resume_all")));
                require(ownsFocus(), "resume_all did not reacquire focus for the existing sound");
            });
            stage = "resume_lifecycle_after_focus_loss";
            onAudio(() -> {
                loseFocus();
                host.setLifecyclePaused(true);
                host.setLifecyclePaused(false);
                require(ownsFocus(), "foreground lifecycle resume did not reacquire focus");
            });
            require(audioError.get() == null, "MediaPlayer failure: " + audioError.get());
            result.putString("summary", "{\"passed\":true,\"audio_focus_resume\":true,\"paused_engine_guard\":true}");
            code = Activity.RESULT_OK;
        } catch (Throwable error) {
            result.putString("failure", stage + " - " + error);
        } finally {
            try { if (handler != null) onAudio(() -> { if (host != null) host.close(); }); }
            catch (Exception error) { result.putString("cleanup_error", error.toString()); }
            if (audioThread != null) audioThread.quitSafely();
            if (sample != null) sample.delete();
            if (activity != null) {
                final Activity started = activity;
                runOnMainSync(started::finish);
            }
            finish(code, result);
        }
    }

    private void loseFocus() throws Exception {
        Method callback = AudioHost.class.getDeclaredMethod("onFocusChanged", int.class);
        callback.setAccessible(true);
        callback.invoke(host, AudioManager.AUDIOFOCUS_LOSS);
        require(!ownsFocus(), "permanent focus loss did not clear ownership");
    }

    private boolean ownsFocus() throws Exception {
        Field field = AudioHost.class.getDeclaredField("ownsFocus");
        field.setAccessible(true);
        return field.getBoolean(host);
    }

    private void onAudio(AudioAction action) throws Exception {
        CountDownLatch complete = new CountDownLatch(1);
        AtomicReference<Throwable> failure = new AtomicReference<>();
        handler.post(() -> {
            try { action.run(); }
            catch (Throwable error) { failure.set(error); }
            finally { complete.countDown(); }
        });
        require(complete.await(10, TimeUnit.SECONDS), "audio handler timed out");
        if (failure.get() != null) throw new IOException("audio handler failed: " + failure.get(), failure.get());
    }

    private static void require(boolean condition, String message) throws IOException {
        if (!condition) throw new IOException(message);
    }
}
