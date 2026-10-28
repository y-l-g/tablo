// Unit tests for `file_lists.js`, on Node's built-in runner:
//
//     node --test crates/tablo-ui/assets/file_lists.test.js
//
// What the cases protect:
// * a row's buttons move it one place, and the rows' order is all the server
//   reads the list's order from;
// * a move fires a bubbling `change`, so the leave-page guard counts it;
// * a button at either end, or outside a file list, moves nothing;
// * the single wiring pass: one click listener, so a click acts once.

const test = require('node:test');
const assert = require('node:assert/strict');

const { listenerDocument } = require('./test-dom');

// --- an element stand-in -----------------------------------------------------

// Only as wide as the script needs: attributes, children, the sibling and
// ancestor walks, and selectors made of `[attr]`, joined by commas.
class Element {
  constructor(attributes = {}, children = []) {
    this.attrs = new Map(Object.entries(attributes));
    this.children = [];
    this.parent = null;
    this.focused = false;
    this.events = [];
    children.forEach((child) => this.appendChild(child));
  }

  getAttribute(name) {
    return this.attrs.has(name) ? this.attrs.get(name) : null;
  }

  hasAttribute(name) {
    return this.attrs.has(name);
  }

  appendChild(child) {
    child.parent = this;
    this.children.push(child);
  }

  insertBefore(child, before) {
    this.children.splice(this.children.indexOf(child), 1);
    this.children.splice(this.children.indexOf(before), 0, child);
  }

  get previousElementSibling() {
    const siblings = this.parent.children;
    return siblings[siblings.indexOf(this) - 1] || null;
  }

  get nextElementSibling() {
    const siblings = this.parent.children;
    return siblings[siblings.indexOf(this) + 1] || null;
  }

  matches(selector) {
    return selector
      .split(',')
      .some((one) => this.hasAttribute(/^\[([\w-]+)\]$/.exec(one.trim())[1]));
  }

  closest(selector) {
    for (let node = this; node; node = node.parent) {
      if (node.matches(selector)) return node;
    }
    return null;
  }

  focus() {
    this.focused = true;
  }

  dispatchEvent(event) {
    this.events.push(event);
  }
}

// A file list holding a row per path, each with its up and down buttons.
function fileList(paths) {
  const rows = paths.map(
    (path) =>
      new Element({ 'data-file-item': path }, [
        new Element({ 'data-file-up': '' }),
        new Element({ 'data-file-down': '' }),
      ]),
  );
  return new Element({ 'data-file-list': 'photos' }, rows);
}

function order(list) {
  return list.children.map((row) => row.getAttribute('data-file-item'));
}

function button(list, path, kind) {
  const row = list.children.find((row) => row.getAttribute('data-file-item') === path);
  return row.children[kind === 'up' ? 0 : 1];
}

// --- the cases -----------------------------------------------------------------

const { act } = require('./file_lists.js');

test('a row moves one place up or down', () => {
  const list = fileList(['/a', '/b', '/c']);
  assert.equal(act(button(list, '/c', 'up')), true);
  assert.deepEqual(order(list), ['/a', '/c', '/b']);
  assert.equal(act(button(list, '/a', 'down')), true);
  assert.deepEqual(order(list), ['/c', '/a', '/b']);
  assert.equal(button(list, '/a', 'down').focused, true, 'the moved button keeps focus');
  assert.deepEqual(
    list.events.map((event) => [event.type, event.bubbles]),
    [
      ['change', true],
      ['change', true],
    ],
    'each move marks the form changed',
  );
});

test('a row at either end stays put', () => {
  const list = fileList(['/a', '/b']);
  act(button(list, '/a', 'up'));
  act(button(list, '/b', 'down'));
  assert.deepEqual(order(list), ['/a', '/b']);
  assert.deepEqual(list.events, [], 'nothing moved, nothing changed');
});

test('a button outside a file list does nothing', () => {
  const stray = new Element({ 'data-file-up': '' });
  new Element({}, [stray]);
  assert.equal(act(stray), false);
});

test('one click listener, so a click acts once', () => {
  const document = listenerDocument();
  global.document = document;
  delete require.cache[require.resolve('./file_lists.js')];
  require('./file_lists.js');
  delete global.document;
  assert.deepEqual(document.types(), ['click']);
  assert.equal(document.listeners('click').length, 1);

  const list = fileList(['/a', '/b', '/c']);
  document.listeners('click')[0]({ target: button(list, '/c', 'up') });
  assert.deepEqual(order(list), ['/a', '/c', '/b']);
});
