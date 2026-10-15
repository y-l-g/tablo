// Unit tests for `theme.js`, on Node's built-in runner:
//
//     node --test crates/tablo-ui/assets/theme.test.js
//
// What the cases protect:
// * a choice shows the theme it names and stores it under Tablo's own cookie,
//   which the server reads for every page;
// * under `system`, the system's scheme is shown and mirrored into the cookie
//   the server reads instead, on the choice, on load and on every change;
// * the chosen control, and only it, is checked;
// * a click outside the controls changes nothing.

const test = require('node:test');
const assert = require('node:assert/strict');

const { listenerDocument } = require('./test-dom');
const { COOKIE, SCHEME, cookies, install, isDark, stored } = require('./theme');

// An `<html>` stand-in holding only a class list.
function root(dark) {
  const classes = new Set(dark ? ['dark'] : []);
  return {
    classList: {
      contains: (name) => classes.has(name),
      toggle(name, force) {
        if (force) classes.add(name);
        else classes.delete(name);
        return force;
      },
    },
  };
}

// A theme control for `theme`, recording its `aria-checked`.
function control(theme) {
  const attrs = {};
  return {
    dataset: { themeSet: theme },
    setAttribute: (name, value) => {
      attrs[name] = value;
    },
    checked: () => attrs['aria-checked'],
  };
}

// A `matchMedia` result whose `matches` the case sets, and whose `change` listeners it fires.
function media(prefersDark) {
  const listeners = [];
  return {
    matches: prefersDark,
    addEventListener: (_type, handler) => listeners.push(handler),
    fire: () => listeners.forEach((handler) => handler()),
  };
}

// Install the script against a stand-in document whose `<html>` starts `dark` and whose cookie
// string starts as `cookie`. Every cookie write lands in `writes` and in the readable string.
function installed({ dark = false, cookie = '', prefersDark = false } = {}) {
  const html = root(dark);
  const controls = ['light', 'dark', 'system'].map(control);
  const writes = [];
  let jar = cookie;
  const doc = listenerDocument({
    documentElement: html,
    head: { appendChild() {} },
    createElement: () => ({ appendChild() {}, remove() {} }),
    createTextNode: () => ({}),
    querySelectorAll: () => controls,
  });
  // An accessor, which `listenerDocument`'s spread would flatten to a value.
  Object.defineProperty(doc, 'cookie', {
    get: () => jar,
    set(value) {
      writes.push(value);
      const [pair] = value.split(';');
      const [name] = pair.split('=');
      const kept = jar.split('; ').filter((part) => part && !part.startsWith(`${name}=`));
      jar = [...kept, pair].join('; ');
    },
  });
  global.requestAnimationFrame = () => {};
  const scheme = media(prefersDark);
  install(doc, scheme);
  return { doc, html, controls, writes, scheme };
}

// A click on the control for `theme`, or outside every control.
function click(theme) {
  return {
    target: { closest: () => (theme ? { dataset: { themeSet: theme } } : null) },
  };
}

test.afterEach(() => {
  delete global.requestAnimationFrame;
});

test('each choice stores what it shows', () => {
  assert.deepEqual(
    cookies('dark', false).map((c) => c.split(';')[0]),
    [`${COOKIE}=dark`],
  );
  assert.deepEqual(
    cookies('system', true).map((c) => c.split(';')[0]),
    [`${COOKIE}=system`, `${SCHEME}=dark`],
  );
  assert.equal(isDark('light', true), false);
  assert.equal(isDark('dark', false), true);
  assert.equal(isDark('system', true), true);
  assert.equal(isDark('system', false), false);
});

test('only Tablo’s own cookie is read', () => {
  assert.equal(stored('xtablo_theme=dark; tablo_theme=system'), 'system');
  assert.equal(stored('theme=dark'), null);
  assert.equal(stored('tablo_theme=sepia'), null);
});

test('choosing dark shows it, stores it and checks its control', () => {
  const { doc, html, controls } = installed();
  const [handler] = doc.listeners('click');

  handler(click('dark'));

  assert.equal(html.classList.contains('dark'), true);
  assert.match(doc.cookie, /tablo_theme=dark/);
  assert.deepEqual(
    controls.map((c) => c.checked()),
    ['false', 'true', 'false'],
  );
});

test('choosing system shows and stores the system’s scheme', () => {
  const { doc, html } = installed({ dark: true, prefersDark: false });
  const [handler] = doc.listeners('click');

  handler(click('system'));

  assert.equal(html.classList.contains('dark'), false);
  assert.match(doc.cookie, /tablo_theme=system/);
  assert.match(doc.cookie, /tablo_scheme=light/);
});

test('under system, a load and a system switch follow the scheme', () => {
  const { doc, html, scheme } = installed({
    cookie: 'tablo_theme=system; tablo_scheme=light',
    prefersDark: true,
  });
  assert.equal(html.classList.contains('dark'), true, 'the load follows the system');
  assert.match(doc.cookie, /tablo_scheme=dark/);

  scheme.matches = false;
  scheme.fire();

  assert.equal(html.classList.contains('dark'), false, 'the switch follows the system');
  assert.match(doc.cookie, /tablo_scheme=light/);
});

test('an explicit choice ignores a system switch', () => {
  const { html, writes, scheme } = installed({ dark: true, cookie: 'tablo_theme=dark' });

  scheme.matches = false;
  scheme.fire();

  assert.equal(html.classList.contains('dark'), true);
  assert.deepEqual(writes, []);
});

test('a click elsewhere changes nothing', () => {
  const { doc, html, writes } = installed({ dark: true });
  const [handler] = doc.listeners('click');

  handler(click(null));

  assert.equal(html.classList.contains('dark'), true);
  assert.deepEqual(writes, []);
});
