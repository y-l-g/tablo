// Unit test for the shared selection-wire codec in `wire.js`.
//
// There is no JS test runner in this workspace — the assets are plain browser
// scripts loaded through `asset!` — so this runs on Node's built-in runner and
// reaches the functions through the guarded `module.exports` at the bottom of
// the script:
//
//     node --test crates/tablo-ui/assets/wire.test.js
//
// What it protects: the one delimiter contract `bulk.js` writes and
// `mutation-submit.js` prunes (GH #341). Both consumers `require` this module
// in Node and read the `TabloWire` browser global in the browser, so this
// single fixture covers the parse both sides share.

const test = require('node:test');
const assert = require('node:assert/strict');

const { wireFrom, wireOf } = require('./wire.js');

test('the wire parses to its keys', () => {
  assert.deepEqual(wireOf(',a,b,'), ['a', 'b']);
  assert.deepEqual(wireOf(',ab,'), ['ab']);
  assert.deepEqual(wireOf(''), []);
});

test('the wire serializes comma-delimited on both ends', () => {
  assert.equal(wireFrom(['a', 'b']), ',a,b,');
  assert.equal(wireFrom(['ab']), ',ab,');
  assert.equal(wireFrom([]), '');
});

test('parse and serialize round-trip', () => {
  assert.equal(wireFrom(wireOf(',a,b,')), ',a,b,');
  assert.deepEqual(wireOf(wireFrom(['a', 'b'])), ['a', 'b']);
  assert.equal(wireFrom(wireOf('')), '');
});

test('wire membership is exact', () => {
  // `,ab,` never matches `b`: the delimiters are what make the comparison a
  // membership test rather than a substring one.
  assert.ok(!wireOf(',ab,').includes('b'), 'a substring is not a member');
});
