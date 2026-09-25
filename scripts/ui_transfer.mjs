/**
 * 库导出 / 导入界面的验证脚本。
 *
 * 覆盖：设置页导出入口、归档概览弹窗、合并 / 替换两种模式的文案差异、
 * 导入完成后的列表刷新与提示。
 *
 * 用法：
 *   node scripts/ui_transfer.mjs [输出目录]
 * 前置：先起 vite（默认 1421），脚本会自己拉起带 --remote-debugging-port 的 Edge。
 */
import { spawn } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { setTimeout as sleep } from "node:timers/promises";

const PORT = 9441;
const BASE = process.env.BASE_URL || "http://127.0.0.1:1421/scripts/mock.html";
const OUT = process.argv[2] || "/tmp/ui-transfer";
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

  /** 按可见文本点按钮（走真实鼠标事件） */
  const clickText = async (label) => {
    const box = await evaluate(`(() => {
      const target = ${JSON.stringify(label)};
      const el = [...document.querySelectorAll('button')]
        .find(b => (b.innerText || '').trim().includes(target));
      if (!el) return null;
      el.scrollIntoView({ block: 'center' });
      const r = el.getBoundingClientRect();
      return { x: r.left + r.width / 2, y: r.top + r.height / 2, w: r.width, h: r.height };
    })()`);
    if (!box) throw new Error(`找不到按钮: ${label}`);
    if (box.w === 0 || box.h === 0) throw new Error(`按钮不可见: ${label}`);
    for (const type of ["mousePressed", "mouseReleased"]) {
      await send("Input.dispatchMouseEvent", {
        type,
        x: box.x,
        y: box.y,
        button: "left",
        clickCount: 1,
      });
    }
    await sleep(250);
  };

  const bodyText = () =>
    evaluate(`document.body.innerText.replace(/\\s+/g, ' ')`);

  console.log("启动无头 Edge…\n=== 库导入 / 导出 ===\n");

  /* ---------- 1. 设置页出现「数据备份与迁移」分区 ---------- */
  await evaluate(`location.hash = "#/settings"`);
  await sleep(900);
  const settingsText = await bodyText();
  check(settingsText.includes("数据备份与迁移"), "设置页出现「数据备份与迁移」分区");
  check(settingsText.includes("导出游戏库"), "出现「导出游戏库」按钮");
  check(settingsText.includes("从归档导入"), "出现「从归档导入」按钮");
  await shot("01-settings-transfer");

  /* ---------- 2. 打开导入对话框，校验摘要渲染 ---------- */
  await evaluate(`window.__galmanagerOpenImport('mock://galmanager-library.json')`);
  await sleep(600);
  const dialogText = await bodyText();
  check(dialogText.includes("从归档导入游戏库"), "导入对话框已打开");
  check(dialogText.includes("2026-09-25 21:40:12"), "显示归档导出时间");
  check(dialogText.includes("合并导入"), "提供「合并导入」选项");
  check(dialogText.includes("覆盖导入"), "提供「覆盖导入」选项");

  // 8 项统计是否都渲染出来（游戏 17 / 游玩记录 63 / 资源链接 14）
  const stats = await evaluate(`(() => {
    const nodes = [...document.querySelectorAll('.grid.grid-cols-4 > div')];
    return nodes.map(n => n.innerText.replace(/\\s+/g, ' ').trim());
  })()`);
  check(stats.length === 8, "归档内容统计渲染 8 项", `实际 ${stats.length}`);
  check(stats.some((s) => s.includes("17")), "统计显示游戏数 17");
  check(stats.some((s) => s.includes("63")), "统计显示游玩记录 63");
  check(dialogText.includes("covers/") && dialogText.includes("saves/"), "提示封面与存档归档需另行复制");
  await shot("02-import-dialog");

  /* ---------- 3. 合并导入 ---------- */
  await clickText("开始导入");
  await sleep(700);
  const afterMerge = await bodyText();
  check(afterMerge.includes("导入完成"), "合并导入后弹出成功提示", afterMerge.match(/导入完成[^·]{0,40}/)?.[0] ?? "");
  check(!afterMerge.includes("从归档导入游戏库"), "导入完成后对话框已关闭");
  await shot("03-after-merge-import");

  /* ---------- 4. 覆盖导入（含安全副本提示） ---------- */
  await evaluate(`window.__galmanagerOpenImport('mock://galmanager-library.json')`);
  await sleep(600);
  await clickText("覆盖导入");
  const replaceSelected = await evaluate(`(() => {
    const el = [...document.querySelectorAll('button')].find(b => b.innerText.includes('覆盖导入'));
    return el ? el.getAttribute('aria-pressed') : null;
  })()`);
  check(replaceSelected === "true", "点击后「覆盖导入」进入选中态 (aria-pressed=true)");

  const confirmLabel = await evaluate(`(() => {
    const el = [...document.querySelectorAll('button')].find(b => b.innerText.includes('清空并导入'));
    return el ? el.innerText.trim() : null;
  })()`);
  check(confirmLabel === "清空并导入", "主按钮文案切换为「清空并导入」", String(confirmLabel));
  await shot("04-replace-selected");

  await clickText("清空并导入");
  await sleep(700);
  const afterReplace = await bodyText();
  check(afterReplace.includes("导入完成"), "覆盖导入后弹出成功提示");
  check(afterReplace.includes("已备份"), "覆盖导入后提示数据库安全副本", afterReplace.match(/导入前的数据库已备份[^·]{0,60}/)?.[0] ?? "");
  await shot("05-after-replace-import");

  /* ---------- 5. 控制台 ---------- */
  console.log(`\n=== 控制台输出 ===\n错误 ${consoleErrors.length} 条 / 异常 ${exceptions.length} 条`);
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
