// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

final class NativeBridge {
    static { System.loadLibrary("yuka_rs"); }
    private NativeBridge() {}
    static native long create(String gameDirectory, String saveDirectory, String audioCacheDirectory);
    static native boolean isDousei(long session);
    static native void command(long session, String operation, String argument);
    static native void tick(long session);
    static native void render(long session, int[] argbPixels);
    static native String state(long session);
    static native String history(long session);
    static native String historyVoice(long session, long id);
    static native String historyPlayback(long session, long id);
    static native String settingsSample(long session, int index);
    static native String extras(long session);
    static native int[] extrasImage(long session, int group, int item, int variant);
    static native int[] image(long session, String resource, int sequence);
    static native String saveInfo(long session, String absolutePath);
    static native int[] saveThumbnail(long session, String absolutePath);
    static native void destroy(long session);
}
