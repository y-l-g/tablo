// Shared selection-wire codec for the shell assets.
//
// The bulk selection lives in the form's hidden transport (`input[name="ids"]`),
// comma-delimited on both ends — `,a,b,`, empty when nothing is selected.
// `bulk.js` writes it, `mutation-submit.js` prunes it after a delete, so both
// read it through this one module: a change to the delimiter contract in one
// desyncs prune vs write. Delimiters make membership exact: `,ab,`
// never matches `b`.
//
// The document loads this before both consumers, which read the
// codec off the namespace below in the browser and `require` it in Node.
(() => {
function wireOf(value) {
  return (value || '')
    .split(',')
    .map((key) => key.trim())
    .filter((key) => key !== '');
}

function wireFrom(keys) {
  return keys.length === 0 ? '' : `,${keys.join(',')},`;
}

// Browser global for the `asset!` scripts the document loads after this one
// (`bulk.js`, `mutation-submit.js`); inert in Node, which uses the export below.
if (typeof window !== 'undefined') {
  window.TabloWire = { wireOf, wireFrom };
}

// Exposed for the Node test in `assets/wire.test.js` (there is no JS test
// runner in this workspace, and this file must stay a plain browser script
// loaded by `asset!`, so it cannot be an ES module). Guarded, so the browser
// branch is inert.
if (typeof module !== 'undefined' && module.exports) {
  module.exports = { wireOf, wireFrom };
}
})();
