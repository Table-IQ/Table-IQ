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
  const root = document.documentElement;
  const tokens = getComputedStyle(root);
  const controls = document.querySelector('[data-screen-controls]');
  let animation;
  let observer;
  let indicator;
  let selected;
  function positionIndicator() {
    if (!indicator || !selected) return;
    indicator.style.transform = `translateX(${selected.offsetLeft}px) scaleX(${selected.offsetWidth})`;
  }
  function select(button, animate = true) {
    const figure = panels.find(panel => panel.dataset.screenPanel === button.dataset.screen);
    if (!figure || button === selected) return;
    animation?.cancel();
    selected = button;
    buttons.forEach(item => item.setAttribute('aria-pressed', String(item === button)));
    panels.forEach(panel => { panel.hidden = panel !== figure; });
    positionIndicator();
    if (animate && !reduced.matches && figure.animate) {
      animation = figure.animate(
        [{ opacity: .35, transform: 'translateY(16px)' }, { opacity: 1, transform: 'translateY(0)' }],
        { duration: parseFloat(tokens.getPropertyValue('--dur-long')), easing: tokens.getPropertyValue('--ease-out').trim() }
      );
    }
  }
  if (buttons.length && panels.length && controls) {
    controls.hidden = false;
    indicator = document.createElement('span');
    indicator.dataset.screenIndicator = '';
    indicator.setAttribute('aria-hidden', 'true');
    controls.append(indicator);
    controls.classList.add('has-indicator');
    select(buttons.find(button => button.getAttribute("aria-pressed") === "true") || buttons[0], false);
    // Commit the initial position before enabling transitions; do not slide in from the edge.
    indicator.getBoundingClientRect();
    controls.classList.add('indicator-ready');
    if ('ResizeObserver' in window) {
      new ResizeObserver(positionIndicator).observe(controls);
    } else {
      window.addEventListener('resize', positionIndicator);
    }
    document.fonts?.ready.then(positionIndicator);
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
    observer = new IntersectionObserver(entries => entries.forEach(entry => {
      if (entry.isIntersecting) {
        entry.target.classList.add('is-in');
        observer.unobserve(entry.target);
      }
    }), { rootMargin: '0px 0px 48px 0px' });
    items.forEach(item => observer.observe(item));
    root.classList.add('js');
  }
  // Finish a keyboard-focused entrance once, so moving focus away cannot replay it.
  root.addEventListener('focusin', event => {
    if (!event.target.matches(':focus-visible')) return;
    event.target.closest('.hero .actions, .hero-product')?.getAnimations().forEach(entrance => entrance.finish());
  });
  if (!reduced.matches) root.classList.add('motion-ready');
  reduced.addEventListener('change', () => {
    if (!reduced.matches) return;
    animation?.cancel();
    root.classList.remove('js', 'motion-ready');
    observer?.disconnect();
  });
})();
