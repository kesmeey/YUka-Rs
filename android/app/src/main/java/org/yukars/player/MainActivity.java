// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

import android.app.Activity;
import android.app.AlertDialog;
import android.content.Intent;
import android.content.SharedPreferences;
import android.database.Cursor;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.RectF;
import android.graphics.Typeface;
import android.net.Uri;
import android.os.Bundle;
import android.os.Build;
import android.os.Handler;
import android.os.HandlerThread;
import android.provider.DocumentsContract;
import android.text.Layout;
import android.text.StaticLayout;
import android.text.TextPaint;
import android.view.MotionEvent;
import android.view.View;
import android.view.WindowManager;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.window.OnBackInvokedDispatcher;
import android.widget.Toast;
import java.io.File;
import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Comparator;
import java.util.List;
import java.util.Locale;
import org.json.JSONArray;
import org.json.JSONObject;

public final class MainActivity extends Activity {
    private static final int PICK_GAME = 1;
    private static final int PICK_SAVE = 2;
    private static final int PICK_EXPORT = 3;
    private static final int WIDTH = 800;
    private static final int HEIGHT = 600;
    private enum Mode { TITLE, PLAYING, PAUSED }

    private HandlerThread engineThread;
    private Handler engine;
    private final Handler ui = new Handler(android.os.Looper.getMainLooper());
    private AudioHost audio;
    private long session;
    private final int[] pixels = new int[WIDTH * HEIGHT];
    private GameView gameView;
    private SharedPreferences preferences;
    private volatile boolean destroyed;
    private volatile boolean background;
    private volatile boolean busy;
    private volatile boolean nativeReady;
    private volatile boolean actionPending;
    private volatile boolean engineFailed;
    private volatile Mode mode = Mode.TITLE;
    private volatile String message = "选择游戏目录，导入 data*.ykc 和 Save";
    private volatile File saveDirectory;
    private volatile File[] saveFiles = new File[0];
    private long lastSaveScan;
    private String lastAudioWarning = "";

    @Override public void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        preferences = getSharedPreferences("player", MODE_PRIVATE);
        gameView = new GameView();
        setContentView(gameView);
        getWindow().getDecorView().post(this::immersive);
        if (Build.VERSION.SDK_INT >= 33)
            getOnBackInvokedDispatcher().registerOnBackInvokedCallback(OnBackInvokedDispatcher.PRIORITY_DEFAULT, this::navigateBack);
        engineThread = new HandlerThread("Yuka engine");
        engineThread.start();
        engine = new Handler(engineThread.getLooper());
        engine.post(() -> {
            audio = new AudioHost(getApplicationContext(), engine, (operation, argument) -> {
                if (session == 0 || destroyed) return;
                try { NativeBridge.command(session, operation, argument); }
                catch (RuntimeException error) { showError(error); }
            });
            String path = preferences.getString("game", "");
            if (!path.isEmpty() && new File(path).isDirectory()) openGame(new File(path));
            engine.post(frame);
        });
    }

    private final Runnable frame = new Runnable() {
        @Override public void run() {
            if (destroyed) return;
            if (session != 0 && !background && !busy && !engineFailed) {
                try {
                    NativeBridge.tick(session);
                    JSONObject state = new JSONObject(NativeBridge.state(session));
                    audio.apply(state.optJSONArray("audio"));
                    String warning = state.isNull("audio_warning") ? "" : state.optString("audio_warning");
                    if (!warning.isEmpty() && !warning.equals(lastAudioWarning))
                        ui.post(() -> { message = "音频播放失败 · " + warning; toast(message); });
                    lastAudioWarning = warning;
                    NativeBridge.render(session, pixels);
                    gameView.update(pixels, state);
                    long now = android.os.SystemClock.elapsedRealtime();
                    if (now - lastSaveScan > 1000) { refreshSaves(); lastSaveScan = now; }
                    if (state.optBoolean("finished") && mode == Mode.PLAYING) {
                        call("title", "");
                        ui.post(() -> { mode = Mode.TITLE; message = "本次路线播放结束"; gameView.invalidate(); });
                    }
                } catch (Exception | LinkageError error) {
                    engineFailed = true;
                    if (session != 0) {
                        try { NativeBridge.command(session, "pause", "1"); } catch (RuntimeException ignored) { }
                    }
                    audio.setLifecyclePaused(true);
                    ui.post(() -> { mode = Mode.PAUSED; gameView.invalidate(); });
                    showError(error);
                }
            }
            engine.postDelayed(this, 33);
        }
    };

    private void openGame(File directory) {
        busy = true;
        progress("正在读取游戏资源");
        long candidate = 0;
        try {
            File saves = new File(directory, "Save");
            GameImporter.mkdir(saves);
            File cache = new File(getCacheDir(), "audio-" + directory.getName());
            GameImporter.mkdir(cache);
            candidate = NativeBridge.create(directory.getAbsolutePath(), saves.getAbsolutePath(), cache.getAbsolutePath());
            if (candidate == 0) throw new IOException("引擎没有创建有效会话");
            NativeBridge.command(candidate, "title", "");
            if (session != 0) NativeBridge.destroy(session);
            audio.close();
            session = candidate;
            candidate = 0;
            saveDirectory = saves;
            refreshSaves();
            nativeReady = true;
            engineFailed = false;
            preferences.edit().putString("game", directory.getAbsolutePath()).apply();
            ui.post(() -> { mode = Mode.TITLE; message = ""; gameView.invalidate(); });
        } catch (Exception | LinkageError error) {
            if (candidate != 0) NativeBridge.destroy(candidate);
            showError(error);
        } finally {
            busy = false;
            gameView.postInvalidate();
        }
    }

    private void command(String operation, String argument, Runnable success) {
        if (busy || actionPending || !nativeReady) return;
        actionPending = true;
        engine.post(() -> {
            try {
                call(operation, argument);
                if (operation.equals("save") || operation.equals("load")) refreshSaves();
                ui.post(() -> { actionPending = false; message = ""; if (success != null) success.run(); gameView.invalidate(); });
            } catch (RuntimeException error) {
                if (operation.equals("advance") || operation.equals("choose")) {
                    engineFailed = true;
                    audio.setLifecyclePaused(true);
                    ui.post(() -> mode = Mode.PAUSED);
                }
                ui.post(() -> actionPending = false);
                showError(error);
            }
        });
    }

    private void call(String operation, String argument) {
        boolean replacing = operation.equals("new") || operation.equals("load") || operation.equals("title");
        if (replacing) audio.reset();
        NativeBridge.command(session, operation, argument);
        if (replacing) engineFailed = false;
    }

    private void begin() {
        command("new", "", this::resumePlayback);
    }

    private void resumePlayback() {
        command("pause", "0", () -> {
            mode = Mode.PLAYING;
            engine.post(() -> audio.setLifecyclePaused(background));
        });
    }

    private void pauseMenu() {
        if (!nativeReady || busy) return;
        mode = Mode.PAUSED;
        command("pause", "1", null);
        gameView.invalidate();
    }

    private void save(File path) {
        command("save", path.getAbsolutePath(), () -> toast("存档已保存"));
    }

    private void load(File path) {
        command("load", path.getAbsolutePath(), () -> { resumePlayback(); toast("存档已读取"); });
    }

    private void saveMenu() {
        if (saveDirectory == null) return;
        String[] labels = {"存档 2", "存档 3", "存档 4"};
        new AlertDialog.Builder(this).setTitle("保存游戏")
            .setItems(labels, (dialog, index) -> save(new File(saveDirectory, String.format(Locale.ROOT, "savedata.%03d", index + 2))))
            .setNegativeButton("返回", null).show();
    }

    private void refreshSaves() {
        if (saveDirectory == null) { saveFiles = new File[0]; return; }
        File[] files = saveDirectory.listFiles(file -> file.isFile()
            && (file.getName().toLowerCase(Locale.ROOT).startsWith("savedata.") || file.getName().startsWith("import-")
                || file.getName().equals("mobile-autosave.dat")));
        if (files == null) { saveFiles = new File[0]; return; }
        Arrays.sort(files, Comparator.comparingLong(File::lastModified).reversed());
        saveFiles = files;
    }

    private void loadMenu() {
        File[] files = saveFiles;
        String[] labels = new String[files.length + 2];
        for (int index = 0; index < files.length; index++) labels[index] = files[index].getName();
        labels[files.length] = "选择手机上的 PC 存档文件";
        labels[files.length + 1] = "导出完整 Save 目录";
        new AlertDialog.Builder(this).setTitle("读取存档")
            .setItems(labels, (dialog, index) -> {
                if (index < files.length) load(files[index]);
                else if (index == files.length) pickSave();
                else exportSaves();
            })
            .setNegativeButton("返回", null).show();
    }

    private void pickGame() {
        if (busy) return;
        if (nativeReady) command("pause", "1", null);
        Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE);
        intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION);
        startActivityForResult(intent, PICK_GAME);
    }

    private void pickSave() {
        if (busy || !nativeReady) return;
        command("pause", "1", null);
        Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT).setType("*/*");
        intent.addCategory(Intent.CATEGORY_OPENABLE);
        intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
        startActivityForResult(intent, PICK_SAVE);
    }

    private void exportSaves() {
        if (busy || !nativeReady) return;
        command("pause", "1", null);
        Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE);
        intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION);
        startActivityForResult(intent, PICK_EXPORT);
    }

    @Override protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (result != RESULT_OK || data == null || data.getData() == null) return;
        Uri uri = data.getData();
        busy = true;
        GameImporter.Progress progress = new GameImporter.Progress() {
            public void update(String text) { progress(text); }
            public boolean cancelled() { return destroyed; }
        };
        engine.post(() -> {
            try {
                if (request == PICK_GAME) {
                    File imported = GameImporter.importGame(getContentResolver(), uri, getFilesDir(), progress);
                    String label = documentName(uri);
                    preferences.edit().putString("label." + imported.getName(), label).apply();
                    openGame(imported);
                } else if (request == PICK_SAVE && saveDirectory != null) {
                    File imported = GameImporter.importSave(getContentResolver(), uri, saveDirectory, progress);
                    call("load", imported.getAbsolutePath());
                    refreshSaves();
                    ui.post(() -> { busy = false; message = ""; resumePlayback(); toast("PC 存档已读取"); });
                } else if (request == PICK_EXPORT && saveDirectory != null) {
                    NativeBridge.command(session, "sync_globals", "");
                    GameImporter.exportSaves(getContentResolver(), uri, saveDirectory, progress);
                    ui.post(() -> toast("已导出全部存档及 System.dat、Already.dat"));
                }
            } catch (Exception error) { showError(error); }
            finally { busy = false; gameView.postInvalidate(); }
        });
    }

    private String documentName(Uri uri) {
        Uri document = DocumentsContract.buildDocumentUriUsingTree(uri, DocumentsContract.getTreeDocumentId(uri));
        try (Cursor cursor = getContentResolver().query(document, new String[]{DocumentsContract.Document.COLUMN_DISPLAY_NAME}, null, null, null)) {
            if (cursor != null && cursor.moveToFirst()) return cursor.getString(0);
        } catch (RuntimeException ignored) { }
        return "Yuka 游戏";
    }

    private void installedGames() {
        engine.post(() -> {
            File[] directories = getFilesDir().listFiles(file -> file.isDirectory() && file.getName().startsWith("game-"));
            if (directories == null || directories.length == 0) { ui.post(this::pickGame); return; }
            Arrays.sort(directories, Comparator.comparingLong(File::lastModified).reversed());
            String[] names = new String[directories.length];
            for (int index = 0; index < names.length; index++)
                names[index] = preferences.getString("label." + directories[index].getName(), directories[index].getName());
            ui.post(() -> new AlertDialog.Builder(this).setTitle("已导入的游戏")
                .setItems(names, (dialog, index) -> engine.post(() -> openGame(directories[index])))
                .setNegativeButton("返回", null).show());
        });
    }

    private void progress(String text) { message = text; gameView.postInvalidate(); }
    private void toast(String text) { Toast.makeText(this, text, Toast.LENGTH_SHORT).show(); }
    private void showError(Throwable error) {
        String text = error.getMessage() == null ? error.getClass().getSimpleName() : error.getMessage();
        ui.post(() -> { if (!destroyed) { message = text; gameView.invalidate(); toast(text); } });
    }

    @SuppressWarnings("deprecation")
    private void immersive() {
        if (Build.VERSION.SDK_INT >= 30) {
            getWindow().setDecorFitsSystemWindows(false);
            WindowInsetsController controller = getWindow().getInsetsController();
            if (controller != null) {
                controller.setSystemBarsBehavior(WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
                controller.hide(WindowInsets.Type.systemBars());
            }
        } else {
            getWindow().getDecorView().setSystemUiVisibility(View.SYSTEM_UI_FLAG_FULLSCREEN
                | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION | View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
                | View.SYSTEM_UI_FLAG_LAYOUT_STABLE);
        }
    }

    @Override public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (hasFocus) immersive();
    }

    @Override @SuppressWarnings("deprecation") public void onBackPressed() { navigateBack(); }

    private void navigateBack() {
        if (busy || actionPending) { toast("正在处理资源，请稍候"); return; }
        if (mode == Mode.PLAYING) pauseMenu();
        else if (mode == Mode.PAUSED && !engineFailed) resumePlayback();
        else if (mode == Mode.PAUSED) command("title", "", () -> mode = Mode.TITLE);
        else finish();
    }

    @Override protected void onPause() {
        background = true;
        if (engine != null) engine.post(() -> {
            if (session != 0) {
                try {
                    NativeBridge.command(session, "pause", "1");
                    JSONObject state = new JSONObject(NativeBridge.state(session));
                    audio.apply(state.optJSONArray("audio"));
                    if (mode != Mode.TITLE && !engineFailed && state.optBoolean("can_save"))
                        NativeBridge.command(session, "save", new File(saveDirectory, "mobile-autosave.dat").getAbsolutePath());
                } catch (Exception error) { showError(error); }
            }
            if (audio != null) audio.setLifecyclePaused(true);
        });
        super.onPause();
    }

    @Override protected void onResume() {
        super.onResume();
        background = false;
        if (engine != null) engine.post(() -> {
            if (session != 0 && mode == Mode.PLAYING) {
                try { NativeBridge.command(session, "pause", "0"); } catch (RuntimeException error) { showError(error); }
            }
            if (audio != null) audio.setLifecyclePaused(false);
        });
    }

    @Override protected void onDestroy() {
        destroyed = true;
        if (engine != null) {
            engine.removeCallbacks(frame);
            engine.post(() -> {
                if (audio != null) audio.close();
                if (session != 0) { NativeBridge.destroy(session); session = 0; }
                engineThread.quitSafely();
            });
        }
        super.onDestroy();
    }

    private final class GameView extends View {
        private final Object frameLock = new Object();
        private final Bitmap bitmap = Bitmap.createBitmap(WIDTH, HEIGHT, Bitmap.Config.ARGB_8888);
        private final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG | Paint.FILTER_BITMAP_FLAG);
        private final TextPaint textPaint = new TextPaint(Paint.ANTI_ALIAS_FLAG);
        private JSONObject state = new JSONObject();
        private final List<Button> buttons = new ArrayList<>();
        private boolean hideText;
        private float scale = 1f;
        private float left;
        private float top;

        GameView() { super(MainActivity.this); setFocusable(true); setContentDescription("Yuka 游戏画面，点击推进，右上角打开菜单"); textPaint.setTypeface(Typeface.create("sans-serif", Typeface.NORMAL)); }
        void update(int[] data, JSONObject nextState) {
            synchronized (frameLock) { bitmap.setPixels(data, 0, WIDTH, 0, 0, WIDTH, HEIGHT); state = nextState; }
            postInvalidate();
        }

        @Override protected void onDraw(Canvas canvas) {
            super.onDraw(canvas);
            canvas.drawColor(Color.BLACK);
            scale = Math.min(getWidth() / (float) WIDTH, getHeight() / (float) HEIGHT);
            if (scale <= 0f) return;
            left = (getWidth() - WIDTH * scale) / 2f;
            top = (getHeight() - HEIGHT * scale) / 2f;
            canvas.save(); canvas.translate(left, top); canvas.scale(scale, scale);
            synchronized (frameLock) {
                canvas.drawBitmap(bitmap, 0, 0, paint);
                buttons.clear();
                if (mode == Mode.PLAYING) drawStory(canvas); else drawMenu(canvas);
                if (!message.isEmpty()) {
                    StaticLayout layout = layout(message, 17f, 730);
                    rect(canvas, 20, 48, 760, Math.min(180, layout.getHeight() + 20), 0xf5101c2c);
                    canvas.save(); canvas.clipRect(35, 56, 765, 218); canvas.translate(35, 56); layout.draw(canvas); canvas.restore();
                }
            }
            canvas.restore();
        }

        private void drawStory(Canvas canvas) {
            if (!hideText && state.optBoolean("dialogue_visible", true)) {
                String dialogue = state.optString("dialogue");
                if (!dialogue.isEmpty()) {
                    float size = 24f;
                    StaticLayout body = layout(dialogue, size, 714);
                    while (body.getHeight() > 330 && size > 14f) body = layout(dialogue, --size, 714);
                    float height = Math.max(125, body.getHeight() + 22);
                    float y = 567 - height;
                    JSONArray bodyOffset = state.optJSONArray("dialogue_offset");
                    float dx = bodyOffset == null ? 0 : bodyOffset.optInt(0);
                    float dy = bodyOffset == null ? 0 : bodyOffset.optInt(1);
                    rect(canvas, 20 + dx, y + dy, 760, height, 0xe6101b2b);
                    canvas.save(); canvas.translate(42 + dx, y + dy + 10); body.draw(canvas); canvas.restore();
                    String speaker = state.optString("speaker");
                    if (!speaker.isEmpty()) {
                        JSONArray nameOffset = state.optJSONArray("speaker_offset");
                        float nx = nameOffset == null ? 0 : nameOffset.optInt(0);
                        float ny = nameOffset == null ? 0 : nameOffset.optInt(1);
                        rect(canvas, 20 + nx, y - 35 + ny, Math.max(120, textWidth(speaker, 23) + 36), 35, 0xf2233951);
                        text(canvas, speaker, 38 + nx, y - 8 + ny, 23, 0xfff7e4b8);
                    }
                }
            }
            add(canvas, "文字", 8, 4, 82, 36, true, () -> { hideText = !hideText; invalidate(); });
            add(canvas, "菜单", 700, 4, 92, 36, true, MainActivity.this::pauseMenu);
            JSONArray choices = state.optJSONArray("choices");
            if (choices != null && choices.length() > 0) {
                int count = choices.length();
                float row = Math.min(52f, 400f / count);
                float start = Math.max(70, (HEIGHT - count * row) / 2f);
                rect(canvas, 115, start - 15, 570, count * row + 25, 0xee101c2c);
                for (int index = 0; index < count; index++) {
                    final int selected = index;
                    add(canvas, choices.optString(index), 130, start + index * row, 540, row - 5, true,
                        () -> command("choose", Integer.toString(selected), null));
                }
            }
            rect(canvas, 0, 575, WIDTH, 25, 0xf508111b);
            text(canvas, state.optString("status"), 15, 593, 14, 0xffc9d7e4);
        }

        private void drawMenu(Canvas canvas) {
            rect(canvas, 0, 0, WIDTH, HEIGHT, 0x8806101e);
            boolean canSave = state.optBoolean("can_save") && !busy && !engineFailed;
            if (mode == Mode.TITLE) {
                add(canvas, "开始游戏", 210, 230, 380, 42, nativeReady, MainActivity.this::begin);
                File[] available = saveFiles;
                add(canvas, "继续上次存档", 210, 278, 380, 42, nativeReady && available.length > 0, () -> load(available[0]));
                add(canvas, "读取存档", 210, 326, 380, 42, nativeReady, MainActivity.this::loadMenu);
                add(canvas, "导入游戏文件夹", 210, 374, 380, 42, true, MainActivity.this::pickGame);
                add(canvas, "切换已导入游戏", 210, 422, 380, 42, true, MainActivity.this::installedGames);
                add(canvas, "退出", 210, 470, 380, 42, true, MainActivity.this::finish);
            } else {
                boolean hasQuickSave = false;
                for (File file : saveFiles) if (file.getName().equals("savedata.099")) hasQuickSave = true;
                add(canvas, "继续游戏", 210, 150, 380, 42, nativeReady && !engineFailed, MainActivity.this::resumePlayback);
                add(canvas, "快速存档", 210, 198, 380, 42, canSave, () -> save(new File(saveDirectory, "savedata.099")));
                add(canvas, "读取快速存档", 210, 246, 380, 42, hasQuickSave, () -> load(new File(saveDirectory, "savedata.099")));
                add(canvas, "保存到存档槽", 210, 294, 380, 42, canSave, MainActivity.this::saveMenu);
                add(canvas, "读取存档", 210, 342, 380, 42, nativeReady, MainActivity.this::loadMenu);
                add(canvas, "返回标题", 210, 390, 380, 42, nativeReady,
                    () -> command("title", "", () -> mode = Mode.TITLE));
                add(canvas, "导出完整存档目录", 210, 438, 380, 42, nativeReady, MainActivity.this::exportSaves);
            }
        }

        private void add(Canvas canvas, String label, float x, float y, float width, float height, boolean enabled, Runnable action) {
            boolean active = enabled && !busy && !actionPending;
            rect(canvas, x, y, width, height, active ? 0xf22c4860 : 0xf2172236);
            float size = Math.min(22f, height - 12f);
            String fitted = android.text.TextUtils.ellipsize(label, sized(size, active ? Color.WHITE : 0xff8290a2), width - 30,
                android.text.TextUtils.TruncateAt.END).toString();
            text(canvas, fitted, x + 15, y + height / 2f + size * 0.35f, size, active ? Color.WHITE : 0xff8290a2);
            buttons.add(new Button(new RectF(x, y, x + width, y + height), active, action));
        }

        private TextPaint sized(float size, int color) { textPaint.setTextSize(size); textPaint.setColor(color); return textPaint; }
        private float textWidth(String value, float size) { return sized(size, Color.WHITE).measureText(value); }
        private StaticLayout layout(String value, float size, int width) {
            return StaticLayout.Builder.obtain(value, 0, value.length(), new TextPaint(sized(size, Color.WHITE)), width)
                .setAlignment(Layout.Alignment.ALIGN_NORMAL).setLineSpacing(5f, 1f).setIncludePad(false).build();
        }
        private void text(Canvas canvas, String value, float x, float y, float size, int color) { canvas.drawText(value, x, y, sized(size, color)); }
        private void rect(Canvas canvas, float x, float y, float width, float height, int color) { paint.setColor(color); canvas.drawRect(x, y, x + width, y + height, paint); paint.setColor(Color.WHITE); }

        @Override public boolean onTouchEvent(MotionEvent event) {
            if (event.getActionMasked() == MotionEvent.ACTION_DOWN) return true;
            if (event.getActionMasked() != MotionEvent.ACTION_UP) return true;
            performClick();
            float x = (event.getX() - left) / scale;
            float y = (event.getY() - top) / scale;
            if (x < 0 || y < 0 || x >= WIDTH || y >= HEIGHT || busy || actionPending) return true;
            for (Button button : buttons) {
                if (button.bounds.contains(x, y)) { if (button.enabled) button.action.run(); return true; }
            }
            synchronized (frameLock) {
                JSONArray choices = state.optJSONArray("choices");
                if (mode == Mode.PLAYING && (choices == null || choices.length() == 0)) {
                    if (hideText) { hideText = false; invalidate(); }
                    else command("advance", "", null);
                }
            }
            return true;
        }
        @Override public boolean performClick() { return super.performClick(); }
    }

    private static final class Button {
        final RectF bounds;
        final boolean enabled;
        final Runnable action;
        Button(RectF bounds, boolean enabled, Runnable action) { this.bounds = bounds; this.enabled = enabled; this.action = action; }
    }
}
