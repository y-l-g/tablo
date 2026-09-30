// Unit test for the query rewrite `live-search.js` performs.
//
//     node --test crates/tablo-ui/assets/live-search.test.js
//
// What it protects: a live search keystroke sets only `q` and drops the
// cursor, keeping the sort, filters and grouping the table shows, and a
// cleared input removes the search instead of sending `q=`.

const test = require('node:test');
const assert = require('node:assert/strict');

// `live-search.js` registers its listeners on `document` at load time.
global.document = { addEventListener() {} };

const { withSearch } = require('./live-search.js');

test('sets the term and keeps the rest of the query', () => {
  const params = new URLSearchParams(
    withSearch('sort=name&dir=desc&f.status=published&group_by=status', 'ada'),
  );
  assert.equal(params.get('q'), 'ada');
  assert.equal(params.get('sort'), 'name');
  assert.equal(params.get('dir'), 'desc');
  assert.equal(params.get('f.status'), 'published');
  assert.equal(params.get('group_by'), 'status');
});

test('a new term is a new result set: the cursor goes', () => {
  const params = new URLSearchParams(withSearch('q=old&after=abc&before=def', 'new'));
  assert.equal(params.get('q'), 'new');
  assert.equal(params.has('after'), false);
  assert.equal(params.has('before'), false);
});

test('a blank term removes the search', () => {
  const params = new URLSearchParams(withSearch('q=old&sort=name', '   '));
  assert.equal(params.has('q'), false);
  assert.equal(params.get('sort'), 'name');
});
