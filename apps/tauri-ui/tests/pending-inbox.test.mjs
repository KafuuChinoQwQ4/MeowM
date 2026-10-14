import assert from 'node:assert/strict';
import { after, before, test } from 'node:test';
import { spawn } from 'node:child_process';
import { chromium } from 'playwright';

// Install the test browser with `npx playwright install chromium`, or set
// PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH to an existing Chromium executable.
const port = process.env.PENDING_INBOX_TEST_PORT || '1427';
const url = `http://127.0.0.1:${port}`;
let browser;
let server;
before(async () => {
  server = spawn(process.execPath, ['node_modules/vite/bin/vite.js', '--host', '127.0.0.1', '--port', port, '--strictPort'], { cwd: new URL('..', import.meta.url), stdio: 'ignore' });
  server.on('error', error => { throw error; });
  for (let attempt = 0; attempt < 100; attempt++) {
    if (server.exitCode !== null) throw new Error('Vite test server exited');
    try { if ((await fetch(url)).ok) break; } catch {}
    if (attempt === 99) throw new Error('Vite test server did not start');
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  browser = await chromium.launch({ headless: true, executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH || undefined, args: ['--no-sandbox'] });
});
after(async () => { await browser?.close(); server?.kill(); });

async function openInbox(page, categoryFailsOnce = false) {
  page.setDefaultTimeout(5000);
  await page.addInitScript(({ categoryFailsOnce }) => {
    let confirmed = false;
    let categoryAttempts = 0;
    const account = '00000000-0000-4000-8000-000000000001';
    window.ledgerCalls = [];
    // The browser has no native Tauri runtime. Only the IPC boundary is mocked;
    // tests exercise real navigation, forms and cache refresh in main.ts.
    window.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
      window.ledgerCalls.push(command);
      if (command === 'list_accounts') return [{ id: account, name: '测试钱包', category: 'wallet' }];
      if (command === 'get_day') return { date: args.date, income: '0.00', expense: confirmed ? '12.30' : '0.00', net: confirmed ? '-12.30' : '0.00', balance: confirmed ? '87.70' : '100.00', reference: { date: args.date, amount: '100.00' }, entries: confirmed ? [{ id: 'confirmed', kind: 'expense', amount: '-12.30', note: '测试商户', time: '12:00' }] : [] };
      if (command === 'query_entries') return { items: confirmed ? [] : [{ id: '00000000-0000-4000-8000-000000000002', status: 'pending', kind: 'expense', occurred_at: new Date().toISOString(), amount: '12.30', merchant: '测试商户', description: '测试', edit_version: 0, reversal_of: null, account_id: account, category_id: null, original_transaction_id: null, sources: ['wechat'] }], total: confirmed ? 0 : 1, offset: 0, limit: 20 };
      if (command === 'confirm_pending') { confirmed = true; return; }
      if (command === 'list_categories') {
        if (categoryFailsOnce && categoryAttempts++ === 0) throw new Error('分类读取失败');
        return [{ id: 'food', name: '餐饮' }];
      }
      throw new Error(`Unexpected IPC command: ${command}`);
    } };
  }, { categoryFailsOnce });
  await page.goto(url);
  await page.waitForFunction(() => document.querySelector('.balance-value')?.textContent === '¥ 100.00');
  await page.locator('#menu').click();
  await page.locator('#nav-pending').click();
  await page.getByText('测试商户', { exact: true }).waitFor();
}

for (const mode of ['single', 'bulk']) {
  test(`${mode} pending confirmation refreshes totals and balance when returning to ledger`, async () => {
    const page = await browser.newPage({ viewport: { width: 430, height: 820 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    try {
      await openInbox(page);
      if (mode === 'bulk') {
        await page.getByLabel('选择本页全部记录').check();
        await page.getByRole('button', { name: '批量确认', exact: true }).click();
      } else {
        await page.getByRole('button', { name: '编辑 / 确认', exact: true }).click();
        await page.getByRole('button', { name: '保存并确认', exact: true }).click();
        await page.locator('#sheet').waitFor({ state: 'hidden' });
      }
      await page.getByText('没有符合条件的记录').waitFor();
      await page.locator('#menu').click();
      await page.locator('#nav-ledger').click();
      await page.waitForFunction(() => document.querySelector('.balance-value') !== null);
      assert.equal(await page.locator('.balance-value').textContent(), '¥ 87.70');
      assert.match(await page.locator('.summary-card.expense').textContent(), /12\.30/);
      assert.ok(await page.evaluate(() => window.ledgerCalls.filter(command => command === 'get_day').length) > 1);
      assert.deepEqual(errors, []);
    } finally { await page.close(); }
  });
}

test('category loading failure is visible and can be retried', async () => {
  const page = await browser.newPage();
  try {
    await openInbox(page, true);
    await page.getByRole('button', { name: '编辑 / 确认', exact: true }).click();
    await page.getByText('分类读取失败', { exact: false }).waitFor();
    await page.getByRole('button', { name: '重试分类', exact: true }).click();
    await page.waitForFunction(() => [...document.querySelectorAll('option')].some(option => option.textContent === '餐饮'));
  } finally { await page.close(); }
});
