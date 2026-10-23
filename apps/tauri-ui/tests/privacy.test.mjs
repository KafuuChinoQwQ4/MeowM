import assert from 'node:assert/strict';
import { test } from 'node:test';
import { browserSuite } from './helpers/browser.mjs';
import { navigate } from './helpers/navigation.mjs';
const { url, newPage } = browserSuite('1438');

async function open(page) {
  await page.addInitScript(() => {
    let locked = true;
    let hide = false;
    window.calls = [];
    window.__TAURI_INTERNALS__ = {
      invoke: async (cmd, args = {}) => {
        window.calls.push({ cmd, args });
        if (cmd === 'privacy_status')
          return { locked, lock_enabled: true, hide_amounts: hide, auto_lock_minutes: 5 };
        if (cmd === 'privacy_unlock') {
          if (args.password !== 'test-only-pass') throw Error('密码错误');
          locked = false;
          return { locked, lock_enabled: true, hide_amounts: hide, auto_lock_minutes: 5 };
        }
        if (cmd === 'privacy_touch') return;
        if (locked) throw Error('应用已锁定');
        if (cmd === 'list_accounts') return [{ id: 'wallet', name: '钱包', category: 'wallet' }];
        if (cmd === 'get_day')
          return {
            date: args.date,
            income: '123.45',
            expense: '0.00',
            net: '123.45',
            balance: '123.45',
            reference: null,
            entries: [],
          };
        if (cmd === 'recurring_state')
          return { templates: [], reminders: [], generated_count: 0, has_more: false };
        if (cmd === 'privacy_set_hide_amounts') {
          hide = args.hidden;
          return { locked, lock_enabled: true, hide_amounts: hide, auto_lock_minutes: 5 };
        }
        if (cmd === 'privacy_delete_preview')
          return {
            id: 'delete-once',
            scope: args.scope,
            counts: [
              ['accounts', 1],
              ['transactions', 2],
            ],
            pending: 1,
            confirmed: 1,
            receipts: 2,
            blocked: 0,
          };
        if (cmd === 'privacy_delete_confirm')
          return { locked, lock_enabled: true, hide_amounts: hide, auto_lock_minutes: 5 };
        throw Error(cmd);
      },
    };
  });
  await page.goto(url);
}

test('locked startup never loads ledger; unlock and amount masking preserve source values', async () => {
  const page = await newPage();
  await open(page);
  await page.getByRole('heading', { name: '应用已锁定' }).waitFor();
  assert.equal(
    await page.evaluate(() => window.calls.some((c) => c.cmd === 'list_accounts')),
    false,
  );
  await page.getByLabel('解锁密码').fill('wrong-pass');
  await page.getByRole('button', { name: '解锁账本' }).click();
  assert.match(await page.getByRole('alert').innerText(), /密码错误/);
  await page.getByLabel('解锁密码').fill('test-only-pass');
  await page.getByRole('button', { name: '解锁账本' }).click();
  await page.getByText('123.45').first().waitFor();
  await navigate(page, 'privacy');
  await page.getByLabel('隐藏金额').check();
  await page.getByRole('button', { name: '打开导航' }).click();
  await page.getByRole('button', { name: '账本', exact: true }).click();
  await page.getByText('••••').first().waitFor();
  assert.equal(await page.getByText('123.45').count(), 0);
  await navigate(page, 'privacy');
  await page.getByLabel('隐藏金额').uncheck();
  await page.getByRole('button', { name: '打开导航' }).click();
  await page.getByRole('button', { name: '账本', exact: true }).click();
  await page.getByText('123.45').first().waitFor();
  await page.close();
});

test('notification deletion requires preview and exact confirmation phrase', async () => {
  const page = await newPage();
  await open(page);
  await page.getByLabel('解锁密码').fill('test-only-pass');
  await page.getByRole('button', { name: '解锁账本' }).click();
  await navigate(page, 'privacy');
  await page.getByRole('button', { name: '预览删除通知数据' }).click();
  assert.match(await page.getByText(/待确认或已忽略候选/).innerText(), /1/);
  const confirm = page.getByRole('button', { name: '确认删除通知数据' });
  assert.equal(await confirm.isDisabled(), true);
  await page.getByLabel('输入“删除通知数据”确认').fill('删除通知数据');
  await confirm.click();
  const call = await page.evaluate(() =>
    window.calls.find((c) => c.cmd === 'privacy_delete_confirm'),
  );
  assert.deepEqual(call.args, { previewId: 'delete-once', phrase: '删除通知数据' });
  await page.close();
});
