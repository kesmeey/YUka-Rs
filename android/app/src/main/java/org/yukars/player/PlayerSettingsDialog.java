// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

import android.app.Activity;
import android.app.AlertDialog;
import android.widget.ArrayAdapter;
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

/** Android controls for a game that does not use the Loverable settings layout. */
final class PlayerSettingsDialog {
    private final Activity activity;
    private final LinearLayout controls;
    private final JSONObject initial;
    private final Map<String, IntSupplier> values = new LinkedHashMap<>();

    private PlayerSettingsDialog(Activity activity, JSONObject flags) {
        this.activity = activity;
        initial = flags;
        controls = new LinearLayout(activity);
        controls.setOrientation(LinearLayout.VERTICAL);
        int padding = Math.round(activity.getResources().getDisplayMetrics().density * 24);
        controls.setPadding(padding, 0, padding, 0);
    }

    static void show(Activity activity, JSONObject flags, Consumer<JSONObject> apply, Runnable tools) {
        PlayerSettingsDialog form = new PlayerSettingsDialog(activity, flags);
        form.toggle("-1", "全屏显示");
        form.toggle("16", "点击时停止当前语音");
        form.toggle("19", "播放语音时降低背景音乐音量");
        form.choice("10", "画面效果", "简化", "普通", "关闭");
        form.choice("15", "文本框透明度", "高", "中", "低");
        form.slider("4", "文字间隔", 66, " ms");
        form.slider("5", "自动阅读等待", 6600, " ms");
        form.slider("100", "背景音乐音量", 1000, "‰");
        form.slider("101", "音效音量", 1000, "‰");
        form.slider("102", "全部角色语音音量", 1000, "‰");
        ScrollView scroll = new ScrollView(activity);
        scroll.addView(form.controls);
        new AlertDialog.Builder(activity).setTitle("播放设置").setView(scroll)
            .setPositiveButton("应用", (dialog, which) -> apply.accept(form.changes()))
            .setNeutralButton("安卓工具", (dialog, which) -> tools.run())
            .setNegativeButton("取消", null).show();
    }

    private void toggle(String flag, String title) {
        CheckBox checkbox = new CheckBox(activity);
        checkbox.setText(title);
        checkbox.setChecked(initial.optInt(flag) != 0);
        controls.addView(checkbox);
        values.put(flag, () -> checkbox.isChecked() ? 1 : 0);
    }

    private void choice(String flag, String title, String... options) {
        TextView label = new TextView(activity);
        label.setText(title);
        controls.addView(label);
        Spinner spinner = new Spinner(activity);
        ArrayAdapter<String> choices = new ArrayAdapter<>(activity, android.R.layout.simple_spinner_item, options);
        choices.setDropDownViewResource(android.R.layout.simple_spinner_dropdown_item);
        spinner.setAdapter(choices);
        spinner.setSelection(Math.max(0, Math.min(initial.optInt(flag), options.length - 1)));
        controls.addView(spinner);
        values.put(flag, spinner::getSelectedItemPosition);
    }

    private void slider(String flag, String title, int max, String unit) {
        TextView label = new TextView(activity);
        SeekBar bar = new SeekBar(activity);
        bar.setMax(max);
        bar.setProgress(initial.optInt(flag));
        bar.setContentDescription(title);
        label.setText(title + " · " + bar.getProgress() + unit);
        bar.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener() {
            @Override public void onProgressChanged(SeekBar slider, int progress, boolean fromUser) {
                label.setText(title + " · " + progress + unit);
            }
            @Override public void onStartTrackingTouch(SeekBar slider) { }
            @Override public void onStopTrackingTouch(SeekBar slider) { }
        });
        controls.addView(label);
        controls.addView(bar);
        values.put(flag, bar::getProgress);
    }

    private JSONObject changes() {
        Map<String, Integer> patch = new LinkedHashMap<>();
        for (Map.Entry<String, IntSupplier> entry : values.entrySet()) {
            int value = entry.getValue().getAsInt();
            if (value != initial.optInt(entry.getKey())) patch.put(entry.getKey(), value);
        }
        return new JSONObject(patch);
    }
}
