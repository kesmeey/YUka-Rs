// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

import android.database.Cursor;
import android.database.MatrixCursor;
import android.os.CancellationSignal;
import android.os.ParcelFileDescriptor;
import android.provider.DocumentsContract;
import android.provider.DocumentsProvider;
import java.io.File;
import java.io.FileNotFoundException;
import java.io.IOException;
import java.nio.file.Files;

/** Synthetic SAF documents exist only inside the test APK's own private directory. */
public final class FixtureDocumentsProvider extends DocumentsProvider {
    static final String AUTHORITY = "org.yukars.player.tests.documents";
    private static final String[] DOCUMENT_COLUMNS = {
        DocumentsContract.Document.COLUMN_DOCUMENT_ID, DocumentsContract.Document.COLUMN_DISPLAY_NAME,
        DocumentsContract.Document.COLUMN_MIME_TYPE, DocumentsContract.Document.COLUMN_FLAGS, DocumentsContract.Document.COLUMN_SIZE
    };

    @Override public boolean onCreate() { return true; }

    private File root() {
        String name = getContext().getSharedPreferences("fixture", 0).getString("root", "uninitialized");
        return new File(getContext().getFilesDir(), name);
    }

    private File file(String id) throws FileNotFoundException {
        try {
            File root = root().getCanonicalFile();
            File file = id.equals("root") ? root : new File(root, id.startsWith("root/") ? id.substring(5) : "../invalid").getCanonicalFile();
            if (!file.equals(root) && !file.getPath().startsWith(root.getPath() + File.separator)) throw new FileNotFoundException("invalid document id");
            return file;
        } catch (IOException error) { throw new FileNotFoundException(error.toString()); }
    }

    private String id(File file) throws FileNotFoundException {
        try {
            File canonicalRoot = root().getCanonicalFile();
            File canonicalFile = file.getCanonicalFile();
            if (canonicalFile.equals(canonicalRoot)) return "root";
            if (!canonicalFile.getPath().startsWith(canonicalRoot.getPath() + File.separator))
                throw new FileNotFoundException("document outside test fixture");
            return "root/" + canonicalRoot.toPath().relativize(canonicalFile.toPath()).toString().replace(File.separatorChar, '/');
        } catch (IOException | IllegalArgumentException error) { throw new FileNotFoundException(error.toString()); }
    }

    private void row(MatrixCursor cursor, File file) throws FileNotFoundException {
        MatrixCursor.RowBuilder row = cursor.newRow();
        for (String column : cursor.getColumnNames()) {
            switch (column) {
                case DocumentsContract.Document.COLUMN_DOCUMENT_ID: row.add(id(file)); break;
                case DocumentsContract.Document.COLUMN_DISPLAY_NAME: row.add(file.getName()); break;
                case DocumentsContract.Document.COLUMN_MIME_TYPE: row.add(file.isDirectory() ? DocumentsContract.Document.MIME_TYPE_DIR : "application/octet-stream"); break;
                case DocumentsContract.Document.COLUMN_FLAGS: row.add(file.isDirectory() ? DocumentsContract.Document.FLAG_DIR_SUPPORTS_CREATE : DocumentsContract.Document.FLAG_SUPPORTS_WRITE | DocumentsContract.Document.FLAG_SUPPORTS_DELETE); break;
                case DocumentsContract.Document.COLUMN_SIZE: row.add(file.length()); break;
                default: row.add(null);
            }
        }
    }

    @Override public Cursor queryRoots(String[] projection) {
        String[] columns = projection == null ? new String[]{DocumentsContract.Root.COLUMN_ROOT_ID, DocumentsContract.Root.COLUMN_DOCUMENT_ID, DocumentsContract.Root.COLUMN_TITLE, DocumentsContract.Root.COLUMN_FLAGS} : projection;
        MatrixCursor cursor = new MatrixCursor(columns);
        MatrixCursor.RowBuilder row = cursor.newRow();
        for (String column : columns) {
            if (column.equals(DocumentsContract.Root.COLUMN_ROOT_ID) || column.equals(DocumentsContract.Root.COLUMN_DOCUMENT_ID)) row.add("root");
            else if (column.equals(DocumentsContract.Root.COLUMN_TITLE)) row.add("Yuka test fixture");
            else if (column.equals(DocumentsContract.Root.COLUMN_FLAGS)) row.add(DocumentsContract.Root.FLAG_SUPPORTS_CREATE);
            else row.add(null);
        }
        return cursor;
    }

    @Override public Cursor queryDocument(String documentId, String[] projection) throws FileNotFoundException {
        MatrixCursor cursor = new MatrixCursor(projection == null ? DOCUMENT_COLUMNS : projection);
        row(cursor, file(documentId));
        return cursor;
    }

    @Override public Cursor queryChildDocuments(String parentDocumentId, String[] projection, String sortOrder) throws FileNotFoundException {
        MatrixCursor cursor = new MatrixCursor(projection == null ? DOCUMENT_COLUMNS : projection);
        File[] children = file(parentDocumentId).listFiles();
        if (children == null) throw new FileNotFoundException(parentDocumentId);
        for (File child : children) {
            row(cursor, child);
            if (parentDocumentId.equals("root/input") && child.getName().equals("data01.ykc")
                    && getContext().getSharedPreferences("fixture", 0).getBoolean("duplicate_archive", false)) row(cursor, child);
        }
        return cursor;
    }

    @Override public ParcelFileDescriptor openDocument(String documentId, String mode, CancellationSignal signal) throws FileNotFoundException {
        return ParcelFileDescriptor.open(file(documentId), ParcelFileDescriptor.parseMode(mode));
    }

    @Override public String createDocument(String parentDocumentId, String mimeType, String displayName) throws FileNotFoundException {
        if (displayName.contains("/") || displayName.contains("\\") || displayName.equals("..")) throw new FileNotFoundException("invalid name");
        File destination = new File(file(parentDocumentId), displayName);
        try {
            if (DocumentsContract.Document.MIME_TYPE_DIR.equals(mimeType)) Files.createDirectories(destination.toPath());
            else if (!destination.createNewFile()) throw new FileNotFoundException("duplicate document");
            return id(destination);
        } catch (IOException error) { throw new FileNotFoundException(error.toString()); }
    }

    @Override public void deleteDocument(String documentId) throws FileNotFoundException {
        try { deleteTree(file(documentId)); }
        catch (IOException error) { throw new FileNotFoundException(error.toString()); }
    }

    @Override public boolean isChildDocument(String parentDocumentId, String documentId) {
        try { return file(documentId).getPath().startsWith(file(parentDocumentId).getPath() + File.separator); }
        catch (FileNotFoundException error) { return false; }
    }

    private static void deleteTree(File file) throws IOException {
        File[] children = file.listFiles();
        if (children != null) for (File child : children) deleteTree(child);
        if (file.exists() && !file.delete()) throw new IOException("cannot delete test fixture");
    }
}
