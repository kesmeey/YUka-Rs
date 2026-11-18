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
import android.graphics.Path;
import android.net.Uri;
import android.os.Bundle;
import android.os.Build;
import android.os.Handler;
import android.os.HandlerThread;
import android.provider.DocumentsContract;
import android.text.Layout;
import android.text.StaticLayout;
import android.text.TextPaint;
import android.text.InputFilter;
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
import java.util.LinkedHashMap;
import java.util.Map;
import org.json.JSONArray;
import org.json.JSONObject;

public final class MainActivity extends Activity {
    private static final int PICK_GAME = 1;
    private static final int PICK_SAVE = 2;
    private static final int PICK_EXPORT = 3;
    private static final int PICK_SAVE_DIRECTORY = 4;
    private static final int WIDTH = 800;
    private static final int HEIGHT = 600;
    private static final String[] DOUSEI_ROUTES = {"花穗", "姬野继美", "琴宫千夏", "柳濑彩月", "三枝奈奈子"};
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
    private EditText familyField;
    private EditText givenField;
    private boolean nameEntry;
    private OriginalSaveMenu slotMenu;
    private OriginalSettingsMenu settingsMenu;
    private OriginalExtrasMenu extrasMenu;
    private int extrasRequest;
    private OriginalSettingsAssets settingsAssets;
    private Mode settingsReturnMode;
    private int settingsRequest;
    private final Map<Integer, Integer> pendingSettings = new LinkedHashMap<>();
    private final Runnable settingsFlush = () -> flushSettings(null);
    private int sampleRequest;
    private OriginalHistoryMenu historyMenu;
    private Mode historyReturnMode;
    private HistoryAudio historyAudio;
    private AndroidMovieHost movieHost;
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
    private boolean douseiMode;
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
        familyField = nameField();
        givenField = nameField();
        gameContainer.addView(familyField);
        gameContainer.addView(givenField);
        movieHost = new AndroidMovieHost(this, new AndroidMovieHost.Listener() {
            public void finished(long id) { movieComplete(id, null); }
            public void error(long id, String error) { movieComplete(id, error); }
        });
        gameContainer.addView(movieHost, new FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT));
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
                        boolean extras = state.optString("end_reason").equals("return_to_extras");
                        call("title", "");
                        ui.post(() -> {
                            mode = Mode.TITLE; message = extras ? "" : "本次路线播放结束"; gameView.invalidate();
                            if (extras) openExtras();
                        });
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
            boolean dousei = NativeBridge.isDousei(candidate);
            OriginalAssets originalAssets = dousei ? null : OriginalAssets.load(candidate);
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
                clearSettings();
                clearExtras();
                closeNameEntry();
                settingsAssets = null;
                douseiMode = dousei;
                gameView.originalAssets = originalAssets;
                movieHost.gameDirectory(directory);
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
                publishState();
                ui.post(() -> {
                    actionPending = false;
                    message = "";
                    if (operation.equals("new") || operation.equals("load") || operation.equals("title")) gameView.hideText = false;
                    if (success != null) success.run();
                    gameView.invalidate();
                });
            } catch (Exception error) {
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

    private void publishState() throws org.json.JSONException {
        JSONObject snapshot = new JSONObject(NativeBridge.state(session));
        audio.apply(snapshot.optJSONArray("audio"));
        NativeBridge.render(session, pixels);
        gameView.update(pixels, snapshot);
    }

    private void movieComplete(long id, String error) {
        if (error != null) toast(error);
        // Media completion is not a user input: it must survive an in-flight UI command.
        engine.post(() -> {
            if (destroyed || session == 0) return;
            try {
                NativeBridge.command(session, error == null ? "movie_finished" : "movie_error",
                    id + (error == null ? "" : "," + error));
                if (mode == Mode.TITLE && !background) NativeBridge.command(session, "title_audio", "");
                publishState();
            } catch (Exception failure) {
                engineFailed = true;
                audio.setLifecyclePaused(true);
                ui.post(() -> mode = Mode.PAUSED);
                showError(failure);
            }
        });
    }

    private void begin() {
        if (!nativeReady || busy || actionPending) return;
        if (douseiMode) {
            new AlertDialog.Builder(this).setTitle("选择角色")
                .setItems(DOUSEI_ROUTES, (dialog, route) -> selectDouseiVariant(route))
                .setNegativeButton("取消", null).show();
            return;
        }
        JSONObject names = gameView.stateObject("player_name");
        familyField.setText(names.optString("family", "爱泽"));
        givenField.setText(names.optString("given", "晴树"));
        nameEntry = true;
        positionNameFields();
        familyField.setVisibility(View.VISIBLE);
        givenField.setVisibility(View.VISIBLE);
        gameView.requestFocus();
        gameView.invalidate();
    }

    private void selectDouseiVariant(int route) {
        String[] variants = {"A", "B", "C"};
        new AlertDialog.Builder(this).setTitle(DOUSEI_ROUTES[route] + " · 选择剧情模式")
            .setItems(variants, (dialog, variant) -> command("new", route + "," + variant, this::resumePlayback))
            .setNegativeButton("返回", (dialog, which) -> begin()).show();
    }

    private void confirmName() {
        try {
            JSONObject names = new JSONObject().put("family", familyField.getText().toString().trim())
                .put("given", givenField.getText().toString().trim());
            command("player_name", names.toString(), () -> {
                closeNameEntry();
                startNewGame();
            });
        } catch (org.json.JSONException error) { showError(error); }
    }

    private void startNewGame() {
        command("new", "", this::resumePlayback);
    }

    private void resumePlayback() {
        clearHistory();
        clearSlotMenu();
        clearSettings();
        clearExtras();
        closeNameEntry();
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
        boolean all = gameView.settingFlags().optInt("3", 0) == 1;
        if (!all && !gameView.currentRead()) { toast("当前为未读对话；长按快进按钮可切换全部快进"); return; }
        command("playback", all ? "skip_all" : "skip_read", null);
    }

    private void showSkipModes() {
        new AlertDialog.Builder(this).setTitle("快进方式")
            .setItems(new String[]{"已读快进（遇到未读停止）", "全部快进（包括未读对话）"}, (dialog, index) -> {
                changeSetting(3, index);
                flushSettings(this::toggleSkip);
            }).setNegativeButton("取消", null).show();
    }

    private EditText nameField() {
        EditText field = new EditText(this);
        field.setSingleLine(true);
        field.setPadding(0, 0, 0, 0);
        field.setIncludeFontPadding(false);
        field.setTextColor(Color.WHITE);
        field.setBackgroundColor(Color.TRANSPARENT);
        field.setShadowLayer(2, 1, 1, 0xff023e39);
        field.setFilters(new InputFilter[]{new InputFilter.LengthFilter(8)});
        field.setImeOptions(android.view.inputmethod.EditorInfo.IME_FLAG_NO_EXTRACT_UI);
        field.setVisibility(View.GONE);
        return field;
    }

    private void closeNameEntry() {
        nameEntry = false;
        familyField.setVisibility(View.GONE);
        givenField.setVisibility(View.GONE);
        hideKeyboard();
    }

    private void positionNameFields() {
        float scale = Math.min(gameView.getWidth() / (float) WIDTH, gameView.getHeight() / (float) HEIGHT);
        EditText[] fields = {familyField, givenField};
        int[] x = {134, 428};
        for (int index = 0; index < fields.length; index++) {
            FrameLayout.LayoutParams bounds = new FrameLayout.LayoutParams(Math.round(239 * scale), Math.round(48 * scale));
            bounds.leftMargin = Math.round((gameView.getWidth() - WIDTH * scale) / 2 + x[index] * scale);
            bounds.topMargin = Math.round((gameView.getHeight() - HEIGHT * scale) / 2 + 265 * scale);
            fields[index].setLayoutParams(bounds);
            fields[index].setTextSize(TypedValue.COMPLEX_UNIT_PX, 36 * scale);
        }
    }

    private void openSettings(int page) {
        if (!nativeReady || busy || actionPending) return;
        if (douseiMode) {
            openPlayerSettings();
            return;
        }
        flushSettings(null);
        Mode previous = settingsMenu != null ? settingsReturnMode : slotMenu != null ? slotReturnMode : mode;
        command("pause", "1", () -> {
            settingsReturnMode = previous;
            clearSlotMenu();
            clearHistory();
            closeNameEntry();
            mode = Mode.PAUSED;
            int request = ++settingsRequest;
            engine.post(() -> {
                try {
                    NativeBridge.command(session, "menu_audio", "");
                    if (settingsAssets == null) settingsAssets = OriginalSettingsAssets.load(session);
                    OriginalSettingsAssets loaded = settingsAssets;
                    ui.post(() -> {
                        if (destroyed || request != settingsRequest) return;
                        settingsMenu = new OriginalSettingsMenu(gameView.originalAssets, loaded, page,
                            gameView.settingFlags(), !gameView.stateBoolean("title", true) && gameView.canSave(),
                            new OriginalSettingsMenu.Listener() {
                                public void change(int flag, int value) { changeSetting(flag, value); }
                                public void page(int selected) { openSettings(selected); }
                                public void action(OriginalSaveMenu.Action action) { settingsAction(action); }
                                public void sample(int selected) { settingsSample(selected); }
                                public void defaults(int section) { resetSettings(section); }
                            });
                        preferences.edit().putInt("settings_page", page).apply();
                        gameView.invalidate();
                    });
                } catch (Exception error) { showError(error); }
            });
        });
    }

    private void openPlayerSettings() {
        Mode previous = mode;
        command("pause", "1", () -> {
            if (previous == Mode.PLAYING) mode = Mode.PAUSED;
            PlayerSettingsDialog.show(this, gameView.settingFlags(), changes -> {
                if (changes.has("-1")) {
                    preferences.edit().putBoolean("fullscreen", changes.optInt("-1") != 0).apply();
                    immersive();
                    changes.remove("-1");
                }
                if (changes.length() > 0) command("settings", changes.toString(), null);
            }, this::playerTools);
            gameView.invalidate();
        });
    }

    private void changeSetting(int flag, int value) {
        if (flag == 19 || (flag >= 100 && flag <= 121)) { sampleRequest++; historyAudio.stop(); }
        if (flag == -1) {
            preferences.edit().putBoolean("fullscreen", value != 0).apply();
            immersive();
            gameView.invalidate();
            return;
        }
        pendingSettings.put(flag, value);
        ui.removeCallbacks(settingsFlush);
        ui.postDelayed(settingsFlush, 35);
    }

    private void flushSettings(Runnable after) {
        ui.removeCallbacks(settingsFlush);
        if (pendingSettings.isEmpty()) { if (after != null) after.run(); return; }
        JSONObject patch = new JSONObject();
        try { for (Map.Entry<Integer, Integer> entry : pendingSettings.entrySet()) patch.put(entry.getKey().toString(), entry.getValue()); }
        catch (org.json.JSONException error) { showError(error); return; }
        pendingSettings.clear();
        engine.post(() -> {
            try { NativeBridge.command(session, "settings", patch.toString()); publishState(); }
            catch (Exception error) { showError(error); }
            if (after != null) ui.post(after);
        });
    }

    private void clearSettings() {
        flushSettings(null);
        settingsMenu = null;
        settingsRequest++;
        sampleRequest++;
        historyAudio.stop();
    }

    private void closeSettings() {
        Mode previous = settingsReturnMode;
        clearSettings();
        if (previous == Mode.PLAYING) resumePlayback();
        else {
            mode = previous;
            if (previous == Mode.TITLE) command("title_audio", "", null);
            gameView.invalidate();
        }
    }

    private void settingsAction(OriginalSaveMenu.Action action) {
        flushSettings(null);
        switch (action) {
            case SCREEN: openSettings(0); break;
            case TEXT: openSettings(1); break;
            case SOUND: openSettings(3); break;
            case VOICE: openSettings(4); break;
            case SAVE: saveMenu(); break;
            case LOAD: loadMenu(); break;
            case RETURN: closeSettings(); break;
            case PREVIOUS_CHOICE:
                File previous = new File(saveDirectory, "savedata.000");
                if (previous.isFile()) load(previous); else toast("尚无前一个选择点的存档");
                break;
            case TITLE: command("title", "", () -> { clearSettings(); mode = Mode.TITLE; }); break;
            case EXIT: finish(); break;
            case OPTIONS: playerTools(); break;
            default: throw new IllegalArgumentException("Unknown settings action");
        }
    }

    private void resetSettings(int section) {
        String[] groups = {"all", "screen", "text", "sound", "voice"};
        new AlertDialog.Builder(this).setTitle("恢复默认设置")
            .setMessage("恢复当前类别的原版默认设置？")
            .setPositiveButton("恢复", (dialog, which) -> {
                flushSettings(null);
                if (section == 1) {
                    preferences.edit().putBoolean("fullscreen", true).apply();
                    immersive();
                }
                command("settings_reset", groups[Math.max(0, Math.min(4, section))], () -> {
                    if (settingsMenu != null) openSettings(settingsMenu.page);
                });
            }).setNegativeButton("取消", null).show();
    }

    private void settingsSample(int selected) {
        flushSettings(null);
        historyAudio.stop();
        int request = ++sampleRequest;
        engine.post(() -> {
            try {
                JSONObject sample = new JSONObject(NativeBridge.settingsSample(session, selected < 0 ? 15 : selected));
                JSONArray paths = sample.getJSONArray("paths");
                ui.post(() -> {
                    if (!destroyed && !background && settingsMenu != null && request == sampleRequest)
                        historyAudio.play(paths, (float)sample.optDouble("volume", 0.65));
                    else for (int index = 0; index < paths.length(); index++) new File(paths.optString(index)).delete();
                });
            } catch (Exception error) { showError(error); }
        });
    }

    private void openExtras() {
        if (!nativeReady || busy || actionPending) return;
        command("pause", "1", () -> {
            int request = ++extrasRequest;
            engine.post(() -> {
                try {
                    JSONObject catalogue = new JSONObject(NativeBridge.extras(session));
                    if (!catalogue.optBoolean("enabled")) {
                        ui.post(() -> toast("附加内容将在原版规定的通关条件达成后开放"));
                        return;
                    }
                    OriginalExtrasMenu loaded = OriginalExtrasMenu.load(session, catalogue, new OriginalExtrasMenu.Listener() {
                        public void close() { closeExtras(); }
                        public void image(int group, int item, int variant) { extrasImage(group, item, variant); }
                        public void replay(int group, int item, int style) { extrasReplay(group, item, style); }
                        public void music(int track) { command("extras_music", Integer.toString(track), null); }
                    });
                    NativeBridge.command(session, "extras_music", "22");
                    ui.post(() -> {
                        if (destroyed || request != extrasRequest) { loaded.release(); return; }
                        clearSlotMenu(); clearSettings(); closeNameEntry();
                        extrasMenu = loaded; mode = Mode.PAUSED; gameView.invalidate();
                    });
                } catch (Exception error) { showError(error); }
            });
        });
    }

    private void extrasImage(int group, int item, int variant) {
        OriginalExtrasMenu menu = extrasMenu;
        if (menu == null) return;
        int request = ++extrasRequest;
        engine.post(() -> {
            try {
                Bitmap image = OriginalAssets.decode(NativeBridge.extrasImage(session, group, item, variant));
                ui.post(() -> {
                    if (!destroyed && menu == extrasMenu && request == extrasRequest) {
                        menu.showImage(image, group, item, variant); gameView.invalidate();
                    } else if (image != null) image.recycle();
                });
            } catch (Exception error) { showError(error); }
        });
    }

    private void extrasReplay(int group, int item, int style) {
        command("extras_replay", group + "," + item + "," + style, this::resumePlayback);
    }

    private void clearExtras() {
        extrasRequest++;
        if (extrasMenu != null) extrasMenu.release();
        extrasMenu = null;
    }

    private void closeExtras() {
        command("extras_music", "0", () -> {
            clearExtras();
            command("title", "", () -> { mode = Mode.TITLE; gameView.invalidate(); });
        });
    }

    private void openHistory() {
        if (busy || actionPending || !nativeReady || (!douseiMode && gameView.originalAssets == null)) return;
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
                JSONObject playback = new JSONObject(NativeBridge.historyPlayback(session, id));
                JSONArray paths = playback.getJSONArray("paths");
                ui.post(() -> {
                    if (!destroyed && !background && historyMenu == menu && request == historyRequest)
                        historyAudio.play(paths, (float)playback.optDouble("volume", 1));
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
        if (saveDirectory == null || busy || actionPending) return;
        if (douseiMode) {
            openPlayerSlotMenu(saving);
            return;
        }
        if (gameView.originalAssets == null) return;
        if (saving && (!gameView.canSave() || engineFailed)) { toast("当前位置无法保存"); return; }
        Mode returnMode = slotMenu != null ? slotReturnMode : settingsMenu != null ? settingsReturnMode : mode;
        int page = slotMenu == null ? gameView.stateInteger("save_page", 0) : slotMenu.page;
        int row = slotMenu == null ? gameView.stateInteger("save_row", 1) : slotMenu.row;
        command("pause", "1", () -> {
            slotReturnMode = returnMode;
            mode = Mode.PAUSED;
            clearSettings();
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

    private void openPlayerSlotMenu(boolean saving) {
        if (saving && (!gameView.canSave() || engineFailed)) { toast("当前位置无法保存"); return; }
        command("pause", "1", () -> {
            if (mode == Mode.PLAYING) mode = Mode.PAUSED;
            if (!saving) { savedFileMenu(); return; }
            File directory = saveDirectory;
            if (directory == null) return;
            File[] slots = new File[20];
            String[] labels = new String[slots.length];
            for (int index = 0; index < slots.length; index++) {
                File file = new File(directory, String.format(Locale.ROOT, "savedata.%03d", index + 2));
                slots[index] = file;
                labels[index] = file.getName() + (file.isFile() ? " · 已保存" : " · 空闲");
            }
            new AlertDialog.Builder(this).setTitle("保存到存档槽").setItems(labels, (dialog, index) -> {
                File slot = slots[index];
                if (slot.isFile()) {
                    new AlertDialog.Builder(this).setTitle("覆盖存档")
                        .setMessage(slot.getName())
                        .setPositiveButton("覆盖", (confirm, which) -> save(slot))
                        .setNegativeButton("取消", null).show();
                } else save(slot);
            }).setNegativeButton("取消", null).show();
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
            case SCREEN: openSettings(0); break;
            case TEXT: openSettings(1); break;
            case SOUND: openSettings(3); break;
            case VOICE: openSettings(4); break;
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
            ? new String[]{"读取其他存档 / 导入 PC 存档", "导出完整 Save 目录", "导入完整 PC Save 目录", "播放 OP 视频", "返回标题"}
            : new String[]{"读取其他存档 / 导入 PC 存档", "导出完整 Save 目录", "导入完整 PC Save 目录", "播放 OP 视频", "返回标题", "导入游戏文件夹", "切换已导入游戏"};
        new AlertDialog.Builder(this).setTitle("安卓工具")
            .setItems(labels, (dialog, index) -> {
                if (index == 0) savedFileMenu();
                else if (index == 1) exportSaves();
                else if (index == 2) pickSaveDirectory();
                else if (index == 3) command("movie_play", douseiMode ? "avi/d_loverable_OP.mpg" : "avi/lave_op.mpg", null);
                else if (index == 4) command("title", "", () -> { clearSlotMenu(); clearSettings(); clearExtras(); mode = Mode.TITLE; });
                else if (index == 5) pickGame();
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

    private void pickSaveDirectory() {
        new AlertDialog.Builder(this).setTitle("导入完整 PC Save 目录")
            .setMessage("将导入 System.dat 中的进度、姓名、设置及同名剧情存档。现有存档会先备份在 Save/backup-before-import-* 下，可随完整 Save 目录导出。请选用相同游戏版本的存档。")
            .setPositiveButton("选择 Save 目录", (dialog, which) -> {
                command("pause", "1", null);
                Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE);
                intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
                startActivityForResult(intent, PICK_SAVE_DIRECTORY);
            }).setNegativeButton("取消", null).show();
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
                } else if (request == PICK_SAVE_DIRECTORY && saveDirectory != null) {
                    File staging = GameImporter.stageSaves(getContentResolver(), uri, getCacheDir(), progress);
                    try {
                        GameImporter.backupSaves(saveDirectory);
                        call("import_globals", staging.getAbsolutePath());
                        GameImporter.publishStorySaves(staging, saveDirectory, progress);
                        call("title", "");
                        refreshSaves();
                        publishState();
                        ui.post(() -> {
                            clearSlotMenu(); clearSettings(); clearExtras(); closeNameEntry();
                            mode = Mode.TITLE; message = ""; toast("PC 完整存档已导入，原存档备份已保留");
                        });
                    } finally { GameImporter.removePrivateTree(staging, getCacheDir()); }
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
        boolean fullscreen = preferences == null || preferences.getBoolean("fullscreen", true);
        if (Build.VERSION.SDK_INT >= 30) {
            getWindow().setDecorFitsSystemWindows(!fullscreen);
            WindowInsetsController controller = getWindow().getInsetsController();
            if (controller != null) {
                controller.setSystemBarsBehavior(WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
                if (fullscreen) controller.hide(WindowInsets.Type.systemBars());
                else controller.show(WindowInsets.Type.systemBars());
            }
        } else {
            getWindow().getDecorView().setSystemUiVisibility(fullscreen ? View.SYSTEM_UI_FLAG_FULLSCREEN
                | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION | View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
                | View.SYSTEM_UI_FLAG_LAYOUT_STABLE : View.SYSTEM_UI_FLAG_LAYOUT_STABLE);
        }
    }

    @Override public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (hasFocus) immersive();
    }

    @Override @SuppressWarnings("deprecation") public void onBackPressed() { navigateBack(); }

    private void navigateBack() {
        if (busy || actionPending) { toast("正在处理资源，请稍候"); return; }
        if (movieHost != null && movieHost.active()) { movieHost.skip(); return; }
        if (nameEntry) { closeNameEntry(); gameView.invalidate(); return; }
        if (extrasMenu != null) { extrasMenu.back(); gameView.invalidate(); return; }
        if (settingsMenu != null) {
            if (settingsMenu.confirming()) { settingsMenu.cancelConfirmation(); gameView.invalidate(); }
            else closeSettings();
            return;
        }
        if (historyMenu != null) { closeHistory(); return; }
        if (slotMenu != null) {
            if (slotMenu.confirming()) cancelSaveConfirmation();
            else closeSlotMenu();
            return;
        }
        if (mode == Mode.PLAYING) {
            if (gameView.settingFlags().optInt("11", 0) == 1) {
                gameView.hideText = !gameView.hideText;
                command("playback", "manual", null);
                gameView.invalidate();
            } else openSettings(preferences.getInt("settings_page", 0));
        }
        else if (mode == Mode.PAUSED && !engineFailed) resumePlayback();
        else if (mode == Mode.PAUSED) command("title", "", () -> mode = Mode.TITLE);
        else finish();
    }

    @Override protected void onPause() {
        flushSettings(null);
        background = true;
        if (movieHost != null) movieHost.pause();
        historyRequest++;
        sampleRequest++;
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
        if (movieHost != null) movieHost.resume();
        if (engine != null) engine.post(() -> {
            if (session != 0 && mode == Mode.PLAYING) {
                try { NativeBridge.command(session, "pause", "0"); } catch (RuntimeException error) { showError(error); }
            }
            if (session != 0 && mode == Mode.TITLE) {
                try { NativeBridge.command(session, "title_audio", ""); } catch (RuntimeException error) { showError(error); }
            }
            if (session != 0 && settingsMenu != null) {
                try { NativeBridge.command(session, "menu_audio", ""); } catch (RuntimeException error) { showError(error); }
            }
            if (session != 0 && extrasMenu != null && extrasMenu.musicTrack() > 0) {
                try { NativeBridge.command(session, "extras_music", Integer.toString(extrasMenu.musicTrack())); }
                catch (RuntimeException error) { showError(error); }
            }
            if (audio != null) audio.setLifecyclePaused(false);
        });
    }

    @Override protected void onDestroy() {
        flushSettings(null);
        ui.removeCallbacks(settingsFlush);
        destroyed = true;
        if (movieHost != null) movieHost.close();
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
        private boolean longPressed;
        private final Runnable skipLongPress = () -> {
            if (mode == Mode.PLAYING && settingsMenu == null && slotMenu == null && !busy && !actionPending) {
                longPressed = true;
                showSkipModes();
            }
        };
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
        boolean stateBoolean(String key, boolean fallback) { synchronized (frameLock) { return state.optBoolean(key, fallback); } }
        JSONObject stateObject(String key) { synchronized (frameLock) { JSONObject value = state.optJSONObject(key); return value == null ? new JSONObject() : value; } }
        JSONObject settingFlags() {
            synchronized (frameLock) {
                JSONObject settings = state.optJSONObject("settings");
                JSONObject flags = settings == null ? null : settings.optJSONObject("flags");
                try {
                    JSONObject result = flags == null ? new JSONObject() : new JSONObject(flags.toString());
                    result.put("-1", preferences.getBoolean("fullscreen", true) ? 1 : 0);
                    return result;
                } catch (org.json.JSONException error) { throw new IllegalStateException(error); }
            }
        }
        String playback() { synchronized (frameLock) { return state.optString("playback", "manual"); } }
        boolean currentRead() { synchronized (frameLock) { return state.optBoolean("current_read"); } }
        int stateInteger(String key, int fallback) { synchronized (frameLock) { return state.optInt(key, fallback); } }
        void update(int[] data, JSONObject nextState) {
            synchronized (frameLock) { bitmap.setPixels(data, 0, WIDTH, 0, 0, WIDTH, HEIGHT); state = nextState; }
            JSONObject movie = nextState.optJSONObject("movie");
            ui.post(() -> {
                if (destroyed) return;
                if (movieHost != null) movieHost.apply(movie);
                if (settingsMenu != null && pendingSettings.isEmpty()) settingsMenu.update(settingFlags());
            });
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
                if (nameEntry) drawNameEntry(canvas);
                else if (extrasMenu != null) extrasMenu.draw(canvas, pressedX, pressedY);
                else if (settingsMenu != null) {
                    settingsMenu.draw(canvas, pressedX, pressedY);
                    add(canvas, "安卓工具", 656, 105, 140, 28, true, MainActivity.this::playerTools);
                }
                else if (historyMenu != null) historyMenu.draw(canvas, pressedX, pressedY);
                else if (slotMenu != null) slotMenu.draw(canvas, pressedX, pressedY);
                else if (mode == Mode.PLAYING) {
                    JSONArray shake = state.optJSONArray("shake_offset");
                    canvas.save();
                    if (shake != null) canvas.translate(shake.optInt(0), shake.optInt(1));
                    drawStory(canvas);
                    canvas.restore();
                }
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
            JSONObject settings = state.optJSONObject("settings");
            if (settings == null) settings = new JSONObject();
            JSONObject flags = settings.optJSONObject("flags");
            if (flags == null) flags = new JSONObject();
            int windowAlpha = settings.optInt("window_alpha", 217);
            textPaint.setTypeface(Typeface.create(flags.optInt("2", 0) == 1 ? "serif" : "sans-serif", Typeface.NORMAL));
            if (!hideText && state.optBoolean("dialogue_visible", true)) {
                JSONArray bodyOffset = state.optJSONArray("dialogue_offset");
                float dx = bodyOffset == null ? 0 : bodyOffset.optInt(0);
                float dy = bodyOffset == null ? 0 : bodyOffset.optInt(1);
                JSONArray nameOffset = state.optJSONArray("speaker_offset");
                float nx = nameOffset == null ? 0 : nameOffset.optInt(0);
                float ny = nameOffset == null ? 0 : nameOffset.optInt(1);
                if (douseiMode) {
                    rect(canvas, 12, 467, 776, 133, (windowAlpha << 24) | 0x081523);
                } else {
                    originalImage(canvas, "window", -1, 3 + dx, 473 + dy, windowAlpha);
                    originalImage(canvas, "name", -1, nx, 437 + ny, windowAlpha);
                }
                String dialogue = state.optString("dialogue");
                int visible = Math.max(0, Math.min(dialogue.codePointCount(0, dialogue.length()), state.optInt("visible_chars", Integer.MAX_VALUE)));
                dialogue = dialogue.substring(0, dialogue.offsetByCodePoints(0, visible));
                if (!dialogue.isEmpty()) {
                    canvas.save();
                    canvas.clipRect(98 + dx, 482 + dy, 712 + dx, (douseiMode ? 568 : 600) + dy);
                    storyText(canvas, dialogue, 98 + dx, 482 + dy, 614, Layout.Alignment.ALIGN_NORMAL, settings, flags);
                    canvas.restore();
                }
                String speaker = state.optString("speaker");
                if (!speaker.isEmpty()) {
                    storyText(canvas, speaker, nx, 442 + ny, 220, Layout.Alignment.ALIGN_CENTER, settings, flags);
                }
                boolean canSave = state.optBoolean("can_save") && !engineFailed;
                if (douseiMode && state.optBoolean("submenu_enabled", true)) {
                    drawPlayerControls(canvas, canSave);
                } else if (state.optBoolean("submenu_enabled", true)) {
                originalButton(canvas, "sys_btn01", 0, 5, "SYSTEM", 388, 580, 72, 16,
                    nativeReady, () -> openSettings(preferences.getInt("settings_page", 0)));
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
                if (state.optBoolean("keywait_icon", true) && state.optBoolean("waiting_key")
                        && state.optBoolean("text_complete", true) && state.optString("playback", "manual").equals("manual")) {
                    float y = 555 + (float)Math.sin(android.os.SystemClock.uptimeMillis() / 180.0) * 2;
                    Path marker = new Path(); marker.moveTo(724, y); marker.lineTo(737, y); marker.lineTo(730.5f, y + 8); marker.close();
                    paint.setColor(0xff163d55); canvas.drawPath(marker, paint);
                    paint.setColor(Color.WHITE);
                }
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

        private void drawPlayerControls(Canvas canvas, boolean canSave) {
            String playback = state.optString("playback", "manual");
            add(canvas, "设置", 12, 568, 81, 30, nativeReady, MainActivity.this::openPlayerSettings);
            add(canvas, "快存", 99, 568, 81, 30, canSave, MainActivity.this::quickSave);
            add(canvas, "快读", 186, 568, 81, 30, nativeReady, MainActivity.this::quickLoad);
            add(canvas, "存档", 273, 568, 81, 30, canSave, MainActivity.this::saveMenu);
            add(canvas, "读档", 360, 568, 81, 30, nativeReady, MainActivity.this::loadMenu);
            add(canvas, "隐藏", 447, 568, 81, 30, true,
                () -> command("playback", "manual", () -> { hideText = true; invalidate(); }));
            add(canvas, playback.equals("auto") ? "停止" : "自动", 534, 568, 81, 30, true, MainActivity.this::toggleAuto);
            add(canvas, playback.startsWith("skip") ? "停止" : "快进", 621, 568, 81, 30, true, MainActivity.this::toggleSkip);
            add(canvas, "履历", 708, 568, 81, 30, true, MainActivity.this::openHistory);
        }

        private void drawMenu(Canvas canvas) {
            boolean canSave = state.optBoolean("can_save") && !busy && !engineFailed;
            if (mode == Mode.TITLE && douseiMode) {
                rect(canvas, 0, 0, WIDTH, HEIGHT, 0xff101c2c);
                text(canvas, "同居情侣热恋中", 210, 176, 40, Color.WHITE);
                add(canvas, "开始游戏", 210, 232, 380, 48, nativeReady, MainActivity.this::begin);
                add(canvas, "读取存档", 210, 292, 380, 48, nativeReady, MainActivity.this::loadMenu);
                add(canvas, "播放 OP 视频", 210, 352, 380, 48, nativeReady,
                    () -> command("movie_play", "avi/d_loverable_OP.mpg", null));
                add(canvas, "播放设置", 210, 412, 380, 48, nativeReady, MainActivity.this::openPlayerSettings);
                add(canvas, "退出", 210, 472, 380, 48, true, MainActivity.this::finish);
                return;
            }
            if (mode == Mode.TITLE) {
                if (bundledMode && !nativeReady)
                    add(canvas, "重试准备游戏", 210, 272, 380, 42, true, MainActivity.this::retryBundledGame);
                else if (!nativeReady)
                    add(canvas, "导入游戏文件夹", 210, 272, 380, 42, true, MainActivity.this::pickGame);
                else originalButton(canvas, "t_btn", 0, 5, "开始游戏", 315, 272, 155, 54,
                    nativeReady, MainActivity.this::begin);
                originalButton(canvas, "t_btn", 1, 6, "继续游戏", 315, 327, 155, 54,
                    nativeReady, MainActivity.this::loadMenu);
                originalButton(canvas, "t_btn", 2, 7, "选项设置", 315, 382, 155, 54,
                    nativeReady, () -> openSettings(preferences.getInt("settings_page", 0)));
                originalButton(canvas, "t_btn", 3, 8, "附加内容", 315, 437, 155, 54,
                    nativeReady && state.optBoolean("extras_enabled"), MainActivity.this::openExtras);
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
                if (douseiMode) add(canvas, "播放设置", 210, 486, 380, 42, nativeReady, MainActivity.this::openPlayerSettings);
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

        private void drawNameEntry(Canvas canvas) {
            originalImage(canvas, "name_input", -1, 0, 0, 255);
            originalButton(canvas, "YesNo_btn", 0, 2, "确定", 200, 391, 166, 42, true, MainActivity.this::confirmName);
            originalButton(canvas, "YesNo_btn", 1, 3, "取消", 434, 391, 166, 42, true,
                () -> { closeNameEntry(); invalidate(); });
        }

        private void storyText(Canvas canvas, String value, float x, float y, int width,
                Layout.Alignment alignment, JSONObject settings, JSONObject flags) {
            int color = rgb(settings.optJSONArray("text_color"), Color.WHITE);
            int border = rgb(settings.optJSONArray("border_color"), Color.BLACK);
            int effect = flags.optInt("14", 1);
            TextPaint font = new TextPaint(sized(25f, color));
            font.setTypeface(Typeface.create(flags.optInt("2", 0) == 1 ? "serif" : "sans-serif", Typeface.NORMAL));
            font.setStrokeJoin(Paint.Join.ROUND); font.setStrokeWidth(2f);
            if (effect == 2) font.setShadowLayer(2, 1.5f, 1.5f, border);
            StaticLayout layout = StaticLayout.Builder.obtain(value, 0, value.length(), font, width)
                .setAlignment(alignment).setLineSpacing(32f - font.getFontSpacing(), 1f).setIncludePad(false).build();
            canvas.save(); canvas.translate(x, y);
            if (effect == 1) {
                font.setStyle(Paint.Style.STROKE); font.setColor(border); layout.draw(canvas);
                font.setStyle(Paint.Style.FILL); font.setColor(color);
            }
            layout.draw(canvas); canvas.restore();
        }

        private int rgb(JSONArray channels, int fallback) {
            return channels == null || channels.length() != 3 ? fallback
                : Color.rgb(channels.optInt(0), channels.optInt(1), channels.optInt(2));
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
                if ((event.getButtonState() & MotionEvent.BUTTON_SECONDARY) != 0) { navigateBack(); return true; }
                touchStartX = x; touchStartY = y; pressedX = x; pressedY = y;
                longPressed = false;
                boolean skipButton = douseiMode ? x >= 621 && x < 702 && y >= 568 && y < 598
                    : x >= 761 && y >= 543 && y < 564;
                if (mode == Mode.PLAYING && skipButton && !hideText)
                    ui.postDelayed(skipLongPress, 550);
                touchLastY = y; historyDragged = false;
                if (historyMenu != null && historyMenu.begin(x, y)) historyDragged = true;
                if (settingsMenu != null && settingsMenu.begin(x, y)) historyDragged = true;
                invalidate(); return true;
            }
            if (event.getActionMasked() == MotionEvent.ACTION_MOVE) {
                if (Math.abs(x - touchStartX) > 20 || Math.abs(y - touchStartY) > 20) ui.removeCallbacks(skipLongPress);
                if (historyMenu != null) {
                    if (Math.abs(y - touchStartY) > 10) historyDragged = true;
                    if (historyDragged) historyMenu.drag(x, y, y - touchLastY);
                }
                if (settingsMenu != null && historyDragged) settingsMenu.drag(x, y, y - touchLastY);
                touchLastY = y; pressedX = x; pressedY = y; invalidate(); return true;
            }
            if (event.getActionMasked() == MotionEvent.ACTION_CANCEL) {
                ui.removeCallbacks(skipLongPress);
                if (historyMenu != null) historyMenu.end();
                if (settingsMenu != null) settingsMenu.end();
                pressedX = -1; pressedY = -1; invalidate(); return true;
            }
            if (event.getActionMasked() != MotionEvent.ACTION_UP) return true;
            ui.removeCallbacks(skipLongPress);
            pressedX = -1; pressedY = -1; invalidate();
            performClick();
            if (longPressed) return true;
            if (x < 0 || y < 0 || x >= WIDTH || y >= HEIGHT || busy || actionPending) return true;
            if (nameEntry) {
                for (Button button : buttons) if (button.bounds.contains(x, y) && button.enabled) { button.action.run(); break; }
                return true;
            }
            if (extrasMenu != null) { extrasMenu.tap(x, y); invalidate(); return true; }
            if (settingsMenu != null) {
                settingsMenu.end();
                for (Button button : buttons) if (button.bounds.contains(x, y)) { if (button.enabled) button.action.run(); return true; }
                if (!historyDragged) settingsMenu.tap(x, y);
                return true;
            }
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
                    else command(state.optBoolean("text_complete", true) ? "advance" : "reveal", "", null);
                }
            }
            return true;
        }
        @Override protected void onSizeChanged(int width, int height, int oldWidth, int oldHeight) {
            super.onSizeChanged(width, height, oldWidth, oldHeight);
            if (remarkField != null) post(MainActivity.this::positionRemark);
            if (familyField != null && givenField != null) post(MainActivity.this::positionNameFields);
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
