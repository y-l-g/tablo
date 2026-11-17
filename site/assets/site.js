// The site's few behaviors. Every one is an enhancement: the page reads whole without it.

const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;

// ---- light and dark: the reader's pick, else the system's ----
{
  const root = document.documentElement;
  const toggle = document.querySelector("[data-scheme-toggle]");
  const systemDark = matchMedia("(prefers-color-scheme: dark)");
  const isDark = () => (root.dataset.scheme ? root.dataset.scheme === "dark" : systemDark.matches);
  const apply = () => {
    const dark = isDark();
    toggle?.setAttribute("aria-label", dark ? "Switch to light pages" : "Switch to dark pages");
    document.querySelector('meta[name="theme-color"]')?.setAttribute("content", dark ? "#0d120e" : "#eceee2");
    for (const img of document.querySelectorAll("img[data-dark-src]")) {
      img.dataset.lightSrc ??= img.getAttribute("src");
      img.setAttribute("src", dark ? img.dataset.darkSrc : img.dataset.lightSrc);
    }
  };
  apply();
  systemDark.addEventListener("change", apply);
  toggle?.addEventListener("click", () => {
    root.dataset.scheme = isDark() ? "light" : "dark";
    try { localStorage.setItem("ld-scheme", root.dataset.scheme); } catch {}
    apply();
  });
}

// ---- copy buttons ----
for (const button of document.querySelectorAll("[data-copy]")) {
  button.addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(button.dataset.copy);
      button.textContent = "Copied";
    } catch {
      button.textContent = "Select it";
    }
    setTimeout(() => (button.textContent = "Copy"), 1600);
  });
}

// ---- the hero's callouts: a code line, its region on the capture, a note ----
for (const figure of document.querySelectorAll("[data-pins]")) {
  const order = [...figure.querySelectorAll(".pin-dots [data-pin]")].map((b) => b.dataset.pin);
  const note = figure.querySelector("[data-pin-note]");
  let current = -1;
  let timer = null;
  let held = false;

  const show = (name) => {
    current = order.indexOf(name);
    for (const el of figure.querySelectorAll(".ld-code [data-pin], .pin")) el.classList.toggle("on", el.dataset.pin === name);
    for (const b of figure.querySelectorAll(".pin-dots button")) b.setAttribute("aria-pressed", String(b.dataset.pin === name));
    const template = figure.querySelector(`template[data-note="${name}"]`);
    if (template) note.replaceChildren(template.content.cloneNode(true));
  };
  const next = () => show(order[(current + 1) % order.length]);
  const play = () => {
    if (reduced || held || timer) return;
    timer = setInterval(next, 3600);
  };
  const stop = () => {
    clearInterval(timer);
    timer = null;
  };
  const hold = (name) => {
    held = true;
    stop();
    show(name);
  };

  for (const el of figure.querySelectorAll(".ld-code [data-pin]")) {
    el.addEventListener("mouseenter", () => hold(el.dataset.pin));
    el.addEventListener("click", () => hold(el.dataset.pin));
  }
  for (const b of figure.querySelectorAll(".pin-dots button")) b.addEventListener("click", () => hold(b.dataset.pin));

  // Cycle only while the figure is on screen, and start on the first callout.
  new IntersectionObserver(([entry]) => {
    if (entry.isIntersecting) {
      if (current < 0) show(order[0]);
      play();
    } else stop();
  }, { threshold: 0.35 }).observe(figure);
}

// ---- the theme switcher ----
for (const block of document.querySelectorAll("[data-themes]")) {
  const shot = block.querySelector("[data-theme-shot]");
  const name = block.querySelector("[data-theme-name]");
  const code = block.querySelector("[data-theme-code]");
  const state = { theme: "ledger", dark: false };
  const title = (s) => s[0].toUpperCase() + s.slice(1);
  const render = () => {
    const file = `assets/captures/theme-${state.theme}-${state.dark ? "dark" : "light"}.webp`;
    // Swap once the shot has decoded, so the caption never names a theme the picture lacks.
    const next = new Image();
    next.src = file;
    next.decode().catch(() => {}).then(() => { if (shot.dataset.want === file) shot.src = file; });
    shot.dataset.want = file;
    shot.alt = `The demo's panel home in the ${title(state.theme)} theme, ${state.dark ? "dark" : "light"}.`;
    name.textContent = `${title(state.theme)}, ${state.dark ? "dark" : "light"}`;
    code.textContent = title(state.theme);
    for (const b of block.querySelectorAll("[data-theme]")) b.setAttribute("aria-pressed", String(b.dataset.theme === state.theme));
    for (const b of block.querySelectorAll("[data-dark]")) b.setAttribute("aria-pressed", String((b.dataset.dark === "true") === state.dark));
  };
  for (const b of block.querySelectorAll("[data-theme]")) b.addEventListener("click", () => { state.theme = b.dataset.theme; render(); });
  for (const b of block.querySelectorAll("[data-dark]")) b.addEventListener("click", () => { state.dark = b.dataset.dark === "true"; render(); });
  // Fetch the other nine shots once the block is near, so a switch shows at once.
  new IntersectionObserver(([entry], observer) => {
    if (!entry.isIntersecting) return;
    observer.disconnect();
    for (const t of ["ledger", "jade", "amber", "graphite", "iris"]) for (const s of ["light", "dark"]) new Image().src = `assets/captures/theme-${t}-${s}.webp`;
  }, { rootMargin: "400px" }).observe(block);
}

// ---- bars grow when their table comes into view ----
for (const table of document.querySelectorAll("[data-speed]")) {
  if (reduced) { table.classList.add("in"); continue; }
  new IntersectionObserver(([entry], observer) => {
    if (!entry.isIntersecting) return;
    table.classList.add("in");
    observer.disconnect();
  }, { threshold: 0.3 }).observe(table);
}

// ---- the gallery's filters ----
for (const gallery of document.querySelectorAll("[data-gallery]")) {
  const entries = [...gallery.querySelectorAll("[data-tags]")];
  const buttons = [...document.querySelectorAll("[data-filter]")];
  const count = document.querySelector("[data-count]");
  const shown = () => {
    const n = entries.filter((e) => !e.hidden && !e.hasAttribute("data-add")).length;
    if (count) count.textContent = `${n} ${n === 1 ? "entry" : "entries"}`;
  };
  shown();
  for (const b of buttons) {
    b.addEventListener("click", () => {
      const tag = b.dataset.filter;
      for (const other of buttons) other.setAttribute("aria-pressed", String(other === b));
      for (const e of entries) e.hidden = tag !== "all" && !e.dataset.tags.split(" ").includes(tag);
      shown();
    });
  }
}
