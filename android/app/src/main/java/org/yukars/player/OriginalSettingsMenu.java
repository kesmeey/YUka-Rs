// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.RectF;
import java.util.ArrayList;
import java.util.List;
import org.json.JSONException;
import org.json.JSONObject;

/** Original m_screen, m_text01/02, m_sound and m_voice01..04 controls in game coordinates. */
final class OriginalSettingsMenu {
    interface Listener {
        void change(int flag, int value);
        void page(int page);
        void action(OriginalSaveMenu.Action action);
        void sample(int voiceId);
        void defaults(int section);
        default void font() { }
    }

    private static final int[] NAV_Y = {185, 217, 249, 281, 338, 370, 426, 458, 490, 546};
    private static final OriginalSaveMenu.Action[] NAV_ACTIONS = {OriginalSaveMenu.Action.SCREEN,
        OriginalSaveMenu.Action.TEXT, OriginalSaveMenu.Action.SOUND, OriginalSaveMenu.Action.VOICE,
        OriginalSaveMenu.Action.SAVE, OriginalSaveMenu.Action.LOAD, OriginalSaveMenu.Action.PREVIOUS_CHOICE,
        OriginalSaveMenu.Action.TITLE, OriginalSaveMenu.Action.RETURN, OriginalSaveMenu.Action.EXIT};
    private static final int[][] VOICE_FLAGS = {{102, 103, 104}, {110, 111, 112, 113, 114},
        {115, 116, 117, 120}, {118, 119, 121}};
    private static final int[][] VOICE_SAMPLES = {{0, 1, 2}, {3, 4, 5, 6, 7}, {8, 9, 10, 11}, {12, 13, 14}};

    final int page;
    private final OriginalAssets common;
    private final OriginalSettingsAssets artwork;
    private final Listener listener;
    private final boolean canSave;
    private JSONObject flags;
    private final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG | Paint.FILTER_BITMAP_FLAG);
    private final List<Radio> radios = new ArrayList<>();
    private final List<Slider> sliders = new ArrayList<>();
    private Slider dragging;
    private OriginalSaveMenu.Action navigation;

    private static final class Radio {
        final int flag, value, sequence;
        final RectF bounds;
        Radio(int flag, int value, int sequence, float x, float y) {
            this.flag = flag; this.value = value; this.sequence = sequence;
            bounds = new RectF(x, y, x + 150, y + 28);
        }
    }
    private static final class Slider {
        final int flag;
        final float y;
        Slider(int flag, float y) { this.flag = flag; this.y = y; }
    }

    OriginalSettingsMenu(OriginalAssets common, OriginalSettingsAssets artwork, int page,
            JSONObject flags, boolean canSave, Listener listener) {
        if (page < 0 || page > 7) throw new IllegalArgumentException("Settings page must be 0..7");
        this.common = common; this.artwork = artwork; this.page = page;
        this.flags = flags; this.canSave = canSave; this.listener = listener;
        configure();
    }

    void update(JSONObject settings) { if (dragging == null) flags = settings; }
    int value(int flag) { return flags.optInt(Integer.toString(flag), defaultValue(flag)); }
    boolean confirming() { return navigation != null; }
    void cancelConfirmation() { navigation = null; }

    private static int defaultValue(int flag) {
        if (flag == 4) return 20;
        if (flag == 5) return 2000;
        if (flag == -1 || flag == 10 || flag == 12 || flag == 13 || flag == 14 || flag == 15) return 1;
        if (flag == 100 || flag == 101) return 650;
        return flag >= 102 ? 1000 : 0;
    }

    private void configure() {
        if (page == 0) {
            radio(-1, new int[]{1, 0}, new int[]{0, 1}, 119);
            radio(10, new int[]{0, 1, 2}, new int[]{2, 3, 16}, 218);
            radio(11, new int[]{0, 1}, new int[]{4, 5}, 317);
            radio(15, new int[]{0, 1, 2}, new int[]{6, 7, 8}, 416);
        } else if (page == 1) {
            sliders.add(new Slider(4, 128));
            radio(3, new int[]{0, 1}, new int[]{13, 14}, 218);
            radio(13, new int[]{1, 0}, new int[]{15, 16}, 317);
            sliders.add(new Slider(5, 425));
            radio(12, new int[]{1, 0}, new int[]{15, 16}, 515);
        } else if (page == 2) {
            radio(2, new int[]{0, 1}, new int[]{9, 10}, 119);
            radio(14, new int[]{0, 1, 2}, new int[]{16, 11, 12}, 218);
            radio(16, new int[]{1, 0}, new int[]{15, 16}, 317);
        } else if (page == 3) {
            sliders.add(new Slider(100, 128));
            sliders.add(new Slider(101, 227));
        } else {
            int[] ids = VOICE_FLAGS[page - 4];
            for (int index = 0; index < ids.length; index++) sliders.add(new Slider(ids[index], 128 + index * 99));
        }
    }

    private void radio(int flag, int[] values, int[] sequences, int y) {
        for (int index = 0; index < values.length; index++)
            radios.add(new Radio(flag, values[index], sequences[index], 74 + index * 162, y));
    }

    void draw(Canvas canvas, float px, float py) {
        image(canvas, common.bitmap("op_base", -1), 0, 0);
        image(canvas, common.bitmap("op_title", page), 15, 20);
        image(canvas, artwork.bitmap(OriginalSettingsAssets.BACKGROUNDS[page], -1), 56, 65);
        int selected = page == 0 ? 0 : page <= 2 ? 1 : page == 3 ? 2 : 3;
        for (int index = 0; index < NAV_Y.length; index++) {
            int sequence = index + 1;
            if (index == selected) sequence += 22;
            else if (inside(px, py, 656, NAV_Y[index], 144, 25)) sequence += 11;
            paint.setAlpha(index == 4 && !canSave ? 128 : 255);
            image(canvas, common.bitmap("op_tab", sequence), 656, NAV_Y[index]);
            paint.setAlpha(255);
        }
        drawPages(canvas, px, py);
        for (Radio radio : radios) {
            boolean chosen = value(radio.flag) == radio.value;
            int sequence = radio.sequence + (chosen ? 34 : radio.bounds.contains(px, py) ? 17 : 0);
            image(canvas, artwork.bitmap("op_btn01", sequence), radio.bounds.left, radio.bounds.top);
        }
        for (int index = 0; index < sliders.size(); index++) {
            Slider slider = sliders.get(index);
            image(canvas, artwork.bitmap("op_var", 0), 130 + sliderPosition(slider.flag), slider.y - 14);
            if (page >= 3) drawSoundButtons(canvas, index, px, py);
        }
        paint.setColor(0xff087cb9);
        paint.setTextSize(15);
        canvas.drawText("恢复默认设置", 74, 578, paint);
        paint.setColor(Color.WHITE);
        if (confirming()) {
            image(canvas, common.bitmap("YesNo_Base", -1), 0, 0);
            int prompt = navigation == OriginalSaveMenu.Action.PREVIOUS_CHOICE ? 4
                : navigation == OriginalSaveMenu.Action.TITLE ? 5 : 7;
            image(canvas, common.bitmap("YesNo_P0" + prompt, -1), 9, 230);
            image(canvas, common.bitmap("YesNo_btn", inside(px, py, 200, 341, 166, 42) ? 2 : 0), 200, 341);
            image(canvas, common.bitmap("YesNo_btn", inside(px, py, 434, 341, 166, 42) ? 3 : 1), 434, 341);
        }
    }

    private void drawPages(Canvas canvas, float px, float py) {
        if (page == 1 || page == 2) {
            for (int index = 0; index < 2; index++)
                image(canvas, artwork.bitmap("op_page", index + (page == index + 1 || inside(px, py, 447 + index * 84, 37, 82, 28) ? 6 : 0)), 447 + index * 84, 37);
        } else if (page >= 4) {
            for (int index = 0; index < 4; index++)
                image(canvas, artwork.bitmap("op_page", index + 2 + (page == index + 4 || inside(px, py, 279 + index * 84, 37, 82, 28) ? 6 : 0)), 279 + index * 84, 37);
        }
    }

    private void drawSoundButtons(Canvas canvas, int row, float px, float py) {
        int y = 81 + 99 * row;
        if (page != 3 || row != 0)
            image(canvas, artwork.bitmap("op_sound_test", inside(px, py, 361, y, 72, 20) ? 2 : 0), 361, y);
        image(canvas, artwork.bitmap("op_sound_test", inside(px, py, 443, y, 72, 20) ? 3 : 1), 443, y);
    }

    private int sliderPosition(int flag) {
        if (flag == 4) return Math.max(0, Math.min(330, 330 - value(flag) / 2 * 10));
        if (flag == 5) return Math.max(0, Math.min(330, (6600 - value(flag)) / 20));
        return value(flag) == 0 ? 0 : Math.max(0, Math.min(330, (value(flag) - 10) / 3));
    }

    private void setSlider(Slider slider, float x) {
        int position = Math.max(0, Math.min(330, Math.round(x - 138)));
        int next = slider.flag == 4 ? 66 - position * 2 / 10
            : slider.flag == 5 ? 6600 - position * 20 : position == 0 ? 0 : position * 3 + 10;
        change(slider.flag, next);
    }

    private void change(int flag, int next) {
        try { flags.put(Integer.toString(flag), next); }
        catch (JSONException error) { throw new IllegalStateException(error); }
        listener.change(flag, next);
    }

    boolean begin(float x, float y) {
        if (confirming()) return false;
        for (Slider slider : sliders) {
            if (inside(x, y, 93, slider.y - 10, 420, 30)) {
                dragging = slider; setSlider(slider, x); return true;
            }
        }
        return false;
    }
    void drag(float x, float y, float deltaY) { if (dragging != null) setSlider(dragging, x); }
    void end() { dragging = null; }

    void tap(float x, float y) {
        if (confirming()) {
            if (inside(x, y, 200, 341, 166, 42)) {
                OriginalSaveMenu.Action action = navigation; navigation = null; listener.action(action);
            } else if (inside(x, y, 434, 341, 166, 42)) navigation = null;
            return;
        }
        for (int index = 0; index < NAV_Y.length; index++) {
            if (!inside(x, y, 656, NAV_Y[index], 144, 25)) continue;
            if (index == 4 && !canSave) return;
            OriginalSaveMenu.Action action = NAV_ACTIONS[index];
            if (action == OriginalSaveMenu.Action.PREVIOUS_CHOICE || action == OriginalSaveMenu.Action.TITLE || action == OriginalSaveMenu.Action.EXIT)
                navigation = action;
            else listener.action(action);
            return;
        }
        if ((page == 1 || page == 2) && y >= 37 && y < 65) {
            if (x >= 447 && x < 529) listener.page(1);
            else if (x >= 531 && x < 613) listener.page(2);
            return;
        }
        if (page >= 4 && y >= 37 && y < 65) {
            for (int index = 0; index < 4; index++)
                if (x >= 279 + index * 84 && x < 361 + index * 84) { listener.page(index + 4); return; }
        }
        for (Radio radio : radios) {
            if (radio.bounds.contains(x, y)) { change(radio.flag, radio.value); return; }
        }
        if (page >= 3) {
            for (int row = 0; row < sliders.size(); row++) {
                int y0 = 81 + 99 * row;
                if (inside(x, y, 443, y0, 72, 20)) { change(sliders.get(row).flag, 0); return; }
                if ((page != 3 || row != 0) && inside(x, y, 361, y0, 72, 20)) {
                    listener.sample(page == 3 ? -1 : VOICE_SAMPLES[page - 4][row]); return;
                }
            }
        }
        if (inside(x, y, 65, 557, 180, 35)) listener.defaults(page == 0 ? 1 : page <= 2 ? 2 : page == 3 ? 3 : 4);
    }

    private void image(Canvas canvas, android.graphics.Bitmap bitmap, float x, float y) { canvas.drawBitmap(bitmap, x, y, paint); }
    private static boolean inside(float x, float y, float left, float top, float width, float height) {
        return x >= left && x < left + width && y >= top && y < top + height;
    }
}
