// Unit test for the query rewrite `filters.js` performs on a live table.
//
//     node --test crates/tablo-ui/assets/filters.test.js
//
// What it protects: a filter change on a live table replaces exactly the
// query's `f.*` parameters and drops the cursor, keeping the search, sort and
// grouping the table shows. The server reads the result with
// `TableState::from_query`, one `?f.<name>=` parameter per active filter.

const test = require('node:test');
const assert = require('node:assert/strict');

const { withFilters } = require('./filters.js');

test('replaces the filter parameters and keeps the rest', () => {
  const next = withFilters('q=ada&sort=name&dir=desc&f.status=draft&group_by=status', [
    ['status', 'published'],
    ['featured', 'true'],
  ]);
  const params = new URLSearchParams(next);
  assert.equal(params.get('q'), 'ada');
  assert.equal(params.get('sort'), 'name');
  assert.equal(params.get('dir'), 'desc');
  assert.equal(params.get('group_by'), 'status');
  assert.deepEqual(params.getAll('f.status'), ['published']);
  assert.equal(params.get('f.featured'), 'true');
});

test('a blank control is no filter', () => {
  const params = new URLSearchParams(withFilters('f.status=draft', [['status', '  ']]));
  assert.equal(params.has('f.status'), false);
});

test('a new filter is a new result set: the cursor goes', () => {
  const params = new URLSearchParams(
    withFilters('after=abc&before=def&q=x', [['status', 'draft']]),
  );
  assert.equal(params.has('after'), false);
  assert.equal(params.has('before'), false);
  assert.equal(params.get('q'), 'x');
});

test('a value with separators round-trips through the query encoding', () => {
  const params = new URLSearchParams(withFilters('', [['author', 'Smith, John: 50%']]));
  assert.equal(params.get('f.author'), 'Smith, John: 50%');
});

test('clearing drops every filter and keeps the newer search and sort', () => {
  // The hoisted "Clear filters" link edits the transport's current query,
  // not the page-load URL its href carries.
  const params = new URLSearchParams(
    withFilters('q=hello&sort=title&dir=asc&f.status=published&filters=status:draft', []),
  );
  assert.equal(params.get('q'), 'hello');
  assert.equal(params.get('sort'), 'title');
  assert.equal(params.has('f.status'), false);
  assert.equal(params.has('filters'), false, 'the retired spelling goes too');
});

// --- the Clear filters click -------------------------------------------------

const { listenerDocument } = require('./test-dom');

// Load a fresh copy of the script against a document stand-in whose one live
// filter form holds `controls` and a transport carrying `query`.
function liveClearHarness(query) {
  const controls = [{ value: 'published' }, { value: 'true' }];
  const transport = {
    value: query,
    events: [],
    dispatchEvent(event) {
      this.events.push(event.type);
    },
  };
  const form = {
    querySelector: (selector) => (selector === '[data-filters-transport]' ? transport : null),
    querySelectorAll: (selector) => (selector === '[data-filter-name]' ? controls : []),
  };
  global.document = listenerDocument({
    querySelector: (selector) => (selector === 'form[data-filters-live]' ? form : null),
  });
  delete require.cache[require.resolve('./filters.js')];
  require('./filters.js');
  const click = (modifiers = {}) => {
    const event = {
      button: 0,
      defaultPrevented: false,
      // The empty table's link: it sits outside the bar's form.
      target: { closest: (s) => (s === '[data-filters-clear]' ? { closest: () => null } : null) },
      preventDefault() {
        this.defaultPrevented = true;
      },
      ...modifiers,
    };
    for (const listener of global.document.listeners('click')) listener(event);
    return event;
  };
  return { controls, transport, click };
}

test('the empty table\'s Clear filters clears the hoisted controls and the current query', () => {
  const { controls, transport, click } = liveClearHarness('q=hello&sort=title&f.status=published');
  const event = click();
  assert.equal(event.defaultPrevented, true, 'cleared in place, not navigated');
  assert.deepEqual(
    controls.map((c) => c.value),
    ['', ''],
    'the controls must not keep a filter the query dropped',
  );
  const params = new URLSearchParams(transport.value);
  assert.equal(params.get('q'), 'hello');
  assert.equal(params.has('f.status'), false);
  assert.deepEqual(transport.events, ['change']);
});

test('a modified click on Clear filters follows the href', () => {
  const { controls, transport, click } = liveClearHarness('f.status=published');
  const event = click({ metaKey: true });
  assert.equal(event.defaultPrevented, false);
  assert.equal(controls[0].value, 'published');
  assert.equal(transport.value, 'f.status=published');
  delete global.document;
});
