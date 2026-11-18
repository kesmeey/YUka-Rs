// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

import android.app.Activity;
import android.app.AlertDialog;
import android.widget.ArrayAdapter;
import android.widget.Button;
import android.widget.CheckBox;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.SeekBar;
import android.widget.Spinner;
import android.widget.TextView;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.function.Consumer;
import java.util.function.IntSupplier;
import org.json.JSONObject;

/** FD options grouped in a native Android dialog with a staged, cancelable edit. */
final class PlayerSettingsDialog {
    interface Preview { void play(int sampleIndex, float volume); }

    private static final String[] CATEGORIES = {"画面", "文字", "声音", "语音"};
    private static final int[] WOMEN = {110, 111, 112, 113, 114, 115, 116, 117, 122, 123, 120};
    private static final int[] MEN = {118, 119, 124, 121};

    private final Activity activity;
    private final JSONObject original;
    private final Preview preview;
    private final LinearLayout controls;
    private final ScrollView scroll;
    private final Button[] tabs = new Button[CATEGORIES.length];
    private final Map<Integer, Integer> draft = new LinkedHashMap<>();
    private final Map<Integer, IntSupplier> values = new LinkedHashMap<>();
    private final Map<Integer, SeekBar> volumeBars = new LinkedHashMap<>();

    private PlayerSettingsDialog(Activity activity, JSONObject flags, Preview preview) {
        this.activity = activity;
        original = flags;
        this.preview = preview;
        controls = new LinearLayout(activity);
        controls.setOrientation(LinearLayout.VERTICAL);
        controls.setPadding(dp(20), 0, dp(20), dp(12));
        scroll = new ScrollView(activity);
        scroll.addView(controls);
    }

    static AlertDialog show(Activity activity, JSONObject flags, int section,
            Consumer<JSONObject> apply, Preview preview, Runnable tools, Runnable dismissed) {
        PlayerSettingsDialog form = new PlayerSettingsDialog(activity, flags, preview);
        LinearLayout body = new LinearLayout(activity);
        body.setOrientation(LinearLayout.VERTICAL);
        LinearLayout categories = new LinearLayout(activity);
        categories.setPadding(form.dp(12), 0, form.dp(12), 0);
        for (int index = 0; index < CATEGORIES.length; index++) {
            int selected = index;
            Button tab = new Button(activity);
            tab.setText(CATEGORIES[index]);
            tab.setOnClickListener(view -> form.open(selected));
            categories.addView(tab, new LinearLayout.LayoutParams(0, -2, 1));
            form.tabs[index] = tab;
        }
        body.addView(categories);
        body.addView(form.scroll, new LinearLayout.LayoutParams(-1, 0, 1));
        form.open(Math.max(0, Math.min(CATEGORIES.length - 1, section)));
        AlertDialog dialog = new AlertDialog.Builder(activity)
            .setTitle("游戏设置").setView(body)
            .setPositiveButton("应用", (whichDialog, which) -> apply.accept(form.changes()))
            .setNeutralButton("安卓工具", (whichDialog, which) -> tools.run())
            .setNegativeButton("取消", null).create();
        dialog.setOnDismissListener(ignored -> dismissed.run());
        dialog.show();
        return dialog;
    }

    private void open(int section) {
        capture();
        rebuild(section);
    }

    private void rebuild(int section) {
        controls.removeAllViews();
        values.clear();
        volumeBars.clear();
        for (int index = 0; index < tabs.length; index++) tabs[index].setEnabled(index != section);
        switch (section) {
            case 0:
                toggle(-1, "全屏显示");
                choice(10, "画面效果", "简化", "普通", "关闭");
                choice(11, "返回键或鼠标右键", "打开菜单", "隐藏文字");
                choice(15, "文本框透明度", "高", "中", "低");
                break;
            case 1:
                slider(4, "逐字显示间隔", 66, " 毫秒", -1);
                slider(5, "自动阅读等待", 6600, " 毫秒", -1);
                choice(3, "快进范围", "已读文字", "全部文字");
                toggle(13, "选择分支后继续快进");
                toggle(12, "选择分支后继续自动阅读");
                toggle(16, "点击时停止当前语音");
                choice(2, "字体", "无衬线", "衬线");
                choice(14, "文字效果", "无", "描边", "阴影");
                break;
            case 2:
                slider(100, "背景音乐音量", 1000, "‰", -1);
                slider(101, "音效音量", 1000, "‰", 18);
                toggle(19, "语音播放时降低背景音乐音量");
                break;
            case 3:
                slider(102, "整体语音音量", 1000, "‰", 0);
                slider(103, "女性角色音量", 1000, "‰", 1);
                slider(104, "男性角色音量", 1000, "‰", 2);
                slider(110, "琴宫千夏", 1000, "‰", 3);
                slider(111, "花穗", 1000, "‰", 4);
                slider(112, "姬野继美", 1000, "‰", 5);
                slider(113, "三枝奈奈子", 1000, "‰", 6);
                slider(114, "柳濑彩月", 1000, "‰", 7);
                slider(115, "冬美", 1000, "‰", 8);
                slider(116, "奈绪", 1000, "‰", 9);
                slider(117, "姬奈", 1000, "‰", 10);
                slider(123, "舞", 1000, "‰", 16);
                slider(122, "诗织", 1000, "‰", 15);
                slider(118, "店长", 1000, "‰", 12);
                slider(119, "流星", 1000, "‰", 13);
                slider(124, "树", 1000, "‰", 17);
                slider(120, "其他女声", 1000, "‰", 11);
                slider(121, "其他男声", 1000, "‰", 14);
                break;
            default:
                throw new IllegalArgumentException("Unknown settings section");
        }
        Button reset = new Button(activity);
        reset.setText("恢复本页默认值");
        reset.setOnClickListener(view -> {
            for (int flag : values.keySet()) draft.put(flag, defaultValue(flag));
            rebuild(section);
        });
        controls.addView(reset);
        scroll.scrollTo(0, 0);
    }

    private int dp(int pixels) {
        return Math.round(activity.getResources().getDisplayMetrics().density * pixels);
    }

    private static int defaultValue(int flag) {
        if (flag == 4) return 20;
        if (flag == 5) return 2000;
        if (flag == -1 || flag == 10 || flag == 12 || flag == 13 || flag == 14 || flag == 15) return 1;
        if (flag == 100 || flag == 101) return 650;
        return flag >= 102 ? 1000 : 0;
    }

    private int value(int flag) {
        return draft.getOrDefault(flag, original.optInt(Integer.toString(flag), defaultValue(flag)));
    }

    private void toggle(int flag, String title) {
        CheckBox checkbox = new CheckBox(activity);
        checkbox.setText(title);
        checkbox.setChecked(value(flag) != 0);
        controls.addView(checkbox);
        values.put(flag, () -> checkbox.isChecked() ? 1 : 0);
    }

    private void choice(int flag, String title, String... options) {
        TextView label = new TextView(activity);
        label.setText(title);
        controls.addView(label);
        Spinner spinner = new Spinner(activity);
        spinner.setContentDescription(title);
        ArrayAdapter<String> choices = new ArrayAdapter<>(activity, android.R.layout.simple_spinner_item, options);
        choices.setDropDownViewResource(android.R.layout.simple_spinner_dropdown_item);
        spinner.setAdapter(choices);
        spinner.setSelection(Math.max(0, Math.min(value(flag), options.length - 1)));
        controls.addView(spinner);
        values.put(flag, spinner::getSelectedItemPosition);
    }

    private void slider(int flag, String title, int max, String unit, int sample) {
        LinearLayout row = new LinearLayout(activity);
        TextView label = new TextView(activity);
        row.addView(label, new LinearLayout.LayoutParams(0, -2, 1));
        SeekBar bar = new SeekBar(activity);
        bar.setMax(max);
        bar.setProgress(value(flag));
        bar.setContentDescription(title);
        label.setText(title + " · " + bar.getProgress() + unit);
        if (flag >= 100) {
            volumeBars.put(flag, bar);
            if (sample >= 0) row.addView(volumeButton("试听", title, () -> preview.play(sample, bar.getProgress() / 1000f)));
            row.addView(volumeButton("静音", title, () -> {
                bar.setProgress(0);
                propagateVolume(flag, 0);
            }));
        }
        controls.addView(row);
        controls.addView(bar);
        values.put(flag, bar::getProgress);
        bar.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener() {
            @Override public void onProgressChanged(SeekBar slider, int progress, boolean fromUser) {
                label.setText(title + " · " + progress + unit);
                if (fromUser) propagateVolume(flag, progress);
            }
            @Override public void onStartTrackingTouch(SeekBar slider) { }
            @Override public void onStopTrackingTouch(SeekBar slider) { }
        });
    }

    private Button volumeButton(String action, String title, Runnable handler) {
        Button button = new Button(activity);
        button.setText(action);
        button.setContentDescription(title + " · " + action);
        button.setOnClickListener(view -> handler.run());
        return button;
    }

    private void propagateVolume(int flag, int gain) {
        if (flag == 102) {
            volumeBars.get(103).setProgress(gain);
            volumeBars.get(104).setProgress(gain);
        }
        if (flag == 102 || flag == 103) {
            for (int child : WOMEN) volumeBars.get(child).setProgress(gain);
        }
        if (flag == 102 || flag == 104) {
            for (int child : MEN) volumeBars.get(child).setProgress(gain);
        }
    }

    private void capture() {
        for (Map.Entry<Integer, IntSupplier> entry : values.entrySet())
            draft.put(entry.getKey(), entry.getValue().getAsInt());
    }

    private JSONObject changes() {
        capture();
        Map<String, Integer> patch = new LinkedHashMap<>();
        boolean voiceChanged = draft.entrySet().stream().anyMatch(entry ->
            entry.getKey() >= 102 && differsFromOriginal(entry.getKey(), entry.getValue()));
        for (Map.Entry<Integer, Integer> entry : draft.entrySet()) {
            int flag = entry.getKey();
            int value = entry.getValue();
            // Send the full staged voice group together. Otherwise an aggregate
            // slider could overwrite a character restored to its previous gain.
            if ((voiceChanged && flag >= 102) || differsFromOriginal(flag, value)) {
                patch.put(Integer.toString(flag), value);
            }
        }
        return new JSONObject(patch);
    }

    private boolean differsFromOriginal(int flag, int value) {
        return value != original.optInt(Integer.toString(flag), defaultValue(flag));
    }
}
