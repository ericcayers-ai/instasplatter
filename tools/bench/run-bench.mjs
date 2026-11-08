#!/usr/bin/env node
/**
 * Renderer benchmark runner.
 *
 * Starts the Vite dev server on a private port, drives `tools/bench/splat-bench.html` in
 * headless Chromium through Playwright, prints the measurement JSON, and always shuts the server
 * down again (a leaked dev server holds a port and poisons every later run).
 *
 * Usage:
 *   node tools/bench/run-bench.mjs [--count 1000000] [--ms 4000] [--budget 4000000] [--json]
 *
 * The numbers are produced by the real renderer, not a model of it. They are SwiftShader numbers
 * on a headless adapter unless a GPU-capable launch is available, so use them for A/B comparisons
 * (culling on vs off, budget on vs off) rather than as absolute desktop FPS claims.
 */

import { spawn } from "node:child_process";
import { setTimeout as delay } from "node:timers/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const port = 5199;

function argValue(name, fallback) {
  const idx = process.argv.indexOf(name);
  if (idx >= 0 && process.argv[idx + 1]) return process.argv[idx + 1];
  const inline = process.argv.find((a) => a.startsWith(`${name}=`));
  if (inline) return inline.slice(name.length + 1);
  return fallback;
}

const count = argValue("--count", "1000000");
const ms = argValue("--ms", "4000");
const budget = argValue("--budget", "4000000");
const frames = argValue("--frames", "30");
const wide = process.argv.includes("--wide") ? "1" : "0";
const jsonOnly = process.argv.includes("--json");

async function waitForServer(url, timeoutMs = 60_000) {
  const start = Date.now();
  for (;;) {
    try {
      const res = await fetch(url, { method: "GET" });
      if (res.ok) return;
    } catch {
      // server not up yet
    }
    if (Date.now() - start > timeoutMs) throw new Error(`dev server did not start: ${url}`);
    await delay(250);
  }
}

const vite = spawn(
  process.platform === "win32" ? "npx.cmd" : "npx",
  ["vite", "--port", String(port), "--strictPort", "--host", "127.0.0.1"],
  { cwd: repoRoot, stdio: ["ignore", "pipe", "pipe"], shell: process.platform === "win32" },
);
let viteLog = "";
vite.stdout.on("data", (d) => {
  viteLog += d.toString();
});
vite.stderr.on("data", (d) => {
  viteLog += d.toString();
});

let browser;
try {
  await waitForServer(`http://127.0.0.1:${port}/`);
  const { chromium } = await import("playwright");
  browser = await chromium.launch({
    args: [
      "--use-angle=swiftshader",
      "--enable-unsafe-swiftshader",
      "--disable-gpu-sandbox",
      "--ignore-gpu-blocklist",
    ],
  });
  const page = await browser.newPage({ viewport: { width: 1600, height: 760 } });
  const consoleErrors = [];
  page.on("console", (m) => {
    if (m.type() === "error") consoleErrors.push(m.text());
  });
  page.on("pageerror", (e) => consoleErrors.push(String(e)));

  const url =
    `http://127.0.0.1:${port}/tools/bench/splat-bench.html` +
    `?count=${count}&ms=${ms}&budget=${budget}&frames=${frames}&wide=${wide}`;
  await page.goto(url, { waitUntil: "load" });

  await page.waitForFunction(
    () => "__BENCH_RESULT__" in window || "__BENCH_ERROR__" in window,
    undefined,
    { timeout: 600_000 },
  );
  const error = await page.evaluate(() => window.__BENCH_ERROR__ ?? null);
  if (error) throw new Error(`bench page failed: ${error}`);
  const result = await page.evaluate(() => window.__BENCH_RESULT__);

  if (jsonOnly) {
    console.log(JSON.stringify(result, null, 2));
  } else {
    const w = result.withCulling;
    const wo = result.withoutCulling;
    const b = result.budgeted;
    console.log("");
    console.log(`splat renderer bench — ${result.splatCount.toLocaleString()} splats, ${result.viewport.width}x${result.viewport.height}`);
    console.log(`chunks                 ${w.chunkCount}`);
    console.log(`culling on             ${w.avgFps.toFixed(1)} fps  avg ${w.avgFrameMs.toFixed(2)} ms  p95 ${w.p95FrameMs.toFixed(2)} ms  drawn ${w.drawnSplats.toLocaleString()} / ${w.candidateSplats.toLocaleString()} candidates  chunks ${w.visibleChunks}`);
    console.log(`culling off            ${wo.avgFps.toFixed(1)} fps  avg ${wo.avgFrameMs.toFixed(2)} ms  p95 ${wo.p95FrameMs.toFixed(2)} ms  drawn ${wo.drawnSplats.toLocaleString()}`);
    console.log(`culling on + budget    ${b.avgFps.toFixed(1)} fps  avg ${b.avgFrameMs.toFixed(2)} ms  stride ${b.budgetStride}  drawn ${b.drawnSplats.toLocaleString()}`);
    console.log(`sort-work reduction    ${(result.sortReduction * 100).toFixed(1)}%`);
    console.log(`frame-time reduction   ${(result.frameReduction * 100).toFixed(1)}%`);
    console.log("");
    console.log("json:");
    console.log(JSON.stringify(result, null, 2));
    if (consoleErrors.length > 0) {
      console.log("\nconsole errors:");
      for (const e of consoleErrors) console.log("  " + e);
    }
  }
} finally {
  if (browser) await browser.close().catch(() => {});
  if (!vite.killed) {
    // Windows needs taskkill for the npx -> vite process tree.
    if (process.platform === "win32") {
      spawn("taskkill", ["/pid", String(vite.pid), "/T", "/F"], { stdio: "ignore" });
    } else {
      vite.kill("SIGTERM");
    }
  }
  await delay(500);
}
