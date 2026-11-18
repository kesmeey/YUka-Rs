// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.RectF;
import android.graphics.Typeface;
import java.io.File;
import java.text.SimpleDateFormat;
import java.util.Date;
import java.util.Locale;
import org.json.JSONObject;

/** Layout and slot numbering shared by the supplied games' m_save.yks / m_load.yks. */
final class OriginalSaveMenu {
    private static final int[] SIDEBAR_Y = {185, 217, 249, 281, 338, 370, 426, 458, 490, 546};
    enum Action { SAVE, LOAD, OPTIONS, SCREEN, TEXT, SOUND, VOICE, PREVIOUS_CHOICE, TITLE, RETURN, EXIT }
    interface Listener {
        void page(int page);
        void slot(int row);
        void confirm();
        void cancel();
        void action(Action action);
    }

    static final class Slot {
        final File file;
        final boolean exists;
        final Bitmap thumbnail;
        final String chapter;
        final String caption;
        final String date;
        final String error;

        private Slot(File file, boolean exists, Bitmap thumbnail, String chapter, String caption, String date, String error) {
            this.file = file;
            this.exists = exists;
            this.thumbnail = thumbnail;
            this.chapter = chapter;
            this.caption = caption;
            this.date = date;
            this.error = error;
        }

        static Slot read(long session, File directory, int page, int row) {
            File file = file(directory, page, row);
            if (!file.isFile()) return new Slot(file, false, null, "", "", "", "");
            try {
                JSONObject info = new JSONObject(NativeBridge.saveInfo(session, file.getAbsolutePath()));
                Bitmap thumbnail = null;
                try { thumbnail = OriginalAssets.decode(NativeBridge.saveThumbnail(session, file.getAbsolutePath())); }
                catch (RuntimeException ignored) { /* A damaged preview does not prevent restoring the VM state. */ }
                String date = new SimpleDateFormat("yyyy/MM/dd HH:mm", Locale.ROOT).format(new Date(info.getLong("modified_ms")));
                return new Slot(file, true, thumbnail, info.optString("chapter"), info.optString("caption"), date, "");
            } catch (Exception error) {
                return new Slot(file, true, null, "", "", "", error.getMessage() == null ? "无法读取存档" : error.getMessage());
            }
        }
    }

    final boolean saving;
    int page;
    int row;
    int confirmation = -1;
    Action navigation;
    Slot[] slots;
    private final OriginalAssets assets;
    private final Listener listener;
    private final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG | Paint.FILTER_BITMAP_FLAG);
    private final Paint text = new Paint(Paint.ANTI_ALIAS_FLAG);

    OriginalSaveMenu(OriginalAssets assets, boolean saving, int page, int row, Listener listener) {
        this.assets = assets;
        this.saving = saving;
        this.page = Math.max(0, Math.min(19, page));
        this.row = Math.max(1, Math.min(5, row));
        this.listener = listener;
        text.setTypeface(Typeface.create("sans-serif", Typeface.BOLD));
        text.setColor(assets.isDousei() ? 0xff877467 : Color.BLACK);
        text.setTextSize(14);
    }

    static File file(File directory, int page, int row) {
        if (page < 0 || page > 19 || row < 1 || row > 5) throw new IllegalArgumentException("Invalid save page or row");
        return new File(directory, String.format(Locale.ROOT, "savedata.%03d", page * 10 + row));
    }

    void draw(Canvas canvas, float pressedX, float pressedY) {
        image(canvas, "op_base", -1, 0, 0);
        image(canvas, "op_title", saving ? 8 : 9, 15, 20);
        String item = saving ? "save_item" : "load_item";
        for (int index = 0; index < 5; index++) {
            int y = 80 + 98 * index;
            image(canvas, item, inside(pressedX, pressedY, 46, y, 502, 89) ? 1 : 0, 46, y);
            Slot slot = slots == null ? null : slots[index];
            if (slot != null && slot.exists) {
                if (slot.thumbnail != null) canvas.drawBitmap(slot.thumbnail, null, new RectF(53, y + 7, 153, y + 82), paint);
                label(canvas, slot.error.isEmpty() ? slot.chapter : "存档损坏", 211, y + 17, 185);
                label(canvas, slot.error.isEmpty() ? slot.caption : slot.error, 211, y + 56, 328);
                label(canvas, slot.date, 406, y + 18, 139);
            }
        }
        if (page == 0) image(canvas, "autosave", -1, 53, 87);
        String pages = saving ? "save_page" : "load_page";
        for (int index = 0; index < 20; index++) {
            int x = index < 10 ? 557 : 602;
            int y = 80 + 49 * (index % 10);
            image(canvas, pages, index + (index == page || inside(pressedX, pressedY, x, y, 40, 40) ? 20 : 0), x, y);
        }
        int[] positions = SIDEBAR_Y;
        for (int index = 0; index < positions.length; index++) {
            int sequence = index + 1;
            if ((saving && sequence == 5) || (!saving && sequence == 6)) sequence += 22;
            else if (inside(pressedX, pressedY, 656, positions[index], 144, 25)) sequence += 11;
            image(canvas, "op_tab", sequence, 656, positions[index]);
        }
        if (confirming()) {
            image(canvas, "YesNo_Base", -1, 0, 0);
            int prompt;
            if (navigation == Action.PREVIOUS_CHOICE) prompt = 4;
            else if (navigation == Action.TITLE) prompt = 5;
            else if (navigation == Action.EXIT) prompt = 7;
            else prompt = saving ? (slots[confirmation].exists ? 2 : 1) : 3;
            image(canvas, "YesNo_P0" + prompt, -1, 9, 230);
            image(canvas, "YesNo_btn", inside(pressedX, pressedY, 200, 341, 166, 42) ? 2 : 0, 200, 341);
            image(canvas, "YesNo_btn", inside(pressedX, pressedY, 434, 341, 166, 42) ? 3 : 1, 434, 341);
        }
    }

    void tap(float x, float y) {
        if (confirming()) {
            if (inside(x, y, 200, 341, 166, 42)) listener.confirm();
            else if (inside(x, y, 434, 341, 166, 42)) listener.cancel();
            return;
        }
        for (int index = 0; index < 5; index++)
            if (inside(x, y, 46, 80 + 98 * index, 502, 89)) { listener.slot(index + 1); return; }
        for (int index = 0; index < 20; index++)
            if (inside(x, y, index < 10 ? 557 : 602, 80 + 49 * (index % 10), 40, 40)) { listener.page(index); return; }
        int[] positions = SIDEBAR_Y;
        Action[] actions = {Action.SCREEN, Action.TEXT, Action.SOUND, Action.VOICE,
            Action.SAVE, Action.LOAD, Action.PREVIOUS_CHOICE, Action.TITLE, Action.RETURN, Action.EXIT};
        for (int index = 0; index < positions.length; index++)
            if (inside(x, y, 656, positions[index], 144, 25)) { listener.action(actions[index]); return; }
    }

    boolean confirming() { return confirmation >= 0 || navigation != null; }

    boolean hitTest(float x, float y) {
        return soundSlot(x, y, false) >= 0;
    }

    int soundSlot(float x, float y, boolean hover) {
        if (confirming()) return inside(x, y, 200, 341, 166, 42) || inside(x, y, 434, 341, 166, 42)
            ? (hover ? 102 : 100) : -1;
        for (int index = 0; index < 5; index++)
            if (inside(x, y, 46, 80 + 98 * index, 502, 89)) return hover ? 102 : 100;
        for (int index = 0; index < 20; index++)
            if (index != page && inside(x, y, index < 10 ? 557 : 602, 80 + 49 * (index % 10), 40, 40)) return hover ? 102 : 100;
        for (int index = 0; index < SIDEBAR_Y.length; index++)
            if (index != (saving ? 4 : 5) && inside(x, y, 656, SIDEBAR_Y[index], 144, 25)) return hover ? 101 : 100;
        return -1;
    }

    private void image(Canvas canvas, String name, int sequence, float x, float y) {
        canvas.drawBitmap(assets.bitmap(name, sequence), x, y, paint);
    }

    private void label(Canvas canvas, String value, float x, float y, float width) {
        canvas.save();
        canvas.clipRect(x, y, x + width, y + 24);
        canvas.drawText(value.replace('\n', ' ').replace('\r', ' '), x, y - text.ascent(), text);
        canvas.restore();
    }

    private static boolean inside(float x, float y, float left, float top, float width, float height) {
        return x >= left && y >= top && x < left + width && y < top + height;
    }
}
