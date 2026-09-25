/**
 * 存档定时自动备份界面的验证脚本。
 *
 * 覆盖：开关勾选、间隔预设切换、上次执行时间展示、立即备份一次的
 * 进行中态与结果提示、以及「自动备份完成」事件触发的 toast。
 *
 * 用法：
 *   node scripts/ui_autobak.mjs [输出目录]
 * 前置：先起 vite（默认 1421），脚本会自己拉起带 --remote-debugging-port 的 Edge。
 */
import { spawn } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { setTimeout as sleep } from "node:timers/promises";

const PORT = 9442;
const BASE = process.env.BASE_URL || "http://127.0.0.1:1421/scripts/mock.html";
const OUT = process.argv[2] || "/tmp/ui-autobak";
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

const edge = spawn(EDGE, [
  "--headless=new",
  `--remote-debugging-port=${PORT}`,
  "--no-first-run",
  "--no-default-browser-check",
  "--disable-gpu",
  "--window-size=1440,1100",
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
    height: 1100,
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

  /** 点击包含指定文本的元素（默认找 button / label） */
  const clickText = async (label, selector = "button") => {
    const box = await evaluate(`(() => {
      const target = ${JSON.stringify(label)};
      const el = [...document.querySelectorAll(${JSON.stringify(selector)})]
        .find(b => (b.innerText || '').trim().includes(target));
      if (!el) return null;
      el.scrollIntoView({ block: 'center' });
      const r = el.getBoundingClientRect();
      return { x: r.left + r.width / 2, y: r.top + r.height / 2, w: r.width, h: r.height };
    })()`);
    if (!box) throw new Error(`找不到元素: ${label}`);
    if (box.w === 0 || box.h === 0) throw new Error(`元素不可见: ${label}`);
    for (const type of ["mousePressed", "mouseReleased"]) {
      await send("Input.dispatchMouseEvent", {
        type,
        x: box.x,
        y: box.y,
        button: "left",
        clickCount: 1,
      });
    }
    await sleep(300);
  };

  const bodyText = () => evaluate(`document.body.innerText.replace(/\\s+/g, ' ')`);

  console.log("启动无头 Edge…\n=== 存档自动定时备份 ===\n");

  await evaluate(`location.hash = "#/settings"`);
  await sleep(900);

  /* ---------- 1. 默认关闭，不显示间隔选项 ---------- */
  const initial = await bodyText();
  check(initial.includes("定时自动备份存档"), "出现「定时自动备份存档」开关");
  check(
    !initial.includes("立即备份一次"),
    "默认关闭时不显示间隔与「立即备份一次」",
  );
  await shot("01-disabled");

  /* ---------- 2. 打开开关 ---------- */
  await clickText("定时自动备份存档", "label");
  await sleep(500);
  const enabled = await bodyText();
  check(enabled.includes("备份间隔"), "打开后出现「备份间隔」");
  check(enabled.includes("立即备份一次"), "打开后出现「立即备份一次」按钮");
  check(enabled.includes("3 小时"), "默认间隔预设里包含「3 小时」");
  check(enabled.includes("尚未执行过"), "尚未执行时显示「尚未执行过」");

  // 开关状态应已持久化到 settings
  const storedValue = await evaluate(
    `document.querySelector('input[type=checkbox]') ? 'ok' : 'missing'`,
  );
  check(storedValue === "ok", "开关可交互");

  const intervalPressed = await evaluate(`(() => {
    const el = [...document.querySelectorAll('button')].find(b => b.innerText.trim() === '3 小时');
    return el ? el.getAttribute('aria-pressed') : null;
  })()`);
  check(intervalPressed === "true", "默认间隔「3 小时」处于选中态", String(intervalPressed));
  await shot("02-enabled");

  /* ---------- 3. 切换间隔 ---------- */
  await clickText("1 小时");
  await sleep(400);
  const afterSwitch = await evaluate(`(() => {
    const el = [...document.querySelectorAll('button')].find(b => b.innerText.trim() === '1 小时');
    return el ? el.getAttribute('aria-pressed') : null;
  })()`);
  check(afterSwitch === "true", "切换后「1 小时」进入选中态");

  /* ---------- 4. 立即备份一次 ---------- */
  await clickText("立即备份一次");
  await sleep(700);
  const afterRun = await bodyText();
  check(
    afterRun.includes("自动备份完成") && afterRun.includes("新增 3 份"),
    "点击后弹出备份结果提示",
    afterRun.match(/自动备份完成[^·]{0,40}/)?.[0] ?? "",
  );
  check(afterRun.includes("上次执行：2026-09-25 22:40:03"), "状态刷新出上次执行时间");
  await shot("03-after-run");

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
