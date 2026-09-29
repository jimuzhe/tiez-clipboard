/**
 * Drives the clipboard virtual list the way a fast macOS fling does.
 * Exits 1 when a fast fling swaps real cards for empty placeholder rows, or when
 * the shell still applies the WKWebView compositing hazards that blank the list.
 */
import { spawn } from "node:child_process";
import { webkit } from "playwright";

const PORT = 4179;
const URL = `http://127.0.0.1:${PORT}/?bench=scroll`;

const vite = spawn(
  "npx",
  ["vite", "--port", String(PORT), "--strictPort", "--host", "127.0.0.1"],
  { stdio: ["ignore", "pipe", "pipe"] },
);

let viteLog = "";
vite.stdout.on("data", (chunk) => {
  viteLog += chunk.toString();
});
vite.stderr.on("data", (chunk) => {
  viteLog += chunk.toString();
});

const waitForServer = async () => {
  const deadline = Date.now() + 20000;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(URL);
      if (response.ok || response.status === 404) return;
    } catch {
      // vite not up yet
    }
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  throw new Error(`vite did not start\n${viteLog}`);
};

const shutdown = () => {
  vite.kill("SIGTERM");
};

try {
  await waitForServer();
  const browser = await webkit.launch();
  const page = await browser.newPage({ viewport: { width: 420, height: 720 } });
  const pageErrors = [];
  page.on("pageerror", (error) => pageErrors.push(String(error)));
  await page.goto(URL, { waitUntil: "networkidle" });
  await page.waitForSelector("[data-scroll-bench='ready'] [data-test-clipboard-item]", { timeout: 15000 });
  await page.waitForTimeout(300);

  const report = await page.evaluate(async () => {
    const root = document.getElementById("root");
    const veil = document.getElementById("shell-backdrop");
    const shell = document.querySelector(".app-container");
    const scroller = document.querySelector("[data-testid='virtuoso-scroller']");
    const rootStyle = getComputedStyle(root);
    const veilStyle = veil ? getComputedStyle(veil) : null;
    const shellStyle = getComputedStyle(shell);
    const sampleCard = scroller.querySelector("[data-test-clipboard-item]");
    const cardHeight = sampleCard ? sampleCard.getBoundingClientRect().height : 0;

    const readBackdrop = (style) => style.getPropertyValue("backdrop-filter") || style.getPropertyValue("-webkit-backdrop-filter");
    const compositor = {
      rootBackdrop: readBackdrop(rootStyle),
      veilBackdrop: veilStyle ? readBackdrop(veilStyle) : "missing",
      shellClip: shellStyle.getPropertyValue("clip-path") || shellStyle.getPropertyValue("-webkit-clip-path"),
      overflowScrolling: scroller.style.getPropertyValue("-webkit-overflow-scrolling"),
      macScroller: scroller.getAttribute("data-mac-scroller"),
      styleAttr: scroller.getAttribute("style") || "",
      cardHeight,
    };

    const duration = 420;
    const distance = Math.max(6000, scroller.scrollHeight - scroller.clientHeight);
    const samples = [];
    let rafGapMax = 0;
    let last = performance.now();
    const t0 = last;

    await new Promise((resolve) => {
      const step = (now) => {
        const gap = now - last;
        if (gap > rafGapMax) rafGapMax = gap;
        last = now;
        const elapsed = now - t0;
        const progress = Math.min(1, elapsed / duration);
        const eased = 1 - Math.pow(1 - progress, 3);
        scroller.scrollTop = eased * distance;
        if (elapsed > 90 && elapsed < duration - 40) {
          samples.push({
            full: scroller.querySelectorAll("[data-test-clipboard-item]").length,
            fast: scroller.querySelectorAll("[data-fast-row]").length,
          });
        }
        if (progress < 1) requestAnimationFrame(step);
        else resolve();
      };
      requestAnimationFrame(step);
    });

    await new Promise((resolve) => setTimeout(resolve, 500));
    const settled = {
      full: scroller.querySelectorAll("[data-test-clipboard-item]").length,
      fast: scroller.querySelectorAll("[data-fast-row]").length,
      scrollTop: scroller.scrollTop,
    };

    const fullSamples = samples.filter((sample) => sample.full > 0 && sample.fast === 0).length;
    return { compositor, samples: samples.length, fullSamples, rafGapMax, settled, distance };
  });

  await browser.close();

  const failures = [];
  const backdropIsNone = (value) => !value || value === "none";
  if (!backdropIsNone(report.compositor.rootBackdrop)) {
    failures.push(`#root still has backdrop-filter (${report.compositor.rootBackdrop})`);
  }
  if (!backdropIsNone(report.compositor.veilBackdrop)) {
    failures.push(`#shell-backdrop still has backdrop-filter (${report.compositor.veilBackdrop})`);
  }
  if (report.compositor.shellClip && report.compositor.shellClip !== "none") {
    failures.push(`.app-container still has clip-path (${report.compositor.shellClip})`);
  }
  if (report.compositor.macScroller !== "auto") {
    failures.push("virtuoso scroller is still the default touch scroller");
  }
  if (report.compositor.styleAttr.includes("touch") || report.compositor.styleAttr.includes("clip")) {
    failures.push(`virtuoso scroller style still hazards the scroll tile (${report.compositor.styleAttr})`);
  }
  if (report.samples === 0 || report.fullSamples / report.samples < 0.8) {
    failures.push(
      `fast fling replaced cards with empty rows (${report.fullSamples}/${report.samples} samples kept real cards)`,
    );
  }
  if (report.settled.fast !== 0 || report.settled.full < 3) {
    failures.push(
      `cards missing after the fling (full=${report.settled.full}, fast=${report.settled.fast})`,
    );
  }
  if (report.rafGapMax > 120) {
    failures.push(`frame gap during fling was ${Math.round(report.rafGapMax)}ms`);
  }

  const summary = {
    compositor: report.compositor,
    flingSamples: report.samples,
    fullCardSamples: report.fullSamples,
    rafGapMax: Math.round(report.rafGapMax),
    settled: report.settled,
    distance: report.distance,
    pageErrors,
    failures,
  };
  console.log(JSON.stringify(summary, null, 2));
  if (failures.length > 0) {
    process.exitCode = 1;
  }
} catch (error) {
  console.error(error);
  process.exitCode = 1;
} finally {
  shutdown();
}
