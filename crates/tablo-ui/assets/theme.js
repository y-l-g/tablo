// The theme toggle.
//
// The `tablo_theme` cookie is the one place the choice lives: the server reads
// it to render `<html class="dark">` on every page, including the pages runtime
// navigation fetches, which copy `<html>`'s attributes and run no script. A
// click flips the class in place and writes the cookie for the next request.
//
// The name is Tablo's own: a cookie is shared by every app on a host whatever
// its port, so a generic `theme` set by another app on `localhost` (as an
// HttpOnly cookie, which no script can overwrite) would decide this panel's
// theme.
//
// Hook: `[data-theme-toggle]` (click).
const COOKIE = 'tablo_theme';

// Flip `root`'s `dark` class and return the cookie that stores the new theme.
function toggle(root) {
  const theme = root.classList.toggle('dark') ? 'dark' : 'light';
  return `${COOKIE}=${theme};path=/;max-age=31536000;samesite=lax`;
}

// Document-level delegation: runtime navigation replaces the page without
// running this script again, so a listener bound to the toggle itself would be
// lost with the first navigation. Transitions are frozen for the frame the
// colors change in, so every surface switches at once.
function install() {
  document.addEventListener('click', (e) => {
    if (!e.target.closest || !e.target.closest('[data-theme-toggle]')) return;
    const freeze = document.createElement('style');
    freeze.appendChild(
      document.createTextNode('*,*::before,*::after,*::backdrop{transition:none!important}'),
    );
    document.head.appendChild(freeze);
    document.cookie = toggle(document.documentElement);
    requestAnimationFrame(() => requestAnimationFrame(() => freeze.remove()));
  });
}

if (typeof document !== 'undefined') install();

// Exposed for the Node unit test (`theme.test.js`); the guard keeps the export
// out of the browser.
if (typeof module !== 'undefined' && module.exports) {
  module.exports = { COOKIE, install, toggle };
}
