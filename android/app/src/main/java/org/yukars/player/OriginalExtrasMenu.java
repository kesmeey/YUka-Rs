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

/** CG_main/create_Type/create_display layouts with the original progress gates. */
final class OriginalExtrasMenu implements ExtrasMenu {
    private final Listener listener;
    private final JSONArray pages;
    private final Map<String, Bitmap> artwork = new HashMap<>();
    private final Map<String, Bitmap> thumbnails = new HashMap<>();
    private final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG | Paint.FILTER_BITMAP_FLAG);
    private int character;
    private int style;
    private int track = 22;
    private boolean musicPlaying = true;
    private JSONObject currentImage;
    private int frame;
    private Bitmap picture;

    private OriginalExtrasMenu(JSONObject catalogue, Listener listener) {
        pages = catalogue.optJSONArray("pages");
        if (pages == null || pages.length() != 5) throw new IllegalArgumentException("Invalid original extras catalogue");
        this.listener = listener;
    }

    /** Called on the engine thread; owns only UI planes and small gallery thumbnails. */
    static OriginalExtrasMenu load(long session, JSONObject catalogue, Listener listener) {
        OriginalExtrasMenu menu = new OriginalExtrasMenu(catalogue, listener);
        for (int index = 1; index <= 5; index++) menu.loadArt(session, "cg_base0" + index, 0);
        menu.loadArt(session, "page_btn", 10);
        menu.loadArt(session, "to_title", 10);
        menu.loadArt(session, "Type", 8);
        menu.loadArt(session, "bgm_no", 60);
        menu.loadArt(session, "bgm_name", 29);
        menu.loadArt(session, "bgm_ctrl", 8);
        menu.loadArt(session, "bgm_win", 1);
        for (int character = 0; character < 5; character++) {
            JSONObject page = menu.pages.optJSONObject(character);
            for (String kind : new String[]{"images", "scenes"}) {
                JSONArray entries = page.optJSONArray(kind);
                for (int index = 0; index < entries.length(); index++) {
                    JSONObject entry = entries.optJSONObject(index);
                    int id = entry.optInt("id") + (kind.equals("scenes") ? 19 : 0);
                    menu.thumbnails.put(character + ":" + id,
                        OriginalAssets.decode(NativeBridge.extrasImage(session, character, id, -1)));
                }
            }
        }
        return menu;
    }

    private void loadArt(long session, String name, int sequences) {
        if (sequences == 0) artwork.put(name + ":-1", OriginalAssets.decode(
            NativeBridge.image(session, "ykg\\extra\\" + name + ".ykg", -1)));
        else for (int index = 0; index < sequences; index++) artwork.put(name + ":" + index,
            OriginalAssets.decode(NativeBridge.image(session, "ykg\\extra\\" + name + ".ykg", index)));
    }

    @Override public void draw(Canvas canvas, float pressedX, float pressedY) {
        if (currentImage != null) {
            canvas.drawColor(Color.BLACK);
            if (picture != null) canvas.drawBitmap(picture, null, new RectF(0, 0, 800, 600), paint);
            else label(canvas, "正在读取图像…", 320, 285, Color.WHITE, 22);
            paint.setColor(0xb0000000); canvas.drawRect(0, 560, 800, 600, paint); paint.setColor(Color.WHITE);
            label(canvas, "‹ 上一张", 20, 586, Color.WHITE, 18);
            label(canvas, (frame + 1) + " / " + frameCount(), 354, 586, Color.WHITE, 18);
            label(canvas, "下一张 ›", 500, 586, Color.WHITE, 18);
            label(canvas, "返回鉴赏 ×", 670, 586, Color.WHITE, 18);
            return;
        }
        image(canvas, "cg_base0" + (character + 1), -1, 0, 0);
        for (int index = 0; index < 5; index++) {
            if (index == character) continue;
            image(canvas, "page_btn", index + (inside(pressedX, pressedY, 48 + 115 * index, 21, 110, 32) ? 5 : 0), 48 + 115 * index, 21);
        }
        image(canvas, "to_title", character + (inside(pressedX, pressedY, 0, 544, 150, 40) ? 5 : 0), 0, 544);
        image(canvas, "Type", (style & 1) == 0 ? 4 : 0, 634, 76);
        image(canvas, "Type", (style & 1) == 0 ? 1 : 5, 634, 98);
        image(canvas, "Type", (style & 2) == 0 ? 6 : 2, 634, 120);
        image(canvas, "Type", (style & 2) == 0 ? 3 : 7, 634, 142);
        JSONObject page = pages.optJSONObject(character);
        for (String kind : new String[]{"images", "scenes"}) {
            JSONArray entries = page.optJSONArray(kind);
            for (int index = 0; index < entries.length(); index++) {
                JSONObject entry = entries.optJSONObject(index);
                int id = entry.optInt("id") + (kind.equals("scenes") ? 19 : 0);
                Bitmap thumbnail = thumbnails.get(character + ":" + id);
                int x = entry.optInt("x"), y = entry.optInt("y");
                if (thumbnail != null) canvas.drawBitmap(thumbnail, x, y, paint);
                if (entry.optBoolean("unlocked") && inside(pressedX, pressedY, x, y, 100, 75)) {
                    paint.setColor(0x44ffffff); canvas.drawRect(x, y, x + 100, y + 75, paint); paint.setColor(Color.WHITE);
                }
            }
        }
        image(canvas, "bgm_name", track == 0 ? 0 : track - 1, 345, 556);
        for (int index = 0; index < 29; index++) {
            int x = 181 + 30 * (index % 15), y = index < 15 ? 492 : 522;
            image(canvas, "bgm_no", index + (track == index + 1 ? 30 : 0), x, y);
        }
        image(canvas, "bgm_ctrl", musicPlaying ? 4 : 0, 631, 492);
        image(canvas, "bgm_ctrl", musicPlaying ? 1 : 5, 691, 492);
        image(canvas, "bgm_ctrl", 2, 631, 522);
        image(canvas, "bgm_ctrl", 3, 691, 522);
    }

    @Override public void tap(float x, float y) {
        if (currentImage != null) {
            if (x >= 650 && y >= 550) back();
            else if (x < 200) requestFrame(Math.max(0, frame - 1));
            else if (frame + 1 < frameCount()) requestFrame(frame + 1);
            else back();
            return;
        }
        for (int index = 0; index < 5; index++) if (inside(x, y, 48 + 115 * index, 21, 110, 32)) { character = index; return; }
        if (inside(x, y, 0, 544, 150, 40)) { listener.close(); return; }
        if (inside(x, y, 634, 76, 102, 19)) { style &= ~1; return; }
        if (inside(x, y, 634, 98, 102, 19)) { style |= 1; return; }
        if (inside(x, y, 634, 120, 102, 19)) { style &= ~2; return; }
        if (inside(x, y, 634, 142, 102, 19)) { style |= 2; return; }
        JSONObject page = pages.optJSONObject(character);
        for (String kind : new String[]{"images", "scenes"}) {
            JSONArray entries = page.optJSONArray(kind);
            for (int index = 0; index < entries.length(); index++) {
                JSONObject entry = entries.optJSONObject(index);
                if (!entry.optBoolean("unlocked") || !inside(x, y, entry.optInt("x"), entry.optInt("y"), 100, 75)) continue;
                if (kind.equals("scenes")) listener.replay(character, entry.optInt("id"), style);
                else { currentImage = entry; requestFrame(0); }
                return;
            }
        }
        for (int index = 0; index < 29; index++) if (inside(x, y, 181 + 30 * (index % 15), index < 15 ? 492 : 522, 28, 28)) {
            play(index + 1); return;
        }
        if (inside(x, y, 631, 492, 58, 28)) play(track == 0 ? 1 : track);
        else if (inside(x, y, 691, 492, 58, 28)) { musicPlaying = false; listener.music(0); }
        else if (inside(x, y, 631, 522, 58, 28)) play(track <= 1 ? 29 : track - 1);
        else if (inside(x, y, 691, 522, 58, 28)) play(track >= 29 ? 1 : track + 1);
    }

    private void play(int value) { track = value; musicPlaying = true; listener.music(track); }
    @Override public int musicTrack() { return musicPlaying ? track : 0; }
    private int frameCount() { return currentImage == null ? 0 : currentImage.optJSONArray("variants").optInt(style); }
    private void requestFrame(int value) {
        frame = value;
        if (picture != null) { picture.recycle(); picture = null; }
        listener.image(character, currentImage.optInt("id"), style * 10000 + frame);
    }
    @Override public void showImage(Bitmap image, int selectedCharacter, int item, int variant) {
        if (currentImage != null && character == selectedCharacter && currentImage.optInt("id") == item && variant == style * 10000 + frame)
            { if (picture != null) picture.recycle(); picture = image; }
        else if (image != null) image.recycle();
    }
    @Override public void back() {
        if (currentImage == null) { listener.close(); return; }
        currentImage = null;
        if (picture != null) { picture.recycle(); picture = null; }
    }
    @Override public void release() {
        if (picture != null) { picture.recycle(); picture = null; }
        for (Bitmap image : artwork.values()) if (image != null) image.recycle();
        for (Bitmap image : thumbnails.values()) if (image != null) image.recycle();
        artwork.clear(); thumbnails.clear();
    }
    private void image(Canvas canvas, String name, int sequence, int x, int y) { canvas.drawBitmap(artwork.get(name + ":" + sequence), x, y, paint); }
    private void label(Canvas canvas, String text, int x, int y, int color, int size) { paint.setColor(color); paint.setTextSize(size); canvas.drawText(text, x, y, paint); paint.setColor(Color.WHITE); }
    private static boolean inside(float x, float y, int left, int top, int width, int height) { return x >= left && x < left + width && y >= top && y < top + height; }
}
