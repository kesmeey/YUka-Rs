// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

import android.content.Context;
import android.content.res.AssetFileDescriptor;
import android.graphics.Color;
import android.media.AudioAttributes;
import android.media.AudioFocusRequest;
import android.media.AudioManager;
import android.media.MediaPlayer;
import android.view.Gravity;
import android.view.MotionEvent;
import android.view.SurfaceHolder;
import android.view.SurfaceView;
import android.view.View;
import android.widget.FrameLayout;
import android.widget.TextView;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.HashMap;
import java.util.Locale;
import java.util.Map;
import org.json.JSONArray;
import org.json.JSONObject;

/** UI-thread native Android video output; the APK's STORED MP4 is read by file descriptor. */
final class AndroidMovieHost extends FrameLayout implements SurfaceHolder.Callback {
    interface Listener { void finished(long id); void error(long id, String message); }
    private final Listener listener;
    private final SurfaceView surface;
    private final AudioManager audioManager;
    private final AudioFocusRequest audioFocus;
    private final Map<String, String> assets = new HashMap<>();
    private MediaPlayer player;
    private File gameDirectory;
    private long currentId;
    private String resource = "";
    private String manifestError;
    private boolean complete;
    private boolean prepared;
    private boolean surfaceReady;
    private boolean lifecyclePaused;
    private boolean focusPaused;
    private boolean ownsFocus;
    private float volume = 0.65f;
    private int videoWidth = 800;
    private int videoHeight = 600;

    AndroidMovieHost(Context context, Listener listener) {
        super(context);
        this.listener = listener;
        setBackgroundColor(Color.BLACK);
        setClickable(true);
        setKeepScreenOn(true);
        setContentDescription("游戏视频，点击跳过");
        surface = new SurfaceView(context);
        surface.getHolder().addCallback(this);
        addView(surface, new FrameLayout.LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT, Gravity.CENTER));
        TextView hint = new TextView(context);
        hint.setText("点击跳过"); hint.setTextColor(Color.WHITE); hint.setTextSize(13);
        hint.setPadding(10, 5, 10, 5); hint.setBackgroundColor(0x66000000);
        addView(hint, new FrameLayout.LayoutParams(LayoutParams.WRAP_CONTENT, LayoutParams.WRAP_CONTENT, Gravity.BOTTOM | Gravity.RIGHT));
        audioManager = (AudioManager) context.getSystemService(Context.AUDIO_SERVICE);
        audioFocus = new AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT)
            .setAudioAttributes(attributes()).setOnAudioFocusChangeListener(change -> {
                if (!active()) return;
                if (change == AudioManager.AUDIOFOCUS_GAIN) { ownsFocus = true; focusPaused = false; }
                else if (change == AudioManager.AUDIOFOCUS_LOSS || change == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT) {
                    focusPaused = true;
                    if (change == AudioManager.AUDIOFOCUS_LOSS) ownsFocus = false;
                }
                updatePlayback();
            }).build();
        try { loadManifest(); } catch (Exception error) { manifestError = error.toString(); }
        setVisibility(View.GONE);
    }

    private static AudioAttributes attributes() {
        return new AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_GAME)
            .setContentType(AudioAttributes.CONTENT_TYPE_MOVIE).build();
    }

    void gameDirectory(File directory) { gameDirectory = directory; }
    boolean active() { return currentId > 0 && !complete; }

    void apply(JSONObject movie) {
        if (movie == null || movie.optLong("id") <= 0) { if (currentId != 0) close(); return; }
        long id = movie.optLong("id");
        volume = (float)Math.max(0, Math.min(1, movie.optDouble("volume", 0.65)));
        if (id == currentId) { updatePlayback(); return; }
        releasePlayer();
        currentId = id;
        resource = movie.optString("resource");
        complete = false;
        videoWidth = 800; videoHeight = 600;
        setVisibility(View.VISIBLE);
        updateSurfaceBounds();
        if (manifestError != null) { fail("视频清单无效: " + manifestError); return; }
        if (surfaceReady) openPlayer();
    }

    private void openPlayer() {
        if (!active() || player != null || !surfaceReady) return;
        MediaPlayer candidate = new MediaPlayer();
        player = candidate;
        try {
            candidate.setAudioAttributes(attributes());
            candidate.setDisplay(surface.getHolder());
            candidate.setOnPreparedListener(value -> {
                if (player != value || !active()) return;
                prepared = true;
                videoWidth = Math.max(1, value.getVideoWidth());
                videoHeight = Math.max(1, value.getVideoHeight());
                updateSurfaceBounds();
                updatePlayback();
            });
            candidate.setOnVideoSizeChangedListener((value, width, height) -> {
                if (player != value || width <= 0 || height <= 0) return;
                videoWidth = width; videoHeight = height; updateSurfaceBounds();
            });
            candidate.setOnCompletionListener(value -> { if (player == value) finish(); });
            candidate.setOnErrorListener((value, what, extra) -> {
                if (player == value) fail("无法播放视频 " + resource + " (MediaPlayer " + what + "/" + extra + ")");
                return true;
            });
            String asset = assets.get(normalize(resource));
            if (asset != null) {
                // AssetManager.openFd needs a STORED entry; build.ps1 enforces that.
                try (AssetFileDescriptor descriptor = getContext().getAssets().openFd(asset)) {
                    candidate.setDataSource(descriptor.getFileDescriptor(), descriptor.getStartOffset(), descriptor.getLength());
                }
            } else {
                File file = localMovie(resource);
                try (FileInputStream input = new FileInputStream(file)) { candidate.setDataSource(input.getFD()); }
            }
            candidate.prepareAsync();
        } catch (Exception error) { fail("无法打开视频 " + resource + ": " + error.getMessage()); }
    }

    private File localMovie(String requested) throws IOException {
        if (gameDirectory == null) throw new IOException("游戏目录未设置，且APK不含该视频");
        String normalized = requested.replace('\\', '/');
        if (normalized.startsWith("/") || normalized.contains(":") || normalized.indexOf('\0') >= 0
            || Arrays.asList(normalized.split("/", -1)).stream().anyMatch(part -> part.isEmpty() || part.equals("..") || part.equals(".")))
            throw new IOException("无效的视频资源路径");
        File directory = gameDirectory.getCanonicalFile();
        File original = new File(directory, normalized).getCanonicalFile();
        if (!original.toPath().startsWith(directory.toPath())) throw new IOException("视频不在游戏目录内");
        String base = original.getName().replaceFirst("(?i)\\.(mpg|mpeg|mp4)$", "");
        for (File candidate : new File[]{new File(directory, "movies/" + base + ".mp4"),
                new File(original.getParentFile(), base + ".mp4"), original}) {
            if (candidate.isFile()) return candidate;
        }
        throw new IOException("缺少视频。通用播放器可把 prepare-video.py 生成的 movies 文件夹放进游戏目录");
    }

    private void updatePlayback() {
        if (!active() || !prepared || player == null) return;
        try {
            player.setVolume(volume, volume);
            if (!lifecyclePaused && !focusPaused && surfaceReady) {
                if (!ownsFocus) {
                    ownsFocus = audioManager.requestAudioFocus(audioFocus) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED;
                    if (!ownsFocus) { fail("视频音频焦点不可用"); return; }
                }
                if (!player.isPlaying()) player.start();
            } else if (player.isPlaying()) player.pause();
        } catch (RuntimeException error) { fail("视频播放器状态错误: " + error.getMessage()); }
    }

    void pause() { lifecyclePaused = true; updatePlayback(); }
    void resume() { lifecyclePaused = false; focusPaused = false; updatePlayback(); }
    void skip() { if (active()) finish(); }
    void close() { complete = true; currentId = 0; releasePlayer(); setVisibility(View.GONE); }

    private void finish() {
        if (!active()) return;
        long id = currentId; complete = true;
        releasePlayer(); setVisibility(View.GONE);
        listener.finished(id);
    }
    private void fail(String message) {
        if (!active()) return;
        long id = currentId; complete = true;
        releasePlayer(); setVisibility(View.GONE);
        listener.error(id, message);
    }

    private void releasePlayer() {
        prepared = false;
        if (player != null) {
            player.setOnPreparedListener(null); player.setOnCompletionListener(null);
            player.setOnErrorListener(null); player.setOnVideoSizeChangedListener(null);
            player.release(); player = null;
        }
        if (ownsFocus) { ownsFocus = false; audioManager.abandonAudioFocusRequest(audioFocus); }
        focusPaused = false;
    }

    @Override public void surfaceCreated(SurfaceHolder holder) {
        surfaceReady = true;
        if (player != null) { player.setDisplay(holder); updatePlayback(); }
        else openPlayer();
    }
    @Override public void surfaceChanged(SurfaceHolder holder, int format, int width, int height) { }
    @Override public void surfaceDestroyed(SurfaceHolder holder) {
        surfaceReady = false; updatePlayback();
        if (player != null) player.setDisplay(null);
    }
    @Override protected void onSizeChanged(int width, int height, int oldWidth, int oldHeight) {
        super.onSizeChanged(width, height, oldWidth, oldHeight); updateSurfaceBounds();
    }
    private void updateSurfaceBounds() {
        if (getWidth() <= 0 || getHeight() <= 0) return;
        float scale = Math.min(getWidth() / (float)videoWidth, getHeight() / (float)videoHeight);
        surface.setLayoutParams(new FrameLayout.LayoutParams(Math.max(1, Math.round(videoWidth * scale)),
            Math.max(1, Math.round(videoHeight * scale)), Gravity.CENTER));
    }

    @Override public boolean onTouchEvent(MotionEvent event) {
        if (!active()) return false;
        if (event.getActionMasked() == MotionEvent.ACTION_UP) performClick();
        return true;
    }
    @Override public boolean performClick() { super.performClick(); skip(); return true; }

    private static String normalize(String resource) { return resource.replace('\\', '/').toLowerCase(Locale.ROOT); }
    private void loadManifest() throws Exception {
        String[] names = getContext().getAssets().list("");
        if (names == null || !Arrays.asList(names).contains("movies.json")) return;
        ByteArrayOutputStream text = new ByteArrayOutputStream();
        try (InputStream input = getContext().getAssets().open("movies.json")) {
            byte[] buffer = new byte[1024]; int count;
            while ((count = input.read(buffer)) != -1) {
                if (text.size() + count > 65536) throw new IOException("视频清单过大");
                text.write(buffer, 0, count);
            }
        }
        JSONObject manifest = new JSONObject(new String(text.toByteArray(), StandardCharsets.UTF_8));
        if (manifest.optInt("version") != 1) throw new IOException("不支持的视频清单版本");
        JSONArray movies = manifest.getJSONArray("movies");
        for (int index = 0; index < movies.length(); index++) {
            JSONObject movie = movies.getJSONObject(index);
            String asset = movie.getString("asset");
            if (!asset.matches("movies/[A-Za-z0-9_-]+\\.mp4")) throw new IOException("无效的视频APK路径");
            String key = normalize(movie.getString("resource"));
            if (assets.put(key, asset) != null) throw new IOException("重复的视频资源映射");
        }
    }
}
