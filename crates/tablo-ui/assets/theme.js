// Document-level delegation: runtime navigation replaces the page without
// running this script again, so a listener bound to the toggle button itself
// would be lost with the first navigation.
document.addEventListener('click', (e) => {
  if (!e.target.closest || !e.target.closest('[data-theme-toggle]')) return;
  const freeze = document.createElement('style');
  freeze.appendChild(document.createTextNode('*,*::before,*::after,*::backdrop{transition:none!important}'));
  document.head.appendChild(freeze);
  document.documentElement.classList.toggle('dark');
  const t = document.documentElement.classList.contains('dark') ? 'dark' : 'light';
  localStorage.setItem('theme', t);
  document.cookie = `theme=${t};path=/;max-age=31536000`;
  requestAnimationFrame(() => requestAnimationFrame(() => freeze.remove()));
});
document.addEventListener('DOMContentLoaded', () => {
  // Reconcile, don't just add: the inline head script normally lands
  // this before first paint, so this is the backstop for a document that
  // reached the client some other way. A stored `light` must remove a
  // server-rendered `dark` class, or the choice is lost on the next page.
  const stored = localStorage.getItem('theme') || (document.cookie.match(/theme=([^;]+)/)?.[1]);
  if (stored === 'dark') document.documentElement.classList.add('dark');
  else if (stored === 'light') document.documentElement.classList.remove('dark');
});
