/**
 * 边界场景测试：最小窗口尺寸、空状态、长文本、滚动到底等。
 * 与 ui_smoke.mjs 共用同一套 CDP 手法，但聚焦「容易出问题的地方」。
 *
 * 用法：node scripts/ui_edge.mjs [输出目录]
 */
import { spawn } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { setTimeout as sleep } from "node:timers/promises";

const PORT = 9334;
const BASE = process.env.BASE_URL || "http://127.0.0.1:1421/scripts/mock.html";
const OUT = process.argv[2] || "/tmp/ui-edge";
const EDGE =
  process.env.EDGE_PATH ||
  "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe";

mkdirSync(OUT, { recursive: true });

const issues = [];
const notes = [];

async function connect() {
  for (let i = 0; i < 60; i += 1) {
    try {
      const res = await fetch(`http://127.0.0.1:${PORT}/json`);
      const list = await res.json();
      const page = list.find((t) => t.type === "page" && t.webSocketDebuggerUrl);
      if (page) return page.webSocketDebuggerUrl;
    } catch {}
    await sleep(500);
  }
  throw new Error("CDP 连接超时");
}

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
      "--window-size=1360,860",
      "--user-data-dir=" + `${OUT}/edge-profile`,
      BASE,
    ],
    { stdio: "ignore" },
  );

  const wsUrl = await connect();
  const ws = new WebSocket(wsUrl);
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
  await new Promise((r) => ws.addEventListener("open", r));
  await send("Runtime.enable");
  await send("Page.enable");

  const ev = async (expression) => {
    const r = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
    if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description ?? "eval 失败");
    return r.result.value;
  };
  const shot = async (name) => {
    const r = await send("Page.captureScreenshot", { format: "png" });
    writeFileSync(`${OUT}/${name}.png`, Buffer.from(r.data, "base64"));
  };
  const setViewport = async (w, h) => {
    await send("Emulation.setDeviceMetricsOverride", {
      width: w,
      height: h,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await sleep(600);
  };

  await sleep(2500);

  /* ---------- 1. 最小窗口尺寸下的工具栏 ---------- */
  await setViewport(1000, 640);
  await shot("01-min-1000x640");
  const overflow = await ev(`(() => {
    const doc = document.documentElement;
    const main = document.querySelector('main');
    const header = document.querySelector('main header');
    const toolbar = header?.querySelector('.flex.items-center.gap-2\\\\.5') ?? header?.firstElementChild;
    const rect = toolbar?.getBoundingClientRect();
    // 找出所有横向溢出的元素
    const bad = [];
    for (const el of document.querySelectorAll('main *')) {
      const r = el.getBoundingClientRect();
      if (r.width > 0 && (r.right > main.getBoundingClientRect().right + 1 || r.left < main.getBoundingClientRect().left - 1)) {
        bad.push((el.tagName + '.' + (el.className || '').toString().slice(0, 40)));
      }
    }
    return {
      docScrollW: doc.scrollWidth, docClientW: doc.clientWidth,
      horizOverflow: doc.scrollWidth > doc.clientWidth + 1,
      toolbarH: rect ? Math.round(rect.height) : null,
      overflowing: bad.slice(0, 6),
    };
  })()`);
  if (overflow.horizOverflow) issues.push(`最小窗口(1000px)下页面横向溢出：scrollWidth ${overflow.docScrollW} > clientWidth ${overflow.docClientW}`);
  if (overflow.overflowing.length) issues.push(`最小窗口下内容超出 main 边界：${overflow.overflowing.join(' | ')}`);
  notes.push(`1000x640 工具栏高度 ${overflow.toolbarH}px，横向溢出=${overflow.horizOverflow}`);

  /* ---------- 2. 极窄窗口（800px，低于 minWidth，看是否崩） ---------- */
  await setViewport(820, 640);
  await shot("02-narrow-820");
  const narrow = await ev(`(() => {
    const doc = document.documentElement;
    return { overflow: doc.scrollWidth > doc.clientWidth + 1, sw: doc.scrollWidth, cw: doc.clientWidth };
  })()`);
  if (narrow.overflow) notes.push(`820px 宽时横向溢出（sw ${narrow.sw} / cw ${narrow.cw}）—— 实际 minWidth 为 1000，可接受`);

  /* ---------- 3. 搜索无结果时的空状态 ---------- */
  await setViewport(1360, 860);
  await ev(`(() => {
    const el = document.querySelector('input[placeholder*="搜索"]');
    el.value = 'zzzz-不存在的游戏';
    el.dispatchEvent(new Event('input', { bubbles: true }));
  })()`);
  await sleep(700);
  await shot("03-empty-search");
  const empty = await ev(`(() => {
    const main = document.querySelector('main');
    const cards = document.querySelectorAll('article').length;
    const txt = (main.innerText || '').replace(/\\s+/g,' ').trim();
    return { cards, txt: txt.slice(0, 160) };
  })()`);
  if (empty.cards !== 0) issues.push(`搜索无结果时仍渲染了 ${empty.cards} 张卡片`);
  if (!/没有|无|空|找不到|试试/.test(empty.txt)) {
    issues.push(`搜索无结果时缺少空状态提示，实际文案：「${empty.txt.slice(0, 80)}」`);
  } else {
    notes.push(`空状态文案：「${empty.txt.slice(0, 70)}」`);
  }
  // 还原
  await ev(`(() => {
    const el = document.querySelector('input[placeholder*="搜索"]');
    el.value = ''; el.dispatchEvent(new Event('input', { bubbles: true }));
  })()`);
  await sleep(600);

  /* ---------- 4. 网格滚到底，最后一行是否可完整看到 ---------- */
  const scrollInfo = await ev(`(() => {
    const box = document.querySelector('main .scroll-y');
    if (!box) return { err: 'no scroll container' };
    box.scrollTop = box.scrollHeight;
    return { scrollH: box.scrollHeight, clientH: box.clientHeight, scrollTop: box.scrollTop,
             canScroll: box.scrollHeight > box.clientHeight };
  })()`);
  await sleep(500);
  await shot("04-scrolled-bottom");
  const lastCard = await ev(`(() => {
    const cards = [...document.querySelectorAll('article')];
    const last = cards[cards.length - 1];
    if (!last) return null;
    const box = document.querySelector('main .scroll-y').getBoundingClientRect();
    const r = last.getBoundingClientRect();
    return { bottom: Math.round(r.bottom), boxBottom: Math.round(box.bottom), visible: r.bottom <= box.bottom + 2 };
  })()`);
  if (lastCard && !lastCard.visible) {
    issues.push(`滚到底后最后一张卡片仍被裁切（卡片底 ${lastCard.bottom} > 容器底 ${lastCard.boxBottom}）`);
  } else if (lastCard) {
    notes.push("滚到底后最后一行可完整显示");
  }

  /* ---------- 5. 长标题截断 ---------- */
  const longTitle = await ev(`(() => {
    // 找标题最长的卡片，检查是否溢出容器
    let worst = null;
    for (const a of document.querySelectorAll('article')) {
      const h3 = a.querySelector('h3');
      if (!h3) continue;
      const over = h3.scrollWidth > h3.clientWidth + 1;
      if (!worst || (h3.textContent || '').length > worst.len) {
        worst = { len: (h3.textContent||'').length, over, text: (h3.textContent||'').slice(0,40),
                  clipped: over, h: Math.round(h3.getBoundingClientRect().height) };
      }
    }
    return worst;
  })()`);
  notes.push(`最长标题「${longTitle?.text}」(${longTitle?.len} 字)，单行裁切=${longTitle?.clipped}`);

  /* ---------- 6. 键盘可达性：Tab 焦点 ---------- */
  const focusables = await ev(`(() => {
    const sel = 'a[href], button:not([disabled]), input:not([disabled]), select, textarea, [tabindex]:not([tabindex="-1"])';
    const all = [...document.querySelectorAll(sel)].filter(el => {
      const r = el.getBoundingClientRect();
      return r.width > 0 && r.height > 0;
    });
    const noName = all.filter(el => {
      const name = (el.getAttribute('aria-label') || el.getAttribute('title') || el.innerText || el.value || '').trim();
      return !name;
    });
    return { total: all.length, noAccessibleName: noName.length,
             samples: noName.slice(0, 5).map(el => el.tagName + ':' + (el.className||'').toString().slice(0,50)) };
  })()`);
  if (focusables.noAccessibleName > 0) {
    issues.push(`有 ${focusables.noAccessibleName} 个可聚焦元素没有可读名称（无障碍/键盘用户看不到用途）：${focusables.samples.join(' | ')}`);
  }
  notes.push(`可聚焦元素 ${focusables.total} 个，其中无可读名称 ${focusables.noAccessibleName} 个`);

  /* ---------- 7. 无封面游戏的占位是否正常 ---------- */
  const covers = await ev(`(() => {
    const imgs = [...document.querySelectorAll('article img')];
    const broken = imgs.filter(i => i.complete && i.naturalWidth === 0);
    return { total: imgs.length, broken: broken.length };
  })()`);
  if (covers.broken > 0) issues.push(`${covers.broken}/${covers.total} 张封面加载失败且没有回退占位`);

  /* ---------- 8. 详情页在最小尺寸下的表现 ---------- */
  await ev(`location.hash = '#/game/1'`);
  await sleep(800);
  await setViewport(1000, 640);
  await shot("05-detail-min");
  const detailOverflow = await ev(`(() => {
    const main = document.querySelector('main');
    const mr = main.getBoundingClientRect();
    const bad = [];
    for (const el of document.querySelectorAll('main *')) {
      const r = el.getBoundingClientRect();
      if (r.width > 0 && r.right > mr.right + 1) bad.push(el.tagName + '.' + (el.className||'').toString().slice(0,40));
    }
    return { bad: bad.slice(0, 5), n: bad.length };
  })()`);
  if (detailOverflow.n > 0) issues.push(`详情页在 1000px 下有 ${detailOverflow.n} 个元素超出右边界：${detailOverflow.bad.join(' | ')}`);

  /* ---------- 9. 统计页在最小尺寸下 ---------- */
  await ev(`location.hash = '#/stats'`);
  await sleep(800);
  await shot("06-stats-min");
  const statsOverflow = await ev(`(() => {
    const main = document.querySelector('main');
    const mr = main.getBoundingClientRect();
    let n = 0; const bad = [];
    for (const el of document.querySelectorAll('main *')) {
      const r = el.getBoundingClientRect();
      if (r.width > 0 && r.right > mr.right + 1) { n++; if (bad.length < 5) bad.push(el.tagName + '.' + (el.className||'').toString().slice(0,40)); }
    }
    return { n, bad };
  })()`);
  if (statsOverflow.n > 0) issues.push(`统计页在 1000px 下有 ${statsOverflow.n} 个元素超出右边界：${statsOverflow.bad.join(' | ')}`);

  await setViewport(1360, 860);
  await ev(`location.hash = '#/'`);
  await sleep(600);

  /* ---------- 报告 ---------- */
  console.log("\n=== 边界测试发现 ===");
  if (!issues.length) console.log("  未发现明显问题");
  issues.forEach((i) => console.log("  ⚠ " + i));
  console.log("\n=== 观察记录 ===");
  notes.forEach((n) => console.log("  · " + n));

  writeFileSync(`${OUT}/report.json`, JSON.stringify({ issues, notes }, null, 2));
  console.log(`\n截图与报告：${OUT}`);

  ws.close();
  edge.kill();
  process.exit(0);
}

main().catch((e) => {
  console.error("脚本出错:", e);
  process.exit(1);
});
