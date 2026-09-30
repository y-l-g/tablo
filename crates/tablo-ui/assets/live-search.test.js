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

test('a prefixed table reads and writes its own search parameter', () => {
  // A relation table's parameters carry the relation's prefix; the bare
  // list's parameters ride the same query untouched.
  const params = new URLSearchParams(
    withSearch('q=ada&sort=name&comments.q=old&comments.after=abc', 'hi', 'comments.'),
  );
  assert.equal(params.get('comments.q'), 'hi');
  assert.equal(params.get('q'), 'ada');
  assert.equal(params.get('sort'), 'name');
  assert.equal(params.has('comments.after'), false);
  assert.equal(params.has('after'), false, 'no bare cursor is invented');
});

test('a prefixed blank term removes the prefixed search only', () => {
  const params = new URLSearchParams(withSearch('q=ada&comments.q=old', '   ', 'comments.'));
  assert.equal(params.has('comments.q'), false);
  assert.equal(params.get('q'), 'ada');
});
