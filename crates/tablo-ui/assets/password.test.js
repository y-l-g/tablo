// Unit tests for `password.js`, on Node's built-in runner:
//
//     node --test crates/tablo-ui/assets/password.test.js
//
// What the cases protect:
// * a press shows the password, the next masks it again, and the toggle says which;
// * a toggle naming no input changes nothing;
// * a click outside a toggle changes nothing.

const test = require('node:test');
const assert = require('node:assert/strict');

const { listenerDocument } = require('./test-dom');
const { install, reveal } = require('./password');

// A toggle controlling the input `id`, recording its attributes.
function toggle(id) {
  const attrs = {
    'aria-controls': id,
    'data-show-label': 'Show password',
    'data-hide-label': 'Hide password',
  };
  return {
    attrs,
    getAttribute: (name) => attrs[name],
    setAttribute: (name, value) => {
      attrs[name] = value;
    },
  };
}

// A document holding the inputs `inputs`, by id.
function doc(inputs) {
  return listenerDocument({ getElementById: (id) => inputs[id] || null });
}

test('a press shows the password and the next masks it', () => {
  const input = { type: 'password' };
  const page = doc({ password: input });
  const button = toggle('password');
  assert.equal(reveal(page, button), true);
  assert.equal(input.type, 'text');
  assert.equal(button.attrs['aria-pressed'], 'true');
  assert.equal(button.attrs['aria-label'], 'Hide password');
  assert.equal(reveal(page, button), false);
  assert.equal(input.type, 'password');
  assert.equal(button.attrs['aria-pressed'], 'false');
  assert.equal(button.attrs['aria-label'], 'Show password');
});

test('a toggle naming no input changes nothing', () => {
  const button = toggle('missing');
  assert.equal(reveal(doc({}), button), false);
  assert.equal(button.attrs['aria-pressed'], undefined);
});

test('a click reaches the toggle it lands in, and only a toggle', () => {
  const input = { type: 'password' };
  const page = doc({ password: input });
  install(page);
  const button = toggle('password');
  const [click] = page.listeners('click');
  click({ target: { closest: () => null } });
  assert.equal(input.type, 'password', 'a click outside a toggle changes nothing');
  click({ target: { closest: (selector) => (selector === '[data-password-toggle]' ? button : null) } });
  assert.equal(input.type, 'text');
});
