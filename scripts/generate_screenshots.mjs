/**
 * 生成 README 配图。
 *
 * 用无头 Edge 打开 `scripts/mock.html`（注入了 Tauri IPC Mock 的前端），
 * 逐个路由截图到 `screenshots/`。沙箱里抓不到真实 WebView2 画面，这是唯一可靠的出图方式。
 *
 * 用法：
 *   1) 先起开发服务器：pnpm dev --port 1421
 *   2) node scripts/generate_screenshots.mjs
 */
import { spawn } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";

const PORT = 9344;
const ORIGIN = process.env.MOCK_ORIGIN || "http://127.0.0.1:1421";
const MOCK = `${ORIGIN}/scripts/mock.html`;
const OUT = resolve(process.env.SHOT_OUT || "screenshots");
const EDGE =
  process.env.EDGE_PATH ||
  "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe";

/** 1360x860 与 tauri.conf.json 里的默认窗口尺寸一致 */
const WIDTH = 1360;
const HEIGHT = 860;

mkdirSync(OUT, { recursive: true });
// 浏览器 profile 放临时目录，避免污染仓库
const PROFILE = join(tmpdir(), `galmanager-shots-${process.pid}`);
mkdirSync(PROFILE, { recursive: true });

async function connect() {
  for (let i = 0; i < 60; i += 1) {
    try {
      const list = await (await fetch(`http://127.0.0.1:${PORT}/json`)).json();
      const page = list.find((t) => t.type === "page" && t.webSocketDebuggerUrl);
      if (page) return page.webSocketDebuggerUrl;
    } catch {}
    await sleep(500);
  }
  throw new Error("CDP 连接超时");
}

/** 需要出图的页面：url 片段 + 输出文件名 + 可选的额外操作 */
const SHOTS = [
  { name: "library", url: "" },
  { name: "list", url: "", action: "switchToList" },
  { name: "detail", url: "#/game/1" },
  { name: "saves", url: "?tab=saves#/game/1" },
  { name: "notes", url: "?tab=notes#/game/1" },
  { name: "stats", url: "#/stats" },
  { name: "settings", url: "#/settings" },
];

async function main() {
  const edge = spawn(
    EDGE,
    [
      "--headless=new",
      `--remote-debugging-port=${PORT}`,
      "--disable-gpu",
      "--no-sandbox",
      "--hide-scrollbars",
      "--force-device-scale-factor=1",
      `--window-size=${WIDTH},${HEIGHT}`,
      `--user-data-dir=${PROFILE}`,
      MOCK,
    ],
    { stdio: "ignore" },
  );

  const ws = new WebSocket(await connect());
  await new Promise((resolve, reject) => {
    ws.addEventListener("open", resolve, { once: true });
    ws.addEventListener("error", reject, { once: true });
  });

  let seq = 0;
  const pending = new Map();
  ws.addEventListener("message", (ev) => {
    const m = JSON.parse(ev.data);
    if (m.id && pending.has(m.id)) {
      const { resolve, reject } = pending.get(m.id);
      pending.delete(m.id);
      m.error ? reject(new Error(JSON.stringify(m.error))) : resolve(m.result);
    }
  });
  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      const id = ++seq;
      pending.set(id, { resolve, reject });
      ws.send(JSON.stringify({ id, method, params }));
    });

  const evalJs = async (expression) => {
    const r = await send("Runtime.evaluate", {
      expression,
      returnByValue: true,
      awaitPromise: true,
    });
    if (r.exceptionDetails) {
      throw new Error(
        r.exceptionDetails.text +
          " " +
          (r.exceptionDetails.exception?.description || ""),
      );
    }
    return r.result.value;
  };

  await send("Page.enable");
  await send("Emulation.setDeviceMetricsOverride", {
    width: WIDTH,
    height: HEIGHT,
    deviceScaleFactor: 1,
    mobile: false,
  });

  const failures = [];

  for (const shot of SHOTS) {
    await send("Page.navigate", { url: MOCK + shot.url });
    // 等路由 + 演示数据 + 过渡动画都结束
    await sleep(2600);

    if (shot.action === "switchToList") {
      const ok = await evalJs(`(() => {
        const el = document.querySelector('[aria-label="列表视图"]');
        if (!el) return false;
        el.click();
        return true;
      })()`);
      if (!ok) failures.push(`${shot.name}: 找不到列表视图切换按钮`);
      await sleep(1200);
    }

    const r = await send("Page.captureScreenshot", { format: "png" });
    writeFileSync(`${OUT}/${shot.name}.png`, Buffer.from(r.data, "base64"));
    console.log(`  ✓ ${OUT}/${shot.name}.png`);
  }

  if (failures.length) {
    console.log("\n问题：");
    for (const f of failures) console.log(`  ⚠ ${f}`);
  } else {
    console.log(`\n共 ${SHOTS.length} 张配图已生成。`);
  }

  edge.kill();
  process.exit(failures.length ? 1 : 0);
}

main().catch((e) => {
  console.error("失败:", e.message);
  process.exit(1);
});
