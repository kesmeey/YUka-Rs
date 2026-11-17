// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

import android.content.ContentProvider;
import android.content.ContentValues;
import android.content.Intent;
import android.database.Cursor;
import android.net.Uri;
import android.os.Binder;
import android.os.Bundle;
import android.provider.DocumentsContract;
import java.io.File;
import java.io.IOException;
import java.nio.file.Files;
import java.util.UUID;

/** Test APK bootstrap grants document access before the target uses the SAF fixture. */
public final class FixtureSetupProvider extends ContentProvider {
    @Override public boolean onCreate() { return true; }

    @Override public Bundle call(String method, String argument, Bundle extras) {
        if (!method.equals("new_fixture")) throw new IllegalArgumentException("Unknown fixture operation");
        String targetPackage = getCallingPackage();
        if (targetPackage == null) throw new SecurityException("Fixture caller is unavailable");
        long identity = Binder.clearCallingIdentity();
        try {
            String name = "saf-" + UUID.randomUUID();
            File root = new File(getContext().getFilesDir(), name);
            File input = new File(root, "input");
            File saves = new File(input, "Save");
            Files.createDirectories(saves.toPath());
            Files.createDirectories(new File(root, "output").toPath());
            Files.write(new File(input, "data01.ykc").toPath(), new byte[]{'Y', 'K', 'C', '0', '0', '1', 0, (byte) 255});
            Files.write(new File(saves, "System.dat").toPath(), new byte[]{0, 17, (byte) 254, 3});
            Files.write(new File(saves, "Already.dat").toPath(), new byte[]{1, 2, 0, (byte) 255, 18});
            Files.write(new File(saves, "savedata.001").toPath(), new byte[]{7, 0, 3, (byte) 128, 9, 11});
            if (!getContext().getSharedPreferences("fixture", 0).edit().putString("root", name)
                    .putBoolean("duplicate_archive", "duplicate".equals(argument)).commit())
                throw new IOException("Cannot publish test fixture directory");
            for (String id : new String[]{"root/input", "root/input/Save", "root/output"})
                getContext().grantUriPermission(targetPackage,
                    DocumentsContract.buildTreeDocumentUri(FixtureDocumentsProvider.AUTHORITY, id),
                    Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION | Intent.FLAG_GRANT_PREFIX_URI_PERMISSION);
            return new Bundle();
        } catch (IOException error) {
            throw new IllegalStateException(error);
        } finally {
            Binder.restoreCallingIdentity(identity);
        }
    }

    @Override public Cursor query(Uri uri, String[] projection, String selection, String[] args, String sortOrder) { return null; }
    @Override public String getType(Uri uri) { return null; }
    @Override public Uri insert(Uri uri, ContentValues values) { throw new UnsupportedOperationException(); }
    @Override public int update(Uri uri, ContentValues values, String selection, String[] args) { throw new UnsupportedOperationException(); }
    @Override public int delete(Uri uri, String selection, String[] args) { throw new UnsupportedOperationException(); }
}
