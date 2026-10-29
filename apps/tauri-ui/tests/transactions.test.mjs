import assert from 'node:assert/strict';
import { test } from 'node:test';
import { browserSuite } from './helpers/browser.mjs';
import { installWorkflowIpc } from './helpers/workflow-ipc.mjs';
const { url, newPage } = browserSuite(process.env.TRANSACTION_UI_TEST_PORT || '1429');

test('manual internal transfer can be saved to the same platform', async () => {
  const page = await newPage();
  try {
    await installWorkflowIpc(page);
    await page.addInitScript(() => {
      const original = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        if (command === 'write_transaction') {
          window.workflowCalls.push({ command, args });
          return 'internal-transfer';
        }
        return original(command, args);
      };
    });
    await page.goto(url);
    await page.getByRole('button', { name: '记一笔', exact: true }).click();
    await page.getByLabel('类型', { exact: true }).selectOption('transfer');
    await page.getByLabel('金额（元）').fill('1.22');
    await page.getByLabel('转入账户（转账时必填）').selectOption('wallet');
    await page.getByRole('button', { name: '查看影响并保存' }).click();
    await page.getByText(/平台内部划转，平台总余额和收支不变/).waitFor();
    await page.getByRole('button', { name: '确认保存修改' }).click();
    await page.waitForFunction(() =>
      window.workflowCalls.some((call) => call.command === 'write_transaction'),
    );
    const { request } = await page.evaluate(
      () => window.workflowCalls.find((call) => call.command === 'write_transaction').args,
    );
    assert.equal(request.edit.account_id, 'wallet');
    assert.equal(request.destination_id, 'wallet');
  } finally {
    await page.close();
  }
});

test('manual entry accepts an empty note and sends separate optional order IDs with leading zeroes', async () => {
  const page = await newPage({ viewport: { width: 390, height: 780 } });
  try {
    await installWorkflowIpc(page);
    await page.addInitScript(() => {
      const original = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        if (command === 'write_transaction') {
          window.workflowCalls.push({ command, args });
          return 'manual-order';
        }
        return original(command, args);
      };
    });
    await page.goto(url);
    await page.getByRole('button', { name: '记一笔', exact: true }).click();
    await page.getByLabel('金额（元）').fill('6.64');
    await page.getByLabel('商户', { exact: true }).fill('饭小侠');
    await page.getByLabel('交易时间（可选）').fill('17:26:28');
    await page.getByText('交易单号（可选，用于去重）', { exact: true }).click();
    await page.getByLabel('渠道交易单号', { exact: true }).fill('00001234');
    await page.getByLabel('商户单号', { exact: true }).fill('00005678');
    await page.getByRole('button', { name: '查看影响并保存' }).click();
    await page.getByRole('alert').getByText('填写单号时请选择支付渠道。').waitFor();
    await page.getByLabel('支付渠道').selectOption('wechat');
    await page.getByRole('button', { name: '查看影响并保存' }).click();
    await page.getByRole('button', { name: '确认保存修改' }).click();
    await page.waitForFunction(() =>
      window.workflowCalls.some((call) => call.command === 'write_transaction'),
    );
    const { request } = await page.evaluate(
      () => window.workflowCalls.find((call) => call.command === 'write_transaction').args,
    );
    assert.equal(request.edit.description, '');
    assert.equal(request.edit.merchant, '饭小侠');
    assert.equal(request.time, '17:26:28');
    assert.equal(request.source, 'wechat');
    assert.equal(request.transaction_number, '00001234');
    assert.equal(request.merchant_order_id, '00005678');
  } finally {
    await page.close();
  }
});

test('transaction detail exposes audit and saves an optimistic edit through Tauri IPC', async () => {
  const page = await newPage({ viewport: { width: 390, height: 780 } });
  await page.addInitScript(() => {
    const account = '00000000-0000-4000-8000-000000000001';
    window.ledgerCalls = [];
    window.__TAURI_INTERNALS__ = {
      invoke: async (command, args) => {
        if (command === 'privacy_status')
          return { locked: false, lock_enabled: false, hide_amounts: false, auto_lock_minutes: 5 };
        window.ledgerCalls.push({ command, args });
        if (command === 'recurring_state')
          return { templates: [], reminders: [], generated_count: 0, has_more: false };
        if (command === 'list_accounts') return [{ id: account, name: '钱包', category: 'wallet' }];
        if (command === 'list_categories') return [{ id: 'food', name: '餐饮' }];
        if (command === 'get_day')
          return {
            date: args.date,
            income: '0.00',
            expense: '24.50',
            net: '-24.50',
            balance: '75.50',
            reference: null,
            entries: [
              { id: 'tx-1', kind: 'expense', amount: '-24.50', note: '面包店', time: '12:30' },
            ],
          };
        if (command === 'transaction_detail')
          return {
            id: 'tx-1',
            kind: 'expense',
            status: 'confirmed',
            amount: '24.50',
            date: '2026-10-14',
            account_id: account,
            destination_id: null,
            merchant: '面包店',
            description: '午餐',
            category_id: 'food',
            original_transaction_id: null,
            edit_version: 3,
            reversal_id: null,
            reversal_of: null,
            linked_ids: ['tx-refund'],
            sources: [{ source: 'manual', digest: 'abc' }],
            history: [['created', 1760445000000000]],
          };
        if (command === 'write_transaction') return 'tx-1';
        throw new Error(`Unexpected IPC command: ${command}`);
      },
    };
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
    assert.match(
      await page.locator('dialog[open] .impact-preview').innerText(),
      /余额减少 ¥ 5\.00，报表支出增加 ¥ 5\.00/,
    );
    await page.getByLabel('金额（元）').fill('30.00');
    await page.getByRole('button', { name: '确认保存修改' }).click();
    await page.getByRole('alert').getByText('内容已修改，请重新查看影响。').waitFor();
    assert.equal(
      await page.evaluate(() =>
        window.ledgerCalls.some((item) => item.command === 'write_transaction'),
      ),
      false,
    );
    await page.getByRole('button', { name: '查看影响并保存' }).click();
    assert.equal(await page.locator('dialog[open] .impact-preview').count(), 1);
    await page.getByRole('button', { name: '确认保存修改' }).click();
    const call = await page.evaluate(() =>
      window.ledgerCalls.find((item) => item.command === 'write_transaction'),
    );
    assert.equal(call.args.request.edit.expected_version, 3);
    assert.equal(call.args.request.edit.amount, '30.00');
  } finally {
    await page.close();
  }
});
