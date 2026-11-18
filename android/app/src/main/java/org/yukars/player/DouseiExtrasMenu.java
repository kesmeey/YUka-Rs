// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.RectF;
import org.json.JSONArray;
import org.json.JSONObject;

/** Gallery and soundtrack browser for the FD catalogue. */
final class DouseiExtrasMenu implements ExtrasMenu {
    private enum Section { IMAGES, SCENES, MUSIC }

    private static final int BACKGROUND = 0xff101c2c;
    private static final int SIDEBAR = 0xff17283e;
    private static final int TILE = 0xff203650;
    private static final int SELECTED = 0xff326d9f;
    private static final int MUTED = 0xffa5b8ce;
    private static final String[] MODES = {"A", "B", "C"};

    private final Listener listener;
    private final JSONArray pages;
    private final Bitmap[][] thumbnails;
    private final int tracks;
    private final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG | Paint.FILTER_BITMAP_FLAG);
    private final RectF destination = new RectF();
    private Section section = Section.IMAGES;
    private int character;
    private int style;
    private int track;
    private boolean musicPlaying;
    private JSONObject currentImage;
    private int frame;
    private Bitmap picture;

    private DouseiExtrasMenu(JSONObject catalogue, Listener listener) {
        this.listener = listener;
        pages = catalogue.optJSONArray("pages");
        tracks = catalogue.optInt("music_tracks");
        track = catalogue.optInt("default_track");
        if (pages == null || pages.length() == 0 || tracks < 1 || track < 1 || track > tracks)
            throw new IllegalArgumentException("无效的鉴赏目录");
        thumbnails = new Bitmap[pages.length()][25];
        musicPlaying = true;
    }

    /** Read the unlocked thumbnails on the engine thread before showing the menu. */
    static DouseiExtrasMenu load(long session, JSONObject catalogue, Listener listener) {
        DouseiExtrasMenu menu = new DouseiExtrasMenu(catalogue, listener);
        try {
            for (int character = 0; character < menu.pages.length(); character++) {
                JSONObject page = menu.pages.optJSONObject(character);
                menu.loadThumbnails(session, character, page.optJSONArray("images"), 0);
                menu.loadThumbnails(session, character, page.optJSONArray("scenes"), 19);
            }
            return menu;
        } catch (RuntimeException | Error failure) {
            menu.release();
            throw failure;
        }
    }

    private void loadThumbnails(long session, int group, JSONArray entries, int offset) {
        if (entries == null) throw new IllegalArgumentException("鉴赏项目缺失");
        for (int index = 0; index < entries.length(); index++) {
            JSONObject entry = entries.optJSONObject(index);
            int id = entry.optInt("id") + offset;
            if (id < 1 || id >= thumbnails[group].length) throw new IllegalArgumentException("无效的鉴赏项目编号");
            if (entry.optBoolean("unlocked")) {
                thumbnails[group][id] = OriginalAssets.decode(NativeBridge.extrasImage(session, group, id, -1));
            }
        }
    }

    @Override public void draw(Canvas canvas, float pressedX, float pressedY) {
        fill(canvas, 0, 0, 800, 600, BACKGROUND);
        if (currentImage != null) {
            drawPreview(canvas);
            return;
        }
        fill(canvas, 0, 0, 186, 600, SIDEBAR);
        label(canvas, "鉴赏", 24, 49, 31, Color.WHITE);
        String[] names = {"CG 鉴赏", "剧情回想", "音乐鉴赏"};
        for (Section candidate : Section.values()) {
            int index = candidate.ordinal();
            button(canvas, names[index], 19, 85 + index * 52, 147, 44, candidate == section);
        }
        button(canvas, "返回标题", 19, 536, 147, 43, false);
        if (section == Section.MUSIC) drawTracks(canvas);
        else drawGallery(canvas, pressedX, pressedY);
    }

    private void drawGallery(Canvas canvas, float pressedX, float pressedY) {
        for (int index = 0; index < pages.length(); index++) {
            button(canvas, pages.optJSONObject(index).optString("name"),
                201 + 116 * index, 23, 109, 44, character == index);
        }
        label(canvas, "模式", 208, 108, 18, MUTED);
        for (int index = 0; index < MODES.length; index++)
            button(canvas, MODES[index], 263 + 63 * index, 81, 55, 38, style == index);
        JSONArray entries = entries();
        for (int index = 0; index < entries.length(); index++) {
            JSONObject entry = entries.optJSONObject(index);
            int x = tileX(index), y = tileY(index);
            boolean unlocked = entry.optBoolean("unlocked");
            boolean pressed = unlocked && hit(pressedX, pressedY, x, y, 135, 98);
            fill(canvas, x, y, 135, 98, pressed ? SELECTED : TILE);
            int id = entry.optInt("id");
            Bitmap thumbnail = thumbnails[character][id + (section == Section.SCENES ? 19 : 0)];
            if (thumbnail != null) {
                fit(canvas, thumbnail, x + 8, y + 6, 119, 64);
                label(canvas, (section == Section.SCENES ? "回想 " : "CG ") + id,
                    x + 10, y + 90, 15, Color.WHITE);
            } else {
                label(canvas, unlocked ? "打开" : "未解锁", x + 25, y + 57, 17, MUTED);
            }
        }
    }

    private void drawTracks(Canvas canvas) {
        label(canvas, musicPlaying ? "正在播放 · 曲目 " + track : "当前曲目 " + track,
            207, 59, 24, Color.WHITE);
        for (int index = 0; index < tracks; index++) {
            button(canvas, Integer.toString(index + 1), 207 + (index % 8) * 72,
                94 + (index / 8) * 59, 63, 51, track == index + 1);
        }
        button(canvas, "上一首", 207, 384, 124, 51, false);
        button(canvas, "播放", 349, 384, 124, 51, musicPlaying);
        button(canvas, "停止", 491, 384, 124, 51, !musicPlaying);
        button(canvas, "下一首", 633, 384, 124, 51, false);
    }

    private void drawPreview(Canvas canvas) {
        if (picture != null) fit(canvas, picture, 0, 0, 800, 548);
        else label(canvas, "正在读取图像…", 324, 288, 22, Color.WHITE);
        button(canvas, "上一张", 22, 553, 126, 39, false);
        label(canvas, (frame + 1) + " / " + frameCount(), 178, 580, 20, Color.WHITE);
        button(canvas, "下一张", 510, 553, 121, 39, false);
        button(canvas, "返回鉴赏", 645, 553, 133, 39, false);
    }

    @Override public void tap(float x, float y) {
        if (currentImage != null) {
            if (hit(x, y, 645, 553, 133, 39)) back();
            else if (hit(x, y, 22, 553, 126, 39) && frame > 0) requestFrame(frame - 1);
            else if (hit(x, y, 510, 553, 121, 39) && frame + 1 < frameCount()) requestFrame(frame + 1);
            return;
        }
        if (hit(x, y, 19, 536, 147, 43)) { listener.close(); return; }
        for (Section candidate : Section.values()) {
            if (hit(x, y, 19, 85 + candidate.ordinal() * 52, 147, 44)) { section = candidate; return; }
        }
        if (section == Section.MUSIC) tapTracks(x, y);
        else tapGallery(x, y);
    }

    private void tapTracks(float x, float y) {
        for (int index = 0; index < tracks; index++) {
            if (hit(x, y, 207 + (index % 8) * 72, 94 + (index / 8) * 59, 63, 51)) {
                play(index + 1);
                return;
            }
        }
        if (hit(x, y, 207, 384, 124, 51)) play(track == 1 ? tracks : track - 1);
        else if (hit(x, y, 349, 384, 124, 51)) play(track);
        else if (hit(x, y, 491, 384, 124, 51)) { musicPlaying = false; listener.music(0); }
        else if (hit(x, y, 633, 384, 124, 51)) play(track == tracks ? 1 : track + 1);
    }

    private void tapGallery(float x, float y) {
        for (int index = 0; index < pages.length(); index++) {
            if (hit(x, y, 201 + 116 * index, 23, 109, 44)) { character = index; return; }
        }
        for (int index = 0; index < MODES.length; index++) {
            if (hit(x, y, 263 + 63 * index, 81, 55, 38)) { style = index; return; }
        }
        JSONArray entries = entries();
        for (int index = 0; index < entries.length(); index++) {
            JSONObject entry = entries.optJSONObject(index);
            if (!entry.optBoolean("unlocked") || !hit(x, y, tileX(index), tileY(index), 135, 98)) continue;
            if (section == Section.SCENES) listener.replay(character, entry.optInt("id"), style);
            else if (entry.optJSONArray("variants").optInt(style) > 0) {
                currentImage = entry;
                requestFrame(0);
            }
            return;
        }
    }

    private JSONArray entries() {
        return pages.optJSONObject(character).optJSONArray(section == Section.SCENES ? "scenes" : "images");
    }

    private void play(int value) { track = value; musicPlaying = true; listener.music(value); }
    @Override public int musicTrack() { return musicPlaying ? track : 0; }
    private int frameCount() { return currentImage.optJSONArray("variants").optInt(style); }
    private static int tileX(int index) { return 204 + (index % 4) * 144; }
    private static int tileY(int index) { return 136 + (index / 4) * 109; }

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
        for (Bitmap[] page : thumbnails) {
            for (int id = 0; id < page.length; id++) {
                if (page[id] != null) { page[id].recycle(); page[id] = null; }
            }
        }
    }

    private void fit(Canvas canvas, Bitmap bitmap, int x, int y, int width, int height) {
        float scale = Math.min((float)width / bitmap.getWidth(), (float)height / bitmap.getHeight());
        float w = bitmap.getWidth() * scale, h = bitmap.getHeight() * scale;
        float left = x + (width - w) / 2, top = y + (height - h) / 2;
        destination.set(left, top, left + w, top + h);
        paint.setColor(Color.WHITE);
        canvas.drawBitmap(bitmap, null, destination, paint);
    }

    private void button(Canvas canvas, String text, int x, int y, int width, int height, boolean selected) {
        fill(canvas, x, y, width, height, selected ? SELECTED : TILE);
        paint.setTextSize(16);
        label(canvas, text, x + (width - paint.measureText(text)) / 2, y + height / 2 + 6, 16, Color.WHITE);
    }

    private void fill(Canvas canvas, int x, int y, int width, int height, int color) {
        paint.setColor(color);
        canvas.drawRect(x, y, x + width, y + height, paint);
    }

    private void label(Canvas canvas, String text, float x, int baseline, int size, int color) {
        paint.setColor(color);
        paint.setTextSize(size);
        canvas.drawText(text, x, baseline, paint);
    }

    private static boolean hit(float x, float y, int left, int top, int width, int height) {
        return x >= left && x < left + width && y >= top && y < top + height;
    }
}
