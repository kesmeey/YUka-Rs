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
import android.view.inputmethod.InputMethodManager;
import android.window.OnBackInvokedDispatcher;
import android.widget.Toast;
import android.widget.EditText;
import android.widget.FrameLayout;
import android.util.TypedValue;
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
    private FrameLayout gameContainer;
    private EditText remarkField;
    private OriginalSaveMenu slotMenu;
    private OriginalHistoryMenu historyMenu;
    private Mode historyReturnMode;
    private HistoryAudio historyAudio;
    private int historyRequest;
    private Mode slotReturnMode;
    private int slotRequest;
    private SharedPreferences preferences;
    private volatile boolean destroyed;
    private volatile boolean background;
    private volatile boolean busy;
    private volatile boolean nativeReady;
    private volatile boolean actionPending;
    private volatile boolean engineFailed;
    private volatile boolean bundledMode;
    private volatile Mode mode = Mode.TITLE;
    private volatile String message = "选择游戏目录，导入 data*.ykc 和 Save";
    private volatile File saveDirectory;
    private volatile File[] saveFiles = new File[0];
    private long lastSaveScan;
    private String lastAudioWarning = "";

    @Override public void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        historyAudio = new HistoryAudio(this);
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        preferences = getSharedPreferences("player", MODE_PRIVATE);
        gameView = new GameView();
        gameContainer = new FrameLayout(this);
        gameContainer.addView(gameView);
        remarkField = new EditText(this);
        remarkField.setSingleLine(true);
        remarkField.setPadding(0, 0, 0, 0);
        remarkField.setTextColor(Color.WHITE);
        remarkField.setBackgroundColor(Color.TRANSPARENT);
        remarkField.setIncludeFontPadding(false);
        remarkField.setImeOptions(android.view.inputmethod.EditorInfo.IME_FLAG_NO_EXTRACT_UI
            | android.view.inputmethod.EditorInfo.IME_ACTION_DONE);
        remarkField.setOnEditorActionListener((view, action, event) -> { hideKeyboard(); gameView.requestFocus(); return true; });
        remarkField.setVisibility(View.GONE);
        gameContainer.addView(remarkField);
        setContentView(gameContainer);
        getWindow().setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_ADJUST_NOTHING);
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
            try {
                bundledMode = BundledGame.available(getAssets());
                if (bundledMode) prepareBundledGame();
                else {
                    String path = preferences.getString("game", "");
                    if (!path.isEmpty() && new File(path).isDirectory()) openGame(new File(path));
                }
            } catch (IOException error) { showError(error); }
            engine.post(frame);
        });
    }

    private void prepareBundledGame() {
        busy = true;
        progress("正在检查随包游戏资源");
        try {
            BundledGame.Bundle bundle = BundledGame.read(getAssets());
            File directory = BundledGame.prepare(getAssets(), bundle, getFilesDir(), new GameImporter.Progress() {
                public void update(String text) { progress(text); }
                public boolean cancelled() { return destroyed; }
            });
            if (destroyed) return;
            preferences.edit().putString("label." + directory.getName(), bundle.name).apply();
            openGame(directory);
        } catch (Exception error) { showError(error); }
        finally { busy = false; gameView.postInvalidate(); }
    }

    private void retryBundledGame() {
        if (busy || destroyed) return;
        busy = true;
        engine.post(this::prepareBundledGame);
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
            OriginalAssets originalAssets = OriginalAssets.load(candidate);
            if (session != 0) NativeBridge.destroy(session);
            audio.close();
            session = candidate;
            candidate = 0;
            saveDirectory = saves;
            refreshSaves();
            nativeReady = true;
            engineFailed = false;
            preferences.edit().putString("game", directory.getAbsolutePath()).apply();
            ui.post(() -> {
                clearSlotMenu();
                gameView.originalAssets = originalAssets;
                gameView.hideText = false;
                mode = Mode.TITLE;
                message = "";
                gameView.invalidate();
            });
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
                if (operation.equals("save") || operation.equals("save_with_comment") || operation.equals("load")) refreshSaves();
                ui.post(() -> {
                    actionPending = false;
                    message = "";
                    if (operation.equals("new") || operation.equals("load") || operation.equals("title")) gameView.hideText = false;
                    if (success != null) success.run();
                    gameView.invalidate();
                });
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
        if (replacing || (operation.equals("pause") && argument.equals("1")))
            NativeBridge.command(session, "playback", "manual");
        if (replacing) audio.reset();
        NativeBridge.command(session, operation, argument);
        if (replacing) engineFailed = false;
    }

    private void begin() {
        command("new", "", this::resumePlayback);
    }

    private void resumePlayback() {
        clearHistory();
        clearSlotMenu();
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

    private void toggleAuto() {
        command("playback", gameView.playback().equals("auto") ? "manual" : "auto", null);
    }

    private void toggleSkip() {
        if (!gameView.playback().equals("manual")) { command("playback", "manual", null); return; }
        new AlertDialog.Builder(this).setTitle("快进方式")
            .setItems(new String[]{"已读快进（遇到未读停止）", "全部快进（包括未读对话）"}, (dialog, index) -> {
                if (index == 0 && !gameView.currentRead()) { toast("当前是未读对话，已读快进会在此停止"); return; }
                command("playback", index == 0 ? "skip_read" : "skip_all", null);
            }).setNegativeButton("取消", null).show();
    }

    private void openHistory() {
        if (busy || actionPending || !nativeReady || gameView.originalAssets == null) return;
        Mode previous = mode;
        command("pause", "1", () -> {
            historyReturnMode = previous;
            mode = Mode.PAUSED;
            int request = ++historyRequest;
            engine.post(() -> {
                try {
                    JSONArray entries = new JSONArray(NativeBridge.history(session));
                    ui.post(() -> {
                        if (destroyed || request != historyRequest) return;
                        historyMenu = new OriginalHistoryMenu(gameView.originalAssets, entries, new OriginalHistoryMenu.Listener() {
                            public void close() { closeHistory(); }
                            public void voice(long id) { replayHistory(id); }
                        });
                        gameView.invalidate();
                    });
                } catch (Exception error) { showError(error); }
            });
        });
    }

    private void replayHistory(long id) {
        OriginalHistoryMenu menu = historyMenu;
        if (menu == null) return;
        historyAudio.stop();
        int request = ++historyRequest;
        engine.post(() -> {
            try {
                JSONArray paths = new JSONArray(NativeBridge.historyVoice(session, id));
                ui.post(() -> {
                    if (!destroyed && !background && historyMenu == menu && request == historyRequest)
                        historyAudio.play(paths);
                    else for (int index = 0; index < paths.length(); index++) new File(paths.optString(index)).delete();
                });
            } catch (Exception error) { showError(error); }
        });
    }

    private void clearHistory() {
        historyMenu = null;
        historyRequest++;
        historyAudio.stop();
    }

    private void closeHistory() {
        Mode previous = historyReturnMode;
        clearHistory();
        if (previous == Mode.PLAYING) resumePlayback();
        else { mode = previous; gameView.invalidate(); }
    }

    private void save(File path) {
        command("save", path.getAbsolutePath(), () -> toast("存档已保存"));
    }

    private void load(File path) {
        command("load", path.getAbsolutePath(), () -> { resumePlayback(); toast("存档已读取"); });
    }

    private void saveMenu() {
        openSlotMenu(true);
    }

    private void loadMenu() {
        openSlotMenu(false);
    }

    private void quickSave() {
        if (saveDirectory != null) save(new File(saveDirectory, "savedata.999"));
    }

    private File quickSaveFile() {
        if (saveDirectory == null) return null;
        File current = new File(saveDirectory, "savedata.999");
        if (current.isFile()) return current;
        File legacy = new File(saveDirectory, "savedata.099");
        return legacy.isFile() ? legacy : null;
    }

    private void quickLoad() {
        File file = quickSaveFile();
        if (file != null) load(file);
        else toast("尚无快速存档");
    }

    private void openSlotMenu(boolean saving) {
        if (saveDirectory == null || gameView.originalAssets == null || busy || actionPending) return;
        if (saving && (!gameView.canSave() || engineFailed)) { toast("当前位置无法保存"); return; }
        Mode returnMode = slotMenu == null ? mode : slotReturnMode;
        int page = slotMenu == null ? gameView.stateInteger("save_page", 0) : slotMenu.page;
        int row = slotMenu == null ? gameView.stateInteger("save_row", 1) : slotMenu.row;
        command("pause", "1", () -> {
            slotReturnMode = returnMode;
            mode = Mode.PAUSED;
            clearSlotMenu();
            slotMenu = new OriginalSaveMenu(gameView.originalAssets, saving, page, row, new OriginalSaveMenu.Listener() {
                public void page(int selected) { readSavePage(selected); }
                public void slot(int selected) { selectSaveSlot(selected); }
                public void confirm() { confirmSaveSlot(); }
                public void cancel() { cancelSaveConfirmation(); }
                public void action(OriginalSaveMenu.Action action) { saveMenuAction(action); }
            });
            readSavePage(page);
        });
    }

    private void readSavePage(int page) {
        OriginalSaveMenu menu = slotMenu;
        if (menu == null) return;
        menu.page = page;
        menu.slots = null;
        int request = ++slotRequest;
        gameView.invalidate();
        engine.post(() -> {
            try {
                NativeBridge.command(session, "save_selection", page + "," + menu.row);
                OriginalSaveMenu.Slot[] slots = new OriginalSaveMenu.Slot[5];
                for (int index = 0; index < slots.length; index++)
                    slots[index] = OriginalSaveMenu.Slot.read(session, saveDirectory, page, index + 1);
                ui.post(() -> {
                    if (!destroyed && slotMenu == menu && request == slotRequest) {
                        menu.slots = slots;
                        gameView.invalidate();
                    }
                });
            } catch (RuntimeException error) { showError(error); }
        });
    }

    private void selectSaveSlot(int row) {
        OriginalSaveMenu menu = slotMenu;
        if (menu == null || menu.slots == null || busy || actionPending) return;
        if (menu.saving && menu.page == 0 && row == 1) return;
        OriginalSaveMenu.Slot slot = menu.slots[row - 1];
        if (!menu.saving && !slot.exists) return;
        if (!menu.saving && !slot.error.isEmpty()) { toast(slot.error); return; }
        menu.row = row;
        command("save_selection", menu.page + "," + row, () -> {
            if (slotMenu != menu) return;
            menu.confirmation = row - 1;
            if (menu.saving) {
                remarkField.setText("");
                remarkField.setSelection(remarkField.length());
                positionRemark();
                remarkField.setVisibility(View.VISIBLE);
                gameView.requestFocus();
            }
        });
    }

    private void confirmSaveSlot() {
        OriginalSaveMenu menu = slotMenu;
        if (menu == null || !menu.confirming()) return;
        if (menu.navigation != null) {
            if (menu.navigation == OriginalSaveMenu.Action.TITLE)
                command("title", "", () -> { clearSlotMenu(); mode = Mode.TITLE; });
            else if (menu.navigation == OriginalSaveMenu.Action.EXIT) finish();
            else {
                File previous = new File(saveDirectory, "savedata.000");
                if (previous.isFile()) load(previous);
                else { cancelSaveConfirmation(); toast("尚无前一个选择点的存档"); }
            }
            return;
        }
        File file = menu.slots[menu.confirmation].file;
        if (!menu.saving) { load(file); return; }
        try {
            JSONObject argument = new JSONObject();
            argument.put("path", file.getAbsolutePath());
            argument.put("comment", remarkField.getText().toString());
            command("save_with_comment", argument.toString(), () -> {
                cancelSaveConfirmation();
                readSavePage(menu.page);
            });
        } catch (org.json.JSONException error) { showError(error); }
    }

    private void cancelSaveConfirmation() {
        if (slotMenu != null) { slotMenu.confirmation = -1; slotMenu.navigation = null; }
        remarkField.setVisibility(View.GONE);
        hideKeyboard();
        gameView.requestFocus();
        gameView.invalidate();
    }

    private void clearSlotMenu() {
        slotMenu = null;
        slotRequest++;
        cancelSaveConfirmation();
    }

    private void closeSlotMenu() {
        Mode previous = slotReturnMode;
        clearSlotMenu();
        if (previous == Mode.PLAYING) resumePlayback();
        else { mode = previous; gameView.invalidate(); }
    }

    private void saveMenuAction(OriginalSaveMenu.Action action) {
        switch (action) {
            case SAVE: saveMenu(); break;
            case LOAD: loadMenu(); break;
            case OPTIONS: playerTools(); break;
            case PREVIOUS_CHOICE:
            case TITLE:
            case EXIT:
                slotMenu.navigation = action;
                gameView.invalidate();
                break;
            case RETURN: closeSlotMenu(); break;
            default: throw new IllegalArgumentException("Unknown save menu action");
        }
    }

    private void playerTools() {
        if (busy || actionPending) return;
        if (mode == Mode.PLAYING) pauseMenu();
        String[] labels = bundledMode
            ? new String[]{"读取其他存档 / 导入 PC 存档", "导出完整 Save 目录", "返回标题"}
            : new String[]{"读取其他存档 / 导入 PC 存档", "导出完整 Save 目录", "返回标题", "导入游戏文件夹", "切换已导入游戏"};
        new AlertDialog.Builder(this).setTitle("安卓工具（原版选项设置仍在实现）")
            .setItems(labels, (dialog, index) -> {
                if (index == 0) savedFileMenu();
                else if (index == 1) exportSaves();
                else if (index == 2) command("title", "", () -> { clearSlotMenu(); mode = Mode.TITLE; });
                else if (index == 3) pickGame();
                else installedGames();
            }).setNegativeButton("返回", null).show();
    }

    private void hideKeyboard() {
        InputMethodManager keyboard = (InputMethodManager) getSystemService(INPUT_METHOD_SERVICE);
        if (keyboard != null) keyboard.hideSoftInputFromWindow(remarkField.getWindowToken(), 0);
    }

    private void positionRemark() {
        float scale = Math.min(gameView.getWidth() / (float) WIDTH, gameView.getHeight() / (float) HEIGHT);
        FrameLayout.LayoutParams bounds = new FrameLayout.LayoutParams(Math.round(295 * scale), Math.round(22 * scale));
        bounds.leftMargin = Math.round((gameView.getWidth() - WIDTH * scale) / 2 + 280 * scale);
        bounds.topMargin = Math.round((gameView.getHeight() - HEIGHT * scale) / 2 + 307 * scale);
        remarkField.setLayoutParams(bounds);
        remarkField.setTextSize(TypedValue.COMPLEX_UNIT_PX, 18 * scale);
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

    private void savedFileMenu() {
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
                    ui.post(() -> {
                        busy = false;
                        message = "";
                        gameView.hideText = false;
                        resumePlayback();
                        toast("PC 存档已读取");
                    });
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
        if (historyMenu != null) { closeHistory(); return; }
        if (slotMenu != null) {
            if (slotMenu.confirming()) cancelSaveConfirmation();
            else closeSlotMenu();
            return;
        }
        if (mode == Mode.PLAYING) pauseMenu();
        else if (mode == Mode.PAUSED && !engineFailed) resumePlayback();
        else if (mode == Mode.PAUSED) command("title", "", () -> mode = Mode.TITLE);
        else finish();
    }

    @Override protected void onPause() {
        background = true;
        historyRequest++;
        historyAudio.stop();
        if (engine != null) engine.post(() -> {
            if (session != 0) {
                try {
                    NativeBridge.command(session, "playback", "manual");
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
        historyAudio.stop();
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
        private volatile OriginalAssets originalAssets;
        private float pressedX = -1;
        private float pressedY = -1;
        private float touchStartX;
        private float touchStartY;
        private float touchLastY;
        private boolean historyDragged;
        private boolean hideText;
        private float scale = 1f;
        private float left;
        private float top;

        GameView() {
            super(MainActivity.this);
            setFocusable(true);
            setFocusableInTouchMode(true);
            setDefaultFocusHighlightEnabled(false);
            setContentDescription("Yuka 游戏画面，点击推进，底部保存和读取，安卓返回键打开菜单");
            textPaint.setTypeface(Typeface.create("sans-serif", Typeface.NORMAL));
        }
        boolean canSave() { synchronized (frameLock) { return state.optBoolean("can_save"); } }
        String playback() { synchronized (frameLock) { return state.optString("playback", "manual"); } }
        boolean currentRead() { synchronized (frameLock) { return state.optBoolean("current_read"); } }
        int stateInteger(String key, int fallback) { synchronized (frameLock) { return state.optInt(key, fallback); } }
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
                if (historyMenu != null) historyMenu.draw(canvas, pressedX, pressedY);
                else if (slotMenu != null) slotMenu.draw(canvas, pressedX, pressedY);
                else if (mode == Mode.PLAYING) drawStory(canvas);
                else drawMenu(canvas);
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
                JSONArray bodyOffset = state.optJSONArray("dialogue_offset");
                float dx = bodyOffset == null ? 0 : bodyOffset.optInt(0);
                float dy = bodyOffset == null ? 0 : bodyOffset.optInt(1);
                JSONArray nameOffset = state.optJSONArray("speaker_offset");
                float nx = nameOffset == null ? 0 : nameOffset.optInt(0);
                float ny = nameOffset == null ? 0 : nameOffset.optInt(1);
                originalImage(canvas, "window", -1, 3 + dx, 473 + dy, 179);
                originalImage(canvas, "name", -1, nx, 437 + ny, 179);
                String dialogue = state.optString("dialogue");
                if (!dialogue.isEmpty()) {
                    canvas.save();
                    canvas.clipRect(98 + dx, 482 + dy, 712 + dx, 600 + dy);
                    outlinedText(canvas, dialogue, 98 + dx, 482 + dy, 25f, 614,
                        Layout.Alignment.ALIGN_NORMAL, Color.WHITE, Color.BLACK);
                    canvas.restore();
                }
                String speaker = state.optString("speaker");
                if (!speaker.isEmpty()) {
                    outlinedText(canvas, speaker, nx, 442 + ny, 25f, 220,
                        Layout.Alignment.ALIGN_CENTER, Color.WHITE, Color.BLACK);
                }
                boolean canSave = state.optBoolean("can_save") && !engineFailed;
                originalButton(canvas, "sys_btn01", 0, 5, "SYSTEM", 388, 580, 72, 16,
                    nativeReady, MainActivity.this::playerTools);
                originalButton(canvas, "sys_btn01", 1, 6, "Q.SAVE", 462, 580, 72, 16,
                    canSave, MainActivity.this::quickSave);
                originalButton(canvas, "sys_btn01", 2, 7, "Q.LOAD", 536, 580, 72, 16,
                    nativeReady, MainActivity.this::quickLoad);
                originalButton(canvas, "sys_btn01", 3, 8, "SAVE", 610, 580, 72, 16,
                    canSave, MainActivity.this::saveMenu);
                originalButton(canvas, "sys_btn01", 4, 9, "LOAD", 684, 580, 72, 16,
                    nativeReady, MainActivity.this::loadMenu);
                originalButton(canvas, "sys_btn02", 0, 8, "×", 761, 497, 38, 21, true,
                    () -> command("playback", "manual", () -> { hideText = true; invalidate(); }));
                String playback = state.optString("playback", "manual");
                originalButton(canvas, "sys_btn02", playback.equals("auto") ? 5 : 1, 9, "▶", 761, 520, 38, 21, true,
                    MainActivity.this::toggleAuto);
                originalButton(canvas, "sys_btn02", playback.startsWith("skip") ? 6 : 2, 10, "▶▶", 761, 543, 38, 21, true,
                    MainActivity.this::toggleSkip);
                originalButton(canvas, "sys_btn02", 3, 11, "◀◀", 761, 566, 38, 21, true,
                    MainActivity.this::openHistory);
                drawButtonHelp(canvas);
            }
            JSONArray choices = state.optJSONArray("choices");
            if (choices != null && choices.length() > 0) {
                int count = choices.length();
                if (count <= 5 && originalAssets != null) {
                    drawOriginalChoices(canvas, choices);
                    return;
                }
                float row = Math.min(52f, 400f / count);
                float start = Math.max(70, (HEIGHT - count * row) / 2f);
                rect(canvas, 115, start - 15, 570, count * row + 25, 0xee101c2c);
                for (int index = 0; index < count; index++) {
                    final int selected = index;
                    add(canvas, choices.optString(index), 130, start + index * row, 540, row - 5, true,
                        () -> command("choose", Integer.toString(selected), null));
                }
            }
            String playback = state.optString("playback", "manual");
            if (!playback.equals("manual")) {
                rect(canvas, 10, 10, 220, 30, 0xb0102636);
                text(canvas, playback.equals("auto") ? "自动阅读 · 点击画面停止"
                    : (playback.equals("skip_read") ? "已读快进 · 点击停止" : "全部快进 · 点击停止"), 18, 31, 17, Color.WHITE);
            }
        }

        private void drawMenu(Canvas canvas) {
            boolean canSave = state.optBoolean("can_save") && !busy && !engineFailed;
            if (mode == Mode.TITLE) {
                if (bundledMode && !nativeReady)
                    add(canvas, "重试准备游戏", 210, 272, 380, 42, true, MainActivity.this::retryBundledGame);
                else originalButton(canvas, "t_btn", 0, 5, "开始游戏", 315, 272, 155, 54,
                    nativeReady, MainActivity.this::begin);
                originalButton(canvas, "t_btn", 1, 6, "继续游戏", 315, 327, 155, 54,
                    nativeReady, MainActivity.this::loadMenu);
                originalButton(canvas, "t_btn", 2, 7, "选项设置", 315, 382, 155, 54,
                    true, MainActivity.this::playerTools);
                originalButton(canvas, "t_btn", 3, 8, "附加内容", 315, 437, 155, 54,
                    nativeReady, () -> toast("原版附加内容与鉴赏模式尚未实现"));
                originalButton(canvas, "t_btn", 4, 9, "游戏结束", 315, 492, 155, 54,
                    true, MainActivity.this::finish);
                for (int index = 0; index < 5; index++) {
                    float y = 272 + index * 55;
                    if (pressedX >= 315 && pressedX < 470 && pressedY >= y && pressedY < y + 54)
                        originalImage(canvas, "t_btn_msg", index, 470, y, 255);
                }
            } else {
                rect(canvas, 0, 0, WIDTH, HEIGHT, 0x8806101e);
                boolean hasQuickSave = false;
                for (File file : saveFiles)
                    if (file.getName().equals("savedata.999") || file.getName().equals("savedata.099")) hasQuickSave = true;
                add(canvas, "继续游戏", 210, 150, 380, 42, nativeReady && !engineFailed, MainActivity.this::resumePlayback);
                add(canvas, "快速存档", 210, 198, 380, 42, canSave, MainActivity.this::quickSave);
                add(canvas, "读取快速存档", 210, 246, 380, 42, hasQuickSave, MainActivity.this::quickLoad);
                add(canvas, "保存到存档槽", 210, 294, 380, 42, canSave, MainActivity.this::saveMenu);
                add(canvas, "读取存档", 210, 342, 380, 42, nativeReady, MainActivity.this::loadMenu);
                add(canvas, "返回标题", 210, 390, 380, 42, nativeReady,
                    () -> command("title", "", () -> mode = Mode.TITLE));
                add(canvas, "导出完整存档目录", 210, 438, 380, 42, nativeReady, MainActivity.this::exportSaves);
            }
        }

        private void originalImage(Canvas canvas, String name, int sequence, float x, float y, int alpha) {
            OriginalAssets assets = originalAssets;
            if (assets == null) return;
            Bitmap image = assets.bitmap(name, sequence);
            int previousAlpha = paint.getAlpha();
            paint.setAlpha(alpha);
            canvas.drawBitmap(image, x, y, paint);
            paint.setAlpha(previousAlpha);
        }

        private void originalButton(Canvas canvas, String name, int normal, int hover, String fallback,
                float x, float y, float width, float height, boolean enabled, Runnable action) {
            if (originalAssets == null) {
                add(canvas, fallback, x, y, width, height, enabled, action);
                return;
            }
            boolean active = enabled && !busy && !actionPending;
            RectF bounds = new RectF(x, y, x + width, y + height);
            boolean pressed = active && bounds.contains(pressedX, pressedY);
            originalImage(canvas, name, pressed ? hover : normal, x, y, active ? 255 : 128);
            buttons.add(new Button(bounds, active, action));
        }

        private void outlinedText(Canvas canvas, String value, float x, float y, float size,
                int width, Layout.Alignment alignment, int color, int borderColor) {
            TextPaint nativeText = new TextPaint(sized(size, color));
            nativeText.setStrokeJoin(Paint.Join.ROUND);
            nativeText.setStrokeWidth(2f);
            StaticLayout text = StaticLayout.Builder.obtain(value, 0, value.length(), nativeText, width)
                .setAlignment(alignment).setLineSpacing(32f - nativeText.getFontSpacing(), 1f)
                .setIncludePad(false).build();
            canvas.save();
            canvas.translate(x, y);
            nativeText.setStyle(Paint.Style.STROKE);
            nativeText.setColor(borderColor);
            text.draw(canvas);
            nativeText.setStyle(Paint.Style.FILL);
            nativeText.setColor(color);
            text.draw(canvas);
            canvas.restore();
        }

        private void drawOriginalChoices(Canvas canvas, JSONArray choices) {
            int[][] positions = {{193}, {151, 235}, {109, 193, 277}, {71, 151, 235, 315}, {63, 128, 193, 258, 323}};
            int[] rows = positions[choices.length() - 1];
            for (int index = 0; index < rows.length; index++) {
                final int selected = index;
                int y = rows[index];
                RectF bounds = new RectF(78, y - 6, 716, y + 51);
                boolean active = !busy && !actionPending;
                boolean pressed = active && bounds.contains(pressedX, pressedY);
                originalImage(canvas, "select", -1, 49, y, 255);
                outlinedText(canvas, choices.optString(index), 108, y + 5, 36f, 608,
                    Layout.Alignment.ALIGN_NORMAL, pressed ? Color.WHITE : 0xfffad457, 0xff291e13);
                buttons.add(new Button(bounds, active, () -> command("choose", Integer.toString(selected), null)));
            }
        }

        private void drawButtonHelp(Canvas canvas) {
            for (int index = 0; index < 5; index++) {
                float x = 388 + index * 74;
                if (pressedX >= x && pressedX < x + 72 && pressedY >= 580 && pressedY < 596)
                    originalImage(canvas, "sys_btn_help", index + 3, 560, 458, 255);
            }
            for (int index = 0; index < 4; index++) {
                float y = 497 + index * 23;
                if (pressedX >= 761 && pressedX < 799 && pressedY >= y && pressedY < y + 21)
                    originalImage(canvas, "sys_btn_help", index == 0 ? 8 : index - 1, 560, 458, 255);
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
        private StaticLayout layout(String value, float size, int width) {
            return StaticLayout.Builder.obtain(value, 0, value.length(), new TextPaint(sized(size, Color.WHITE)), width)
                .setAlignment(Layout.Alignment.ALIGN_NORMAL).setLineSpacing(5f, 1f).setIncludePad(false).build();
        }
        private void text(Canvas canvas, String value, float x, float y, float size, int color) { canvas.drawText(value, x, y, sized(size, color)); }
        private void rect(Canvas canvas, float x, float y, float width, float height, int color) { paint.setColor(color); canvas.drawRect(x, y, x + width, y + height, paint); paint.setColor(Color.WHITE); }

        @Override public boolean onTouchEvent(MotionEvent event) {
            float x = (event.getX() - left) / scale;
            float y = (event.getY() - top) / scale;
            if (event.getActionMasked() == MotionEvent.ACTION_DOWN) {
                touchStartX = x; touchStartY = y; pressedX = x; pressedY = y;
                touchLastY = y; historyDragged = false;
                if (historyMenu != null && historyMenu.begin(x, y)) historyDragged = true;
                invalidate(); return true;
            }
            if (event.getActionMasked() == MotionEvent.ACTION_MOVE) {
                if (historyMenu != null) {
                    if (Math.abs(y - touchStartY) > 10) historyDragged = true;
                    if (historyDragged) historyMenu.drag(x, y, y - touchLastY);
                }
                touchLastY = y; pressedX = x; pressedY = y; invalidate(); return true;
            }
            if (event.getActionMasked() == MotionEvent.ACTION_CANCEL) {
                if (historyMenu != null) historyMenu.end();
                pressedX = -1; pressedY = -1; invalidate(); return true;
            }
            if (event.getActionMasked() != MotionEvent.ACTION_UP) return true;
            pressedX = -1; pressedY = -1; invalidate();
            performClick();
            if (x < 0 || y < 0 || x >= WIDTH || y >= HEIGHT || busy || actionPending) return true;
            if (historyMenu != null) {
                historyMenu.end();
                if (!historyDragged) historyMenu.tap(x, y);
                return true;
            }
            if (Math.abs(x - touchStartX) > 20 || Math.abs(y - touchStartY) > 20) return true;
            if (slotMenu != null) { slotMenu.tap(x, y); return true; }
            for (Button button : buttons) {
                if (button.bounds.contains(x, y)) { if (button.enabled) button.action.run(); return true; }
            }
            synchronized (frameLock) {
                JSONArray choices = state.optJSONArray("choices");
                if (mode == Mode.PLAYING && (choices == null || choices.length() == 0)) {
                    if (!playback().equals("manual")) command("playback", "manual", null);
                    else if (hideText) { hideText = false; invalidate(); }
                    else command("advance", "", null);
                }
            }
            return true;
        }
        @Override protected void onSizeChanged(int width, int height, int oldWidth, int oldHeight) {
            super.onSizeChanged(width, height, oldWidth, oldHeight);
            if (remarkField != null) post(MainActivity.this::positionRemark);
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
