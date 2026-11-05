// Real-browser checks. See website/README.md for the optional Puppeteer setup.
import assert from 'node:assert/strict';
import {after, before, test} from 'node:test';
import {createRequire} from 'node:module';
import {resolve} from 'node:path';

const require = createRequire(resolve(process.env.HALLMARK_PUPPETEER || '.', 'package.json'));
const puppeteer = require('puppeteer-core');
const url = process.env.WEBSITE_URL || 'http://127.0.0.1:4175/';
let browser;
before(async () => {
  browser = await puppeteer.launch({
    executablePath: process.env.CHROME_PATH || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    headless: true,
  });
});
after(async () => { await browser?.close(); });

async function open(t, motion = 'no-preference', javascript = true) {
  const page = await browser.newPage();
  t.after(() => page.close());
  await page.setViewport({width: 1440, height: 900});
  await page.setJavaScriptEnabled(javascript);
  await page.emulateMediaFeatures([{name: 'prefers-reduced-motion', value: motion}]);
  await page.goto(url, {waitUntil: 'domcontentloaded'});
  return page;
}

test('hero enters in sequence, finishes within 800ms, and exposes focused actions immediately', async t => {
  const page = await open(t);
  const entrances = await page.evaluate(() =>
    ['.hero h1', '.hero .lead', '.hero .actions', '.hero-product'].map(selector => {
      const animation = document.querySelector(selector).getAnimations()[0];
      return animation ? animation.effect.getTiming() : null;
    }));
  assert(entrances.every(Boolean), 'Each hero element should join the coordinated entrance');
  assert(entrances.every(timing => timing.delay + timing.duration <= 800));
  assert(entrances.every((timing, index) => !index || timing.delay > entrances[index - 1].delay));
  await page.focus('.hero .primary');
  assert.equal(await page.$eval('.hero .actions', element => getComputedStyle(element).opacity), '1');
  await page.focus('[data-screen="schema"]');
  assert.equal(await page.$eval('.hero .actions', element => getComputedStyle(element).opacity), '1', 'Leaving an action must not replay its entrance');
  await page.waitForFunction(() => document.querySelector('.hero-product').getAnimations().length === 0);
  assert.equal(await page.$eval('.hero-product', element => getComputedStyle(element).transform), 'none');
});

test('screen indicator follows mouse, keyboard, rapid selection, and responsive layout', async t => {
  const page = await open(t);
  assert(await page.$('[data-screen-indicator]'), 'Sliding selection indicator should exist');
  async function aligned() {
    await page.waitForFunction(() => {
      const indicator = document.querySelector('[data-screen-indicator]').getBoundingClientRect();
      const selected = document.querySelector('[data-screen][aria-pressed="true"]').getBoundingClientRect();
      return Math.abs(indicator.left - selected.left) < 1 && Math.abs(indicator.width - selected.width) < 1;
    });
  }
  await aligned();
  await page.click('[data-screen="browse"]');
  await aligned();
  await page.focus('[data-screen="browse"]');
  await page.keyboard.press('End');
  await aligned();
  await page.evaluate(() => ['query', 'review', 'browse', 'schema'].forEach(name => document.querySelector(`[data-screen="${name}"]`).click()));
  await page.waitForFunction(() => document.querySelector('[data-screen-panel]:not([hidden])').getAnimations().length === 0);
  assert.deepEqual(await page.$$eval('[data-screen-panel]:not([hidden])', panels => panels.map(panel => panel.dataset.screenPanel)), ['schema']);
  await page.setViewport({width: 375, height: 844});
  await aligned();
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
});

test('MCP screenshot settles as it enters the viewport', async t => {
  const page = await open(t);
  if (!await page.evaluate(() => CSS.supports('animation-timeline: view()'))) return t.skip('Browser uses the static fallback');
  await page.addStyleTag({content: 'html { scroll-behavior: auto !important; }'});
  async function atEntry(offset) {
    return page.evaluate(async offset => {
      const figure = document.querySelector('#agents > .product-figure');
      scrollTo(0, scrollY + figure.getBoundingClientRect().top - innerHeight + offset);
      await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      return {transform: getComputedStyle(figure).transform, opacity: Number(getComputedStyle(figure).opacity)};
    }, offset);
  }
  const entering = await atEntry(50);
  const settled = await atEntry(650);
  assert.notEqual(entering.transform, settled.transform, 'Screenshot should move with entry progress');
  assert(entering.opacity < settled.opacity);
  assert.equal(settled.opacity, 1);
});

test('reduced motion disables movement initially and when changed during an entrance', async t => {
  const page = await open(t, 'reduce');
  async function still() {
    // The shared base stylesheet collapses native transitions to 0.01ms.
    assert.equal(await page.evaluate(() => document.getAnimations().filter(animation => animation.effect.getTiming().duration > 1).length), 0);
    assert.equal(await page.$$eval('[data-reveal]', elements => elements.every(element => getComputedStyle(element).opacity === '1')), true);
  }
  await still();
  await page.click('[data-screen="review"]');
  await still();
  await page.emulateMediaFeatures([{name: 'prefers-reduced-motion', value: 'no-preference'}]);
  await page.reload({waitUntil: 'domcontentloaded'});
  await page.click('[data-screen="schema"]');
  await page.emulateMediaFeatures([{name: 'prefers-reduced-motion', value: 'reduce'}]);
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  await still();
  assert.equal(await page.$eval('[data-screen-panel]:not([hidden])', panel => panel.dataset.screenPanel), 'schema');
});

test('without JavaScript, content stays visible and screenshot controls stay hidden', async t => {
  const page = await open(t, 'no-preference', false);
  assert.equal(await page.evaluate(() => document.getAnimations().length), 0);
  assert.equal(await page.$eval('[data-screen-controls]', element => getComputedStyle(element).display), 'none');
  assert.equal(await page.$$eval('.hero h1, .hero .lead, .hero .actions, .hero-product, [data-reveal]', elements => elements.every(element => getComputedStyle(element).opacity === '1')), true);
});
