// SPDX-License-Identifier: GPL-3.0-only
package org.yukars.player;

import android.app.Activity;
import android.app.AlertDialog;
import android.app.Dialog;
import android.content.res.ColorStateList;
import android.graphics.Color;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.graphics.drawable.RippleDrawable;
import android.text.InputFilter;
import android.text.InputType;
import android.text.TextUtils;
import android.view.Gravity;
import android.view.View;
import android.view.ViewGroup;
import android.view.Window;
import android.widget.AdapterView;
import android.widget.ArrayAdapter;
import android.widget.Button;
import android.widget.EditText;
import android.widget.ImageView;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.Spinner;
import android.widget.TextView;
import java.io.File;
import java.util.Locale;

/** Native save slots, presented as a list with page controls and an action panel. */
final class FdSaveMenu {
    interface Listener {
        void page(int page, int row);
        void save(File file, String comment, int page, int row);
        void load(File file, int page, int row);
        void action(OriginalSaveMenu.Action action);
    }

    private static final int PAGE_COUNT = 20;
    private static final int ROW_COUNT = 5;
    private static final int REMARK_LIMIT = 26;
    private static final int INK = 0xff172331;
    private static final int MUTED = 0xff526477;
    private static final int ACCENT = 0xff176e68;
    private static final int PANEL = 0xffe8eef2;

    private final Activity activity;
    private final boolean saving;
    private final boolean canSave;
    private final boolean canPreviousChoice;
    private final Listener listener;
    private final Dialog dialog;
    private final LinearLayout list;
    private final Spinner pages;
    private final Button previous;
    private final Button next;
    private AlertDialog prompt;
    private int page;
    private int row;

    FdSaveMenu(Activity activity, boolean saving, int page, int row, boolean canSave,
               boolean canPreviousChoice, Listener listener) {
        this.activity = activity;
        this.saving = saving;
        this.canSave = canSave;
        this.canPreviousChoice = canPreviousChoice;
        this.listener = listener;
        this.page = Math.max(0, Math.min(PAGE_COUNT - 1, page));
        this.row = Math.max(1, Math.min(ROW_COUNT, row));

        dialog = new Dialog(activity);
        dialog.requestWindowFeature(Window.FEATURE_NO_TITLE);
        dialog.setCanceledOnTouchOutside(false);
        dialog.setOnCancelListener(ignored -> listener.action(OriginalSaveMenu.Action.RETURN));

        LinearLayout content = new LinearLayout(activity);
        content.setOrientation(LinearLayout.HORIZONTAL);
        content.setBackgroundColor(0xfff4f7fa);
        content.setPadding(dp(16), dp(12), dp(12), dp(12));

        LinearLayout main = column();
        content.addView(main, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.MATCH_PARENT, 1));
        LinearLayout header = new LinearLayout(activity);
        header.setGravity(Gravity.CENTER_VERTICAL);
        TextView heading = label(saving ? "保存进度" : "读取进度", 21, INK, true);
        header.addView(heading, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1));
        previous = button("上一页", () -> changePage(this.page - 1));
        header.addView(previous);
        pages = new Spinner(activity);
        pages.setContentDescription("存档页");
        String[] labels = new String[PAGE_COUNT];
        for (int index = 0; index < labels.length; index++) labels[index] = "第 " + (index + 1) + " 页";
        ArrayAdapter<String> choices = new ArrayAdapter<>(activity, android.R.layout.simple_spinner_item, labels);
        choices.setDropDownViewResource(android.R.layout.simple_spinner_dropdown_item);
        pages.setAdapter(choices);
        pages.setSelection(this.page);
        header.addView(pages, new LinearLayout.LayoutParams(dp(110), dp(48)));
        next = button("下一页", () -> changePage(this.page + 1));
        header.addView(next);
        main.addView(header, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(56)));

        ScrollView scroll = new ScrollView(activity);
        scroll.setFillViewport(true);
        list = column();
        scroll.addView(list);
        main.addView(scroll, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1));

        ScrollView actions = new ScrollView(activity);
        actions.setBackground(drawable(PANEL));
        LinearLayout navigation = column();
        navigation.setPadding(dp(8), dp(8), dp(8), dp(8));
        actions.addView(navigation);
        LinearLayout.LayoutParams actionLayout = new LinearLayout.LayoutParams(dp(144), ViewGroup.LayoutParams.MATCH_PARENT);
        actionLayout.setMarginStart(dp(12));
        content.addView(actions, actionLayout);
        addAction(navigation, "保存进度", OriginalSaveMenu.Action.SAVE, canSave && !saving);
        addAction(navigation, "读取进度", OriginalSaveMenu.Action.LOAD, saving);
        divider(navigation);
        addAction(navigation, "画面设置", OriginalSaveMenu.Action.SCREEN, true);
        addAction(navigation, "文本设置", OriginalSaveMenu.Action.TEXT, true);
        addAction(navigation, "声音设置", OriginalSaveMenu.Action.SOUND, true);
        addAction(navigation, "语音设置", OriginalSaveMenu.Action.VOICE, true);
        addAction(navigation, "安卓工具", OriginalSaveMenu.Action.OPTIONS, true);
        divider(navigation);
        addAction(navigation, "回到上个选择", OriginalSaveMenu.Action.PREVIOUS_CHOICE, canPreviousChoice);
        addAction(navigation, "返回标题", OriginalSaveMenu.Action.TITLE, true);
        addAction(navigation, "关闭游戏", OriginalSaveMenu.Action.EXIT, true);
        addAction(navigation, "返回", OriginalSaveMenu.Action.RETURN, true);

        dialog.setContentView(content);
        pages.setOnItemSelectedListener(new AdapterView.OnItemSelectedListener() {
            @Override public void onItemSelected(AdapterView<?> parent, View view, int selected, long id) {
                changePage(selected);
            }
            @Override public void onNothingSelected(AdapterView<?> parent) { }
        });
    }

    int page() { return page; }
    int row() { return row; }

    boolean isShowing() { return dialog.isShowing(); }

    void show() {
        dialog.show();
        Window window = dialog.getWindow();
        if (window != null) window.setLayout(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT);
        refresh();
    }

    /** Programmatic dismissal does not call the return action. */
    void close() {
        if (prompt != null) prompt.dismiss();
        dialog.dismiss();
    }

    void refresh() {
        if (!isShowing()) return;
        previous.setEnabled(page > 0);
        next.setEnabled(page + 1 < PAGE_COUNT);
        list.removeAllViews();
        list.addView(label("正在读取存档…", 15, MUTED, false));
        listener.page(page, row);
    }

    void updateSlots(int selectedPage, OriginalSaveMenu.Slot[] slots) {
        if (!isShowing() || selectedPage != page) return;
        if (slots.length != ROW_COUNT) throw new IllegalArgumentException("Invalid FD save page");
        list.removeAllViews();
        for (int index = 0; index < slots.length; index++) {
            addSlot(slots[index], index + 1);
        }
    }

    private void changePage(int selected) {
        if (!isShowing() || selected < 0 || selected >= PAGE_COUNT || selected == page) return;
        page = selected;
        if (pages.getSelectedItemPosition() != selected) pages.setSelection(selected);
        refresh();
    }

    private void addSlot(OriginalSaveMenu.Slot slot, int position) {
        boolean automatic = page == 0 && position == 1;
        boolean active = saving ? canSave && !automatic : slot.exists && slot.error.isEmpty();
        LinearLayout card = new LinearLayout(activity);
        card.setGravity(Gravity.CENTER_VERTICAL);
        card.setPadding(dp(8), dp(6), dp(10), dp(6));
        card.setMinimumHeight(dp(62));
        int surface = position == row ? 0xffdfefed : Color.WHITE;
        card.setBackground(new RippleDrawable(ColorStateList.valueOf(0x2a176e68), drawable(surface), null));
        card.setFocusable(active);
        if (active) card.setOnClickListener(view -> selectSlot(slot, position));
        LinearLayout.LayoutParams cardLayout = new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT,
            ViewGroup.LayoutParams.WRAP_CONTENT);
        cardLayout.bottomMargin = dp(6);
        list.addView(card, cardLayout);

        ImageView preview = new ImageView(activity);
        preview.setScaleType(ImageView.ScaleType.FIT_CENTER);
        preview.setBackground(drawable(PANEL));
        if (slot.thumbnail != null) preview.setImageBitmap(slot.thumbnail);
        preview.setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_NO);
        card.addView(preview, new LinearLayout.LayoutParams(dp(64), dp(48)));

        LinearLayout detail = column();
        LinearLayout.LayoutParams detailLayout = new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1);
        detailLayout.setMarginStart(dp(10));
        card.addView(detail, detailLayout);
        String slotName = String.format(Locale.ROOT, "%03d", page * 10 + position);
        String title = !slot.exists ? "空存档位" : !slot.error.isEmpty() ? "无法读取预览"
            : slot.chapter.isEmpty() ? "剧情存档" : slot.chapter;
        String heading = slotName + (automatic ? " · 自动存档 · " : " · ") + title;
        detail.addView(label(heading, 15, INK, true));
        if (slot.exists && !slot.date.isEmpty()) detail.addView(label(slot.date, 12, MUTED, false));
        String remark = !slot.error.isEmpty() ? slot.error : slot.caption;
        if (!remark.isEmpty()) detail.addView(label(remark, 13, MUTED, false));
        if (saving && automatic) detail.addView(label("自动保存专用", 12, ACCENT, false));
    }

    private void selectSlot(OriginalSaveMenu.Slot slot, int position) {
        if (prompt != null) return;
        int selectedPage = page;
        row = position;
        if (saving) {
            EditText remark = new EditText(activity);
            remark.setSingleLine(true);
            remark.setInputType(InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_FLAG_CAP_SENTENCES);
            remark.setHint("备注（可选）");
            remark.setFilters(new InputFilter[]{new InputFilter.LengthFilter(REMARK_LIMIT)});
            LinearLayout area = column();
            area.setPadding(dp(24), 0, dp(24), 0);
            area.addView(remark);
            showPrompt(new AlertDialog.Builder(activity)
                .setTitle(slot.exists ? "覆盖存档" : "保存进度")
                .setMessage(slot.exists ? "覆盖 " + slot.file.getName() + " 中的原有进度？" : slot.file.getName())
                .setView(area).setPositiveButton("保存", (ignored, which) ->
                    listener.save(slot.file, remark.getText().toString(), selectedPage, position))
                .setNegativeButton("取消", null));
        } else {
            showPrompt(new AlertDialog.Builder(activity).setTitle("读取进度")
                .setMessage("读取 " + slot.file.getName() + " 并继续游玩？")
                .setPositiveButton("读取", (ignored, which) -> listener.load(slot.file, selectedPage, position))
                .setNegativeButton("取消", null));
        }
    }

    private void addAction(LinearLayout group, String title, OriginalSaveMenu.Action action, boolean enabled) {
        Button button = button(title, () -> navigate(action));
        button.setEnabled(enabled);
        group.addView(button, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(46)));
    }

    private void navigate(OriginalSaveMenu.Action action) {
        if (prompt != null) return;
        String title;
        String message;
        switch (action) {
            case PREVIOUS_CHOICE:
                title = "回到上个选择";
                message = "读取最近一个选择点的存档？";
                break;
            case TITLE:
                title = "返回标题";
                message = "返回标题并结束当前游戏进度？";
                break;
            case EXIT:
                title = "关闭游戏";
                message = "结束本次游玩？";
                break;
            default:
                listener.action(action);
                return;
        }
        showPrompt(new AlertDialog.Builder(activity).setTitle(title).setMessage(message)
            .setPositiveButton("确认", (ignored, which) -> listener.action(action)).setNegativeButton("取消", null));
    }

    private void showPrompt(AlertDialog.Builder builder) {
        AlertDialog confirmation = builder.create();
        prompt = confirmation;
        confirmation.setOnDismissListener(ignored -> { if (prompt == confirmation) prompt = null; });
        confirmation.show();
    }

    private void divider(LinearLayout group) {
        View line = new View(activity);
        line.setBackgroundColor(0xffcbd6de);
        LinearLayout.LayoutParams layout = new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(1));
        layout.topMargin = dp(6);
        layout.bottomMargin = dp(6);
        group.addView(line, layout);
    }

    private TextView label(String title, int size, int color, boolean bold) {
        TextView view = new TextView(activity);
        view.setText(title);
        view.setTextSize(size);
        view.setTextColor(color);
        view.setSingleLine(true);
        view.setEllipsize(TextUtils.TruncateAt.END);
        if (bold) view.setTypeface(Typeface.DEFAULT, Typeface.BOLD);
        return view;
    }

    private Button button(String title, Runnable action) {
        Button button = new Button(activity);
        button.setText(title);
        button.setAllCaps(false);
        button.setOnClickListener(view -> action.run());
        return button;
    }

    private LinearLayout column() {
        LinearLayout linear = new LinearLayout(activity);
        linear.setOrientation(LinearLayout.VERTICAL);
        return linear;
    }

    private GradientDrawable drawable(int color) {
        GradientDrawable background = new GradientDrawable();
        background.setCornerRadius(dp(8));
        background.setColor(color);
        return background;
    }

    private int dp(int value) {
        return Math.round(value * activity.getResources().getDisplayMetrics().density);
    }
}
