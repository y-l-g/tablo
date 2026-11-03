// Steps for Tablo's `Wizard` blocks.
//
// A block renders `[data-wizard]` holding:
//
// * one `button[data-wizard-trigger]` per step, naming its panel by id in
//   `aria-controls`;
// * one `[data-wizard-step]` panel per step, naming its trigger by id in
//   `aria-labelledby`;
// * a `[data-wizard-back]` and a `[data-wizard-next]` button.
//
// The server marks the current step's trigger `aria-current="step"`, the
// triggers before it `data-wizard-done`, hides every other panel, and hides
// the back button on the first step and the next button on the last.
//
// Moving forward runs the browser's own checks on each step it leaves, and
// stops on the first step refusing a control, with the browser's message on
// it. Moving back never checks. Enter in a field of any step but the last
// moves forward rather than submitting the form.
//
// Every panel stays in the form, so the steps' controls all post and the
// browser checks them all on submit. A control the browser refuses on submit
// opens its step during the `invalid` event, as `tabs.js` opens its tab.
//
// A script rather than Topcoat handlers: Topcoat binds a handler to its own
// element, and `invalid` does not bubble.

// The steps of `wizard` a condition shows, as their triggers, and `keep`
// besides, in order. A step a condition hides sits under a `[hidden]`
// fieldset.
function triggers(wizard, keep) {
  return Array.from(wizard.querySelectorAll('[data-wizard-trigger]')).filter(
    (trigger) =>
      trigger.closest('[data-wizard]') === wizard &&
      (trigger === keep || !trigger.closest('[hidden]')),
  );
}

// The current step's trigger of `wizard`.
function current(wizard) {
  return triggers(wizard).find((t) => t.getAttribute('aria-current') === 'step') || null;
}

// The back or next button of `wizard`.
function button(wizard, name) {
  return (
    Array.from(wizard.querySelectorAll(`[data-wizard-${name}]`)).find(
      (b) => b.closest('[data-wizard]') === wizard,
    ) || null
  );
}

// Make `trigger` the current step of `wizard` in `doc`: show its panel alone,
// mark the steps before it passed, and offer back and next where they lead
// somewhere.
function show(doc, wizard, trigger) {
  const all = Array.from(wizard.querySelectorAll('[data-wizard-trigger]')).filter(
    (t) => t.closest('[data-wizard]') === wizard,
  );
  const at = all.indexOf(trigger);
  all.forEach((other, index) => {
    if (other === trigger) other.setAttribute('aria-current', 'step');
    else other.removeAttribute('aria-current');
    if (index < at) other.setAttribute('data-wizard-done', '');
    else other.removeAttribute('data-wizard-done');
    const panel = doc.getElementById(other.getAttribute('aria-controls'));
    if (panel) panel.hidden = other !== trigger;
  });
  const shown = triggers(wizard, trigger);
  const back = button(wizard, 'back');
  const next = button(wizard, 'next');
  if (back) back.hidden = shown.indexOf(trigger) <= 0;
  if (next) next.hidden = shown.indexOf(trigger) >= shown.length - 1;
}

// Whether the browser accepts every control of the step `trigger` opens in
// `doc`; else shows its message on the first it refuses.
function passes(doc, trigger) {
  const panel = doc.getElementById(trigger.getAttribute('aria-controls'));
  if (!panel) return true;
  const refused = Array.from(panel.querySelectorAll('input, select, textarea')).find(
    (control) => control.willValidate && !control.validity.valid,
  );
  if (!refused) return true;
  refused.reportValidity();
  return false;
}

// Move `wizard` in `doc` to `target`: back at once, forward one step at a
// time, each step it leaves checked first.
function go(doc, wizard, target) {
  let from = current(wizard);
  const shown = triggers(wizard, from);
  const to = shown.indexOf(target);
  if (!from || to < 0) {
    if (to >= 0) show(doc, wizard, target);
    return;
  }
  while (shown.indexOf(from) < to) {
    if (!passes(doc, from)) return;
    from = shown[shown.indexOf(from) + 1];
    show(doc, wizard, from);
  }
  show(doc, wizard, target);
}

// The step `offset` places after the current one of `wizard`, or `null`.
function step(wizard, offset) {
  const from = current(wizard);
  const shown = triggers(wizard, from);
  return shown[shown.indexOf(from) + offset] || null;
}

// Open every hidden step holding `control`, from the innermost out.
function reveal(doc, control) {
  let panel = control.closest && control.closest('[data-wizard-step]');
  while (panel) {
    if (panel.hidden) {
      const trigger = doc.getElementById(panel.getAttribute('aria-labelledby'));
      const wizard = trigger && trigger.closest('[data-wizard]');
      if (wizard) show(doc, wizard, trigger);
    }
    panel = panel.parentElement && panel.parentElement.closest('[data-wizard-step]');
  }
}

// Whether Enter on `node` would submit its form: a field that is not a
// textarea, a button or a submit control.
function submitsOnEnter(node) {
  if (!node || node.tagName !== 'INPUT') return false;
  const type = (node.getAttribute('type') || 'text').toLowerCase();
  return !['button', 'submit', 'reset', 'image', 'checkbox', 'radio', 'file'].includes(type);
}

// Document-level delegation: runtime navigation replaces the page without
// running this script again, so a listener bound to a step would be lost with
// the first navigation.
function install(doc, defer = (f) => setTimeout(f, 0)) {
  doc.addEventListener('click', (e) => {
    const hit =
      e.target &&
      e.target.closest &&
      e.target.closest('[data-wizard-trigger], [data-wizard-back], [data-wizard-next]');
    const wizard = hit && hit.closest('[data-wizard]');
    if (!wizard) return;
    if (hit.hasAttribute('data-wizard-trigger')) go(doc, wizard, hit);
    else {
      const target = step(wizard, hit.hasAttribute('data-wizard-next') ? 1 : -1);
      if (target) go(doc, wizard, target);
    }
  });
  doc.addEventListener('keydown', (e) => {
    if (e.key !== 'Enter' || e.defaultPrevented || e.isComposing) return;
    if (!submitsOnEnter(e.target)) return;
    const panel = e.target.closest('[data-wizard-step]');
    const wizard = panel && panel.parentElement && panel.parentElement.closest('[data-wizard]');
    const next = wizard && step(wizard, 1);
    if (!next) return;
    e.preventDefault();
    go(doc, wizard, next);
  });
  // One submit fires `invalid` at every refused control in document order, then
  // focuses the first: only that one's step opens.
  let checking = false;
  doc.addEventListener(
    'invalid',
    (e) => {
      if (checking) return;
      checking = true;
      defer(() => {
        checking = false;
      });
      reveal(doc, e.target);
    },
    true,
  );
}

if (typeof document !== 'undefined') {
  install(document);
}

// Exposed for the Node unit test (`wizard.test.js`); the guard keeps the export
// out of the browser.
if (typeof module !== 'undefined' && module.exports) {
  module.exports = { go, install, reveal, show };
}
