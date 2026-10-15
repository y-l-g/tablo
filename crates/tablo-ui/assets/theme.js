// The theme choice: light, dark, or the system's.
//
// The `tablo_theme` cookie is the one place the choice lives: the server reads
// it to render `<html class="dark">` on every page, including the pages runtime
// navigation fetches, which copy `<html>`'s attributes and run no script. A
// click sets the class in place and writes the cookie for the next request.
//
// The server cannot see the system's scheme, so under `system` this script
// mirrors it into `tablo_scheme` (`light` or `dark`): when the choice is made,
// on every full load, and whenever the system switches. Only the first page a
// visitor loads before choosing `system` can render the panel's default.
//
// The names are Tablo's own: a cookie is shared by every app on a host whatever
// its port, so a generic `theme` set by another app on `localhost` (as an
// HttpOnly cookie, which no script can overwrite) would decide this panel's
// theme.
//
// Hook: `[data-theme-set="light|dark|system"]` (click); the matching control
// gets `aria-checked="true"`, the others `"false"`.
const COOKIE = 'tablo_theme';
const SCHEME = 'tablo_scheme';
const THEMES = ['light', 'dark', 'system'];
const ATTRIBUTES = ';path=/;max-age=31536000;samesite=lax';

// The cookies that store `theme`, given whether the system prefers dark.
function cookies(theme, prefersDark) {
  const out = [`${COOKIE}=${theme}${ATTRIBUTES}`];
  if (theme === 'system') out.push(`${SCHEME}=${prefersDark ? 'dark' : 'light'}${ATTRIBUTES}`);
  return out;
}

// Whether `theme` shows dark, given whether the system prefers dark.
function isDark(theme, prefersDark) {
  return theme === 'dark' || (theme === 'system' && prefersDark);
}

// The stored choice in a `document.cookie` string, if any.
function stored(cookie) {
  const match = cookie.match(new RegExp(`(?:^|;\\s*)${COOKIE}=([^;]*)`));
  return match && THEMES.includes(match[1]) ? match[1] : null;
}

// Show `theme` on `doc` and store it: the `dark` class, the cookies, and which control is checked.
function choose(doc, theme, prefersDark) {
  doc.documentElement.classList.toggle('dark', isDark(theme, prefersDark));
  for (const cookie of cookies(theme, prefersDark)) doc.cookie = cookie;
  for (const control of doc.querySelectorAll('[data-theme-set]')) {
    control.setAttribute('aria-checked', String(control.dataset.themeSet === theme));
  }
}

// Run `change` with transitions frozen for the frame the colors change in, so every surface
// switches at once.
function frozen(doc, change) {
  const freeze = doc.createElement('style');
  freeze.appendChild(
    doc.createTextNode('*,*::before,*::after,*::backdrop{transition:none!important}'),
  );
  doc.head.appendChild(freeze);
  change();
  requestAnimationFrame(() => requestAnimationFrame(() => freeze.remove()));
}

// Document-level delegation: runtime navigation replaces the page without
// running this script again, so a listener bound to a control would be lost
// with the first navigation.
function install(doc, media) {
  doc.addEventListener('click', (e) => {
    const control = e.target.closest && e.target.closest('[data-theme-set]');
    if (!control || !THEMES.includes(control.dataset.themeSet)) return;
    frozen(doc, () => choose(doc, control.dataset.themeSet, media.matches));
  });
  // Under `system`, keep the page and the cookie the server reads on the system's scheme.
  const follow = () => {
    if (stored(doc.cookie) === 'system') choose(doc, 'system', media.matches);
  };
  follow();
  media.addEventListener('change', () => frozen(doc, follow));
}

if (typeof document !== 'undefined') {
  install(document, window.matchMedia('(prefers-color-scheme: dark)'));
}

// Exposed for the Node unit test (`theme.test.js`); the guard keeps the export
// out of the browser.
if (typeof module !== 'undefined' && module.exports) {
  module.exports = { COOKIE, SCHEME, choose, cookies, install, isDark, stored };
}
