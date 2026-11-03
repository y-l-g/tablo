// Unit tests for `wizard.js`, on Node's built-in runner:
//
//     node --test crates/tablo-ui/assets/wizard.test.js
//
// What the cases protect:
// * next opens the following step once the current one passes, and stays put,
//   reporting the refused control, when it does not;
// * back and an earlier trigger open their step without checking;
// * a later trigger checks each step before it and stops on the first refusal;
// * the back and next buttons hide where they lead nowhere, and skip a step a
//   condition hides;
// * Enter in a field moves forward until the last step, where it submits;
// * of the controls one submit refuses, only the first opens its step.

const test = require('node:test');
const assert = require('node:assert/strict');

const { listenerDocument } = require('./test-dom');
const { install } = require('./wizard');

// --- an element stand-in -----------------------------------------------------

// Only as wide as the script needs: attributes, the ancestor and descendant
// walks, `hidden`, a control's validity, and selectors listing tag names,
// `[attr]` or `[attr=value]`.
class Element {
  constructor(tag, attributes = {}, children = []) {
    this.tagName = tag.toUpperCase();
    this.attrs = new Map(Object.entries(attributes));
    this.children = children;
    this.parentElement = null;
    this.valid = true;
    this.reported = 0;
    for (const child of children) child.parentElement = this;
  }

  getAttribute(name) {
    return this.attrs.has(name) ? this.attrs.get(name) : null;
  }

  setAttribute(name, value) {
    this.attrs.set(name, String(value));
  }

  removeAttribute(name) {
    this.attrs.delete(name);
  }

  hasAttribute(name) {
    return this.attrs.has(name);
  }

  get hidden() {
    return this.attrs.has('hidden');
  }

  set hidden(value) {
    if (value) this.attrs.set('hidden', '');
    else this.attrs.delete('hidden');
  }

  get willValidate() {
    return ['INPUT', 'SELECT', 'TEXTAREA'].includes(this.tagName);
  }

  get validity() {
    return { valid: this.valid };
  }

  reportValidity() {
    this.reported += 1;
    return this.valid;
  }

  matches(selector) {
    return selector.split(',').some((one) => {
      const attr = one.trim().match(/^\[([\w-]+)(?:=(.+))?\]$/);
      if (!attr) return this.tagName === one.trim().toUpperCase();
      const [, name, value] = attr;
      return value === undefined ? this.attrs.has(name) : this.getAttribute(name) === value;
    });
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
}

const el = (tag, attributes, ...children) => new Element(tag, attributes, children);
const input = (name) => el('input', { name });

// A wizard of steps `ids`, the first current; `bodies[id]` fills that step's
// panel, and a step in `hidden` sits under a hidden condition guard.
function wizard(ids, bodies = {}, hidden = []) {
  const triggers = ids.map((id, at) => {
    const attrs = { id: `${id}-step`, 'aria-controls': `${id}-steppanel`, 'data-wizard-trigger': '' };
    if (at === 0) attrs['aria-current'] = 'step';
    const trigger = el('button', attrs);
    return hidden.includes(id) ? el('fieldset', { hidden: '' }, trigger) : trigger;
  });
  const panels = ids.map((id, at) => {
    const attrs = { id: `${id}-steppanel`, 'aria-labelledby': `${id}-step`, 'data-wizard-step': '' };
    if (at !== 0) attrs.hidden = '';
    return el('div', attrs, ...(bodies[id] || []));
  });
  return el(
    'div',
    { 'data-wizard': '' },
    el('div', {}, ...triggers),
    ...panels,
    el('button', { 'data-wizard-back': '', hidden: '' }),
    el('button', { 'data-wizard-next': '' }),
  );
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
  const find = (attr) => root.querySelectorAll(`[${attr}]`)[0];
  return {
    get: (id) => byId.get(id),
    back: find('data-wizard-back'),
    next: find('data-wizard-next'),
    fire(type, event) {
      for (const handler of doc.listeners(type)) handler(event);
    },
    click(target) {
      this.fire('click', { target });
    },
    enter(target) {
      let prevented = false;
      this.fire('keydown', { key: 'Enter', target, preventDefault: () => (prevented = true) });
      return prevented;
    },
    flush() {
      deferred.splice(0).forEach((f) => f());
    },
  };
}

// The ids of the steps `p` shows the panels of, among `ids`.
const shown = (p, ids) => ids.filter((id) => !p.get(`${id}-steppanel`).hidden);

test('next opens the following step and marks the one it leaves passed', () => {
  const p = page(wizard(['a', 'b', 'c']));
  p.click(p.next);
  assert.deepEqual(shown(p, ['a', 'b', 'c']), ['b']);
  assert.equal(p.get('b-step').getAttribute('aria-current'), 'step');
  assert.equal(p.get('a-step').getAttribute('aria-current'), null);
  assert.ok(p.get('a-step').hasAttribute('data-wizard-done'));
  assert.ok(!p.back.hidden && !p.next.hidden);
  p.click(p.next);
  assert.deepEqual(shown(p, ['a', 'b', 'c']), ['c']);
  assert.ok(p.next.hidden, 'the last step offers no next');
});

test('next stays on a step refusing a control, and reports it', () => {
  const name = input('name');
  name.valid = false;
  const p = page(wizard(['a', 'b'], { a: [name] }));
  p.click(p.next);
  assert.deepEqual(shown(p, ['a', 'b']), ['a']);
  assert.equal(name.reported, 1);
  name.valid = true;
  p.click(p.next);
  assert.deepEqual(shown(p, ['a', 'b']), ['b']);
});

test('back and an earlier trigger open their step without checking', () => {
  const late = input('late');
  const p = page(wizard(['a', 'b', 'c'], { c: [late] }));
  p.click(p.get('c-step'));
  late.valid = false;
  p.click(p.back);
  assert.deepEqual(shown(p, ['a', 'b', 'c']), ['b']);
  p.click(p.get('a-step'));
  assert.deepEqual(shown(p, ['a', 'b', 'c']), ['a']);
  assert.ok(p.back.hidden, 'the first step offers no back');
  assert.ok(!p.get('a-step').hasAttribute('data-wizard-done'));
  assert.equal(late.reported, 0);
});

test('a later trigger stops on the first step before it that refuses', () => {
  const middle = input('middle');
  middle.valid = false;
  const p = page(wizard(['a', 'b', 'c'], { b: [middle] }));
  p.click(p.get('c-step'));
  assert.deepEqual(shown(p, ['a', 'b', 'c']), ['b']);
  assert.equal(middle.reported, 1);
});

test('the buttons skip a step a condition hides', () => {
  const p = page(wizard(['a', 'b', 'c'], {}, ['b', 'c']));
  assert.ok(!p.enter(input('x')), 'a field outside the wizard is left alone');
  p.click(p.next);
  assert.deepEqual(shown(p, ['a', 'b', 'c']), ['a'], 'no step follows the first');
  const q = page(wizard(['a', 'b', 'c'], {}, ['b']));
  q.click(q.next);
  assert.deepEqual(shown(q, ['a', 'b', 'c']), ['c']);
  assert.ok(q.next.hidden);
  q.click(q.back);
  assert.deepEqual(shown(q, ['a', 'b', 'c']), ['a']);
});

test('Enter in a field moves forward until the last step', () => {
  const first = input('first');
  const last = input('last');
  const notes = el('textarea', { name: 'notes' });
  const p = page(wizard(['a', 'b'], { a: [first, notes], b: [last] }));
  assert.ok(!p.enter(notes), 'Enter in a textarea breaks the line');
  assert.ok(p.enter(first));
  assert.deepEqual(shown(p, ['a', 'b']), ['b']);
  assert.ok(!p.enter(last), 'Enter on the last step submits');
});

test('of one submit, only the first refused control opens its step', () => {
  const first = input('email');
  const second = input('password');
  const p = page(wizard(['a', 'b', 'c'], { b: [first], c: [second] }));
  p.fire('invalid', { target: first });
  p.fire('invalid', { target: second });
  assert.deepEqual(shown(p, ['a', 'b', 'c']), ['b']);
  p.flush();
  p.fire('invalid', { target: second });
  assert.deepEqual(shown(p, ['a', 'b', 'c']), ['c']);
});
