// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

final class NativeBridge {
    static { System.loadLibrary("yuka_rs"); }
    private NativeBridge() {}
    static native long create(String gameDirectory, String saveDirectory, String audioCacheDirectory);
    static native void command(long session, String operation, String argument);
    static native void tick(long session);
    static native void render(long session, int[] argbPixels);
    static native String state(long session);
    static native void destroy(long session);
}
