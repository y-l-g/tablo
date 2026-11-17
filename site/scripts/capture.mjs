// Captures the site's UI shots from a running tablo-demo, as WebP, and prints where the hero's
// callouts sit on the post list, as percentages of its shot.
//
//   cd "$(mktemp -d)" && npm i playwright-core
//   node /path/to/tablo/site/scripts/capture.mjs http://localhost:3000 [http://localhost:3200]
//
// The second address, a running tablo-saas, adds its home page to the shots.
//
// The demo must answer on `localhost`, not `127.0.0.1`: its cookies are `__Host-` cookies, which a
// browser keeps over plain HTTP only there. Chrome comes from CHROME, else /usr/bin/google-chrome.
// Run it against a fresh start: the demo reseeds on every start, and the audit shot edits records.

import { createRequire } from "node:module";
import { mkdir, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

// playwright-core resolves from the working directory, so the repository carries no node_modules.
const require = createRequire(join(process.cwd(), "noop.js"));
const { chromium } = require("playwright-core");

const base = (process.argv[2] ?? "http://localhost:3000").replace(/\/$/, "");
const out = join(dirname(fileURLToPath(import.meta.url)), "..", "assets", "captures");
const viewport = { width: 1600, height: 1000 };

/** The themes Tablo ships, with the sidebar frame each is drawn beside. The demo renders Ledger, so
 *  its pages declare Ledger's faces, and the others are worn over it in the page. */
const THEMES = {
  ledger: { attr: "ledger", variant: "sidebar" },
  jade: { attr: null, variant: "inset" },
  amber: { attr: "amber", variant: "sidebar" },
  graphite: { attr: "graphite", variant: "sidebar" },
  iris: { attr: "iris", variant: "floating" },
};

const browser = await chromium.launch({
  executablePath: process.env.CHROME ?? "/usr/bin/google-chrome",
  // Native date inputs and validation bubbles speak the browser's language, which Chrome takes
  // from the environment on Linux.
  args: ["--lang=en-US"],
  env: { ...process.env, LANG: "en_US.UTF-8", LANGUAGE: "en_US" },
});
const context = await browser.newContext({ viewport, deviceScaleFactor: 2, locale: "en-US", colorScheme: "light" });
await mkdir(out, { recursive: true });

/** Encodes a PNG as WebP at `width` pixels through a canvas, so no image tool is needed. */
async function webp(png, width, quality = 0.82) {
  const page = await context.newPage();
  const url = await page.evaluate(
    async ({ src, width, quality }) => {
      const img = new Image();
      img.src = src;
      await img.decode();
      const canvas = document.createElement("canvas");
      canvas.width = Math.min(width, img.width);
      canvas.height = Math.round((img.height * canvas.width) / img.width);
      const g = canvas.getContext("2d");
      g.imageSmoothingQuality = "high";
      g.drawImage(img, 0, 0, canvas.width, canvas.height);
      return canvas.toDataURL("image/webp", quality);
    },
    { src: `data:image/png;base64,${png.toString("base64")}`, width, quality },
  );
  await page.close();
  return Buffer.from(url.split(",")[1], "base64");
}

/** The box of `selector`'s first match, in page pixels. */
async function box(page, selector) {
  const found = await page.locator(selector).first().boundingBox();
  if (!found) throw new Error(`no box for ${selector}`);
  return found;
}

/** Screenshots the page, or the `clip` box of it, to `name.webp`, `width` pixels wide. */
async function shoot(page, name, { width = 1600, clip } = {}) {
  // Off every hover state, then let fonts, charts and transitions settle.
  await page.mouse.move(viewport.width - 2, viewport.height - 2);
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(450);
  const png = await page.screenshot({ type: "png", clip });
  const file = join(out, `${name}.webp`);
  await writeFile(file, await webp(png, width));
  console.log("wrote", file);
}

/** Where `selector`'s first match sits inside `frame`, as percentages of the frame. */
async function within(page, frame, selector) {
  const b = await box(page, selector);
  const pct = (v, of) => +((v / of) * 100).toFixed(2);
  return { x: pct(b.x - frame.x, frame.width), y: pct(b.y - frame.y, frame.height), w: pct(b.width, frame.width), h: pct(b.height, frame.height) };
}

/** The page beside the sidebar, `height` pixels of it from the top of the page. */
async function content(page, height = viewport.height) {
  const main = await box(page, "main");
  return { x: main.x, y: main.y, width: main.width, height: Math.min(height, main.height) };
}

/** Wears `theme` the way the server renders it: its token attribute and its sidebar frame. */
async function wear(page, theme, dark) {
  await page.evaluate(
    ({ attr, variant, dark }) => {
      const html = document.documentElement;
      if (attr) html.dataset.tabloTheme = attr;
      else delete html.dataset.tabloTheme;
      html.classList.toggle("dark", dark);
      const aside = document.querySelector('aside[data-sidebar="sidebar"]');
      aside.dataset.variant = variant;
      // The two classes the sidebar component adds per variant; the rest key on `data-variant`.
      aside.classList.remove("md:py-2", "md:p-2");
      if (variant === "inset") aside.classList.add("md:py-2");
      if (variant === "floating") aside.classList.add("md:p-2");
    },
    { ...THEMES[theme], dark },
  );
}

/** Posts the welcome page's one-click sign-in for `panel`, and waits for the panel home. */
async function signIn(page, panel) {
  await page.goto(`${base}/`);
  await page.locator(`form[action="/${panel}/login"] button[type="submit"]`).first().click();
  await page.waitForURL(new RegExp(`/${panel}/?$`));
}

// ---- the front pages, as the home page's "Built with" row shows them ----
{
  const page = await context.newPage();
  await page.goto(`${base}/`);
  await shoot(page, "demo-home");
  if (process.argv[3]) {
    await page.goto(`${process.argv[3].replace(/\/$/, "")}/`);
    // A development run names itself in a ribbon, and a rule above it, that production never shows.
    await page.evaluate(() => {
      const ribbon = document.querySelector("[data-environment]");
      if (ribbon?.previousElementSibling?.getAttribute("aria-hidden") === "true") ribbon.previousElementSibling.remove();
      ribbon?.remove();
    });
    await shoot(page, "saas-home");
  }
  await page.close();
}

// ---- the customer panel: two-factor sign-in ----
{
  const page = await context.newPage();
  await signIn(page, "app");
  await page.goto(`${base}/app/settings/two-factor`);
  // The page asks for the account's password first: every demo account's is `password`.
  await page.locator('main input[type="password"]').first().fill("password");
  await Promise.all([page.waitForLoadState("networkidle"), page.locator("main button").filter({ hasText: /authenticator/i }).first().click()]);
  await page.locator("main svg, main img").last().waitFor();
  // From the heading to the buttons, without the empty page to their right.
  // The page title: the appearance drawer, a banner ahead of it, holds a hidden heading too.
  const head = await box(page, "main h1");
  const end = await page.locator("main button").last().boundingBox();
  const top = head.y - 48;
  await shoot(page, "two-factor", { width: 1100, clip: { x: head.x - 56, y: top, width: 880, height: end.y + end.height + 56 - top } });
  await page.close();
}

// ---- signed in as the administrator ----
const page = await context.newPage();
await signIn(page, "admin");

// The hero: the post list, and where each callout sits on it.
await page.goto(`${base}/admin/posts`);
await page.locator('th[data-column="title"]').waitFor();
const hero = await content(page, 900);
const callouts = {
  create: await within(page, hero, 'main a[href$="/-/create"]'),
  widget: await within(page, hero, 'main [data-widget="post-status"]'),
  search: await within(page, hero, 'main form:has(button:has-text("Filters")) input[name="q"]'),
  filters: await within(page, hero, 'main button:has-text("Filters")'),
  sort: await within(page, hero, 'th[data-column="title"]'),
  author: await within(page, hero, "tbody tr:first-child td:nth-child(4)"),
  action: await within(page, hero, 'tbody tr:first-child button[aria-label="Actions"]'),
};
await shoot(page, "posts", { width: 1800, clip: hero });

// The post form, then the same form refusing an empty title. The browser's own `required` check
// is lifted so the server's answer shows.
const edit = await page.locator('tbody tr:first-child a[href$="/edit"]').first().getAttribute("href");
await page.goto(`${base}${edit}`);
const title = page.locator('input[name="title"]');
await title.waitFor();
const original = await title.inputValue();
await title.fill("");
await page.evaluate(() => document.querySelectorAll("form").forEach((f) => (f.noValidate = true)));
await Promise.all([page.waitForLoadState("networkidle"), page.locator('main button[type="submit"]').last().click()]);
await page.locator('main [aria-invalid="true"], main [data-error]').first().waitFor();
await shoot(page, "post-form", { width: 1800, clip: await content(page, 900) });

// A few writes for the audit log: the title back with a change, a product's price, a publish.
await title.fill(`${original}, revised`);
await Promise.all([page.waitForNavigation(), page.locator('main button[type="submit"]').last().click()]);
await page.goto(`${base}/admin/products`);
const product = await page.locator('tbody tr:first-child a[href$="/edit"]').first().getAttribute("href");
await page.goto(`${base}${product}`);
const price = page.locator('input[name="price"]');
if (await price.count()) {
  await price.fill(String((Number(await price.inputValue()) + 10).toFixed(2)));
  await Promise.all([page.waitForNavigation(), page.locator('main button[type="submit"]').last().click()]);
}
// The publish action, from the post list's row menu.
await page.goto(`${base}/admin/posts`);
await page.locator('tbody tr:nth-child(2) button[aria-label="Actions"]').click();
await Promise.all([page.waitForLoadState("networkidle"), page.locator("tbody tr:nth-child(2) button").filter({ hasText: /publish/i }).first().click()]);
await page.goto(`${base}/admin/audit-log`);
await page.locator("tbody tr").first().waitFor();
await shoot(page, "audit-log", { width: 1800, clip: await content(page, 640) });

// The themes, on the panel home.
await page.goto(`${base}/admin`);
for (const theme of Object.keys(THEMES)) {
  for (const dark of [false, true]) {
    await wear(page, theme, dark);
    await shoot(page, `theme-${theme}-${dark ? "dark" : "light"}`, { width: 1600 });
  }
}

console.log(JSON.stringify(callouts, null, 2));
await browser.close();
