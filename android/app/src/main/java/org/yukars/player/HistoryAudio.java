// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

import android.content.Context;
import android.media.AudioAttributes;
import android.media.AudioFocusRequest;
import android.media.AudioManager;
import android.media.MediaPlayer;
import java.io.File;
import java.io.FileInputStream;
import java.util.ArrayList;
import java.util.List;
import org.json.JSONArray;

/** UI-thread history voice queue, independent of the paused scene audio. */
final class HistoryAudio {
    private final List<File> queue = new ArrayList<>();
    private final AudioManager manager;
    private final AudioFocusRequest focus;
    private MediaPlayer player;
    private boolean ownsFocus;

    HistoryAudio(Context context) {
        manager = (AudioManager) context.getSystemService(Context.AUDIO_SERVICE);
        focus = new AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT)
            .setAudioAttributes(new AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_GAME)
                .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build())
            .setOnAudioFocusChangeListener(change -> {
                if (change == AudioManager.AUDIOFOCUS_LOSS || change == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT) stop();
            }).build();
    }

    void play(JSONArray paths) {
        stop();
        for (int index = 0; index < paths.length(); index++) queue.add(new File(paths.optString(index)));
        if (queue.isEmpty()) return;
        ownsFocus = manager.requestAudioFocus(focus) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED;
        if (!ownsFocus) { stop(); return; }
        next();
    }

    private void next() {
        release();
        if (queue.isEmpty()) { stop(); return; }
        File source = queue.remove(0);
        MediaPlayer candidate = new MediaPlayer();
        player = candidate;
        try {
            candidate.setAudioAttributes(new AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_GAME)
                .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build());
            candidate.setOnPreparedListener(value -> { if (player == value) value.start(); });
            candidate.setOnCompletionListener(value -> next());
            candidate.setOnErrorListener((value, what, extra) -> { stop(); return true; });
            try (FileInputStream input = new FileInputStream(source)) { candidate.setDataSource(input.getFD()); }
            candidate.prepareAsync();
        } catch (Exception error) { stop(); }
        finally { source.delete(); }
    }

    void stop() {
        release();
        for (File file : queue) file.delete();
        queue.clear();
        if (ownsFocus) { ownsFocus = false; manager.abandonAudioFocusRequest(focus); }
    }

    private void release() {
        if (player != null) {
            player.setOnPreparedListener(null);
            player.setOnCompletionListener(null);
            player.setOnErrorListener(null);
            player.release();
            player = null;
        }
    }
}
