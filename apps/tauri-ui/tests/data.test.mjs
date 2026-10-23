import assert from 'node:assert/strict';
import { test } from 'node:test';
import { browserSuite } from './helpers/browser.mjs';
import { navigate } from './helpers/navigation.mjs';
const { url, newPage } = browserSuite('1437');
async function open(page) {
  await page.addInitScript(() => {
    window.calls = [];
    window.__TAURI_INTERNALS__ = {
      invoke: async (cmd, args = {}) => {
        if (cmd === 'privacy_status')
          return { locked: false, lock_enabled: false, hide_amounts: false, auto_lock_minutes: 5 };
        window.calls.push({ cmd, args });
        if (cmd === 'list_accounts') return [{ id: 'wallet', name: '钱包', category: 'wallet' }];
        if (cmd === 'get_day')
          return {
            date: args.date,
            income: '0.00',
            expense: '0.00',
            net: '0.00',
            balance: null,
            reference: null,
            entries: [],
          };
        if (cmd === 'recurring_state')
          return { templates: [], reminders: [], generated_count: 0, has_more: false };
        if (cmd === 'classification_state')
          return {
            categories: [{ id: 'food', name: '餐饮', enabled: true }],
            rules: [],
            aliases: [],
          };
        if (cmd === 'create_backup')
          return {
            name: 'meowm-test.meowm',
            mime: 'application/octet-stream',
            data: btoa('encrypted-test'),
            count: null,
          };
        if (cmd === 'inspect_backup') {
          if (args.password === 'wrong-password') throw Error('密码错误或备份已损坏');
          return {
            id: 'preview-once',
            backup: {
              format_version: 1,
              schema_version: 9,
              created_at: '2026-10-16T00:00:00Z',
              counts: [
                ['accounts', 2],
                ['transactions', 3],
                ['reconciliation_events', 7],
              ],
            },
            current_counts: [
              ['accounts', 1],
              ['transactions', 2],
            ],
          };
        }
        if (cmd === 'restore_backup') return;
        if (cmd === 'export_data')
          return {
            name: 'meowm-test.csv',
            mime: 'text/csv',
            data: btoa('id,status\r\n1,confirmed\r\n'),
            count: 1,
          };
        throw Error(cmd);
      },
    };
  });
  await page.goto(url);
  await navigate(page, 'data');
}
test('backup download clears password and restore requires successful preview plus explicit replacement consent', async () => {
  const page = await newPage({ viewport: { width: 390, height: 844 } });
  await open(page);
  await page.getByLabel('备份密码', { exact: true }).fill('test-only-pass');
  await page.getByLabel('再次输入备份密码').fill('different-pass');
  await page.getByRole('button', { name: '创建并保存备份' }).click();
  assert.match(await page.getByRole('alert').innerText(), /不一致/);
  assert.equal(
    await page.evaluate(() => window.calls.filter((c) => c.cmd === 'create_backup').length),
    0,
  );
  await page.getByLabel('再次输入备份密码').fill('test-only-pass');
  const download = page.waitForEvent('download');
  await page.getByRole('button', { name: '创建并保存备份' }).click();
  assert.equal((await download).suggestedFilename(), 'meowm-test.meowm');
  assert.equal(await page.getByLabel('备份密码', { exact: true }).inputValue(), '');
  const file = page.getByLabel('选择备份文件');
  await file.setInputFiles({
    name: 'saved.meowm',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from('encrypted-test'),
  });
  await page.getByLabel('恢复密码').fill('wrong-password');
  await page.getByRole('button', { name: '检查备份并预览' }).click();
  assert.match(await page.getByRole('alert').innerText(), /密码错误/);
  assert.equal(await page.getByRole('button', { name: '确认替换并恢复' }).count(), 0);
  await page.getByLabel('恢复密码').fill('test-only-pass');
  await page.getByRole('button', { name: '检查备份并预览' }).click();
  const restore = page.getByRole('button', { name: '确认替换并恢复' });
  await restore.waitFor();
  assert.equal(await restore.isDisabled(), true);
  assert.match(await page.getByRole('table').innerText(), /对账审计/);
  assert.equal(await page.getByLabel('恢复密码').inputValue(), '');
  await page.getByLabel('我确认用此备份替换当前全部账本').check();
  await restore.click();
  await page.getByText('备份恢复成功，账本已替换。', { exact: true }).waitFor();
  const calls = await page.evaluate(() => window.calls);
  assert.deepEqual(calls.find((c) => c.cmd === 'restore_backup').args, {
    previewId: 'preview-once',
    confirmed: true,
  });
  assert.ok(calls.filter((c) => c.cmd === 'list_accounts').length >= 2);
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
  await page.close();
});
test('export sends confirmed by default and explicit unconfirmed filters with the chosen format', async () => {
  const page = await newPage();
  await open(page);
  const exportForm = page.getByRole('region', { name: '筛选导出' });
  const first = page.waitForEvent('download');
  await exportForm.getByRole('button', { name: '导出全部筛选结果' }).click();
  await first;
  let call = await page.evaluate(() => window.calls.find((c) => c.cmd === 'export_data'));
  assert.equal(call.args.query.status, 'confirmed');
  assert.equal(call.args.includeUnconfirmed, false);
  assert.equal(call.args.bom, true);
  await exportForm.getByLabel('状态', { exact: true }).selectOption('pending');
  await exportForm.getByLabel('关键词', { exact: true }).fill('早餐');
  await exportForm.getByLabel('导出格式').selectOption('json');
  const second = page.waitForEvent('download');
  await exportForm.getByRole('button', { name: '导出全部筛选结果' }).click();
  await second;
  call = await page.evaluate(() => window.calls.filter((c) => c.cmd === 'export_data').at(-1));
  assert.equal(call.args.query.status, 'pending');
  assert.equal(call.args.query.keyword, '早餐');
  assert.equal(call.args.includeUnconfirmed, true);
  assert.equal(call.args.format, 'json');
  assert.equal(
    await page.evaluate(() =>
      window.calls.some((c) =>
        ['confirm_pending', 'ignore_pending', 'write_transaction'].includes(c.cmd),
      ),
    ),
    false,
  );
  await page.close();
});
