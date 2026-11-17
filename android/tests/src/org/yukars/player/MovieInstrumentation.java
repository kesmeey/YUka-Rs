// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

import android.app.Activity;
import android.app.Instrumentation;
import android.content.Intent;
import android.graphics.Bitmap;
import android.media.MediaPlayer;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import android.view.PixelCopy;
import android.view.SurfaceView;
import android.view.ViewGroup;
import android.widget.FrameLayout;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.UUID;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.atomic.AtomicReference;
import org.json.JSONObject;

/** Full-APK Android decoder/surface lifecycle regression; does not run or skip story scripts. */
public final class MovieInstrumentation extends Instrumentation {
    private String stage = "launch";
    private Activity activity;
    private AndroidMovieHost movie;
    @Override public void onCreate(Bundle arguments) { super.onCreate(arguments); start(); }
    @Override public void onStart() { new Thread(this::runTests, "Yuka video tests").start(); }

    private void runTests() {
        Bundle result = new Bundle();
        JSONObject summary = new JSONObject();
        File scratch = new File(getTargetContext().getCacheDir(), "movie-" + UUID.randomUUID());
        int code = Activity.RESULT_CANCELED;
        AtomicLong completed = new AtomicLong();
        AtomicInteger completions = new AtomicInteger();
        AtomicLong failed = new AtomicLong();
        AtomicReference<String> failure = new AtomicReference<>();
        try {
            GameImporter.mkdir(scratch);
            activity = startActivitySync(new Intent(Intent.ACTION_MAIN).setClassName(getTargetContext(), "org.yukars.player.MainActivity")
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_CLEAR_TASK));
            runOnMainSync(() -> {
                movie = new AndroidMovieHost(activity, new AndroidMovieHost.Listener() {
                    public void finished(long id) { completed.set(id); completions.incrementAndGet(); }
                    public void error(long id, String message) { failed.set(id); failure.set(message); }
                });
                ((FrameLayout) activity.findViewById(android.R.id.content)).addView(movie,
                    new FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
            });
            stage = "apk_descriptor_playback";
            JSONObject first = new JSONObject("{\"id\":1,\"resource\":\"avi\\\\lave_op.mpg\",\"volume\":0}");
            runOnMainSync(() -> movie.apply(first));
            await(() -> failed.get() != 0 || playing(), 15000, "H.264/AAC video did not start: " + failure.get());
            require(failed.get() == 0 && movie.active(), "movie failed: " + failure.get());
            MediaPlayer player = (MediaPlayer) field(movie, "player");
            require(player.getVideoWidth() == 800 && player.getVideoHeight() == 600, "video dimensions are not original 800x600");
            require(player.getDuration() >= 94000 && player.getDuration() <= 97000, "OP duration changed unexpectedly");
            await(() -> ((MediaPlayer) field(movie, "player")).getCurrentPosition() > 200, 5000, "video clock did not advance");
            summary.put("apk_descriptor_playback", true);

            stage = "pause_resume";
            runOnMainSync(movie::pause);
            int position = player.getCurrentPosition();
            Thread.sleep(350);
            require(!player.isPlaying() && Math.abs(player.getCurrentPosition() - position) < 150, "paused video kept advancing");
            runOnMainSync(movie::resume);
            await(() -> playing() && ((MediaPlayer) field(movie, "player")).getCurrentPosition() > position + 150,
                5000, "video did not resume at its preserved position");
            summary.put("pause_resume", true);

            stage = "decoded_frame";
            runOnMainSync(() -> player.seekTo(10000));
            Thread.sleep(650);
            Bitmap image = Bitmap.createBitmap(800, 600, Bitmap.Config.ARGB_8888);
            CountDownLatch copied = new CountDownLatch(1);
            AtomicInteger copyResult = new AtomicInteger(-1);
            runOnMainSync(() -> PixelCopy.request((SurfaceView) uncheckedField(movie, "surface"), image,
                status -> { copyResult.set(status); copied.countDown(); }, new Handler(Looper.getMainLooper())));
            require(copied.await(5, TimeUnit.SECONDS) && copyResult.get() == PixelCopy.SUCCESS, "decoded video frame could not be copied");
            boolean varied = false;
            int center = image.getPixel(400, 300);
            for (int y = 40; y < 600 && !varied; y += 40)
                for (int x = 40; x < 800; x += 40) if (image.getPixel(x, y) != center) { varied = true; break; }
            require(varied, "decoded frame is an empty uniform surface");
            try (FileOutputStream output = new FileOutputStream(new File(scratch, "movie-frame.png"))) {
                require(image.compress(Bitmap.CompressFormat.PNG, 100, output), "frame PNG encoding failed");
            } finally { image.recycle(); }
            summary.put("decoded_frame", true);

            stage = "skip_and_no_duplicate_restart";
            runOnMainSync(movie::skip);
            require(completed.get() == 1 && !movie.active() && completions.get() == 1, "skip did not complete exactly once");
            runOnMainSync(() -> movie.apply(first));
            Thread.sleep(200);
            require(!movie.active() && field(movie, "player") == null && completions.get() == 1,
                "old runtime movie state restarted an already completed video");
            summary.put("skip_and_no_duplicate_restart", true);

            stage = "natural_completion";
            runOnMainSync(() -> { movie.apply(null); movie.apply(movieState(2)); });
            await(() -> failed.get() != 0 || playing(), 15000, "second video did not start");
            require(failed.get() == 0, "second video failed: " + failure.get());
            MediaPlayer second = (MediaPlayer) field(movie, "player");
            runOnMainSync(() -> second.seekTo(Math.max(0, second.getDuration() - 700)));
            await(() -> completed.get() == 2, 15000, "natural OP completion was not reported");
            require(!movie.active() && completions.get() == 2, "completion duplicated callback or retained movie");
            summary.put("natural_completion", true);

            stage = "missing_movie_error";
            JSONObject invalid = new JSONObject("{\"id\":3,\"resource\":\"avi\\\\missing.mpg\",\"volume\":0}");
            runOnMainSync(() -> movie.apply(invalid));
            await(() -> failed.get() == 3, 5000, "missing video did not report its request ID");
            require(!movie.active() && completions.get() == 2, "missing video incorrectly completed as successful playback");
            summary.put("missing_movie_error", true);

            stage = "main_activity_movie_round_trip";
            runOnMainSync(movie::close);
            // The GameView is a plain View rather than a ViewGroup; observe its immutable frame state.
            Object mainView = field(activity, "gameView");
            await(() -> Boolean.TRUE.equals(field(activity, "nativeReady")) && !Boolean.TRUE.equals(field(activity, "busy")),
                30000, "MainActivity did not finish initialization");
            JSONObject initial = mainState(mainView);
            require(initial.optBoolean("title") && initial.optInt("history_count") == 0, "movie integration must start from an untouched title");
            AndroidMovieHost mainHost = (AndroidMovieHost) field(activity, "movieHost");
            runOnMainSync(() -> mainCommand("movie_play", "avi/lave_op.mpg"));
            await(() -> hostPlaying(mainHost), 15000, "actual MainActivity movie host did not start OP");
            require(mainState(mainView).optJSONObject("movie") != null, "MainActivity lost native movie wait while playing");
            runOnMainSync(mainHost::skip);
            await(() -> mainState(mainView).optJSONObject("movie") == null && !mainHost.active()
                && ((Long) field(mainHost, "currentId")).longValue() == 0,
                5000, "MainActivity skip callback did not clear the native movie wait");
            require(mainState(mainView).optBoolean("title") && mainState(mainView).optInt("history_count") == 0,
                "returning from menu OP executed hidden story");
            summary.put("main_activity_movie_round_trip", true);

            stage = "main_error_during_pending_command";
            // Give the actual host a live Surface before requesting a missing local resource.
            // This makes openPlayer fail synchronously inside apply(), before command's UI acknowledgement.
            runOnMainSync(() -> mainHost.setVisibility(android.view.View.VISIBLE));
            await(() -> Boolean.TRUE.equals(field(mainHost, "surfaceReady")), 5000, "could not prepare the actual movie Surface");
            AtomicReference<Throwable> pendingFailure = new AtomicReference<>();
            runOnMainSync(() -> {
                try {
                    mainCommand("movie_play", "avi/missing.mpg");
                    CountDownLatch requested = new CountDownLatch(1);
                    AtomicReference<JSONObject> request = new AtomicReference<>();
                    ((Handler) uncheckedField(activity, "engine")).post(() -> {
                        try {
                            JSONObject state = new JSONObject(NativeBridge.state(((Long) uncheckedField(activity, "session")).longValue()));
                            request.set(state.getJSONObject("movie"));
                        } catch (Throwable error) { pendingFailure.set(error); }
                        finally { requested.countDown(); }
                    });
                    require(requested.await(5, TimeUnit.SECONDS), "native menu movie request timed out");
                    require(pendingFailure.get() == null && request.get() != null, "missing movie request was not queued");
                    require(Boolean.TRUE.equals(field(activity, "actionPending")), "test did not reach the in-flight command window");
                    mainHost.apply(request.get());
                    require(!mainHost.active(), "missing resource did not immediately fail on a live Surface");
                } catch (Throwable error) { pendingFailure.set(error); }
            });
            require(pendingFailure.get() == null, "could not exercise pending command error: " + pendingFailure.get());
            await(() -> mainState(mainView).optJSONObject("movie") == null && !Boolean.TRUE.equals(field(activity, "actionPending")),
                5000, "media failure callback was dropped while a UI command was pending");
            require(mainState(mainView).optBoolean("title") && mainState(mainView).optInt("history_count") == 0
                && !Boolean.TRUE.equals(field(activity, "engineFailed")), "failed menu video changed title/story state");
            summary.put("main_error_during_pending_command", true).put("scratch", scratch.getAbsolutePath()).put("passed", true);
            code = Activity.RESULT_OK;
        } catch (Throwable error) {
            try { summary.put("stage", stage).put("error", error.toString()).put("passed", false); }
            catch (Exception ignored) { }
            result.putString("failure", stage + " - " + error);
        } finally {
            if (activity != null) runOnMainSync(() -> { if (movie != null) movie.close(); activity.finish(); });
            String text = summary.toString(); result.putString("summary", text);
            try { Files.write(new File(scratch, "result.json").toPath(), text.getBytes(StandardCharsets.UTF_8)); }
            catch (IOException error) { result.putString("report_error", error.toString()); }
            finish(code, result);
        }
    }

    private JSONObject movieState(long id) {
        try { return new JSONObject().put("id", id).put("resource", "avi\\lave_op.mpg").put("volume", 0); }
        catch (Exception error) { throw new IllegalStateException(error); }
    }
    private boolean playing() throws Exception {
        MediaPlayer player = (MediaPlayer) field(movie, "player");
        return player != null && Boolean.TRUE.equals(field(movie, "prepared")) && player.isPlaying();
    }
    private static boolean hostPlaying(AndroidMovieHost host) throws Exception {
        MediaPlayer player = (MediaPlayer) field(host, "player");
        return player != null && Boolean.TRUE.equals(field(host, "prepared")) && player.isPlaying();
    }
    private static JSONObject mainState(Object gameView) throws Exception {
        synchronized (field(gameView, "frameLock")) { return new JSONObject(field(gameView, "state").toString()); }
    }
    private void mainCommand(String operation, String argument) {
        try {
            Method method = MainActivity.class.getDeclaredMethod("command", String.class, String.class, Runnable.class);
            method.setAccessible(true); method.invoke(activity, operation, argument, null);
        } catch (Exception error) { throw new IllegalStateException(error); }
    }
    private static Object field(Object owner, String name) throws Exception {
        Field field = owner.getClass().getDeclaredField(name); field.setAccessible(true); return field.get(owner);
    }
    private static Object uncheckedField(Object owner, String name) {
        try { return field(owner, name); } catch (Exception error) { throw new IllegalStateException(error); }
    }
    private interface Condition { boolean ready() throws Exception; }
    private static void await(Condition condition, long timeout, String message) throws Exception {
        long deadline = SystemClock.elapsedRealtime() + timeout;
        do { if (condition.ready()) return; Thread.sleep(25); } while (SystemClock.elapsedRealtime() < deadline);
        throw new IOException(message);
    }
    private static void require(boolean condition, String message) throws IOException { if (!condition) throw new IOException(message); }
}
