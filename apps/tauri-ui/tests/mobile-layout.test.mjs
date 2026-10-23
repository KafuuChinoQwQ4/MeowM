import assert from 'node:assert/strict';
import { test } from 'node:test';
import { browserSuite } from './helpers/browser.mjs';
import { installWorkflowIpc } from './helpers/workflow-ipc.mjs';
import { navigate } from './helpers/navigation.mjs';

const { url, newPage } = browserSuite('1448');

async function open(page) {
  await installWorkflowIpc(page);
  await page.addInitScript(() => {
    const original = window.__TAURI_INTERNALS__.invoke;
    window.__TAURI_INTERNALS__.invoke = async (command, args) => {
      if (command === 'recurring_state')
        return { templates: [], reminders: [], generated_count: 0, has_more: false };
      if (command === 'classification_state') return { categories: [], rules: [], aliases: [] };
      if (command === 'notification_access')
        return { android: true, granted: false, connected: false };
      if (command === 'notification_state')
        return {
          sources: [
            { package_name: 'com.tencent.mm', enabled: false, account_id: null },
            { package_name: 'com.eg.android.AlipayGphone', enabled: false, account_id: null },
          ],
          latest: [],
        };
      return original(command, args);
    };
  });
  await page.goto(url);
  await page.locator('#day-content[aria-busy=false]').waitFor();
}

async function navigationReady(page) {
  await page.waitForFunction(
    () => Math.abs(document.querySelector('.side-nav').getBoundingClientRect().left) < 1,
  );
}

async function scrollToBottom(page, target) {
  const box = await page.locator('#day-content').boundingBox();
  await page.mouse.move(box.x + box.width / 2, box.y + Math.min(100, box.height / 2));
  await page.mouse.wheel(0, 10000);
  await page.waitForFunction(
    (selector) => {
      const element = document.querySelector(selector);
      if (!element) return false;
      const r = element.getBoundingClientRect();
      return r.top >= 0 && r.bottom <= innerHeight && r.height > 0;
    },
    target,
    { timeout: 1500 },
  );
}

for (const viewport of [
  { width: 320, height: 568 },
  { width: 407, height: 840 },
  { width: 760, height: 360 },
]) {
  test(`long settings pages scroll to their final controls at ${viewport.width}x${viewport.height}`, async () => {
    const page = await newPage({ viewport, isMobile: true, hasTouch: true });
    try {
      await open(page);
      for (const [view, target] of [
        ['data', '.data-card[aria-label="筛选导出"] button[type=submit]'],
        ['privacy', '.button-danger'],
        ['notifications', '.notification-settings > button:last-child'],
      ]) {
        await navigate(page, view);
        assert.ok(
          await page.evaluate(
            () =>
              document.querySelector('#day-content').getBoundingClientRect().top >=
              document.querySelector('.back-button').getBoundingClientRect().bottom,
          ),
          'scrolling content must not overlap the back button',
        );
        assert.equal(await page.locator('#day-content').evaluate((e) => e.scrollTop), 0);
        await scrollToBottom(page, target);
        const layout = await page.locator('#day-content').evaluate((root) => {
          const box = root.getBoundingClientRect();
          return {
            outside: [...root.querySelectorAll('input,select,button')]
              .filter((e) => {
                const r = e.getBoundingClientRect();
                return r.width > 0 && (r.left < box.left - 1 || r.right > box.right + 1);
              })
              .map((e) => e.outerHTML.slice(0, 100)),
            cardBottom: document.querySelector('.ledger').getBoundingClientRect().bottom,
          };
        });
        assert.deepEqual(layout.outside, [], `${view}: controls must fit inside the card`);
        assert.ok(layout.cardBottom <= viewport.height, `${view}: card must fit the viewport`);
      }
    } finally {
      await page.close();
    }
  });
}

test('short-screen navigation keeps settings and its nested destinations reachable', async () => {
  const page = await newPage({ viewport: { width: 320, height: 480 } });
  try {
    await open(page);
    await page.locator('#menu').click();
    await navigationReady(page);
    await page.mouse.move(80, 200);
    await page.mouse.wheel(0, 1500);
    await page.waitForFunction(() => {
      const r = document.querySelector('#nav-settings').getBoundingClientRect();
      return r.top >= 0 && r.bottom <= innerHeight;
    });
    const last = page.locator('#nav-settings');
    assert.ok((await last.boundingBox()).height >= 40);
    await last.click();
    await page.locator('#nav-security').click();
    await page.locator('#nav-privacy').click();
    await page.getByRole('heading', { name: '隐私与数据', exact: true }).waitFor();
  } finally {
    await page.close();
  }
});

test('hierarchy back button is rounded, centered, and has a press ripple', async () => {
  const page = await newPage({ viewport: { width: 390, height: 844 } });
  try {
    await open(page);
    await navigate(page, 'privacy');
    const back = page.locator('.back-button');
    const box = await back.boundingBox();
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.down();
    const style = await back.evaluate((button) => {
      const box = button.getBoundingClientRect();
      const computed = getComputedStyle(button);
      const icon = button.querySelector('.back-icon').getBoundingClientRect();
      return {
        round: computed.borderRadius,
        rectangular: Math.round(box.width) > Math.round(box.height),
        centered: computed.display === 'grid' && computed.placeItems === 'center',
        iconCentered: Math.abs(icon.top + icon.height / 2 - (box.top + box.height / 2)) < 1,
        ripple: getComputedStyle(button, '::after').animationName,
      };
    });
    await page.mouse.up();
    assert.equal(style.round, '12px');
    assert.equal(style.rectangular, true);
    assert.equal(style.centered, true);
    assert.equal(style.iconCentered, true);
    assert.notEqual(style.ripple, 'none');
  } finally {
    await page.close();
  }
});

test('unlock form remains reachable when the keyboard leaves a short viewport', async () => {
  const page = await newPage({ viewport: { width: 320, height: 240 } });
  try {
    await page.addInitScript(() => {
      window.__TAURI_INTERNALS__ = {
        invoke: async () => ({ locked: true, lock_enabled: true, hide_amounts: false }),
      };
    });
    await page.goto(url);
    const screen = page.locator('.privacy-lock-screen');
    await page.getByLabel('解锁密码').waitFor();
    const box = await screen.boundingBox();
    await page.mouse.move(box.x + 100, 100);
    await page.mouse.wheel(0, 10000);
    await page.waitForFunction(() => {
      const r = document.querySelector('button[type=submit]').getBoundingClientRect();
      return r.top >= 0 && r.bottom <= innerHeight;
    });
  } finally {
    await page.close();
  }
});

test('transaction dialog fits narrow and keyboard-sized viewports and scrolls to save', async () => {
  const page = await newPage({ viewport: { width: 320, height: 480 } });
  try {
    await open(page);
    await page.getByRole('button', { name: '记一笔' }).click();
    await page.getByLabel('金额（元）').waitFor();
    await page.setViewportSize({ width: 320, height: 300 });
    const layout = await page.locator('dialog').evaluate((dialog) => {
      const box = dialog.getBoundingClientRect();
      return {
        top: box.top,
        bottom: box.bottom,
        outside: [...dialog.querySelectorAll('input,select,fieldset')].filter((e) => {
          const r = e.getBoundingClientRect();
          return r.left < box.left || r.right > box.right;
        }).length,
      };
    });
    assert.ok(layout.top >= 0 && layout.bottom <= 300);
    assert.equal(layout.outside, 0);
    const box = await page.locator('dialog').boundingBox();
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.wheel(0, 10000);
    await page.waitForFunction(() => {
      const r = document.querySelector('dialog form > button').getBoundingClientRect();
      return r.top >= 0 && r.bottom <= innerHeight;
    });
  } finally {
    await page.close();
  }
});
