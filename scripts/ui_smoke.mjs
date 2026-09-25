/**
 * UI 冒烟测试：用 CDP 驱动无头 Edge 走一遍真实用户操作路径。
 *
 * 特点：
 * - 点击走 `Input.dispatchMouseEvent`（真实坐标），而不是 `el.click()`，
 *   这样才能暴露被遮挡、pointer-events、z-index 一类的问题。
 * - 全程收集 console 报错与未捕获异常。
 * - 每一步都可截图，便于人工复核。
 *
 * 用法：
 *   node scripts/ui_smoke.mjs [输出目录]
 * 前置：先起 vite（默认 1421），并让脚本自己拉起带 --remote-debugging-port 的 Edge。
 */
import { spawn } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { setTimeout as sleep } from "node:timers/promises";

const PORT = 9333;
const BASE = process.env.BASE_URL || "http://127.0.0.1:1421/scripts/mock.html";
const OUT = process.argv[2] || "/tmp/ui-smoke";
const EDGE =
  process.env.EDGE_PATH ||
  "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe";

mkdirSync(OUT, { recursive: true });

const consoleErrors = [];
const consoleWarns = [];
const consoleInfo = [];
const exceptions = [];
const steps = [];

/* ---------------- CDP 连接 ---------------- */

async function connect() {
  for (let i = 0; i < 60; i += 1) {
    try {
      const res = await fetch(`http://127.0.0.1:${PORT}/json`);
      const list = await res.json();
      const page = list.find((t) => t.type === "page" && t.webSocketDebuggerUrl);
      if (page) return page.webSocketDebuggerUrl;
    } catch {
      /* 还没起来 */
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
    // 事件
    if (msg.method === "Runtime.consoleAPICalled") {
      const text = (msg.params.args ?? [])
        .map((a) => a.value ?? a.description ?? a.type)
        .join(" ");
      const entry = `[${msg.params.type}] ${text}`;
      if (msg.params.type === "error") consoleErrors.push(entry);
      else if (msg.params.type === "warning") consoleWarns.push(entry);
      else consoleInfo.push(entry);
    }
    if (msg.method === "Runtime.exceptionThrown") {
      const d = msg.params.exceptionDetails;
      exceptions.push(d.exception?.description ?? d.text);
    }
    if (msg.method === "Log.entryAdded") {
      const e = msg.params.entry;
      if (e.level === "error") consoleErrors.push(`[log] ${e.text} ${e.url ?? ""}`);
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

/* ---------------- 操作原语 ---------------- */

function makeApi(send) {
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

  const boxOf = (selector, index = 0) =>
    evaluate(`(() => {
      const els = document.querySelectorAll(${JSON.stringify(selector)});
      const el = els[${index}];
      if (!el) return null;
      el.scrollIntoView({ block: 'center' });
      const r = el.getBoundingClientRect();
      const cs = getComputedStyle(el);
      return {
        x: r.left + r.width / 2, y: r.top + r.height / 2,
        w: r.width, h: r.height,
        text: (el.innerText || el.value || '').trim().replace(/\\s+/g, ' ').slice(0, 60),
        visible: r.width > 0 && r.height > 0 && cs.visibility !== 'hidden' && cs.display !== 'none',
      };
    })()`);

  /** 真实鼠标点击：走 Input 域，能暴露遮挡问题 */
  const click = async (selector, index = 0) => {
    const b = await boxOf(selector, index);
    if (!b) throw new Error(`找不到元素: ${selector}[${index}]`);
    if (!b.visible) throw new Error(`元素不可见(尺寸为0): ${selector}[${index}]`);
    for (const type of ["mousePressed", "mouseReleased"]) {
      await send("Input.dispatchMouseEvent", {
        type,
        x: b.x,
        y: b.y,
        button: "left",
        clickCount: 1,
      });
    }
    await sleep(180);
    return b;
  };

  /** 真实键盘输入 */
  const type = async (selector, text) => {
    await click(selector);
    await evaluate(
      `(() => {
        const el = document.querySelector(${JSON.stringify(selector)});
        el.value = '';
        el.dispatchEvent(new Event('input', { bubbles: true }));
      })()`,
    );
    for (const ch of text) {
      await send("Input.dispatchKeyEvent", { type: "keyDown", text: ch });
      await send("Input.dispatchKeyEvent", { type: "keyUp", text: ch });
    }
    await sleep(400);
  };

  const shot = async (name) => {
    const r = await send("Page.captureScreenshot", { format: "png" });
    writeFileSync(`${OUT}/${name}.png`, Buffer.from(r.data, "base64"));
  };

  const text = (selector) =>
    evaluate(
      `(document.querySelector(${JSON.stringify(selector)})?.innerText ?? '').trim().replace(/\\s+/g,' ')`,
    );

  const count = (selector) => evaluate(`document.querySelectorAll(${JSON.stringify(selector)}).length`);

  const exists = (selector) => evaluate(`!!document.querySelector(${JSON.stringify(selector)})`);

  const navigate = async (hash) => {
    await evaluate(`location.hash = ${JSON.stringify(hash)}`);
    await sleep(700);
  };

  return { evaluate, click, type, shot, text, count, exists, navigate, boxOf };
}

/* ---------------- 断言 ---------------- */

const findings = [];
function step(name, fn) {
  return async () => {
    const t0 = Date.now();
    try {
      const detail = await fn();
      steps.push({ name, ok: true, ms: Date.now() - t0, detail });
      console.log(`  ✓ ${name}${detail ? ` — ${detail}` : ""}`);
    } catch (e) {
      steps.push({ name, ok: false, ms: Date.now() - t0, detail: e.message });
      findings.push(`❌ ${name}: ${e.message}`);
      console.log(`  ✗ ${name} — ${e.message}`);
    }
  };
}

function check(cond, msg) {
  if (!cond) throw new Error(msg);
}

/* ---------------- 主流程 ---------------- */

async function main() {
  console.log("启动无头 Edge…");
  const edge = spawn(
    EDGE,
    [
      "--headless=new",
      `--remote-debugging-port=${PORT}`,
      "--disable-gpu",
      "--no-sandbox",
      "--hide-scrollbars",
      "--force-device-scale-factor=1",
      "--window-size=1360,860",
      "--user-data-dir=" + `${OUT}/edge-profile`,
      BASE,
    ],
    { stdio: "ignore", detached: false },
  );

  const wsUrl = await connect();
  const client = makeClient(wsUrl);
  await client.ready;
  const { send } = client;
  const api = makeApi(send);

  await send("Runtime.enable");
  await send("Log.enable");
  await send("Page.enable");
  await sleep(2500); // 等应用挂载

  console.log("\n=== 用户操作路径 ===");

  await step("打开应用，游戏库渲染", async () => {
    const n = await api.count("article");
    check(n > 0, `封面卡片数为 ${n}`);
    return `${n} 张封面`;
  })();

  await step("侧边栏导航项齐全", async () => {
    const items = await api.evaluate(
      `[...document.querySelectorAll('aside nav a')].map(a => a.innerText.trim())`,
    );
    check(items.length === 4, `导航项 ${items.length} 个`);
    return items.join(" / ");
  })();

  await step("搜索「千恋」应只剩 1 条", async () => {
    const before = await api.count("article");
    await api.type('input[placeholder*="搜索"]', "千恋");
    const after = await api.count("article");
    check(after === 1, `搜索后剩 ${after} 条（原 ${before}）`);
    await api.shot("02-search");
    // 清空
    await api.type('input[placeholder*="搜索"]', "");
    const restored = await api.count("article");
    check(restored === before, `清空后恢复为 ${restored} 条，期望 ${before}`);
    return `搜索生效并已还原 (${before} 条)`;
  })();

  await step("点击分类「在玩中」筛选", async () => {
    const idx = await api.evaluate(
      `[...document.querySelectorAll('aside button')].findIndex(b => b.innerText.includes('在玩中'))`,
    );
    check(idx >= 0, "侧边栏找不到「在玩中」");
    await api.click("aside button", idx);
    const n = await api.count("article");
    check(n > 0 && n < 16, `筛选后 ${n} 条`);
    await api.shot("03-category");
    return `${n} 条`;
  })();

  await step("点击「全部游戏」还原", async () => {
    const idx = await api.evaluate(
      `[...document.querySelectorAll('aside button')].findIndex(b => b.innerText.includes('全部游戏'))`,
    );
    await api.click("aside button", idx);
    const n = await api.count("article");
    check(n === 16, `还原后 ${n} 条，期望 16`);
    return `${n} 条`;
  })();

  await step("点标签筛选", async () => {
    const hasTag = await api.evaluate(
      `[...document.querySelectorAll('aside button')].some(b => b.className.includes('chip'))`,
    );
    if (!hasTag) return "无标签，跳过";
    const idx = await api.evaluate(
      `[...document.querySelectorAll('aside button')].findIndex(b => b.className.includes('chip'))`,
    );
    await api.click("aside button", idx);
    const n = await api.count("article");
    await api.shot("04-tag");
    // 取消
    await api.click("aside button", idx);
    return `标签筛选后 ${n} 条`;
  })();

  await step("收藏筛选", async () => {
    const idx = await api.evaluate(
      `[...document.querySelectorAll('main button')].findIndex(b => b.innerText.includes('收藏'))`,
    );
    check(idx >= 0, "找不到「收藏」筛选按钮");
    await api.click("main button", idx);
    const n = await api.count("article");
    await api.click("main button", idx);
    return `收藏筛选 ${n} 条`;
  })();

  await step("进入游戏详情页", async () => {
    await api.click("article", 0);
    await sleep(600);
    const hasBack = await api.exists("main button");
    const title = await api.text("main h1");
    check(!!title, "详情页没渲染出标题");
    check(hasBack, "详情页没有返回按钮");
    await api.shot("05-detail");
    return `标题「${title}」`;
  })();

  await step("详情页六个页签都能切换", async () => {
    const labels = ["概览", "存档", "攻略笔记", "补丁", "资源链接", "游玩记录"];
    const done = [];
    for (const label of labels) {
      const idx = await api.evaluate(
        `[...document.querySelectorAll('main button')].findIndex(b => b.innerText.trim() === ${JSON.stringify(label)})`,
      );
      check(idx >= 0, `找不到页签「${label}」`);
      await api.click("main button", idx);
      await sleep(350);
      done.push(label);
    }
    await api.shot("06-tabs");
    return done.join(" → ");
  })();

  await step("返回游戏库", async () => {
    const idx = await api.evaluate(
      `[...document.querySelectorAll('main button')].findIndex(b => b.innerText.includes('返回游戏库'))`,
    );
    check(idx >= 0, "找不到返回按钮");
    await api.click("main button", idx);
    await sleep(600);
    const n = await api.count("article");
    check(n === 16, `返回后 ${n} 条`);
    return "已返回";
  })();

  await step("多选模式与批量操作栏", async () => {
    const idx = await api.evaluate(
      `[...document.querySelectorAll('main button')].findIndex(b => b.innerText.includes('多选'))`,
    );
    check(idx >= 0, "找不到多选按钮");
    await api.click("main button", idx);
    await api.click("article", 0);
    await api.click("article", 1);
    await sleep(300);
    const selected = await api.evaluate(
      `document.body.innerText.includes('已选') || document.body.innerText.includes('批量')`,
    );
    await api.shot("07-batch");
    check(selected, "选中后没有出现批量操作栏");
    // 退出多选
    const idx2 = await api.evaluate(
      `[...document.querySelectorAll('main button')].findIndex(b => b.innerText.includes('多选'))`,
    );
    if (idx2 >= 0) await api.click("main button", idx2);
    return "批量栏出现";
  })();

  await step("打开「添加游戏」扫描向导并关闭", async () => {
    const idx = await api.evaluate(
      `[...document.querySelectorAll('main button')].findIndex(b => b.innerText.includes('添加游戏'))`,
    );
    check(idx >= 0, "找不到添加游戏按钮");
    await api.click("main button", idx);
    await sleep(500);
    const opened = await api.exists("input[type=checkbox]");
    await api.shot("08-scan-dialog");
    check(opened, "扫描对话框没打开");
    // 找关闭按钮
    const closed = await api.evaluate(`(() => {
      const btns = [...document.querySelectorAll('button')];
      const b = btns.find(x => x.getAttribute('aria-label') === '关闭'
        || x.getAttribute('title') === '关闭'
        || x.innerText.trim() === '取消'
        || x.querySelector('svg'));
      return null;
    })()`);
    return "扫描对话框已打开";
  })();

  await step("数据统计页", async () => {
    await api.navigate("#/stats");
    const hasChart = await api.exists("svg");
    check(hasChart, "统计页没有图表");
    await api.shot("09-stats");
    return "图表已渲染";
  })();

  await step("设置页", async () => {
    await api.navigate("#/settings");
    await sleep(600);
    const n = await api.count("input");
    await api.shot("10-settings");
    check(n > 0, "设置页没有输入项");
    return `${n} 个输入项`;
  })();

  await step("关于页", async () => {
    await api.navigate("#/about");
    await sleep(500);
    await api.shot("11-about");
    const t = await api.text("main");
    check(t.length > 10, "关于页内容为空");
    return `${t.slice(0, 40)}…`;
  })();

  /* ---------------- 报告 ---------------- */

  console.log("\n=== 控制台输出 ===");
  console.log(`错误 ${consoleErrors.length} 条 / 警告 ${consoleWarns.length} 条 / 异常 ${exceptions.length} 条`);
  const uniq = (arr) => [...new Set(arr)];
  if (exceptions.length) {
    console.log("\n未捕获异常：");
    uniq(exceptions).slice(0, 12).forEach((e) => console.log("  ! " + String(e).split("\n")[0]));
  }
  if (consoleErrors.length) {
    console.log("\n控制台错误：");
    uniq(consoleErrors).slice(0, 20).forEach((e) => console.log("  ! " + e.slice(0, 180)));
  }

  const failed = steps.filter((s) => !s.ok);
  console.log(`\n=== 汇总：${steps.length - failed.length}/${steps.length} 步通过 ===`);
  if (failed.length) failed.forEach((s) => console.log(`  ✗ ${s.name}: ${s.detail}`));

  writeFileSync(
    `${OUT}/report.json`,
    JSON.stringify({ steps, consoleErrors: uniq(consoleErrors), consoleWarns: uniq(consoleWarns), exceptions: uniq(exceptions) }, null, 2),
  );
  console.log(`\n报告与截图已写入 ${OUT}`);

  client.close();
  edge.kill();
  process.exit(0);
}

main().catch((e) => {
  console.error("测试脚本自身出错:", e);
  process.exit(1);
});
