/**
 * 重复游戏检测的界面验证脚本。
 *
 * 覆盖：扫描候选里「已在库中」与「可能重复」两种标注、默认只勾选安全项、
 * 顶部疑似重复提示、导入按钮计数、导入前二次确认框的列出与取消。
 *
 * 用法：
 *   node scripts/ui_dup.mjs [输出目录]
 * 前置：先起 vite（默认 1421），脚本会自己拉起带 --remote-debugging-port 的 Edge。
 */
import { spawn } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { setTimeout as sleep } from "node:timers/promises";

const PORT = 9444;
const BASE = process.env.BASE_URL || "http://127.0.0.1:1421/scripts/mock.html";
const OUT = process.argv[2] || "/tmp/ui-dup";
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
  "--window-size=1440,1000",
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
    height: 1000,
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

  console.log("启动无头 Edge…\n=== 重复游戏检测 ===\n");

  await evaluate(`location.hash = "#/"`);
  await sleep(900);

  /* ---------- 1. 打开扫描对话框并扫描 ---------- */
  await clickText("添加游戏");
  await sleep(600);

  await evaluate(`(() => {
    const el = document.querySelector('input[aria-label="游戏根目录"]');
    el.value = 'D:\\\\Galgame';
    el.dispatchEvent(new Event('input', { bubbles: true }));
  })()`);
  await sleep(200);
  await clickText("开始扫描");
  await sleep(1200);

  const dialog = await bodyText();
  check(dialog.includes("共 4 个候选"), "扫描出 4 个候选", dialog.match(/共 \d+ 个候选[^ ]*/)?.[0] ?? "");
  check(dialog.includes("已在库中"), "路径重复项标注「已在库中」");
  check(
    dialog.includes("可能重复：库中已有《白色相簿2》"),
    "换目录的同名游戏标注「可能重复」",
  );

  /* ---------- 2. 默认勾选数应排除两类可疑项 ---------- */
  const counts = await evaluate(`(() => {
    const rows = [...document.querySelectorAll('input[type=checkbox]')];
    // 最后一组是候选行上的复选框（前面还有「全选」「识别引擎」等）
    const boxes = rows.filter(b => b.getAttribute('aria-label')?.startsWith('选择 '));
    return {
      total: boxes.length,
      checked: boxes.filter(b => b.checked).length,
      disabled: boxes.filter(b => b.disabled).length,
    };
  })()`);
  check(counts.total === 4, "4 行候选都有复选框", `实际 ${counts.total}`);
  check(counts.disabled === 1, "已在库中的项复选框被禁用", `禁用 ${counts.disabled}`);
  check(
    counts.checked === 2,
    "默认只勾选 2 个安全项（排除已在库中与疑似重复）",
    `已勾选 ${counts.checked}`,
  );

  check(dialog.includes("1 个疑似重复已默认不勾选"), "顶部提示疑似重复数量");
  check(dialog.includes("导入 2 个游戏"), "导入按钮显示 2 个");
  await shot("01-scan-duplicates");

  /* ---------- 3. 勾选疑似重复项 → 导入前应二次确认 ---------- */
  await evaluate(`(() => {
    const box = [...document.querySelectorAll('input[type=checkbox]')]
      .find(b => b.getAttribute('aria-label') === '选择 白色相簿2');
    box.click();
  })()`);
  await sleep(400);
  const afterCheck = await bodyText();
  check(afterCheck.includes("导入 3 个游戏"), "勾选后导入按钮变为 3 个");

  await clickText("导入 3 个游戏");
  await sleep(700);
  const confirm = await bodyText();
  check(confirm.includes("确认导入疑似重复的游戏"), "导入前弹出二次确认");
  check(
    confirm.includes("库中已有《白色相簿2》"),
    "确认框列出了冲突的游戏",
    confirm.match(/确认导入疑似重复的游戏[^·]{0,60}/)?.[0] ?? "",
  );
  await shot("02-confirm-duplicate");

  /* ---------- 4. 取消确认，不应真的导入 ---------- */
  await clickText("取消", "button");
  await sleep(600);
  const afterCancel = await bodyText();
  check(
    !afterCancel.includes("确认导入疑似重复的游戏"),
    "取消后确认框关闭，未执行导入",
  );

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
