// Unit tests for `search_shortcut.js`, on Node's built-in runner:
//
//     node --test crates/tablo-ui/assets/search_shortcut.test.js
//
// What the cases protect:
// * Cmd+K and Ctrl+K focus the search box and keep the browser from acting on the keys;
// * another key, or K with a further modifier, does nothing;
// * a page without a shown search box leaves the keys to the browser.

const test = require('node:test');
const assert = require('node:assert/strict');

const { listenerDocument } = require('./test-dom');
const { install } = require('./search_shortcut');

// A search input, shown unless `hidden`, recording whether it was focused and selected.
function searchInput(hidden = false) {
  return {
    focused: false,
    selected: false,
    getClientRects: () => (hidden ? [] : [{}]),
    focus() {
      this.focused = true;
    },
    select() {
      this.selected = true;
    },
  };
}

// A document holding `input` as its search box, with the shortcut installed.
function page(input) {
  const doc = listenerDocument({
    querySelector: (selector) =>
      selector === '[data-global-search] input[type="search"]' ? input : null,
  });
  install(doc);
  return doc;
}

// Fire a keydown of `key` with `modifiers` on `doc`; answers whether the default was prevented.
function press(doc, key, modifiers = {}) {
  let prevented = false;
  const event = {
    key,
    metaKey: false,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    ...modifiers,
    preventDefault: () => {
      prevented = true;
    },
  };
  for (const listener of doc.listeners('keydown')) listener(event);
  return prevented;
}

test('Cmd+K and Ctrl+K focus the search box', () => {
  for (const modifier of ['metaKey', 'ctrlKey']) {
    const input = searchInput();
    assert.equal(press(page(input), 'k', { [modifier]: true }), true);
    assert.equal(input.focused, true);
    assert.equal(input.selected, true);
  }
  const input = searchInput();
  press(page(input), 'K', { metaKey: true });
  assert.equal(input.focused, true, 'Caps Lock spells the key upper case');
});

test('another key or a further modifier does nothing', () => {
  const input = searchInput();
  const doc = page(input);
  assert.equal(press(doc, 'k'), false);
  assert.equal(press(doc, 'j', { metaKey: true }), false);
  assert.equal(press(doc, 'k', { metaKey: true, shiftKey: true }), false);
  assert.equal(press(doc, 'k', { ctrlKey: true, altKey: true }), false);
  assert.equal(input.focused, false);
});

test('a page without a shown search box leaves the keys to the browser', () => {
  assert.equal(press(page(null), 'k', { metaKey: true }), false);
  const hidden = searchInput(true);
  assert.equal(press(page(hidden), 'k', { metaKey: true }), false);
  assert.equal(hidden.focused, false);
});
