// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.RectF;
import java.util.HashMap;
import java.util.Map;
import org.json.JSONArray;
import org.json.JSONObject;

/** The FD CG_main, cg_mode, create_Type and create_display sprite layouts. */
final class DouseiExtrasMenu implements ExtrasMenu {
    private final Listener listener;
    private final JSONArray pages;
    private final int tracks;
    private final Map<String, Bitmap> artwork = new HashMap<>();
    private final Map<String, Bitmap> thumbnails = new HashMap<>();
    private final Paint paint = new Paint(Paint.FILTER_BITMAP_FLAG);
    private int character;
    private int style;
    private int track;
    // CG_main's ambient BGM starts before the music browser has a selected track.
    private int selectedTrack;
    private boolean musicPlaying = true;
    private JSONObject currentImage;
    private int frame;
    private Bitmap picture;

    private DouseiExtrasMenu(JSONObject catalogue, Listener listener) {
        this.listener = listener;
        pages = catalogue.optJSONArray("pages");
        tracks = catalogue.optInt("music_tracks");
        track = catalogue.optInt("default_track");
        if (pages == null || pages.length() != 5 || tracks != 32 || track < 1 || track > tracks)
            throw new IllegalArgumentException("Invalid FD extras catalogue");
    }

    /** Only UI planes and 100 x 75 thumbnails are retained, never the full CG catalogue. */
    static DouseiExtrasMenu load(long session, JSONObject catalogue, Listener listener) {
        DouseiExtrasMenu menu = new DouseiExtrasMenu(catalogue, listener);
        try {
            for (int index = 1; index <= 5; index++) menu.loadArt(session, "cg_base0" + index, 0);
            menu.loadArt(session, "page_btn", 10);
            menu.loadArt(session, "to_title", 10);
            menu.loadArt(session, "Type", 6);
            menu.loadArt(session, "bgm_no", 64);
            menu.loadArt(session, "bgm_name", 32);
            menu.loadArt(session, "bgm_ctrl", 8);
            menu.loadArt(session, "bgm_win", 1);
            menu.loadArt(session, "NoData", 1);
            for (int group = 0; group < menu.pages.length(); group++) {
                JSONObject page = menu.pages.optJSONObject(group);
                for (String kind : new String[]{"images", "scenes"}) {
                    JSONArray entries = page.optJSONArray(kind);
                    if (entries == null) throw new IllegalArgumentException("Missing FD extras entries");
                    for (int index = 0; index < entries.length(); index++) {
                        JSONObject entry = entries.optJSONObject(index);
                        int id = entry.optInt("id") + (kind.equals("scenes") ? 19 : 0);
                        menu.thumbnails.put(group + ":" + id, OriginalAssets.decode(
                            NativeBridge.extrasImage(session, group, id, -1)));
                        if (entry.optBoolean("unlocked")) menu.thumbnails.put(group + ":" + id + ":hover", OriginalAssets.decode(
                            NativeBridge.extrasImage(session, group, id, -2)));
                    }
                }
            }
            return menu;
        } catch (RuntimeException | Error failure) {
            menu.release();
            throw failure;
        }
    }

    private void loadArt(long session, String name, int sequences) {
        if (sequences == 0) putArt(session, name, -1);
        for (int sequence = 0; sequence < sequences; sequence++) putArt(session, name, sequence);
    }

    private void putArt(long session, String name, int sequence) {
        Bitmap image = OriginalAssets.decode(NativeBridge.image(session, "ykg\\extra\\" + name + ".ykg", sequence));
        if (image == null) throw new IllegalArgumentException("Missing FD extras artwork " + name);
        artwork.put(name + ":" + sequence, image);
    }

    @Override public void draw(Canvas canvas, float pressedX, float pressedY) {
        if (currentImage != null) {
            canvas.drawColor(Color.BLACK);
            if (picture != null) canvas.drawBitmap(picture, null, new RectF(0, 0, 800, 600), paint);
            return;
        }
        sprite(canvas, "cg_base0" + (character + 1), -1, 0, 0);
        for (int index = 0; index < pages.length(); index++) {
            if (index == character) continue;
            int x = 48 + 115 * index;
            sprite(canvas, "page_btn", index + (hit(pressedX, pressedY, x, 21, 110, 32) ? 5 : 0), x, 21);
        }
        sprite(canvas, "to_title", character + (hit(pressedX, pressedY, 0, 524, 120, 60) ? 5 : 0), 0, 524);
        for (int mode = 0; mode < 3; mode++) {
            int x = 421 + 104 * mode;
            boolean selected = style == mode || hit(pressedX, pressedY, x, 299, 100, 24);
            sprite(canvas, "Type", mode + (selected ? 3 : 0), x, 299);
        }
        // Empty native slots are also sprites, including characters with fewer than 15 CGs.
        for (int index = 0; index < 15; index++)
            sprite(canvas, "NoData", 0, 65 + index % 6 * 114, 84 + index / 6 * 89);
        for (int index = 0; index < 4; index++) sprite(canvas, "NoData", 0, 293 + 114 * index, 351);
        JSONObject page = pages.optJSONObject(character);
        for (String kind : new String[]{"images", "scenes"}) {
            JSONArray entries = page.optJSONArray(kind);
            for (int index = 0; index < entries.length(); index++) {
                JSONObject entry = entries.optJSONObject(index);
                int id = entry.optInt("id") + (kind.equals("scenes") ? 19 : 0);
                boolean hovered = entry.optBoolean("unlocked")
                    && hit(pressedX, pressedY, entry.optInt("x"), entry.optInt("y"), 100, 75);
                Bitmap thumbnail = thumbnails.get(character + ":" + id + (hovered ? ":hover" : ""));
                if (thumbnail != null) canvas.drawBitmap(thumbnail, entry.optInt("x"), entry.optInt("y"), paint);
            }
        }
        sprite(canvas, "bgm_win", 0, 301, 556);
        if (selectedTrack != 0) sprite(canvas, "bgm_name", selectedTrack - 1, 301, 556);
        for (int index = 0; index < tracks; index++) {
            int x = 151 + 30 * (index < 20 ? index : index - 20), y = index < 20 ? 492 : 522;
            boolean selected = selectedTrack == index + 1 || hit(pressedX, pressedY, x, y, 28, 28);
            sprite(canvas, "bgm_no", index + (selected ? 32 : 0), x, y);
        }
        for (int control = 0; control < 4; control++) {
            int x = 511 + control * 60;
            boolean selected = hit(pressedX, pressedY, x, 522, 58, 28)
                || control == (selectedTrack != 0 && musicPlaying ? 0 : 1);
            sprite(canvas, "bgm_ctrl", control + (selected ? 4 : 0), x, 522);
        }
    }

    @Override public int soundSlot(float x, float y, boolean hover) {
        // cg_cancel uses a silent full-screen next-image click.
        if (currentImage != null) return -1;
        for (int index = 0; index < pages.length(); index++)
            if (index != character && hit(x, y, 48 + 115 * index, 21, 110, 32)) return hover ? 102 : 100;
        if (hit(x, y, 0, 524, 120, 60)) return hover ? 101 : 100;
        for (int mode = 0; mode < 3; mode++)
            if (mode != style && hit(x, y, 421 + 104 * mode, 299, 100, 24)) return hover ? -1 : 100;
        JSONObject page = pages.optJSONObject(character);
        JSONArray scenes = page.optJSONArray("scenes");
        for (int index = 0; index < scenes.length(); index++) {
            JSONObject entry = scenes.optJSONObject(index);
            if (entry.optBoolean("unlocked") && hit(x, y, entry.optInt("x"), entry.optInt("y"), 100, 75))
                return hover ? -1 : 100;
        }
        // CG thumbnails, music numbers and all four music controls have no SE in CG_main.
        return -1;
    }

    @Override public void tap(float x, float y) {
        if (currentImage != null) {
            if (frame + 1 < frameCount()) requestFrame(frame + 1);
            else back();
            return;
        }
        for (int index = 0; index < pages.length(); index++)
            if (hit(x, y, 48 + 115 * index, 21, 110, 32)) { character = index; return; }
        if (hit(x, y, 0, 524, 120, 60)) { listener.close(); return; }
        for (int mode = 0; mode < 3; mode++)
            if (hit(x, y, 421 + 104 * mode, 299, 100, 24)) { style = mode; return; }
        JSONObject page = pages.optJSONObject(character);
        for (String kind : new String[]{"images", "scenes"}) {
            JSONArray entries = page.optJSONArray(kind);
            for (int index = 0; index < entries.length(); index++) {
                JSONObject entry = entries.optJSONObject(index);
                if (!entry.optBoolean("unlocked") || !hit(x, y, entry.optInt("x"), entry.optInt("y"), 100, 75)) continue;
                if (kind.equals("scenes")) listener.replay(character, entry.optInt("id"), style);
                else if (entry.optJSONArray("variants").optInt(style) > 0) { currentImage = entry; requestFrame(0); }
                return;
            }
        }
        for (int index = 0; index < tracks; index++) {
            int tx = 151 + 30 * (index < 20 ? index : index - 20), ty = index < 20 ? 492 : 522;
            if (hit(x, y, tx, ty, 28, 28)) { play(index + 1); return; }
        }
        if (hit(x, y, 511, 522, 58, 28)) play(selectedTrack == 0 ? 1 : selectedTrack);
        else if (hit(x, y, 571, 522, 58, 28)) { musicPlaying = false; listener.music(0); }
        else if (hit(x, y, 631, 522, 58, 28)) play(selectedTrack <= 1 ? tracks : selectedTrack - 1);
        else if (hit(x, y, 691, 522, 58, 28)) play(selectedTrack >= tracks ? 1 : selectedTrack + 1);
    }

    private void play(int value) { track = value; selectedTrack = value; musicPlaying = true; listener.music(value); }
    @Override public int musicTrack() { return musicPlaying ? track : 0; }
    private int frameCount() { return currentImage.optJSONArray("variants").optInt(style); }

    private void requestFrame(int value) {
        frame = value;
        replacePicture(null);
        listener.image(character, currentImage.optInt("id"), style * 10000 + frame);
    }

    @Override public void showImage(Bitmap image, int group, int item, int variant) {
        if (currentImage != null && character == group && currentImage.optInt("id") == item && variant == style * 10000 + frame)
            replacePicture(image);
        else if (image != null) image.recycle();
    }

    private void replacePicture(Bitmap next) {
        if (picture != null) picture.recycle();
        picture = next;
    }

    @Override public void back() {
        if (currentImage == null) listener.close();
        else { currentImage = null; replacePicture(null); }
    }

    @Override public void release() {
        replacePicture(null);
        for (Bitmap image : artwork.values()) if (image != null) image.recycle();
        for (Bitmap image : thumbnails.values()) if (image != null) image.recycle();
        artwork.clear();
        thumbnails.clear();
    }

    private void sprite(Canvas canvas, String name, int sequence, int x, int y) {
        canvas.drawBitmap(artwork.get(name + ":" + sequence), x, y, paint);
    }

    private static boolean hit(float x, float y, int left, int top, int width, int height) {
        return x >= left && x < left + width && y >= top && y < top + height;
    }
}
