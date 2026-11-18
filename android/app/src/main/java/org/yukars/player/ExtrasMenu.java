// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

import android.graphics.Bitmap;
import android.graphics.Canvas;

interface ExtrasMenu {
    interface Listener {
        void close();
        void image(int character, int item, int variant);
        void replay(int character, int scene, int style);
        void music(int track);
    }

    void draw(Canvas canvas, float pressedX, float pressedY);
    default int soundSlot(float x, float y, boolean hover) { return -1; }
    void tap(float x, float y);
    void back();
    void release();
    int musicTrack();
    void showImage(Bitmap image, int character, int item, int variant);
}
