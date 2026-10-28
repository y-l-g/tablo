// Unit tests for `forms.js`, on Node's built-in runner:
//
//     node --test crates/tablo-ui/assets/forms.test.js
//
// What the cases protect:
// * `Ctrl+S` and `Cmd+S` submit the guarded form, the one holding the focus
//   first, and leave the browser's own shortcut alone where no form is guarded;
// * typing marks the form changed, and a changed form asks before a followed
//   link leaves, holding the click when the answer is no;
// * an unchanged form, a refused save the user accepted to leave, a link
//   opening elsewhere and a submit leave without asking;
// * a refused save the server rendered asks from the start;
// * the reload prompt shows only while a form is changed;
// * a refused control opens every folded section around it.

const test = require('node:test');
const assert = require('node:assert/strict');

const { listenerDocument } = require('./test-dom');
const { install } = require('./forms');

// --- an element stand-in -----------------------------------------------------

// Only as wide as the script needs: attributes, the ancestor and descendant
// walks, `open`, `requestSubmit`, and selectors of a tag and `[attr]`s.
class Element {
  constructor(tag, attributes = {}, children = []) {
    this.tag = tag;
    this.attrs = new Map(Object.entries(attributes));
    this.children = children;
    this.parentElement = null;
    this.submitted = 0;
    for (const child of children) child.parentElement = this;
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

  removeAttribute(name) {
    this.attrs.delete(name);
  }

  get open() {
    return this.attrs.has('open');
  }

  set open(value) {
    if (value) this.attrs.set('open', '');
    else this.attrs.delete('open');
  }

  matches(selector) {
    const [, tag, rest] = selector.match(/^(\w*)((?:\[[\w-]+\])*)$/);
    if (tag && tag !== this.tag) return false;
    const names = Array.from(rest.matchAll(/\[([\w-]+)\]/g), (m) => m[1]);
    return names.every((name) => this.attrs.has(name));
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

  requestSubmit() {
    this.submitted += 1;
  }
}

const el = (tag, attributes, ...children) => new Element(tag, attributes, children);

// A page over `root` with the script installed; `answer` is what `confirm`
// replies, and `asked` records each question.
function page(root, answer = true) {
  const doc = listenerDocument({
    querySelectorAll: (selector) => root.querySelectorAll(selector),
    querySelector: (selector) => root.querySelectorAll(selector)[0] || null,
  });
  const win = listenerDocument();
  const asked = [];
  install(doc, win, (question) => {
    asked.push(question);
    return answer;
  });
  const fire = (target, type, event = {}) => {
    let prevented = false;
    let stopped = false;
    const e = {
      target,
      button: 0,
      preventDefault: () => (prevented = true),
      stopPropagation: () => (stopped = true),
      ...event,
    };
    const listeners = type === 'beforeunload' ? win.listeners(type) : doc.listeners(type);
    for (const handler of listeners) handler(e);
    return { prevented, stopped, event: e };
  };
  return { asked, fire };
}

// A page holding one guarded form with a `title` input, a link out of the
// page, and a link inside the form.
function formPage(attrs = {}) {
  const input = el('input', { name: 'title' });
  const cancel = el('a', { href: '/admin/posts' });
  const form = el('form', { 'data-form-guard': 'Leave?', ...attrs }, input, cancel);
  const away = el('a', { href: '/admin/users' });
  const root = el('body', {}, el('nav', {}, away), form);
  return { root, form, input, cancel, away };
}

const save = (key = 's', mods = { ctrlKey: true }) => ({ key, ...mods });

test('Ctrl+S and Cmd+S submit the guarded form', () => {
  const { root, form, input } = formPage();
  const p = page(root);
  assert.ok(p.fire(input, 'keydown', save()).prevented);
  assert.ok(p.fire(root, 'keydown', save('s', { metaKey: true })).prevented);
  assert.equal(form.submitted, 2);
  assert.ok(!p.fire(input, 'keydown', save('s', { ctrlKey: true, shiftKey: true })).prevented);
  assert.ok(!p.fire(input, 'keydown', { key: 's' }).prevented);
  assert.equal(form.submitted, 2);
});

test('Ctrl+S saves the guarded form holding the focus', () => {
  const first = el('input', { name: 'a' });
  const one = el('form', { 'data-form-guard': '' }, first);
  const two = el('form', { 'data-form-guard': '' }, el('input', { name: 'b' }));
  const root = el('body', {}, one, two);
  const p = page(root);
  p.fire(first, 'keydown', save());
  assert.deepEqual([one.submitted, two.submitted], [1, 0]);
  // Two forms and the focus in neither: the browser keeps its shortcut.
  assert.ok(!p.fire(root, 'keydown', save()).prevented);
});

test('Ctrl+S leaves a page without a guarded form alone', () => {
  const search = el('form', {}, el('input', { name: 'search' }));
  const p = page(el('body', {}, search));
  assert.ok(!p.fire(search.children[0], 'keydown', save()).prevented);
  assert.equal(search.submitted, 0);
});

test('an unchanged form leaves without asking', () => {
  const { root, away } = formPage();
  const p = page(root, false);
  assert.ok(!p.fire(away, 'click').prevented);
  assert.deepEqual(p.asked, []);
});

test('a changed form asks before a link leaves, and holds the click on no', () => {
  const { root, input, cancel, away } = formPage();
  const p = page(root, false);
  p.fire(input, 'input');
  for (const link of [away, cancel]) {
    const click = p.fire(link, 'click');
    assert.ok(click.prevented && click.stopped);
  }
  assert.deepEqual(p.asked, ['Leave?', 'Leave?']);
});

test('leaving a changed form on yes asks no more', () => {
  const { root, input, away } = formPage();
  const p = page(root, true);
  p.fire(input, 'change');
  assert.ok(!p.fire(away, 'click').prevented);
  assert.ok(!p.fire(root, 'beforeunload').prevented);
  assert.equal(p.asked.length, 1);
});

test('a link opening elsewhere or a modified click does not ask', () => {
  const { root, input } = formPage();
  const blank = el('a', { href: '/admin/users', target: '_blank' });
  const hash = el('a', { href: '#top' });
  root.children.push(blank, hash);
  blank.parentElement = root;
  hash.parentElement = root;
  const p = page(root, false);
  p.fire(input, 'input');
  assert.ok(!p.fire(blank, 'click').prevented);
  assert.ok(!p.fire(hash, 'click').prevented);
  assert.ok(!p.fire(root.children[0].children[0], 'click', { metaKey: true }).prevented);
  assert.ok(!p.fire(root.children[0].children[0], 'click', { button: 1 }).prevented);
  assert.deepEqual(p.asked, []);
});

test('submitting the form clears the mark', () => {
  const { root, form, input, away } = formPage();
  const p = page(root, false);
  p.fire(input, 'input');
  p.fire(form, 'submit');
  assert.ok(!p.fire(root, 'beforeunload').prevented);
  assert.ok(!p.fire(away, 'click').prevented);
});

test('a refused save the server rendered asks from the start', () => {
  const { root, away } = formPage({ 'data-form-dirty': '' });
  const p = page(root, false);
  assert.ok(p.fire(away, 'click').prevented);
});

test('the reload prompt shows only while a form is changed', () => {
  const { root, input } = formPage();
  const p = page(root);
  assert.ok(!p.fire(root, 'beforeunload').prevented);
  p.fire(input, 'input');
  const unload = p.fire(root, 'beforeunload');
  assert.ok(unload.prevented);
  assert.equal(unload.event.returnValue, '');
});

test('typing outside a guarded form marks nothing', () => {
  const search = el('input', { name: 'search' });
  const { root, away } = formPage();
  root.children.push(el('form', {}, search));
  root.children[2].parentElement = root;
  const p = page(root, false);
  p.fire(search, 'input');
  assert.ok(!p.fire(away, 'click').prevented);
});

test('a refused control opens every folded section around it', () => {
  const control = el('input', { name: 'age' });
  const inner = el('details', { 'data-section': '' }, el('div', {}, control));
  const outer = el('details', { 'data-section': '' }, inner);
  const other = el('details', { 'data-section': '' });
  const p = page(el('form', { 'data-form-guard': '' }, outer, other));
  p.fire(control, 'invalid');
  assert.ok(inner.open && outer.open);
  assert.ok(!other.open);
});
