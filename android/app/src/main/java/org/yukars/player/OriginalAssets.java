// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

import android.graphics.Bitmap;
import java.util.HashMap;
import java.util.Map;

/** Original YKG image planes and the first frame of each scripted sprite sequence. */
final class OriginalAssets {
    private final Map<String, Bitmap> images = new HashMap<>();

    static OriginalAssets load(long session) {
        OriginalAssets assets = new OriginalAssets();
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
        assets.load(session, "ykg\\title\\", "t_btn", 10);
        assets.load(session, "ykg\\title\\", "t_btn_msg", 5);
        assets.load(session, "ykg\\title\\", "name_input", 0);
        assets.load(session, "ykg\\system\\window\\", "window", 0);
        assets.load(session, "ykg\\system\\window\\", "name", 0);
        assets.load(session, "ykg\\system\\window\\", "sys_btn01", 15);
        assets.load(session, "ykg\\system\\window\\", "sys_btn02", 12);
        assets.load(session, "ykg\\system\\window\\", "sys_btn_help", 9);
        assets.load(session, "ykg\\system\\select\\", "select", 0);
        assets.load(session, "ykg\\system\\history\\", "his_base", 0);
        assets.load(session, "ykg\\system\\history\\", "his_btn", 6);
        assets.load(session, "ykg\\system\\history\\", "his_gage", 2);
        assets.load(session, "ykg\\system\\history\\", "sb_rireki_voice", 2);
        return assets;
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
