// Runs against the preview harness (e2e/harness, started by the runner): the
// real React UI over a stateful mock backend (harness/mock-backend.ts) that
// starts from four sample records and the app's real first-run settings
// (override per page with ?set=key:value). Every invoke is recorded in
// `window.__invokeLog`, so tests can assert both what the UI asked the backend
// to do and what the user then sees.
import { test } from '@e2e-dev/web';
import { expect } from 'e2e';

type Call = { cmd: string; args: Record<string, unknown> | null };

const SAMPLE = {
  pinned: { id: 90001, text: '项目周会：周五 15:00，先确认需求范围，再安排下周计划。' },
  chat: { id: 90002, text: '你好，资料已收到。我会在明天下午前给你反馈，谢谢。' },
  url: { id: 90003, text: 'https://example.com/project/overview' },
  rich: { id: 90004, text: '设计检查' },
};

// Collects uncaught errors, rejections and console.error (React reports render
// crashes caught by ErrorBoundary only there). Clears localStorage so state
// such as language or color mode does not leak between tests through the
// shared Chrome profile.
test.beforeEach(async ({ browser }) => {
  await browser.addInitScript(() => {
    try {
      localStorage.clear();
    } catch {
      // about:blank and other opaque origins have no storage.
    }
    const errors: string[] = [];
    (window as any).__pageErrors = errors;
    addEventListener('error', (e) => errors.push(`error: ${e.message}`));
    addEventListener('unhandledrejection', (e) => errors.push(`unhandledrejection: ${String(e.reason)}`));
    const consoleError = console.error.bind(console);
    console.error = (...args: unknown[]) => {
      errors.push(`console.error: ${args.map(String).join(' ')}`);
      consoleError(...args);
    };
  });
});

// Harness gaps, not app bugs: the mock refusing a command it does not
// implement, and Tauri modules the preview does not alias (e.g.
// @tauri-apps/api/app getVersion) reaching for the missing IPC bridge.
// Anything else fails the test.
const HARNESS_NOISE = ['audit preview:', "reading 'invoke'"];

test.afterEach(async ({ browser }) => {
  const errors = await browser.evaluate(() => ((window as any).__pageErrors ?? []) as string[]);
  expect(errors.filter((e) => !HARNESS_NOISE.some((noise) => e.includes(noise)))).toEqual([]);
});

const calls = (browser: { evaluate: (fn: () => Call[]) => Promise<Call[]> }) =>
  // Round-trip through JSON: args from the real data flow carry `undefined` fields.
  browser.evaluate(() => JSON.parse(JSON.stringify((window as any).__invokeLog ?? [])) as Call[]);

const resetCalls = (browser: { evaluate: (fn: () => null) => Promise<null> }) =>
  browser.evaluate(() => {
    ((window as any).__invokeLog ?? []).length = 0;
    return null;
  });

const clearPageErrors = (browser: { evaluate: (fn: () => null) => Promise<null> }) =>
  browser.evaluate(() => {
    ((window as any).__pageErrors ?? []).length = 0;
    return null;
  });

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

const callsTo = async (browser: Parameters<typeof calls>[0], cmd: string) =>
  (await calls(browser)).filter((c) => c.cmd === cmd);

test('主窗口：顶栏入口、搜索框、分类 Tab 与样例记录都渲染出来', async ({ app, screen }) => {
  await app.open('/');
  for (const name of ['设置', '标签管理', '清空记录']) {
    await expect(screen.getByRole('button', name)).toBeVisible();
  }
  await expect(screen.getByRole('textbox', '搜索复制过的内容')).toBeVisible();
  for (const tab of ['全部', '文字', '图片', '文件', 'URL', '代码', '视频', '带格式']) {
    await expect(screen.getByRole('button', tab)).toBeVisible();
  }
  await expect(screen.getByRole('main').getByText(SAMPLE.chat.text)).toBeVisible();
  await expect(screen.getByRole('main').getByText(SAMPLE.url.text)).toBeVisible();
});

test('搜索：关键词只保留匹配记录，无结果显示空状态，清空后恢复', async ({ app, screen }) => {
  await app.open('/');
  // Whole <main> (list + detail pane): a hidden check over a narrower scope
  // passes vacuously when the item merely lives in another list.
  const main = screen.getByRole('main');
  const search = screen.getByRole('textbox', '搜索复制过的内容');
  await expect(main.getByText(SAMPLE.chat.text)).toBeVisible();

  await search.fill('周会');
  await expect(main.getByText(SAMPLE.pinned.text).first()).toBeVisible();
  await expect(main.getByText(SAMPLE.chat.text)).toBeHidden();
  await expect(main.getByText(SAMPLE.url.text)).toBeHidden();

  await search.fill('不存在的关键词xyz');
  await expect(screen.getByText('没找到记录，换个关键词试试。')).toBeVisible();

  await search.fill('');
  await expect(main.getByText(SAMPLE.chat.text)).toBeVisible();
});

test('分类 Tab：URL / 图片 只显示对应类型，切回全部恢复', async ({ app, screen }) => {
  await app.open('/');
  const main = screen.getByRole('main');

  await expect(main.getByText(SAMPLE.chat.text)).toBeVisible();

  await screen.getByRole('button', 'URL').tap();
  await expect(main.getByText(SAMPLE.url.text).first()).toBeVisible();
  await expect(main.getByText(SAMPLE.chat.text)).toBeHidden();
  await expect(screen.getByRole('button', 'URL')).toHaveAttribute('class', /\bactive\b/);

  await screen.getByRole('button', '图片').tap();
  await expect(main.getByText(SAMPLE.url.text)).toBeHidden();
  await expect(main.getByText(SAMPLE.chat.text)).toBeHidden();

  await screen.getByRole('button', '全部').tap();
  await expect(main.getByText(SAMPLE.chat.text)).toBeVisible();
});

test('单击记录：请求粘贴该条（copy_to_clipboard, paste=true）', async ({ app, screen, browser }) => {
  await app.open('/');
  await resetCalls(browser);
  await screen.getByRole('main').getByText(SAMPLE.chat.text).tap();
  await expect.poll(() => callsTo(browser, 'copy_to_clipboard')).toEqual([
    expect.objectContaining({
      args: expect.objectContaining({ id: SAMPLE.chat.id, content: SAMPLE.chat.text, paste: true }),
    }),
  ]);
});

test('详情侧栏：粘贴按钮粘贴当前选中项', async ({ app, screen, browser }) => {
  await app.open('/');
  const detail = screen.getByRole('complementary');
  await expect(detail.getByText(SAMPLE.pinned.text)).toBeVisible();
  for (const name of ['取消固定', '编辑标签', '编辑备注', '删除']) {
    await expect(detail.getByRole('button', name)).toBeVisible();
  }
  // AI is off on a fresh install, so its button is not offered.
  await expect(detail.getByRole('button', 'AI 助手')).toBeHidden();

  await resetCalls(browser);
  await detail.getByRole('button', '粘贴').tap();
  await expect.poll(() => callsTo(browser, 'copy_to_clipboard')).toEqual([
    expect.objectContaining({ args: expect.objectContaining({ id: SAMPLE.pinned.id, paste: true }) }),
  ]);
});

test('详情侧栏：开启 AI 助手后出现 AI 按钮', async ({ app, screen }) => {
  await app.open('/?set=ai_enabled:true');
  await expect(screen.getByRole('complementary').getByRole('button', 'AI 助手')).toBeVisible();
});

test('键盘：关闭方向键选择后，↓ + Enter 不会粘贴', async ({ app, screen, browser }) => {
  await app.open('/?set=app.arrow_key_selection:false');
  await expect(screen.getByRole('main').getByText(SAMPLE.chat.text)).toBeVisible();
  await resetCalls(browser);
  await browser.keyboard.press('ArrowDown');
  await sleep(150);
  await browser.keyboard.press('Enter');
  await sleep(500);
  expect(await callsTo(browser, 'copy_to_clipboard')).toEqual([]);
});

test('键盘：方向键选择默认开启，↓↓ 选到第二条，Enter 粘贴它', async ({ app, screen, browser }) => {
  await app.open('/');
  // Only macOS handles arrow selection in the page; on Windows the native
  // (Rust) keyboard hook does it (usesNativeNavigation), which the browser
  // harness does not have. CI runs on Linux, which takes the Windows path.
  const isMac = await browser.evaluate(() => /Mac/i.test(navigator.userAgent) || /Mac/i.test(navigator.platform));
  test.skip(!isMac, 'arrow navigation runs in the native hook outside macOS');
  await expect(screen.getByRole('main').getByText(SAMPLE.chat.text)).toBeVisible();
  await resetCalls(browser);
  // The app drops a repeat of the same arrow key within 90 ms (key-repeat
  // throttle in useKeyboardNavigation), so pace the presses like a person.
  await browser.keyboard.press('ArrowDown');
  await sleep(150);
  await browser.keyboard.press('ArrowDown');
  await sleep(150);
  await browser.keyboard.press('Enter');
  await expect.poll(() => callsTo(browser, 'copy_to_clipboard')).toEqual([
    expect.objectContaining({ args: expect.objectContaining({ id: SAMPLE.chat.id }) }),
  ]);
});

test('键盘：选中后按 Delete 删除；固定的不删，退格不删', async ({ app, screen, browser }) => {
  await app.open('/');
  const isMac = await browser.evaluate(() => /Mac/i.test(navigator.userAgent) || /Mac/i.test(navigator.platform));
  test.skip(!isMac, 'arrow navigation runs in the native hook outside macOS');
  const main = screen.getByRole('main');
  await expect(main.getByText(SAMPLE.chat.text).first()).toBeVisible();
  await resetCalls(browser);
  // First entry is pinned: Delete leaves it alone.
  await browser.keyboard.press('ArrowDown');
  await sleep(150);
  await browser.keyboard.press('Delete');
  await sleep(300);
  expect(await callsTo(browser, 'delete_clipboard_entry')).toEqual([]);
  // Second entry: plain Backspace is for the search box, Delete removes it.
  await browser.keyboard.press('ArrowDown');
  await sleep(150);
  await browser.keyboard.press('Backspace');
  await sleep(300);
  expect(await callsTo(browser, 'delete_clipboard_entry')).toEqual([]);
  await browser.keyboard.press('Delete');
  await expect.poll(() => callsTo(browser, 'delete_clipboard_entry')).toEqual([
    expect.objectContaining({ args: { id: SAMPLE.chat.id } }),
  ]);
  await expect(main.getByText(SAMPLE.chat.text)).toBeHidden();
});

test('删除：详情栏删除按钮删除当前项，列表中随之消失', async ({ app, screen, browser }) => {
  await app.open('/');
  const main = screen.getByRole('main');
  await expect(main.getByText(SAMPLE.pinned.text).first()).toBeVisible();
  await resetCalls(browser);
  await screen.getByRole('complementary').getByRole('button', '删除').tap();
  await expect.poll(() => callsTo(browser, 'delete_clipboard_entry')).toEqual([
    expect.objectContaining({ args: { id: SAMPLE.pinned.id } }),
  ]);
  await expect(main.getByText(SAMPLE.pinned.text)).toBeHidden();
  // The detail pane moves on to the next record, so its text shows twice.
  await expect(main.getByText(SAMPLE.chat.text).first()).toBeVisible();
});

test('清空记录：取消不清空，确认才清空', async ({ app, screen, browser }) => {
  await app.open('/');
  await resetCalls(browser);

  const notice = screen.getByText('固定和带标签的记录会保留。', { exact: false });
  await screen.getByRole('button', '清空记录').tap();
  await expect(notice).toBeVisible();
  await screen.getByRole('button', '取消').tap();
  await expect(notice).toBeHidden();
  expect(await callsTo(browser, 'clear_clipboard_history')).toEqual([]);

  await screen.getByRole('button', '清空记录').tap();
  await screen.getByRole('button', '确认').tap();
  await expect.poll(() => callsTo(browser, 'clear_clipboard_history')).toHaveLength(1);

  // Pinned and tagged records survive; the rest are gone.
  const main = screen.getByRole('main');
  await expect(main.getByText(SAMPLE.chat.text)).toBeHidden();
  await expect(main.getByText(SAMPLE.url.text)).toBeHidden();
  await expect(main.getByText(SAMPLE.pinned.text).first()).toBeVisible();
  await expect(main.getByText(SAMPLE.rich.text, { exact: false }).first()).toBeVisible();
});

test('设置：切换分组替换内容，修改选项会保存', async ({ app, screen, browser }) => {
  await app.open('/');
  await screen.getByRole('button', '设置').tap();
  await expect(screen.getByText('开机启动')).toBeVisible();

  await screen.getByRole('button', '外观设置').tap();
  await expect(screen.getByText('颜色模式')).toBeVisible();
  await expect(screen.getByText('开机启动')).toBeHidden();

  await resetCalls(browser);
  const dark = screen.getByRole('button', '深色');
  await dark.tap();
  await expect(dark).toHaveAttribute('aria-pressed', 'true');
  await expect.poll(() => callsTo(browser, 'save_setting')).toEqual([
    expect.objectContaining({ args: { key: 'app.color_mode', value: 'dark' } }),
  ]);
});

test('标签管理：面板列出后端返回的标签与计数', async ({ app, screen, browser }) => {
  await app.open('/');
  await resetCalls(browser);
  await screen.getByRole('button', '标签管理').tap();
  await expect(screen.getByText('代码片段')).toBeVisible();
  await expect(screen.getByText('链接收藏')).toBeVisible();
  await expect.poll(async () => (await callsTo(browser, 'get_all_tags_info')).length).toBeGreaterThan(0);
});

test('标签管理：选中某个标签按该标签加载记录', async ({ app, screen, browser }) => {
  await app.open('/');
  await screen.getByRole('button', '标签管理').tap();
  await expect(screen.getByText('代码片段')).toBeVisible();
  await resetCalls(browser);
  const chip = screen.getByRole('button', '链接收藏 · 13');
  await expect(chip).toHaveAttribute('aria-pressed', 'false');
  // The colour dot is its own control with its own label.
  await expect(screen.getByRole('button', '链接收藏: 标签颜色')).toBeAttached();
  await chip.tap();
  await expect.poll(() => callsTo(browser, 'get_tag_items')).toEqual([
    expect.objectContaining({ args: { tag: '链接收藏' } }),
  ]);
  await expect(chip).toHaveAttribute('aria-pressed', 'true');
  await expect(screen.getByText(/^#链接收藏/)).toBeVisible();
});

// In split view the per-row buttons are hidden by design (split-view.css) and
// live in the detail pane's action bar; hovering a row makes it the pane's item.
test('固定：悬停记录后，详情栏的固定按钮作用于该条', async ({ app, screen, browser }) => {
  await app.open('/');
  const detail = screen.getByRole('complementary');
  await expect(screen.getByRole('main').getByText(SAMPLE.chat.text)).toBeVisible();
  await expect(detail.getByText(SAMPLE.chat.text)).toBeHidden();
  const center = await browser.evaluate((text: string) => {
    const row = [...document.querySelectorAll('.history-item')].find((e) => (e as HTMLElement).innerText.includes(text))!;
    const r = row.getBoundingClientRect();
    return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
  }, SAMPLE.chat.text);
  await browser.mouse.move(center.x, center.y);
  await expect(detail.getByText(SAMPLE.chat.text)).toBeVisible();
  await resetCalls(browser);
  await detail.getByRole('button', '固定').tap();
  await expect.poll(() => callsTo(browser, 'toggle_clipboard_pin')).toEqual([
    expect.objectContaining({ args: { id: SAMPLE.chat.id, isPinned: true } }),
  ]);
});

// The preview exposes window.__pushItem in place of the backend's
// clipboard-update event; this checks how the list takes a new record.
test('新复制的内容：出现在未固定记录的最前面，固定项仍在顶部', async ({ app, browser }) => {
  await app.open('/');
  // Push only once the app listens for clipboard-updated, like the real backend
  // event arriving after startup.
  await expect.poll(() => browser.evaluate(() => (window as any).__listenerCount('clipboard-updated'))).toBeGreaterThan(0);
  await browser.evaluate(() => {
    (window as any).__pushItem({
      id: 95555, content_type: 'text', content: '新复制的内容 E2E', preview: '新复制的内容 E2E',
      source_app: '终端', timestamp: Date.now(), is_pinned: false, tags: [], use_count: 0,
    });
    return null;
  });
  const order = () =>
    browser.evaluate(() =>
      [...document.querySelectorAll('.history-item')].map((e) => (e as HTMLElement).innerText),
    );
  await expect.poll(async () => (await order()).findIndex((t) => t.includes('新复制的内容 E2E'))).toBe(1);
  expect((await order())[0]).toContain('项目周会');
});

test('紧凑模式：列表条目使用紧凑布局，默认模式不使用', async ({ app, browser }) => {
  const compactItems = () => browser.evaluate(() => document.querySelectorAll('.history-item.compact').length);
  await app.open('/');
  await expect.poll(() => browser.evaluate(() => document.querySelectorAll('.history-item').length)).toBeGreaterThan(0);
  expect(await compactItems()).toBe(0);

  await app.open('/?compact=1');
  await expect.poll(compactItems).toBeGreaterThan(0);
});

test('表情包视图：渲染表情网格', async ({ app, screen }) => {
  await app.open('/?view=emoji');
  await expect(screen.getByText('表情包')).toBeVisible();
  await expect(screen.getByText('😀').first()).toBeVisible();
});

test('局域网互传视图：加载并显示聊天记录', async ({ app, screen }) => {
  await app.open('/?view=transfer');
  await expect(screen.getByText('局域网互传')).toBeVisible();
  await expect(screen.getByText('手机发来的一段文字：会议纪要已整理好。')).toBeVisible();
});

// ?crash=1 injects an image record with content=null; ClipboardItem throws on
// it. This checks the safety net, not the record handling: the window must
// show the ErrorBoundary fallback with a working reload, never go blank.
test('渲染崩溃：ErrorBoundary 显示兜底界面和重新加载按钮', async ({ app, screen, browser }) => {
  await app.open('/?crash=1');
  const alert = screen.getByRole('alert');
  await expect(alert.getByText('界面异常')).toBeVisible();
  await expect(alert.getByRole('button', '重新加载')).toBeVisible();
  // The crash is the point of this test; keep afterEach strict for the rest.
  await clearPageErrors(browser);
});
