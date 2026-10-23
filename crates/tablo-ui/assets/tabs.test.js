// Unit tests for `tabs.js`, on Node's built-in runner:
//
//     node --test crates/tablo-ui/assets/tabs.test.js
//
// What the cases protect:
// * a click shows the clicked tab's panel alone, and marks that tab selected;
// * the arrow, `Home` and `End` keys move the selection, wrapping, past a tab
//   a condition hides;
// * a refused control in a hidden panel opens its tab, and an outer tab
//   holding it, before the browser focuses it;
// * of the controls one submit refuses, only the first opens its tab.

const test = require('node:test');
const assert = require('node:assert/strict');

const { listenerDocument } = require('./test-dom');
const { install } = require('./tabs');

// --- an element stand-in -----------------------------------------------------

// Only as wide as the script needs: attributes, the ancestor and descendant
// walks, `hidden`, and selectors of one `[attr]` or `[attr=value]`.
class Element {
  constructor(attributes = {}, children = []) {
    this.attrs = new Map(Object.entries(attributes));
    this.children = children;
    this.parentElement = null;
    this.focused = false;
    for (const child of children) child.parentElement = this;
  }

  getAttribute(name) {
    return this.attrs.has(name) ? this.attrs.get(name) : null;
  }

  setAttribute(name, value) {
    this.attrs.set(name, String(value));
  }

  get hidden() {
    return this.attrs.has('hidden');
  }

  set hidden(value) {
    if (value) this.attrs.set('hidden', '');
    else this.attrs.delete('hidden');
  }

  matches(selector) {
    const [, name, value] = selector.match(/^\[([\w-]+)(?:=(.+))?\]$/);
    return value === undefined ? this.attrs.has(name) : this.getAttribute(name) === value;
  }

  closest(selector) {
    for (let node = this; node; node = node.parentElement) {
      if (node.matches(selector)) return node;
    }
    return null;
  }

  querySelectorAll(selector) {
    const out = [];
    const walk = (node) => {
      for (const child of node.children) {
        if (child.matches(selector)) out.push(child);
        walk(child);
      }
    };
    walk(this);
    return out;
  }

  focus() {
    this.focused = true;
  }
}

const el = (attributes, ...children) => new Element(attributes, children);

// A block of tabs `ids`, the first active; `bodies[id]` fills that tab's panel.
function block(ids, bodies = {}) {
  const tabs = ids.map((id, at) =>
    el({
      role: 'tab',
      id: `${id}-tab`,
      'aria-controls': `${id}-tabpanel`,
      'aria-selected': String(at === 0),
    }),
  );
  const panels = ids.map((id, at) => {
    const attrs = {
      role: 'tabpanel',
      id: `${id}-tabpanel`,
      'aria-labelledby': `${id}-tab`,
      'data-tab-panel': '',
    };
    if (at !== 0) attrs.hidden = '';
    return el(attrs, ...(bodies[id] || []));
  });
  return el({ 'data-tabs': '' }, el({ role: 'tablist' }, ...tabs), ...panels);
}

// A document over `root`, with the script installed; `flush()` runs what it
// deferred to after the current submit.
function page(root) {
  const byId = new Map();
  const index = (node) => {
    const id = node.getAttribute('id');
    if (id) byId.set(id, node);
    node.children.forEach(index);
  };
  index(root);
  const doc = listenerDocument({ getElementById: (id) => byId.get(id) || null });
  const deferred = [];
  install(doc, (f) => deferred.push(f));
  return {
    get: (id) => byId.get(id),
    fire(type, event) {
      for (const handler of doc.listeners(type)) handler(event);
    },
    flush() {
      deferred.splice(0).forEach((f) => f());
    },
  };
}

// The ids of the tabs `p` shows the panels of, among `ids`.
const shown = (p, ids) => ids.filter((id) => !p.get(`${id}-tabpanel`).hidden);

test('a click shows the clicked tab alone', () => {
  const p = page(block(['a', 'b', 'c']));
  p.fire('click', { target: p.get('b-tab') });
  assert.deepEqual(shown(p, ['a', 'b', 'c']), ['b']);
  assert.equal(p.get('b-tab').getAttribute('aria-selected'), 'true');
  assert.equal(p.get('b-tab').getAttribute('tabindex'), '0');
  assert.equal(p.get('a-tab').getAttribute('aria-selected'), 'false');
  assert.equal(p.get('a-tab').getAttribute('tabindex'), '-1');
});

test('a click outside a tab changes nothing', () => {
  const p = page(block(['a', 'b']));
  p.fire('click', { target: p.get('b-tabpanel') });
  assert.deepEqual(shown(p, ['a', 'b']), ['a']);
});

test('the keys move the selection and wrap', () => {
  const p = page(block(['a', 'b', 'c']));
  const press = (id, key) => {
    let prevented = false;
    p.fire('keydown', { target: p.get(`${id}-tab`), key, preventDefault: () => (prevented = true) });
    return prevented;
  };
  assert.ok(press('a', 'ArrowLeft'));
  assert.deepEqual(shown(p, ['a', 'b', 'c']), ['c']);
  assert.ok(p.get('c-tab').focused);
  press('c', 'ArrowRight');
  assert.deepEqual(shown(p, ['a', 'b', 'c']), ['a']);
  press('a', 'End');
  assert.deepEqual(shown(p, ['a', 'b', 'c']), ['c']);
  press('c', 'Home');
  assert.deepEqual(shown(p, ['a', 'b', 'c']), ['a']);
  assert.ok(!press('a', 'Enter'));
  assert.deepEqual(shown(p, ['a', 'b', 'c']), ['a']);
});

test('the keys skip a tab a condition hides', () => {
  const root = block(['a', 'b', 'c']);
  const list = root.children[0];
  const b = list.children[1];
  const guard = el({ hidden: '' }, b);
  guard.parentElement = list;
  list.children[1] = guard;
  const p = page(root);
  p.fire('keydown', { target: p.get('a-tab'), key: 'ArrowRight', preventDefault() {} });
  assert.deepEqual(shown(p, ['a', 'b', 'c']), ['c']);
});

test('a refused control opens its tab and the tab around it', () => {
  const control = el({ name: 'password' });
  const inner = block(['inner-a', 'inner-b'], { 'inner-b': [control] });
  const p = page(block(['outer-a', 'outer-b'], { 'outer-b': [inner] }));
  p.fire('invalid', { target: control });
  assert.deepEqual(shown(p, ['outer-a', 'outer-b']), ['outer-b']);
  assert.deepEqual(shown(p, ['inner-a', 'inner-b']), ['inner-b']);
});

test('of one submit, only the first refused control opens its tab', () => {
  const first = el({ name: 'email' });
  const second = el({ name: 'password' });
  const p = page(block(['a', 'b', 'c'], { b: [first], c: [second] }));
  p.fire('invalid', { target: first });
  p.fire('invalid', { target: second });
  assert.deepEqual(shown(p, ['a', 'b', 'c']), ['b']);
  p.flush();
  p.fire('invalid', { target: second });
  assert.deepEqual(shown(p, ['a', 'b', 'c']), ['c']);
});
