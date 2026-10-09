// Unit tests for `repeaters.js`, on Node's built-in runner:
//
//     node --test crates/tablo-ui/assets/repeaters.test.js
//
// What the cases protect:
// * a copied row takes its number in every key and id, and the next copy
//   takes the next number;
// * the hidden order follows every add, remove and move, which is all the
//   server reads the rows' order from;
// * the single wiring pass: one click listener, so a click acts once.

const test = require('node:test');
const assert = require('node:assert/strict');

const { listenerDocument } = require('./test-dom');

// --- an element stand-in -----------------------------------------------------

// Only as wide as the script needs: attributes, children, the sibling and
// ancestor walks, and selectors made of a tag, `[attr]`, `[attr="value"]` and
// `:not([attr="value"])`, joined by commas.
class Element {
  constructor(tag, attributes = {}, children = []) {
    this.tag = tag;
    this.attrs = new Map(Object.entries(attributes));
    this.children = [];
    this.parent = null;
    this.value = attributes.value || '';
    this.focused = false;
    children.forEach((child) => this.appendChild(child));
  }

  get attributes() {
    return Array.from(this.attrs, ([name, value]) => ({ name, value }));
  }

  getAttribute(name) {
    return this.attrs.has(name) ? this.attrs.get(name) : null;
  }

  setAttribute(name, value) {
    this.attrs.set(name, String(value));
  }

  hasAttribute(name) {
    return this.attrs.has(name);
  }

  appendChild(child) {
    if (child instanceof Fragment) {
      child.children.splice(0).forEach((node) => this.appendChild(node));
      return;
    }
    child.remove();
    child.parent = this;
    this.children.push(child);
  }

  insertBefore(child, before) {
    child.remove();
    child.parent = this;
    this.children.splice(this.children.indexOf(before), 0, child);
  }

  remove() {
    if (!this.parent) return;
    this.parent.children.splice(this.parent.children.indexOf(this), 1);
    this.parent = null;
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
    return selector.split(',').some((one) => matchesOne(this, one.trim()));
  }

  closest(selector) {
    for (let node = this; node; node = node.parent) {
      if (node.matches(selector)) return node;
    }
    return null;
  }

  querySelector(selector) {
    for (const child of this.children) {
      if (child.matches(selector)) return child;
      const found = child.querySelector(selector);
      if (found) return found;
    }
    return null;
  }

  focus() {
    this.focused = true;
  }

  clone() {
    const copy = new Element(this.tag, Object.fromEntries(this.attrs));
    this.children.forEach((child) => copy.appendChild(child.clone()));
    return copy;
  }
}

class Fragment {
  constructor(children) {
    this.children = children;
  }

  get firstElementChild() {
    return this.children[0];
  }

  cloneNode() {
    return new Fragment(this.children.map((child) => child.clone()));
  }
}

function matchesOne(element, selector) {
  const not = /:not\(\[([\w-]+)="([^"]*)"\]\)$/.exec(selector);
  if (not) {
    if (element.getAttribute(not[1]) === not[2]) return false;
    selector = selector.slice(0, not.index);
  }
  const [, tag, attr, value] = /^(\w*)(?:\[([\w-]+)(?:="([^"]*)")?\])?$/.exec(selector);
  if (tag && element.tag !== tag) return false;
  if (attr && !element.hasAttribute(attr)) return false;
  if (value !== undefined && element.getAttribute(attr) !== value) return false;
  return true;
}

// A repeater of `links` with the rows `numbers`, each holding one text input.
function repeater(numbers) {
  const row = (n) =>
    new Element('li', { 'data-repeater-row': String(n) }, [
      new Element('input', { type: 'hidden', name: `links.${n}.id` }),
      new Element('input', { name: `links.${n}.url`, id: `links.${n}.url` }),
      new Element('button', { 'data-repeater-up': '' }),
      new Element('button', { 'data-repeater-down': '' }),
      new Element('button', { 'data-repeater-remove': '' }),
    ]);
  const blank = new Element('template', { 'data-repeater-blank': '' });
  blank.content = new Fragment([row('__row__')]);
  const list = new Element('ol', { 'data-repeater-rows': '' }, numbers.map(row));
  const order = new Element('input', {
    type: 'hidden',
    name: 'links',
    value: numbers.join(','),
    'data-repeater-order': '',
  });
  const add = new Element('button', { 'data-repeater-add': '' });
  const root = new Element(
    'fieldset',
    { 'data-repeater': 'links', 'data-repeater-next': String(Math.max(-1, ...numbers) + 1) },
    [order, list, blank, add],
  );
  return { root, order, list, add };
}

function button(row, kind) {
  return row.querySelector(`[data-repeater-${kind}]`);
}

// --- the cases -----------------------------------------------------------------

const { act, numberRow, rowOrder } = require('./repeaters.js');

test('a copied row takes its number in every key and id', () => {
  const row = new Element('li', { 'data-repeater-row': '__row__' }, [
    new Element('input', { name: 'links.__row__.url', id: 'f-links.__row__.url' }),
    new Element('label', { for: 'f-links.__row__.url' }),
  ]);
  numberRow(row, 7);
  assert.equal(row.getAttribute('data-repeater-row'), '7');
  assert.equal(row.children[0].getAttribute('name'), 'links.7.url');
  assert.equal(row.children[0].getAttribute('id'), 'f-links.7.url');
  assert.equal(row.children[1].getAttribute('for'), 'f-links.7.url');
});

test('the order lists the rows as they show', () => {
  const rows = [3, 0, 5].map((n) => new Element('li', { 'data-repeater-row': String(n) }));
  assert.equal(rowOrder(rows), '3,0,5');
  assert.equal(rowOrder([]), '');
});

test('adding appends the next number, focuses it, and orders it last', () => {
  const { root, order, list, add } = repeater([0, 1]);
  assert.equal(act(add), true);
  assert.equal(act(add), true);
  assert.deepEqual(
    list.children.map((row) => row.getAttribute('data-repeater-row')),
    ['0', '1', '2', '3'],
  );
  assert.equal(order.value, '0,1,2,3');
  assert.equal(root.getAttribute('data-repeater-next'), '4');
  const added = list.children[3];
  assert.equal(added.querySelector('input:not([type="hidden"])').getAttribute('name'), 'links.3.url');
  assert.equal(added.querySelector('input:not([type="hidden"])').focused, true);
});

test('removing a row drops it from the order and never reuses its number', () => {
  const { root, order, list, add } = repeater([0, 1, 2]);
  act(button(list.children[1], 'remove'));
  assert.equal(order.value, '0,2');
  assert.equal(add.focused, true);
  act(add);
  assert.equal(order.value, '0,2,3');
  assert.equal(root.getAttribute('data-repeater-next'), '4');
});

test('moving a row reorders it and keeps its number', () => {
  const { order, list } = repeater([0, 1, 2]);
  act(button(list.children[2], 'up'));
  assert.equal(order.value, '0,2,1');
  act(button(list.children[0], 'down'));
  assert.equal(order.value, '2,0,1');
  // The ends stay where they are.
  act(button(list.children[0], 'up'));
  act(button(list.children[2], 'down'));
  assert.equal(order.value, '2,0,1');
  assert.equal(list.children[0].querySelector('input:not([type="hidden"])').getAttribute('name'), 'links.2.url');
});

test('a button outside any repeater does nothing', () => {
  assert.equal(act(new Element('button', { 'data-repeater-add': '' })), false);
});

test('one click listener, so a click acts once', () => {
  const doc = listenerDocument();
  global.document = doc;
  delete require.cache[require.resolve('./repeaters.js')];
  require('./repeaters.js');
  delete global.document;
  assert.deepEqual(doc.types(), ['click']);
  assert.equal(doc.listeners('click').length, 1);
  const { order, add } = repeater([]);
  doc.listeners('click')[0]({ target: add });
  assert.equal(order.value, '0');
});
