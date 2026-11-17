// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

import android.content.res.AssetManager;
import android.os.SystemClock;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.StandardCopyOption;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashSet;
import java.util.List;
import java.util.Locale;
import java.util.Set;
import java.util.Iterator;
import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

/** Streams the signed APK's archives into seekable files, preserving the game's Save directory. */
final class BundledGame {
    interface AssetSource { InputStream open(String path) throws IOException; }
    static final String MANIFEST = "bundled-game.json";
    private static final String RECEIPT = ".bundle-state.json";

    static final class Bundle {
        final String id;
        final String name;
        final List<Resource> files = new ArrayList<>();
        long bytes;

        Bundle(String text) throws IOException {
            try {
                JSONObject json = new JSONObject(text);
                id = json.getString("id");
                name = json.getString("name");
                if (!id.matches("[a-z0-9][a-z0-9-]{0,63}") || name.isEmpty())
                    throw new IOException("游戏资源清单标识无效");
                JSONArray entries = json.getJSONArray("files");
                if (entries.length() == 0 || entries.length() > 100)
                    throw new IOException("游戏资源清单数量无效");
                Set<String> names = new HashSet<>();
                for (int index = 0; index < entries.length(); index++) {
                    Resource resource = new Resource(entries.getJSONObject(index));
                    if (!names.add(resource.name)) throw new IOException("游戏资源清单含重复文件");
                    bytes = Math.addExact(bytes, resource.size);
                    files.add(resource);
                }
            } catch (JSONException | ArithmeticException error) {
                throw new IOException("无法读取游戏资源清单", error);
            }
        }
    }

    private static final class Resource {
        final String name;
        final String asset;
        final long size;
        final String sha256;
        final String encoding;

        Resource(JSONObject json) throws JSONException, IOException {
            name = json.getString("name");
            asset = json.getString("asset");
            size = json.getLong("size");
            sha256 = json.getString("sha256").toLowerCase(Locale.ROOT);
            encoding = json.optString("encoding", "raw");
            if (!name.matches("data[0-9]+\\.ykc") || size <= 0 || !sha256.matches("[0-9a-f]{64}")
                || !(encoding.equals("raw") || encoding.equals("xz"))
                || asset.isEmpty() || asset.startsWith("/") || asset.contains("\\")
                || Arrays.asList(asset.split("/", -1)).stream().anyMatch(part -> part.isEmpty() || part.equals(".") || part.equals("..")))
                throw new IOException("游戏资源清单文件无效");
        }
    }

    private BundledGame() {}

    static boolean available(AssetManager assets) throws IOException {
        String[] root = assets.list("");
        return root != null && Arrays.asList(root).contains(MANIFEST);
    }

    static Bundle read(AssetManager assets) throws IOException {
        try (InputStream input = assets.open(MANIFEST)) { return new Bundle(readText(input)); }
    }

    static File prepare(AssetManager assets, Bundle bundle, File privateDirectory, GameImporter.Progress progress) throws IOException {
        return prepare(path -> assets.open(path, AssetManager.ACCESS_STREAMING), bundle, privateDirectory, progress);
    }

    static synchronized File prepare(AssetSource assets, Bundle bundle, File privateDirectory, GameImporter.Progress progress) throws IOException {
        File directory = new File(privateDirectory, "game-bundled-" + bundle.id);
        GameImporter.mkdir(directory);
        JSONObject receipt = readReceipt(new File(directory, RECEIPT));
        Set<String> currentNames = new HashSet<>();
        long completed = 0;
        int index = 0;
        for (Resource resource : bundle.files) {
            currentNames.add(resource.name);
            checkCancelled(progress);
            File target = new File(directory, resource.name);
            File partial = new File(directory, "." + resource.name + ".part");
            Files.deleteIfExists(partial.toPath());
            JSONObject record = receipt.optJSONObject(resource.name);
            if (target.isFile() && target.length() == resource.size && record != null
                && record.optLong("size", -1) == resource.size && resource.sha256.equals(record.optString("sha256"))) {
                completed += resource.size;
                index++;
                showProgress(progress, bundle, resource, completed, index);
                continue;
            }
            if (directory.getUsableSpace() < resource.size)
                throw new IOException("存储空间不足，准备 " + resource.name + " 需要 " + (resource.size / (1024 * 1024)) + " MiB 可用空间");
            copyResource(assets, resource, partial, bundle, completed, index + 1, progress);
            checkCancelled(progress);
            Files.move(partial.toPath(), target.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
            try {
                receipt.put(resource.name, new JSONObject().put("size", resource.size).put("sha256", resource.sha256));
            } catch (JSONException error) { throw new IOException("无法记录资源准备状态", error); }
            writeReceipt(new File(directory, RECEIPT), receipt);
            completed += resource.size;
            index++;
            showProgress(progress, bundle, resource, completed, index);
        }
        checkCancelled(progress);
        List<String> obsolete = new ArrayList<>();
        Iterator<String> names = receipt.keys();
        while (names.hasNext()) {
            String name = names.next();
            if (name.matches("data[0-9]+\\.ykc") && !currentNames.contains(name)) obsolete.add(name);
        }
        for (String name : obsolete) {
            Files.deleteIfExists(new File(directory, name).toPath());
            receipt.remove(name);
        }
        if (!obsolete.isEmpty()) writeReceipt(new File(directory, RECEIPT), receipt);
        progress.update(bundle.name + " · 资源准备完成");
        return directory;
    }

    private static void copyResource(AssetSource assets, Resource resource, File partial, Bundle bundle,
                                     long completed, int index, GameImporter.Progress progress) throws IOException {
        MessageDigest digest;
        try { digest = MessageDigest.getInstance("SHA-256"); }
        catch (NoSuchAlgorithmException error) { throw new IOException("设备不支持 SHA-256", error); }
        long copied = 0;
        long lastUpdate = 0;
        long started = SystemClock.elapsedRealtime();
        byte[] buffer = new byte[1024 * 1024];
        try (InputStream raw = assets.open(resource.asset);
             InputStream input = resource.encoding.equals("xz") ? new NativeXzInputStream(raw) : raw;
             FileOutputStream output = new FileOutputStream(partial)) {
            int count;
            while ((count = input.read(buffer)) != -1) {
                checkCancelled(progress);
                if (count > resource.size - copied) throw new IOException("资源大小不符 " + resource.name);
                output.write(buffer, 0, count);
                digest.update(buffer, 0, count);
                copied += count;
                long now = SystemClock.elapsedRealtime();
                if (now - lastUpdate >= 250) {
                    String message = progressText(bundle, resource, completed + copied, index);
                    long elapsed = now - started;
                    if (elapsed >= 500 && copied > 0) {
                        double bytesPerSecond = copied * 1000.0 / elapsed;
                        long remainingSeconds = (long)Math.ceil((bundle.bytes - completed - copied) / bytesPerSecond);
                        message += String.format(Locale.ROOT, "\n原生%s · %.1f MiB/s · 约剩%s",
                            resource.encoding.equals("xz") ? "解压" : "复制", bytesPerSecond / (1024 * 1024),
                            remainingSeconds >= 60 ? (remainingSeconds / 60) + "分" + (remainingSeconds % 60) + "秒"
                                : remainingSeconds + "秒");
                    }
                    progress.update(message);
                    lastUpdate = now;
                }
            }
            if (copied != resource.size || !hex(digest.digest()).equals(resource.sha256))
                throw new IOException("资源校验失败 " + resource.name);
            output.getFD().sync();
        } catch (IOException | RuntimeException error) {
            try { Files.deleteIfExists(partial.toPath()); }
            catch (IOException cleanup) { error.addSuppressed(cleanup); }
            throw error;
        }
    }

    private static String hex(byte[] bytes) {
        StringBuilder result = new StringBuilder(bytes.length * 2);
        for (byte value : bytes) result.append(String.format(Locale.ROOT, "%02x", value & 255));
        return result.toString();
    }

    private static void showProgress(GameImporter.Progress progress, Bundle bundle, Resource resource, long bytes, int index) {
        progress.update(progressText(bundle, resource, bytes, index));
    }

    private static String progressText(Bundle bundle, Resource resource, long bytes, int index) {
        return "正在准备 " + bundle.name + " · " + (int) (100.0 * bytes / bundle.bytes) + "%\n"
            + resource.name + "（" + index + "/" + bundle.files.size() + "）· " + (bytes / (1024 * 1024))
            + " / " + (bundle.bytes / (1024 * 1024)) + " MiB";
    }

    private static void checkCancelled(GameImporter.Progress progress) throws IOException {
        if (progress.cancelled()) throw new IOException("游戏资源准备已暂停，下次启动继续");
    }

    private static JSONObject readReceipt(File file) {
        try (InputStream input = new FileInputStream(file)) { return new JSONObject(readText(input)); }
        catch (IOException | JSONException error) { return new JSONObject(); }
    }

    private static String readText(InputStream input) throws IOException {
        ByteArrayOutputStream output = new ByteArrayOutputStream();
        byte[] buffer = new byte[4096];
        int count;
        while ((count = input.read(buffer)) != -1) {
            if (output.size() + count > 65536) throw new IOException("资源清单过大");
            output.write(buffer, 0, count);
        }
        return new String(output.toByteArray(), StandardCharsets.UTF_8);
    }

    private static void writeReceipt(File destination, JSONObject receipt) throws IOException {
        File partial = new File(destination.getParentFile(), RECEIPT + ".part");
        try (FileOutputStream output = new FileOutputStream(partial)) {
            output.write(receipt.toString().getBytes(StandardCharsets.UTF_8));
            output.getFD().sync();
        }
        Files.move(partial.toPath(), destination.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
    }
}
