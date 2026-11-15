// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

import android.app.Activity;
import android.app.Instrumentation;
import android.os.Bundle;
import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.security.MessageDigest;
import java.util.Arrays;
import java.util.HashMap;
import java.util.Locale;
import java.util.Map;
import java.util.UUID;
import org.json.JSONArray;
import org.json.JSONObject;
import org.tukaani.xz.LZMA2Options;
import org.tukaani.xz.XZOutputStream;

/** Exercises bundled installation independently of the multi-gigabyte game payload. */
public final class BundledGameInstrumentation extends Instrumentation {
    private String stage = "install";
    private final Map<String, byte[]> assets = new HashMap<>();
    private final Map<String, Integer> opens = new HashMap<>();
    private boolean cancelled;
    private final GameImporter.Progress progress = new GameImporter.Progress() {
        public void update(String message) { }
        public boolean cancelled() { return cancelled; }
    };

    @Override public void onCreate(Bundle arguments) { super.onCreate(arguments); start(); }
    @Override public void onStart() { new Thread(this::runTest, "Yuka bundled install regression").start(); }

    private void runTest() {
        Bundle result = new Bundle();
        File root = new File(getTargetContext().getCacheDir(), "bundle-test-" + UUID.randomUUID());
        int code = Activity.RESULT_CANCELED;
        try {
            GameImporter.mkdir(root);
            assets.put("bundled/data01.ykc", "first archive".getBytes(StandardCharsets.UTF_8));
            assets.put("bundled/data02.ykc", "second archive".getBytes(StandardCharsets.UTF_8));
            BundledGame.Bundle bundle = manifest("fixture", "data01.ykc", "data02.ykc");
            File installed = BundledGame.prepare(this::open, bundle, root, progress);
            require(opens.size() == 2, "first install did not open both assets");
            assertBytes(new File(installed, "data01.ykc"), assets.get("bundled/data01.ykc"));
            assertBytes(new File(installed, "data02.ykc"), assets.get("bundled/data02.ykc"));
            File save = new File(installed, "Save/savedata.002");
            GameImporter.mkdir(save.getParentFile());
            byte[] savedBytes = new byte[]{0, 1, (byte) 255, 4};
            Files.write(save.toPath(), savedBytes);

            stage = "fast_restart";
            opens.clear();
            Files.write(new File(installed, ".data01.ykc.part").toPath(), new byte[]{1, 2});
            BundledGame.prepare(this::open, bundle, root, progress);
            require(opens.isEmpty(), "unchanged bundle reopened a large asset");
            require(!new File(installed, ".data01.ykc.part").exists(), "abandoned partial file remains");
            assertBytes(save, savedBytes);

            stage = "interrupted_resume";
            BundledGame.Bundle interrupted = manifest("interrupted", "data01.ykc", "data02.ykc");
            boolean interruptedAsExpected = false;
            try {
                BundledGame.prepare(path -> {
                    if (path.endsWith("data02.ykc")) cancelled = true;
                    return open(path);
                }, interrupted, root, progress);
            } catch (IOException expected) { interruptedAsExpected = true; }
            require(interruptedAsExpected, "cancelled extraction unexpectedly succeeded");
            File resumedDirectory = new File(root, "game-bundled-interrupted");
            require(new File(resumedDirectory, "data01.ykc").isFile(), "completed archive was lost after interruption");
            require(!new File(resumedDirectory, "data02.ykc").exists(), "partial archive was published");
            cancelled = false;
            opens.clear();
            BundledGame.prepare(this::open, interrupted, root, progress);
            require(!opens.containsKey("bundled/data01.ykc"), "resume recopied the completed archive");
            require(opens.get("bundled/data02.ykc") == 1, "resume did not copy the incomplete archive");

            stage = "update_preserves_save";
            assets.put("bundled/data01.ykc", "updated first archive".getBytes(StandardCharsets.UTF_8));
            BundledGame.Bundle updated = manifest("fixture", "data01.ykc", "data02.ykc");
            opens.clear();
            BundledGame.prepare(this::open, updated, root, progress);
            require(opens.size() == 1 && opens.containsKey("bundled/data01.ykc"), "update recopied unchanged content");
            assertBytes(new File(installed, "data01.ykc"), assets.get("bundled/data01.ykc"));
            assertBytes(save, savedBytes);

            stage = "checksum_failure_preserves_current";
            byte[] validArchive = assets.get("bundled/data01.ykc");
            assets.put("bundled/data01.ykc", "next content".getBytes(StandardCharsets.UTF_8));
            BundledGame.Bundle next = manifest("fixture", "data01.ykc", "data02.ykc");
            assets.put("bundled/data01.ykc", new byte["next content".length()]);
            boolean checksumRejected = false;
            try { BundledGame.prepare(this::open, next, root, progress); }
            catch (IOException expected) { checksumRejected = true; }
            require(checksumRejected, "bad checksum unexpectedly succeeded");
            assertBytes(new File(installed, "data01.ykc"), validArchive);
            assertBytes(save, savedBytes);
            require(!new File(installed, ".data01.ykc.part").exists(), "failed checksum left a partial resource");

            stage = "archive_set_update";
            assets.put("bundled/data01.ykc", validArchive);
            BundledGame.prepare(this::open, manifest("fixture", "data01.ykc"), root, progress);
            require(!new File(installed, "data02.ykc").exists(), "obsolete archive would override the new package");
            assertBytes(save, savedBytes);
            stage = "xz_extraction";
            byte[] original = "compressed bundled archive".getBytes(StandardCharsets.UTF_8);
            ByteArrayOutputStream compressed = new ByteArrayOutputStream();
            try (XZOutputStream output = new XZOutputStream(compressed, new LZMA2Options(3))) { output.write(original); }
            byte[] xzBytes = compressed.toByteArray();
            assets.put("bundled/data01.ykc.xz", xzBytes);
            JSONObject xzEntry = entry("data01.ykc", "bundled/data01.ykc.xz", original).put("encoding", "xz");
            BundledGame.Bundle xzBundle = new BundledGame.Bundle(new JSONObject().put("id", "xz-fixture")
                .put("name", "XZ 合成测试").put("files", new JSONArray().put(xzEntry)).toString());
            File xzDirectory = BundledGame.prepare(this::open, xzBundle, root, progress);
            assertBytes(new File(xzDirectory, "data01.ykc"), original);

            stage = "xz_truncated_retry";
            Files.delete(new File(xzDirectory, "data01.ykc").toPath());
            assets.put("bundled/data01.ykc.xz", Arrays.copyOf(xzBytes, xzBytes.length - 8));
            boolean truncatedRejected = false;
            try { BundledGame.prepare(this::open, xzBundle, root, progress); }
            catch (IOException expected) { truncatedRejected = true; }
            require(truncatedRejected, "truncated XZ was accepted after decoding its payload");
            require(!new File(xzDirectory, "data01.ykc").exists(), "truncated XZ published a resource");
            assets.put("bundled/data01.ykc.xz", xzBytes);
            BundledGame.prepare(this::open, xzBundle, root, progress);
            assertBytes(new File(xzDirectory, "data01.ykc"), original);
            result.putString("summary", "{\"passed\":true,\"bundled_install\":true,\"interrupted_resume\":true,"
                + "\"unchanged_restart\":true,\"update_preserves_saves\":true,\"checksum_failure_preserves_current\":true,"
                + "\"xz_extraction\":true,\"xz_truncated_retry\":true}");
            code = Activity.RESULT_OK;
        } catch (Throwable error) { result.putString("failure", stage + " - " + error); }
        finally {
            try { GameImporter.removePrivateTree(root, getTargetContext().getCacheDir()); }
            catch (IOException error) { result.putString("cleanup_error", error.toString()); }
            finish(code, result);
        }
    }

    private ByteArrayInputStream open(String name) throws IOException {
        byte[] bytes = assets.get(name);
        if (bytes == null) throw new IOException("fixture asset missing " + name);
        opens.put(name, opens.getOrDefault(name, 0) + 1);
        return new ByteArrayInputStream(bytes);
    }

    private BundledGame.Bundle manifest(String id, String... files) throws Exception {
        JSONArray entries = new JSONArray();
        for (String name : files) {
            String asset = "bundled/" + name;
            entries.put(entry(name, asset, assets.get(asset)));
        }
        return new BundledGame.Bundle(new JSONObject().put("id", id).put("name", "合成安装测试").put("files", entries).toString());
    }

    private static JSONObject entry(String name, String asset, byte[] bytes) throws Exception {
        StringBuilder hash = new StringBuilder();
        for (byte value : MessageDigest.getInstance("SHA-256").digest(bytes))
            hash.append(String.format(Locale.ROOT, "%02x", value & 255));
        return new JSONObject().put("name", name).put("asset", asset).put("size", bytes.length).put("sha256", hash.toString());
    }

    private static void assertBytes(File file, byte[] expected) throws IOException {
        require(Arrays.equals(expected, Files.readAllBytes(file.toPath())), "file contents changed " + file.getName());
    }

    private static void require(boolean condition, String message) throws IOException {
        if (!condition) throw new IOException(message);
    }
}
