// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

import java.io.IOException;
import java.io.InputStream;

/** Uses the release-mode Rust decoder without moving the whole archive into memory. */
final class NativeXzInputStream extends InputStream {
    static { System.loadLibrary("yuka_rs"); }
    private static native long create() throws IOException;
    private static native long decode(long handle, byte[] input, int offset, int count,
                                      byte[] output, boolean eof) throws IOException;
    private static native void release(long handle) throws IOException;
    private final InputStream source;
    private final byte[] input = new byte[256 * 1024];
    private final byte[] output = new byte[1024 * 1024];
    private long handle;
    private int inputOffset;
    private int inputSize;
    private int outputOffset;
    private int outputSize;
    private boolean eof;
    private boolean finished;

    NativeXzInputStream(InputStream source) throws IOException {
        this.source = source;
        handle = create();
    }

    @Override public int read() throws IOException {
        if (!fill()) return -1;
        return output[outputOffset++] & 255;
    }

    @Override public int read(byte[] bytes, int offset, int length) throws IOException {
        if (bytes == null) throw new NullPointerException("bytes");
        if (offset < 0 || length < 0 || offset > bytes.length - length)
            throw new IndexOutOfBoundsException();
        if (length == 0) return 0;
        if (!fill()) return -1;
        int count = Math.min(length, outputSize - outputOffset);
        System.arraycopy(output, outputOffset, bytes, offset, count);
        outputOffset += count;
        return count;
    }

    private boolean fill() throws IOException {
        if (handle == 0) throw new IOException("XZ stream is closed");
        while (outputOffset == outputSize && !finished) {
            if (inputOffset == inputSize && !eof) {
                inputSize = source.read(input);
                inputOffset = 0;
                if (inputSize < 0) { inputSize = 0; eof = true; }
                else if (inputSize == 0) continue;
            }
            long result = decode(handle, input, inputOffset, inputSize - inputOffset, output, eof);
            int consumed = (int) ((result >>> 32) & 0x7fffffffL);
            outputSize = (int) result;
            outputOffset = 0;
            inputOffset += consumed;
            finished = result < 0;
            if (consumed == 0 && outputSize == 0 && !finished)
                throw new IOException("XZ decoder made no progress");
        }
        return outputOffset < outputSize;
    }

    @Override public void close() throws IOException {
        long closing = handle;
        handle = 0;
        try { if (closing != 0) release(closing); }
        finally { source.close(); }
    }
}
