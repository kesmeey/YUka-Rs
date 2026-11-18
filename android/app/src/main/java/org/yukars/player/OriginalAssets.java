// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

import android.graphics.Bitmap;
import java.util.HashMap;
import java.util.Map;

/** Original YKG image planes and the first frame of each scripted sprite sequence. */
final class OriginalAssets {
    private final Map<String, Bitmap> images = new HashMap<>();
    private final boolean dousei;
    private final Bitmap[][] playbackIcons = new Bitmap[2][12];

    private OriginalAssets(boolean dousei) { this.dousei = dousei; }

    static OriginalAssets load(long session) {
        return load(session, false);
    }

    static OriginalAssets loadDousei(long session) {
        return load(session, true);
    }

    boolean isDousei() { return dousei; }

    private static OriginalAssets load(long session, boolean dousei) {
        OriginalAssets assets = new OriginalAssets(dousei);
        String option = "ykg\\system\\option\\";
        assets.load(session, option, "op_base", 0);
        assets.load(session, option, "op_title", 10);
        assets.load(session, option, "op_tab", 33);
        assets.load(session, option, "save_item", 2);
        assets.load(session, option, "load_item", 2);
        assets.load(session, option, "save_page", 40);
        assets.load(session, option, "load_page", 40);
        assets.load(session, option, "autosave", 0);
        assets.load(session, option, "YesNo_Base", 0);
        assets.load(session, option, "YesNo_btn", 4);
        for (int index = 1; index <= 7; index++)
            assets.load(session, option, "YesNo_P0" + index, 0);
        if (!dousei) {
            assets.load(session, "ykg\\title\\", "t_btn", 10);
            assets.load(session, "ykg\\title\\", "t_btn_msg", 5);
            assets.load(session, "ykg\\title\\", "name_input", 0);
        }
        assets.load(session, "ykg\\system\\window\\", "window", 0);
        assets.load(session, "ykg\\system\\window\\", "name", 0);
        // The FD has normal/hover states only. The base game also includes disabled states.
        assets.load(session, "ykg\\system\\window\\", "sys_btn01", dousei ? 10 : 15);
        assets.load(session, "ykg\\system\\window\\", "sys_btn02", dousei ? 8 : 12);
        assets.load(session, "ykg\\system\\window\\", "sys_btn_help", 9);
        assets.load(session, "ykg\\system\\select\\", "select", 0);
        assets.load(session, "ykg\\system\\history\\", "his_base", 0);
        assets.load(session, "ykg\\system\\history\\", "his_btn", 6);
        assets.load(session, "ykg\\system\\history\\", "his_gage", 2);
        assets.load(session, "ykg\\system\\history\\", "sb_rireki_voice", 2);
        if (dousei) {
            assets.load(session, "ykg\\system\\icon\\", "icon_saq", 0);
            Bitmap sheet = assets.bitmap("icon_saq", -1);
            // createIcon.yks uses sequences 0/1; both have twelve 149x25 frames.
            for (int sequence = 0; sequence < 2; sequence++)
                for (int frame = 0; frame < 12; frame++)
                    assets.playbackIcons[sequence][frame] = Bitmap.createBitmap(sheet, sequence * 149, frame * 25, 149, 25);
        }
        return assets;
    }

    Bitmap playbackIcon(boolean skipping, long elapsedMillis) {
        if (!dousei) throw new IllegalStateException("FD playback icon requested for another game");
        long phase = Math.max(0, elapsedMillis) % 1830;
        int frame = 0;
        for (; frame < 11; frame++) {
            int duration = frame == 5 ? 1500 : 30;
            if (phase < duration) break;
            phase -= duration;
        }
        return playbackIcons[skipping ? 1 : 0][frame];
    }

    private void load(long session, String directory, String name, int sequences) {
        if (sequences == 0) images.put(name + ":-1", decode(NativeBridge.image(session, directory + name + ".ykg", -1)));
        for (int sequence = 0; sequence < sequences; sequence++)
            images.put(name + ":" + sequence, decode(NativeBridge.image(session, directory + name + ".ykg", sequence)));
    }

    Bitmap bitmap(String name, int sequence) {
        Bitmap bitmap = images.get(name + ":" + sequence);
        if (bitmap == null) throw new IllegalArgumentException("Missing UI image " + name + " sequence " + sequence);
        return bitmap;
    }

    static Bitmap decode(int[] pixels) {
        if (pixels.length == 0) return null;
        if (pixels.length < 2 || pixels[0] <= 0 || pixels[1] <= 0
                || (long) pixels[0] * pixels[1] != pixels.length - 2)
            throw new IllegalArgumentException("Invalid native UI image dimensions");
        return Bitmap.createBitmap(pixels, 2, pixels[0], pixels[0], pixels[1], Bitmap.Config.ARGB_8888);
    }
}
