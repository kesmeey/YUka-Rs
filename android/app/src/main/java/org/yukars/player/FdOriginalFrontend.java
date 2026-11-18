// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.graphics.Paint;
import java.util.HashMap;
import java.util.Map;

/** The FD's original title and route sprites, at the coordinates in its YKS scripts. */
final class FdOriginalFrontend {
    enum TitleAction { START, LOAD, SETTINGS, EXTRAS, EXIT }

    private static final String TITLE = "ykg\\title\\";
    private static final String SELECT = TITLE + "select\\";
    private static final String[] HEROES = {"kaho", "tugumi", "tinatu", "satuki", "nanako"};
    private final Map<String, Bitmap> images = new HashMap<>();
    private final Paint paint = new Paint(Paint.FILTER_BITMAP_FLAG);

    /** Decode on the engine thread, before publishing this object to the view. */
    static FdOriginalFrontend load(long session) {
        FdOriginalFrontend frontend = new FdOriginalFrontend();
        try {
            frontend.load(session, TITLE, "base", 0);
            frontend.load(session, TITLE, "logo_02", 0);
            frontend.load(session, TITLE, "t_btn", 10);
            frontend.load(session, TITLE, "t_btn_msg", 5);
            frontend.load(session, TITLE, "name_input", 0);
            frontend.load(session, TITLE, "name_btn", 6);
            frontend.load(session, SELECT, "select_base", 0);
            frontend.load(session, SELECT, "select_base02", 0);
            frontend.load(session, SELECT, "select_btn01", 10);
            frontend.load(session, SELECT, "name_go", 3);
            for (String hero : HEROES) frontend.load(session, SELECT, "type_" + hero, 6);
            return frontend;
        } catch (RuntimeException | Error failure) {
            frontend.release();
            throw failure;
        }
    }

    private void load(long session, String directory, String name, int sequences) {
        if (sequences == 0) put(session, directory, name, -1);
        for (int sequence = 0; sequence < sequences; sequence++) put(session, directory, name, sequence);
    }

    private void put(long session, String directory, String name, int sequence) {
        Bitmap image = OriginalAssets.decode(NativeBridge.image(session, directory + name + ".ykg", sequence));
        if (image == null) throw new IllegalArgumentException("Missing FD artwork: " + name);
        images.put(name + ":" + sequence, image);
    }

    private void sprite(Canvas canvas, String name, int sequence, int x, int y) {
        canvas.drawBitmap(images.get(name + ":" + sequence), x, y, paint);
    }

    void drawTitle(Canvas canvas, float pressedX, float pressedY, boolean extrasUnlocked) {
        sprite(canvas, "base", -1, 0, 0);
        sprite(canvas, "logo_02", -1, 190, 46);
        for (int button = 0; button < 5; button++) {
            if (button == 3 && !extrasUnlocked) continue;
            int y = 272 + button * 55;
            boolean hovered = hit(pressedX, pressedY, 315, y, 155, 54);
            sprite(canvas, "t_btn", button + (hovered ? 5 : 0), 315, y);
            if (hovered) sprite(canvas, "t_btn_msg", button, 470, y);
        }
    }

    void drawRoutes(Canvas canvas, float pressedX, float pressedY) {
        sprite(canvas, "select_base", -1, 0, 0);
        for (int route = 0; route < HEROES.length; route++) {
            int x = 24 + route * 152;
            boolean hovered = hit(pressedX, pressedY, x, 94, 144, 450);
            sprite(canvas, "select_btn01", route + (hovered ? 5 : 0), x, 94);
        }
    }

    void drawVariants(Canvas canvas, int route, float pressedX, float pressedY) {
        if (route < 0 || route >= HEROES.length) throw new IllegalArgumentException("Invalid FD route");
        sprite(canvas, "select_base02", -1, 0, 0);
        for (int variant = 0; variant < 3; variant++) {
            int x = 32 + variant * 248;
            boolean hovered = hit(pressedX, pressedY, x, 128, 240, 420);
            sprite(canvas, "type_" + HEROES[route], variant + (hovered ? 3 : 0), x, 128);
        }
        sprite(canvas, "name_go", nameAt(pressedX, pressedY) ? 1 : 2, 567, 51);
    }

    void drawNameEntry(Canvas canvas, float pressedX, float pressedY) {
        sprite(canvas, "name_input", -1, 0, 0);
        int[] x = {200, 434, 591};
        int[] y = {391, 391, 524};
        for (int button = 0; button < 3; button++) {
            boolean hovered = hit(pressedX, pressedY, x[button], y[button], 167, 43);
            sprite(canvas, "name_btn", button + (hovered ? 3 : 0), x[button], y[button]);
        }
    }

    static TitleAction titleAction(float x, float y, boolean extrasUnlocked) {
        for (int button = 0; button < 5; button++) {
            if ((button != 3 || extrasUnlocked) && hit(x, y, 315, 272 + button * 55, 155, 54))
                return TitleAction.values()[button];
        }
        return null;
    }

    static int routeAt(float x, float y) {
        for (int route = 0; route < HEROES.length; route++)
            if (hit(x, y, 24 + route * 152, 94, 144, 450)) return route;
        return -1;
    }

    static int variantAt(float x, float y) {
        for (int variant = 0; variant < 3; variant++)
            if (hit(x, y, 32 + variant * 248, 128, 240, 420)) return variant;
        return -1;
    }

    static boolean nameAt(float x, float y) { return hit(x, y, 567, 51, 176, 32); }

    /** 0 = accept, 1 = cancel, 2 = restore original default names. */
    static int nameAction(float x, float y) {
        if (hit(x, y, 200, 391, 167, 43)) return 0;
        if (hit(x, y, 434, 391, 167, 43)) return 1;
        if (hit(x, y, 591, 524, 167, 43)) return 2;
        return -1;
    }

    void release() {
        for (Bitmap bitmap : images.values()) if (bitmap != null) bitmap.recycle();
        images.clear();
    }

    private static boolean hit(float x, float y, int left, int top, int width, int height) {
        return x >= left && x < left + width && y >= top && y < top + height;
    }
}
