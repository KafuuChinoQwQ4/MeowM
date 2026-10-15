import assert from 'node:assert/strict';
import { after, before, test } from 'node:test';
import { spawn } from 'node:child_process';
import { chromium } from 'playwright';

const port = process.env.TRANSACTION_UI_TEST_PORT || '1429';
const url = `http://127.0.0.1:${port}`;
let browser;
let server;
before(async () => {
  server = spawn(process.execPath, ['node_modules/vite/bin/vite.js', '--host', '127.0.0.1', '--port', port, '--strictPort'], { cwd: new URL('..', import.meta.url), stdio: 'ignore' });
  for (let attempt = 0; attempt < 100; attempt++) {
    if (server.exitCode !== null) throw new Error('Vite test server exited');
    try { if ((await fetch(url)).ok) break; } catch {}
    if (attempt === 99) throw new Error('Vite test server did not start');
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  browser = await chromium.launch({ headless: true, executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH || undefined, args: ['--no-sandbox'] });
});
after(async () => { await browser?.close(); server?.kill(); });

test('transaction detail exposes audit and saves an optimistic edit through Tauri IPC', async () => {
  const page = await browser.newPage({ viewport: { width: 390, height: 780 } });
  await page.addInitScript(() => {
    const account = '00000000-0000-4000-8000-000000000001';
    window.ledgerCalls = [];
    window.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
      window.ledgerCalls.push({ command, args });
      if (command === 'list_accounts') return [{ id: account, name: '钱包', category: 'wallet' }];
      if (command === 'list_categories') return [{ id: 'food', name: '餐饮' }];
      if (command === 'get_day') return { date: args.date, income: '0.00', expense: '24.50', net: '-24.50', balance: '75.50', reference: null, entries: [{ id: 'tx-1', kind: 'expense', amount: '-24.50', note: '面包店', time: '12:30' }] };
      if (command === 'transaction_detail') return { id: 'tx-1', kind: 'expense', status: 'confirmed', amount: '24.50', date: '2026-10-14', account_id: account, destination_id: null, merchant: '面包店', description: '午餐', category_id: 'food', original_transaction_id: null, edit_version: 3, reversal_id: null, reversal_of: null, linked_ids: ['tx-refund'], sources: [{source:'manual',digest:'abc'}], history: [['created', 1760445000000000]] };
      if (command === 'write_transaction') return 'tx-1';
      throw new Error(`Unexpected IPC command: ${command}`);
    } };
  });
  try {
    await page.goto(url);
    await page.locator('#menu').click();
    await page.locator('#nav-records').click();
    await page.getByRole('button', { name: /面包店/ }).click();
    await page.getByText('关联记录', { exact: true }).waitFor();
    await page.getByText(/created/).waitFor();
    await page.getByRole('button', { name: '编辑交易' }).click();
    await page.getByLabel('金额（元）').fill('29.50');
    await page.getByRole('button', { name: '查看影响并保存' }).click();
    assert.match(await page.locator('dialog[open] .impact-preview').innerText(), /余额减少 ¥ 5\.00，报表支出增加 ¥ 5\.00/);
    await page.getByLabel('金额（元）').fill('30.00');
    await page.getByRole('button', { name: '确认保存修改' }).click();
    await page.getByRole('alert').getByText('内容已修改，请重新查看影响。').waitFor();
    assert.equal(await page.evaluate(() => window.ledgerCalls.some(item => item.command === 'write_transaction')), false);
    await page.getByRole('button', { name: '查看影响并保存' }).click();
    assert.equal(await page.locator('dialog[open] .impact-preview').count(), 1);
    await page.getByRole('button', { name: '确认保存修改' }).click();
    const call = await page.evaluate(() => window.ledgerCalls.find(item => item.command === 'write_transaction'));
    assert.equal(call.args.request.edit.expected_version, 3);
    assert.equal(call.args.request.edit.amount, '30.00');
  } finally { await page.close(); }
});
