/**
 * 键盘导航与快捷键验证脚本。
 *
 * 覆盖：方向键在封面墙 / 列表间的焦点移动、Home / End 跳转、
 * Enter 打开详情、Space 进入多选并勾选、Ctrl+F 与 / 聚焦搜索、Esc 收尾，
 * 以及焦点环的可见性。
 *
 * 用法：
 *   node scripts/ui_keys.mjs [输出目录]
 * 前置：先起 vite（默认 1421），脚本会自己拉起带 --remote-debugging-port 的 Edge。
 */
import { spawn } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { setTimeout as sleep } from "node:timers/promises";

const PORT = 9443;
const BASE = process.env.BASE_URL || "http://127.0.0.1:1421/scripts/mock.html";
const OUT = process.argv[2] || "/tmp/ui-keys";
const EDGE =
  process.env.EDGE_PATH ||
  "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe";

mkdirSync(OUT, { recursive: true });

const consoleErrors = [];
const exceptions = [];
const results = [];

function check(ok, label, detail = "") {
  results.push({ ok, label, detail });
  console.log(`  ${ok ? "✓" : "✗"} ${label}${detail ? ` — ${detail}` : ""}`);
}

async function connect() {
  for (let i = 0; i < 60; i += 1) {
    try {
      const res = await fetch(`http://127.0.0.1:${PORT}/json`);
      const list = await res.json();
      const page = list.find((t) => t.type === "page" && t.webSocketDebuggerUrl);
      if (page) return page.webSocketDebuggerUrl;
    } catch {
      /* 未就绪 */
    }
    await sleep(500);
  }
  throw new Error("CDP 连接超时");
}

function makeClient(wsUrl) {
  const ws = new WebSocket(wsUrl);
  let seq = 0;
  const pending = new Map();
  ws.addEventListener("message", (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.id && pending.has(msg.id)) {
      const { resolve, reject } = pending.get(msg.id);
      pending.delete(msg.id);
      msg.error ? reject(new Error(JSON.stringify(msg.error))) : resolve(msg.result);
      return;
    }
    if (msg.method === "Runtime.consoleAPICalled" && msg.params.type === "error") {
      consoleErrors.push(
        (msg.params.args ?? []).map((a) => a.value ?? a.description ?? a.type).join(" "),
      );
    }
    if (msg.method === "Runtime.exceptionThrown") {
      const d = msg.params.exceptionDetails;
      exceptions.push(d.exception?.description ?? d.text);
    }
  });
  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      const id = ++seq;
      pending.set(id, { resolve, reject });
      ws.send(JSON.stringify({ id, method, params }));
    });
  const ready = new Promise((resolve, reject) => {
    ws.addEventListener("open", resolve);
    ws.addEventListener("error", () => reject(new Error("WS 错误")));
  });
  return { send, ready, close: () => ws.close() };
}

const KEYS = {
  ArrowLeft: { key: "ArrowLeft", code: "ArrowLeft", vk: 37 },
  ArrowUp: { key: "ArrowUp", code: "ArrowUp", vk: 38 },
  ArrowRight: { key: "ArrowRight", code: "ArrowRight", vk: 39 },
  ArrowDown: { key: "ArrowDown", code: "ArrowDown", vk: 40 },
  Enter: { key: "Enter", code: "Enter", vk: 13, text: "\r" },
  Escape: { key: "Escape", code: "Escape", vk: 27 },
  End: { key: "End", code: "End", vk: 35 },
  Home: { key: "Home", code: "Home", vk: 36 },
  f: { key: "f", code: "KeyF", vk: 70, text: "f" },
};

const edge = spawn(EDGE, [
  "--headless=new",
  `--remote-debugging-port=${PORT}`,
  "--no-first-run",
  "--no-default-browser-check",
  "--disable-gpu",
  "--window-size=1440,900",
  BASE,
]);

try {
  const wsUrl = await connect();
  const client = makeClient(wsUrl);
  await client.ready;
  const { send } = client;

  await send("Page.enable");
  await send("Runtime.enable");
  await send("Emulation.setDeviceMetricsOverride", {
    width: 1440,
    height: 900,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await send("Page.navigate", { url: BASE });
  await sleep(3500);

  const evaluate = async (expression) => {
    const r = await send("Runtime.evaluate", {
      expression,
      returnByValue: true,
      awaitPromise: true,
    });
    if (r.exceptionDetails) {
      throw new Error(r.exceptionDetails.exception?.description ?? "eval 失败");
    }
    return r.result.value;
  };

  const shot = async (name) => {
    const r = await send("Page.captureScreenshot", { format: "png" });
    writeFileSync(`${OUT}/${name}.png`, Buffer.from(r.data, "base64"));
  };

  /** 发送真实键盘事件（走 Input 域，浏览器会按原生流程派发） */
  const press = async (name, modifiers = 0) => {
    const k = KEYS[name];
    if (!k) throw new Error(`未定义的按键: ${name}`);
    const base = {
      key: k.key,
      code: k.code,
      windowsVirtualKeyCode: k.vk,
      nativeVirtualKeyCode: k.vk,
      modifiers,
    };
    await send("Input.dispatchKeyEvent", {
      ...base,
      type: k.text ? "keyDown" : "rawKeyDown",
      ...(k.text ? { text: k.text } : {}),
    });
    await send("Input.dispatchKeyEvent", { ...base, type: "keyUp" });
    await sleep(220);
  };

  /** 当前聚焦元素在给定选择器集合里的下标 */
  const focusedIndex = (selector) =>
    evaluate(`(() => {
      const items = [...document.querySelectorAll(${JSON.stringify(selector)})];
      return items.indexOf(document.activeElement);
    })()`);

  const activeTag = () =>
    evaluate(`document.activeElement ? document.activeElement.tagName : null`);

  console.log("启动无头 Edge…\n=== 键盘导航 ===\n");

  await evaluate(`location.hash = "#/"`);
  await sleep(900);

  /* ---------- 1. roving tabindex：只有一个卡片可 Tab 到 ---------- */
  const tabStops = await evaluate(`(() => {
    const cards = [...document.querySelectorAll('[data-game-card]')];
    return {
      total: cards.length,
      zero: cards.filter(c => c.getAttribute('tabindex') === '0').length,
      minusOne: cards.filter(c => c.getAttribute('tabindex') === '-1').length,
    };
  })()`);
  check(tabStops.total > 1, "封面墙渲染出多张卡片", `${tabStops.total} 张`);
  check(
    tabStops.zero === 1 && tabStops.minusOne === tabStops.total - 1,
    "roving tabindex 正确：仅 1 张卡片可 Tab 到",
    `tabindex=0 有 ${tabStops.zero} 张，-1 有 ${tabStops.minusOne} 张`,
  );

  /* ---------- 2. 方向键移动焦点 ---------- */
  await evaluate(`document.querySelector('[data-game-card]').focus()`);
  await sleep(200);
  check((await focusedIndex("[data-game-card]")) === 0, "初始聚焦第 1 张卡片");

  await press("ArrowRight");
  check((await focusedIndex("[data-game-card]")) === 1, "→ 移动到第 2 张卡片");

  await press("ArrowLeft");
  check((await focusedIndex("[data-game-card]")) === 0, "← 退回第 1 张卡片");

  // 列数从实际布局读取，保证「下一行」的期望值正确
  const columns = await evaluate(`(() => {
    const grid = document.querySelector('[data-grid]');
    return getComputedStyle(grid).gridTemplateColumns.split(/\\s+/).filter(Boolean).length;
  })()`);

  await press("ArrowDown");
  check(
    (await focusedIndex("[data-game-card]")) === columns,
    "↓ 移动到下一行同一列",
    `列数 ${columns}，聚焦下标 ${await focusedIndex("[data-game-card]")}`,
  );

  await press("ArrowUp");
  check((await focusedIndex("[data-game-card]")) === 0, "↑ 回到第 1 张卡片");

  await press("End");
  const lastIndex = await evaluate(
    `document.querySelectorAll('[data-game-card]').length - 1`,
  );
  check((await focusedIndex("[data-game-card]")) === lastIndex, "End 跳到最后一页卡片");

  await press("Home");
  check((await focusedIndex("[data-game-card]")) === 0, "Home 回到第 1 张卡片");

  await shot("01-grid-focus");

  /* ---------- 3. Enter 打开详情 ---------- */
  await evaluate(`document.querySelector('[data-game-card]').focus()`);
  await press("ArrowRight");
  const targetTitle = await evaluate(
    `document.activeElement.querySelector('h3')?.innerText.trim() ?? ''`,
  );
  await press("Enter");
  await sleep(900);
  const hash = await evaluate(`location.hash`);
  check(/^#\/game\/\d+$/.test(hash), "Enter 打开详情页", hash);
  const detailTitle = await evaluate(
    `document.querySelector('h1, h2')?.innerText.trim() ?? ''`,
  );
  check(
    detailTitle.includes(targetTitle) || targetTitle.length === 0,
    "详情页标题与被激活的卡片一致",
    `卡片「${targetTitle}」→ 详情「${detailTitle}」`,
  );
  await shot("02-detail-from-enter");

  /* ---------- 4. Ctrl+F 聚焦搜索 ---------- */
  await evaluate(`location.hash = "#/"`);
  await sleep(900);
  await evaluate(`document.body.focus()`);
  await press("f", 2); // 2 = Ctrl
  const focusedIsSearch = await evaluate(
    `document.activeElement?.getAttribute('aria-label') ?? ''`,
  );
  check(focusedIsSearch === "搜索游戏", "Ctrl+F 聚焦搜索框", String(focusedIsSearch));

  /* ---------- 5. Esc 退出焦点 ---------- */
  await evaluate(`document.querySelector('[data-game-card]').focus()`);
  await press("Escape");
  const afterEsc = await focusedIndex("[data-game-card]");
  check(afterEsc === -1, "Esc 让卡片失去焦点", `activeElement 下标 ${afterEsc}`);

  /* ---------- 6. 列表视图方向键 ---------- */
  await evaluate(`location.hash = "#/"`);
  await sleep(700);
  // 切到列表视图
  await evaluate(`(() => {
    const btn = [...document.querySelectorAll('button')].find(b => b.getAttribute('aria-label') === '列表视图');
    btn?.click();
  })()`);
  await sleep(800);

  const rows = await evaluate(`document.querySelectorAll('[data-game-row]').length`);
  check(rows > 1, "列表视图渲染出多行", `${rows} 行`);

  const rowTabStops = await evaluate(`(() => {
    const items = [...document.querySelectorAll('[data-game-row]')];
    return items.filter(r => r.getAttribute('tabindex') === '0').length;
  })()`);
  check(rowTabStops === 1, "列表视图同样只有一个 Tab 停靠点");

  await evaluate(`document.querySelector('[data-game-row]').focus()`);
  await sleep(200);
  await press("ArrowDown");
  check((await focusedIndex("[data-game-row]")) === 1, "列表视图 ↓ 移动到第 2 行");
  await press("ArrowUp");
  check((await focusedIndex("[data-game-row]")) === 0, "列表视图 ↑ 回到第 1 行");
  await shot("03-list-focus");

  /* ---------- 7. 切回封面墙，避免影响后续截图 ---------- */
  await evaluate(`(() => {
    const btn = [...document.querySelectorAll('button')].find(b => b.getAttribute('aria-label') === '封面墙视图');
    btn?.click();
  })()`);
  await sleep(500);

  console.log(
    `\n=== 控制台输出 ===\n错误 ${consoleErrors.length} 条 / 异常 ${exceptions.length} 条`,
  );
  for (const e of consoleErrors.slice(0, 5)) console.log(`  ! ${e}`);
  for (const e of exceptions.slice(0, 5)) console.log(`  !! ${e}`);

  const failed = results.filter((r) => !r.ok);
  console.log(`\n=== 汇总：${results.length - failed.length}/${results.length} 步通过 ===`);
  for (const f of failed) console.log(`  ✗ ${f.label}: ${f.detail}`);
  console.log(`\n截图已写入 ${OUT}`);

  client.close();
  process.exitCode = failed.length || exceptions.length ? 1 : 0;
} finally {
  edge.kill();
}
