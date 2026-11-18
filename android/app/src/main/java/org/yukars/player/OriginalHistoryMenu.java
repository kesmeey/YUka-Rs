// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.RectF;
import android.text.Layout;
import android.text.StaticLayout;
import android.text.TextPaint;
import java.util.ArrayList;
import java.util.List;
import org.json.JSONArray;
import org.json.JSONObject;

/** Scrollable dialogue history, with optional resource artwork. */
final class OriginalHistoryMenu {
    interface Listener { void close(); void voice(long id); }
    private static final int TOP = 70;
    private static final int BOTTOM = 520;
    private final OriginalAssets assets;
    private final Listener listener;
    private final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG | Paint.FILTER_BITMAP_FLAG);
    private final List<Entry> entries = new ArrayList<>();
    private final int textColor;
    private final int selectedTextColor;
    private float scroll;
    private float totalHeight;
    private boolean draggingBar;

    private static final class Entry {
        final long id;
        final boolean voice;
        final StaticLayout text;
        final float y;
        Entry(long id, boolean voice, StaticLayout text, float y) {
            this.id = id; this.voice = voice; this.text = text; this.y = y;
        }
    }

    OriginalHistoryMenu(OriginalAssets assets, JSONArray history, Listener listener) {
        this.assets = assets;
        this.listener = listener;
        textColor = assets == null ? Color.WHITE : assets.isDousei() ? 0xffdadada : 0xff163d55;
        selectedTextColor = assets != null && assets.isDousei() ? 0xfff3ff91 : textColor;
        TextPaint textPaint = new TextPaint(Paint.ANTI_ALIAS_FLAG);
        textPaint.setTextSize(25f);
        textPaint.setColor(textColor);
        for (int index = 0; index < history.length(); index++) {
            JSONObject entry = history.optJSONObject(index);
            if (entry == null) continue;
            String speaker = entry.optString("speaker");
            String value = (speaker.isEmpty() ? "" : "【" + speaker + "】\n") + entry.optString("dialogue");
            StaticLayout layout = StaticLayout.Builder.obtain(value, 0, value.length(), textPaint, 675)
                .setAlignment(Layout.Alignment.ALIGN_NORMAL)
                .setLineSpacing(assets != null && assets.isDousei() ? 40f - textPaint.getFontSpacing() : 5f, 1f)
                .setIncludePad(false).build();
            JSONArray voices = entry.optJSONArray("voices");
            entries.add(new Entry(entry.optLong("id"), voices != null && voices.length() > 0, layout, totalHeight));
            totalHeight += layout.getHeight() + 24;
        }
        scroll = maximumScroll();
    }

    int size() { return entries.size(); }
    float maximumScroll() { return Math.max(0, totalHeight - (BOTTOM - TOP)); }
    float scrollPosition() { return scroll; }
    void move(float delta) { scroll = Math.max(0, Math.min(maximumScroll(), scroll + delta)); }

    void draw(Canvas canvas, float pressedX, float pressedY) {
        if (assets == null) {
            paint.setColor(0xff101c2c);
            canvas.drawRect(0, 0, 800, 600, paint);
            paint.setColor(Color.WHITE);
            paint.setTextSize(28);
            canvas.drawText("对话履历", 70, 46, paint);
        } else canvas.drawBitmap(assets.bitmap("his_base", -1), 0, 0, paint);
        button(canvas, 0, 756, 15, pressedX, pressedY);
        button(canvas, 1, 756, 59, pressedX, pressedY);
        button(canvas, 2, 756, 545, pressedX, pressedY);
        float fraction = maximumScroll() == 0 ? 1 : scroll / maximumScroll();
        if (assets == null) {
            paint.setColor(0xff476781);
            canvas.drawRect(776, 122, 781, 520, paint);
            paint.setColor(0xffd4e4ef);
            canvas.drawRoundRect(770, 122 + fraction * 359, 787, 157 + fraction * 359, 7, 7, paint);
            paint.setColor(Color.WHITE);
        } else canvas.drawBitmap(assets.bitmap("his_gage", draggingBar ? 1 : 0), 766, 122 + fraction * 359, paint);
        canvas.save();
        canvas.clipRect(16, TOP, 745, BOTTOM);
        if (entries.isEmpty() && assets == null) {
            TextPaint empty = new TextPaint(Paint.ANTI_ALIAS_FLAG);
            empty.setTextSize(25); empty.setColor(textColor);
            canvas.drawText("暂无对话履历", 70, 110, empty);
        }
        for (Entry entry : entries) {
            float y = TOP + entry.y - scroll;
            if (y > BOTTOM || y + entry.text.getHeight() < TOP) continue;
            boolean pressed = entry.voice && new RectF(16, y, 745, y + entry.text.getHeight()).contains(pressedX, pressedY);
            if (entry.voice) {
                if (assets != null) canvas.drawBitmap(assets.bitmap("sb_rireki_voice", pressed ? 1 : 0), 20, y, paint);
                else plainButton(canvas, "▶", 20, y, pressed);
            }
            entry.text.getPaint().setColor(pressed ? selectedTextColor : textColor);
            canvas.save(); canvas.translate(70, y); entry.text.draw(canvas); canvas.restore();
        }
        canvas.restore();
        if (assets == null) {
            paint.setColor(textColor);
            paint.setTextSize(15f);
            canvas.drawText("上下滑动查看履历 · 点击喇叭重听语音", 70, 554, paint);
        }
        paint.setColor(Color.WHITE);
    }

    private void button(Canvas canvas, int sequence, float x, float y, float px, float py) {
        boolean pressed = new RectF(x, y, x + 40, y + 41).contains(px, py);
        if (assets == null) plainButton(canvas, new String[]{"×", "↑", "↓"}[sequence], x, y, pressed);
        else canvas.drawBitmap(assets.bitmap("his_btn", pressed ? sequence + 3 : sequence), x, y, paint);
    }

    private void plainButton(Canvas canvas, String label, float x, float y, boolean pressed) {
        paint.setColor(pressed ? 0xff587995 : 0xff2c4860);
        canvas.drawRoundRect(x, y, x + 40, y + 40, 8, 8, paint);
        paint.setColor(Color.WHITE);
        paint.setTextSize(22);
        canvas.drawText(label, x + (40 - paint.measureText(label)) / 2, y + 28, paint);
    }

    boolean begin(float x, float y) {
        draggingBar = x >= 756 && y >= 112 && y <= 532;
        if (draggingBar) drag(x, y, 0);
        return draggingBar;
    }

    void drag(float x, float y, float deltaY) {
        if (draggingBar) scroll = maximumScroll() * Math.max(0, Math.min(1, (y - 142) / 359));
        else move(-deltaY);
    }

    void end() { draggingBar = false; }

    boolean hitTest(float x, float y) {
        if (new RectF(756, 15, 796, 56).contains(x, y)
                || new RectF(756, 59, 796, 100).contains(x, y)
                || new RectF(756, 545, 796, 586).contains(x, y)
                || new RectF(766, 122, 786, 521).contains(x, y)) return true;
        return voiceAt(x, y) != null;
    }

    int soundSlot(float x, float y, boolean hover) {
        if (hover && (new RectF(756, 15, 796, 56).contains(x, y)
                || new RectF(756, 59, 796, 100).contains(x, y)
                || new RectF(756, 545, 796, 586).contains(x, y))) return 101;
        return -1;
    }

    private Entry voiceAt(float x, float y) {
        if (x < 16 || x > 65 || y < TOP || y >= BOTTOM) return null;
        for (Entry entry : entries) {
            float top = TOP + entry.y - scroll;
            if (entry.voice && y >= top && y < top + 40) return entry;
        }
        return null;
    }

    void tap(float x, float y) {
        if (x >= 756) {
            if (y >= 15 && y < 56) listener.close();
            else if (y >= 59 && y < 100) move(-330);
            else if (y >= 545 && y < 586) move(330);
            return;
        }
        Entry voice = voiceAt(x, y);
        if (voice != null) listener.voice(voice.id);
    }
}
