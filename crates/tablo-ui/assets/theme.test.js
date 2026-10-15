// Unit tests for `theme.js`, on Node's built-in runner:
//
//     node --test crates/tablo-ui/assets/theme.test.js
//
// What the cases protect:
// * a toggle stores the theme it shows, both ways, under Tablo's own cookie,
//   which the server reads for every page;
// * a click outside the toggle changes nothing.

const test = require('node:test');
const assert = require('node:assert/strict');

const { listenerDocument } = require('./test-dom');

// An `<html>` stand-in holding only a class list.
function root(dark) {
  const classes = new Set(dark ? ['dark'] : []);
  return {
    classList: {
      contains: (name) => classes.has(name),
      toggle(name) {
        if (classes.delete(name)) return false;
        classes.add(name);
        return true;
      },
    },
  };
}

// A click event whose target is or is not inside the toggle.
function click(onToggle) {
  return { target: { closest: () => (onToggle ? {} : null) } };
}

// Install the script against a stand-in document whose `<html>` starts `dark`.
function installed(dark) {
  const html = root(dark);
  const doc = listenerDocument({
    cookie: '',
    documentElement: html,
    head: { appendChild() {} },
    createElement: () => ({ appendChild() {}, remove() {} }),
    createTextNode: () => ({}),
  });
  // The handler reads the globals when it fires, so they stay until the case ends.
  global.document = doc;
  global.requestAnimationFrame = () => {};
  require('./theme').install();
  return { doc, html };
}

test.afterEach(() => {
  delete global.document;
  delete global.requestAnimationFrame;
});

test('a toggle stores the theme it switches to', () => {
  const { toggle, COOKIE } = require('./theme');
  const html = root(false);

  assert.match(toggle(html), new RegExp(`^${COOKIE}=dark;path=/;`));
  assert.equal(html.classList.contains('dark'), true);

  assert.match(toggle(html), new RegExp(`^${COOKIE}=light;path=/;`));
  assert.equal(html.classList.contains('dark'), false);
});

test('a click on the toggle flips the page and writes the cookie', () => {
  const { doc, html } = installed(true);
  const [handler] = doc.listeners('click');

  handler(click(true));

  assert.equal(html.classList.contains('dark'), false);
  assert.match(doc.cookie, /^tablo_theme=light;/);
});

test('a click elsewhere changes nothing', () => {
  const { doc, html } = installed(true);
  const [handler] = doc.listeners('click');

  handler(click(false));

  assert.equal(html.classList.contains('dark'), true);
  assert.equal(doc.cookie, '');
});
