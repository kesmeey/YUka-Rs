// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

import android.content.ContentResolver;
import android.database.Cursor;
import android.net.Uri;
import android.provider.DocumentsContract;
import java.io.File;
import java.io.FileOutputStream;
import java.io.FileInputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Locale;
import java.util.Set;
import java.util.UUID;

final class GameImporter {
    interface Progress {
        void update(String message);
        boolean cancelled();
    }
    private static final class Document {
        final Uri uri;
        final String id;
        final String name;
        final boolean directory;
        Document(Uri uri, String id, String name, boolean directory) {
            this.uri = uri; this.id = id; this.name = name; this.directory = directory;
        }
    }

    private GameImporter() {}

    static synchronized File importGame(ContentResolver resolver, Uri tree, File privateDirectory, Progress progress) throws IOException {
        File[] abandoned = privateDirectory.listFiles(file -> file.isDirectory()
            && file.getName().matches("\\.import-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}"));
        if (abandoned == null) throw new IOException("无法读取应用资源目录");
        for (File directory : abandoned) {
            progress.update("正在清理上次未完成的导入");
            removePrivateTree(directory, privateDirectory);
        }
        String token = UUID.randomUUID().toString();
        File staging = new File(privateDirectory, ".import-" + token);
        File installed = new File(privateDirectory, "game-" + token);
        mkdir(staging);
        try {
            List<Document> documents = children(resolver, tree, DocumentsContract.getTreeDocumentId(tree));
            List<Document> archives = new ArrayList<>();
            Document saves = null;
            for (Document document : documents) {
                String name = document.name.toLowerCase(Locale.ROOT);
                if (!document.directory && name.startsWith("data") && name.endsWith(".ykc")) archives.add(document);
                if (document.directory && name.equals("save")) saves = document;
            }
            if (archives.isEmpty()) throw new IOException("所选目录没有 data*.ykc，请选择游戏资源所在的文件夹");
            archives.sort((a, b) -> a.name.compareToIgnoreCase(b.name));
            int index = 0;
            for (Document document : archives) {
                copy(resolver, document.uri, new File(staging, document.name), document.name + "（" + (++index) + "/" + archives.size() + "）", progress);
            }
            if (saves != null) copyDirectory(resolver, tree, saves.id, new File(staging, "Save"), new HashSet<>(), progress);
            if (progress.cancelled()) throw new IOException("导入已取消");
            if (!staging.renameTo(installed)) throw new IOException("无法完成资源目录写入");
            return installed;
        } catch (IOException | RuntimeException error) {
            try { removePrivateTree(staging, privateDirectory); }
            catch (IOException cleanupError) { error.addSuppressed(cleanupError); }
            throw error;
        }
    }

    static File importSave(ContentResolver resolver, Uri document, File directory, Progress progress) throws IOException {
        mkdir(directory);
        File destination = new File(directory, "import-" + UUID.randomUUID() + ".dat");
        try {
            copy(resolver, document, destination, "PC 存档", progress);
            return destination;
        } catch (IOException | RuntimeException error) {
            if (destination.exists()) destination.delete();
            throw error;
        }
    }

    static Uri exportSaves(ContentResolver resolver, Uri tree, File saves, Progress progress) throws IOException {
        if (!saves.isDirectory()) throw new IOException("尚无存档目录");
        Uri parent = DocumentsContract.buildDocumentUriUsingTree(tree, DocumentsContract.getTreeDocumentId(tree));
        String name = "Save-" + new java.text.SimpleDateFormat("yyyyMMdd-HHmmss", Locale.ROOT).format(new java.util.Date())
            + "-" + UUID.randomUUID().toString().substring(0, 8);
        Uri destination = DocumentsContract.createDocument(resolver, parent, DocumentsContract.Document.MIME_TYPE_DIR, name);
        if (destination == null) throw new IOException("所选位置无法创建存档目录");
        try {
            exportDirectory(resolver, saves, destination, progress);
            progress.update("已导出完整存档目录 " + name);
            return destination;
        } catch (IOException | RuntimeException error) {
            try { DocumentsContract.deleteDocument(resolver, destination); }
            catch (Exception cleanupError) { error.addSuppressed(cleanupError); }
            throw error;
        }
    }

    private static void exportDirectory(ContentResolver resolver, File source, Uri destination, Progress progress) throws IOException {
        File[] files = source.listFiles();
        if (files == null) throw new IOException("无法读取存档目录");
        java.util.Arrays.sort(files, java.util.Comparator.comparing(File::getName));
        for (File file : files) {
            if (progress.cancelled()) throw new IOException("导出已取消");
            String mime = file.isDirectory() ? DocumentsContract.Document.MIME_TYPE_DIR : "application/octet-stream";
            Uri document = DocumentsContract.createDocument(resolver, destination, mime, file.getName());
            if (document == null) throw new IOException("无法导出 " + file.getName());
            if (file.isDirectory()) { exportDirectory(resolver, file, document, progress); continue; }
            try (InputStream input = new FileInputStream(file); OutputStream output = resolver.openOutputStream(document, "w")) {
                if (output == null) throw new IOException("无法写入导出文件 " + file.getName());
                byte[] buffer = new byte[64 * 1024];
                int count;
                while ((count = input.read(buffer)) != -1) {
                    if (progress.cancelled()) throw new IOException("导出已取消");
                    output.write(buffer, 0, count);
                }
                output.flush();
            }
            progress.update("正在导出 " + file.getName());
        }
    }

    private static void copyDirectory(ContentResolver resolver, Uri tree, String id, File destination,
                                      Set<String> visited, Progress progress) throws IOException {
        if (!visited.add(id)) throw new IOException("存档目录包含循环引用");
        mkdir(destination);
        for (Document document : children(resolver, tree, id)) {
            File target = new File(destination, document.name);
            if (document.directory) copyDirectory(resolver, tree, document.id, target, visited, progress);
            else copy(resolver, document.uri, target, "Save/" + document.name, progress);
        }
    }

    private static List<Document> children(ContentResolver resolver, Uri tree, String id) throws IOException {
        Uri uri = DocumentsContract.buildChildDocumentsUriUsingTree(tree, id);
        String[] columns = {DocumentsContract.Document.COLUMN_DOCUMENT_ID, DocumentsContract.Document.COLUMN_DISPLAY_NAME,
                            DocumentsContract.Document.COLUMN_MIME_TYPE};
        List<Document> result = new ArrayList<>();
        try (Cursor cursor = resolver.query(uri, columns, null, null, null)) {
            if (cursor == null) throw new IOException("无法读取所选目录");
            while (cursor.moveToNext()) {
                String childId = cursor.getString(0);
                String name = cursor.getString(1);
                if (name == null || name.isEmpty() || name.equals(".") || name.equals("..") || name.contains("/") || name.contains("\\"))
                    throw new IOException("资源文件名无效");
                result.add(new Document(DocumentsContract.buildDocumentUriUsingTree(tree, childId), childId, name,
                    DocumentsContract.Document.MIME_TYPE_DIR.equals(cursor.getString(2))));
            }
        }
        return result;
    }

    private static void copy(ContentResolver resolver, Uri source, File destination, String label, Progress progress) throws IOException {
        long count = 0;
        long lastUpdate = 0;
        byte[] buffer = new byte[1024 * 1024];
        if (!destination.createNewFile()) throw new IOException("导入目录含有重复文件名 " + destination.getName());
        try (InputStream input = resolver.openInputStream(source); FileOutputStream output = new FileOutputStream(destination)) {
            if (input == null) throw new IOException("无法读取 " + label);
            int length;
            while ((length = input.read(buffer)) != -1) {
                if (progress.cancelled()) throw new IOException("导入已取消");
                output.write(buffer, 0, length);
                count += length;
                long now = android.os.SystemClock.elapsedRealtime();
                if (now - lastUpdate >= 250) {
                    progress.update("正在导入 " + label + " · " + (count / (1024 * 1024)) + " MiB");
                    lastUpdate = now;
                }
            }
            output.getFD().sync();
        }
        // The original menu displays filesystem mtime as the slot's save date.
        try (Cursor cursor = resolver.query(source, new String[]{DocumentsContract.Document.COLUMN_LAST_MODIFIED}, null, null, null)) {
            if (cursor != null && cursor.moveToFirst() && !cursor.isNull(0) && cursor.getLong(0) > 0)
                destination.setLastModified(cursor.getLong(0));
        } catch (RuntimeException ignored) { /* Some document providers do not expose a timestamp. */ }
        progress.update("已导入 " + label);
    }

    static void mkdir(File directory) throws IOException {
        if (!directory.isDirectory() && !directory.mkdirs()) throw new IOException("无法创建 " + directory.getName());
    }

    static void removePrivateTree(File directory, File root) throws IOException {
        String rootPath = root.getCanonicalPath() + File.separator;
        if (!directory.getCanonicalPath().startsWith(rootPath)) throw new IOException("清理目录超出应用资源目录");
        if (directory.isDirectory()) {
            File[] files = directory.listFiles();
            if (files == null) throw new IOException("无法读取待清理目录");
            for (File file : files) removePrivateTree(file, root);
        }
        if (directory.exists() && !directory.delete()) throw new IOException("无法清理临时资源 " + directory.getName());
    }
}
