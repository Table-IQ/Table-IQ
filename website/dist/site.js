// Real product captures only: this switcher does not simulate a live database.
(() => {
  document.querySelectorAll('.mobile-menu a').forEach(link => {
    link.addEventListener('click', () => { link.closest('details').open = false; });
  });
  const engineIndex = document.querySelector('.engine-index');
  document.querySelectorAll('a[href="#engine-list"]').forEach(link => {
    link.addEventListener('click', () => { if (engineIndex) engineIndex.open = true; });
  });
  const buttons = [...document.querySelectorAll('[data-screen]')];
  const panels = [...document.querySelectorAll('[data-screen-panel]')];
  const reduced = matchMedia('(prefers-reduced-motion: reduce)');
  let animation;
  function select(button) {
    const figure = panels.find(panel => panel.dataset.screenPanel === button.dataset.screen);
    if (!figure) return;
    buttons.forEach(item => item.setAttribute('aria-pressed', String(item === button)));
    panels.forEach(panel => { panel.hidden = panel !== figure; });
    if (!reduced.matches && figure.animate) {
      animation?.cancel();
      animation = figure.animate([{ opacity: .5, transform: 'translateY(4px)' }, { opacity: 1, transform: 'translateY(0)' }], { duration: 220, easing: 'cubic-bezier(.16,1,.3,1)' });
    }
  }
  if (buttons.length && panels.length) {
    select(buttons.find(button => button.getAttribute("aria-pressed") === "true") || buttons[0]);
    document.querySelector('[data-screen-controls]').hidden = false;
    buttons.forEach((button, index) => {
      button.addEventListener('click', () => select(button));
      button.addEventListener('keydown', event => {
        let next;
        if (event.key === 'ArrowRight') next = (index + 1) % buttons.length;
        if (event.key === 'ArrowLeft') next = (index + buttons.length - 1) % buttons.length;
        if (event.key === 'Home') next = 0;
        if (event.key === 'End') next = buttons.length - 1;
        if (next === undefined) return;
        event.preventDefault();
        buttons[next].focus();
        select(buttons[next]);
      });
    });
  }
  if ('IntersectionObserver' in window && !reduced.matches) {
    const items = [...document.querySelectorAll('[data-reveal]')];
    const observer = new IntersectionObserver(entries => entries.forEach(entry => {
      if (entry.isIntersecting) {
        entry.target.classList.add('is-in');
        observer.unobserve(entry.target);
      }
    }), { rootMargin: '0px 0px 48px 0px' });
    items.forEach(item => observer.observe(item));
    document.documentElement.classList.add('js');
    reduced.addEventListener('change', () => {
      if (reduced.matches) {
        animation?.cancel();
        document.documentElement.classList.remove('js');
        observer.disconnect();
      }
    });
  }
})();
