// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

import android.content.Context;
import android.media.AudioAttributes;
import android.media.AudioFocusRequest;
import android.media.AudioManager;
import android.media.MediaPlayer;
import android.os.Handler;
import java.io.IOException;
import java.io.File;
import java.io.FileInputStream;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Map;
import java.util.Set;
import org.json.JSONArray;
import org.json.JSONObject;

/** All methods and MediaPlayer callbacks run on the engine HandlerThread. */
final class AudioHost {
    interface Listener { void event(String operation, String argument); }
    private static final class Slot {
        final int id;
        final long generation;
        final MediaPlayer player = new MediaPlayer();
        boolean prepared;
        boolean requested;
        boolean looping;
        boolean restartRequested;
        float volume = 1f;
        Slot(int id, long generation) { this.id = id; this.generation = generation; }
    }

    private final Map<Integer, Slot> slots = new HashMap<>();
    private final Listener listener;
    private final AudioManager manager;
    private final AudioFocusRequest focusRequest;
    private boolean enginePaused;
    private boolean lifecyclePaused;
    private boolean focusPaused;
    private float focusGain = 1f;
    private boolean ownsFocus;

    AudioHost(Context context, Handler handler, Listener listener) {
        this.listener = listener;
        manager = (AudioManager) context.getSystemService(Context.AUDIO_SERVICE);
        AudioAttributes attributes = new AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_GAME)
            .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).build();
        focusRequest = new AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
            .setAudioAttributes(attributes).setAcceptsDelayedFocusGain(true)
            .setOnAudioFocusChangeListener(this::onFocusChanged, handler).build();
    }

    private void onFocusChanged(int change) {
        focusPaused = change == AudioManager.AUDIOFOCUS_LOSS || change == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT;
        focusGain = change == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK ? 0.2f : 1f;
        if (change == AudioManager.AUDIOFOCUS_LOSS) ownsFocus = false;
        if (change == AudioManager.AUDIOFOCUS_GAIN) { ownsFocus = true; focusPaused = false; }
        for (Slot slot : slots.values()) updatePlayback(slot);
    }

    void apply(JSONArray events) {
        if (events == null) return;
        Set<File> consumedFiles = new HashSet<>();
        for (int index = 0; index < events.length(); index++) {
            JSONObject event = events.optJSONObject(index);
            if (event == null) continue;
            int id = event.optInt("slot");
            long generation = event.optLong("generation");
            String operation = event.optString("op");
            try {
                if (operation.equals("pause_all") || operation.equals("resume_all")) {
                    enginePaused = operation.equals("pause_all");
                    restoreFocusIfNeeded();
                    for (Slot slot : slots.values()) updatePlayback(slot);
                    continue;
                }
                if (operation.equals("load")) {
                    String path = event.getString("path");
                    load(id, generation, path);
                    consumedFiles.add(new File(path));
                    continue;
                }
                Slot slot = slots.get(id);
                if (slot == null || slot.generation != generation) continue;
                switch (operation) {
                    case "play":
                        slot.requested = true;
                        slot.restartRequested = true;
                        slot.looping = event.optBoolean("loop");
                        slot.volume = volume(event);
                        if (!ownsFocus) {
                            int result = requestFocus();
                            if (result == AudioManager.AUDIOFOCUS_REQUEST_FAILED) throw new IOException("音频焦点不可用");
                        }
                        updatePlayback(slot);
                        break;
                    case "volume": slot.volume = volume(event); updatePlayback(slot); break;
                    case "stop": slot.requested = false; updatePlayback(slot); break;
                    case "delete": release(id); break;
                    default: throw new IOException("未知音频事件 " + operation);
                }
            } catch (Exception error) {
                release(id);
                listener.event("audio_error", id + "," + generation + "," + error.getMessage());
            }
        }
        // MediaPlayer duplicates the file descriptor. Keep shared paths until every
        // load in this event batch has opened them, then unlink cache files.
        for (File file : consumedFiles) file.delete();
    }

    private static float volume(JSONObject event) {
        return (float) Math.max(0.0, Math.min(1.0, event.optDouble("volume", 1.0)));
    }

    private void load(int id, long generation, String path) throws IOException {
        release(id);
        Slot slot = new Slot(id, generation);
        slots.put(id, slot);
        slot.player.setAudioAttributes(new AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_GAME)
            .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).build());
        slot.player.setOnPreparedListener(player -> {
            if (slots.get(id) != slot) return;
            slot.prepared = true;
            updatePlayback(slot);
        });
        slot.player.setOnCompletionListener(player -> {
            if (slots.get(id) != slot) return;
            slot.requested = false;
            listener.event("audio_finished", id + "," + generation);
        });
        slot.player.setOnErrorListener((player, what, extra) -> {
            if (slots.get(id) == slot) {
                release(id);
                listener.event("audio_error", id + "," + generation + ",MediaPlayer " + what + "/" + extra);
            }
            return true;
        });
        try (FileInputStream file = new FileInputStream(path)) { slot.player.setDataSource(file.getFD()); }
        slot.player.prepareAsync();
    }

    private void updatePlayback(Slot slot) {
        if (!slot.prepared) return;
        try {
            slot.player.setLooping(slot.looping);
            slot.player.setVolume(slot.volume * focusGain, slot.volume * focusGain);
            boolean shouldPlay = slot.requested && !enginePaused && !lifecyclePaused && !focusPaused;
            if (shouldPlay && slot.restartRequested) { slot.player.seekTo(0); slot.restartRequested = false; }
            if (shouldPlay && !slot.player.isPlaying()) slot.player.start();
            else if (!shouldPlay && slot.player.isPlaying()) slot.player.pause();
        } catch (IllegalStateException error) {
            slot.requested = false;
            slot.prepared = false;
            listener.event("audio_error", slot.id + "," + slot.generation + "," + error.getMessage());
        }
    }

    void setLifecyclePaused(boolean paused) {
        lifecyclePaused = paused;
        restoreFocusIfNeeded();
        for (Slot slot : slots.values()) updatePlayback(slot);
    }

    private int requestFocus() {
        int result = manager.requestAudioFocus(focusRequest);
        ownsFocus = result == AudioManager.AUDIOFOCUS_REQUEST_GRANTED;
        focusPaused = !ownsFocus;
        return result;
    }

    private void restoreFocusIfNeeded() {
        if (ownsFocus || lifecyclePaused || enginePaused) return;
        for (Slot slot : slots.values()) {
            if (slot.requested) { requestFocus(); return; }
        }
    }

    private void release(int id) {
        Slot old = slots.remove(id);
        if (old != null) {
            old.player.setOnPreparedListener(null);
            old.player.setOnCompletionListener(null);
            old.player.setOnErrorListener(null);
            old.player.release();
        }
    }

    void reset() {
        for (Slot slot : slots.values()) slot.player.release();
        slots.clear();
        enginePaused = false;
    }

    void close() {
        reset();
        if (ownsFocus) manager.abandonAudioFocusRequest(focusRequest);
        ownsFocus = false;
    }
}
