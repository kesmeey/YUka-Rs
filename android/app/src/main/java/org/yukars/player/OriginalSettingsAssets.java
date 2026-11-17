// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

import android.graphics.Bitmap;
import java.util.HashMap;
import java.util.Map;

/** Lazily loaded original option backgrounds and sprite sequences, shared by all eight pages. */
final class OriginalSettingsAssets {
    static final String[] BACKGROUNDS = {"op_base_screen", "op_base_text01", "op_base_text02", "op_base_sound",
        "op_base_voice01", "op_base_voice02", "op_base_voice03", "op_base_voice04"};
    private final Map<String, Bitmap> images = new HashMap<>();

    static OriginalSettingsAssets load(long session) {
        OriginalSettingsAssets result = new OriginalSettingsAssets();
        for (String name : BACKGROUNDS) result.load(session, name, 0);
        result.load(session, "op_btn01", 51);
        result.load(session, "op_page", 12);
        result.load(session, "op_var", 1);
        result.load(session, "op_sound_test", 6);
        return result;
    }

    private void load(long session, String name, int count) {
        String path = "ykg\\system\\option\\" + name + ".ykg";
        if (count == 0) images.put(name + ":-1", OriginalAssets.decode(NativeBridge.image(session, path, -1)));
        for (int index = 0; index < count; index++)
            images.put(name + ":" + index, OriginalAssets.decode(NativeBridge.image(session, path, index)));
    }

    Bitmap bitmap(String name, int sequence) {
        Bitmap bitmap = images.get(name + ":" + sequence);
        if (bitmap == null) throw new IllegalArgumentException("Missing option image " + name + " sequence " + sequence);
        return bitmap;
    }
}
