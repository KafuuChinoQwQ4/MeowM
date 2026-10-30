import assert from 'node:assert/strict';
import { test } from 'node:test';
import { browserSuite } from './helpers/browser.mjs';
import { installWorkflowIpc } from './helpers/workflow-ipc.mjs';
import { navigate } from './helpers/navigation.mjs';

const { url, newPage } = browserSuite(process.env.MULTI_CURRENCY_TEST_PORT || '1435');
const accounts = [
  { id: 'wallet', name: '钱包', category: 'wallet', currency_code: 'CNY', currency_exponent: 2 },
  { id: 'usd', name: '美元账户', category: 'wallet', currency_code: 'USD', currency_exponent: 2 },
  { id: 'usd2', name: '美元储蓄', category: 'bank', currency_code: 'USD', currency_exponent: 2 },
  { id: 'jpy', name: '日元账户', category: 'bank', currency_code: 'JPY', currency_exponent: 0 },
];

async function open(seed = {}) {
  const page = await newPage({ viewport: { width: 430, height: 820 } });
  await installWorkflowIpc(page, { accounts, ...seed });
  await page.goto(url);
  await page.locator('.balance-value').waitFor();
  return page;
}

test('new accounts expose the fixed currency choices and default to CNY', async () => {
  const page = await newPage({ viewport: { width: 430, height: 820 } });
  try {
    await installWorkflowIpc(page);
    await page.goto(url);
    await page.locator('.balance-value').waitFor();
    await navigate(page, 'platforms');
    await page.getByRole('button', { name: '＋ 新建平台' }).click();

    const currency = page.getByLabel('账户币种', { exact: true });
    assert.equal(await currency.count(), 1);
    assert.equal(await currency.inputValue(), 'CNY');
    const choices = await currency
      .locator('option')
      .evaluateAll((options) => options.map((option) => option.value));
    assert.deepEqual(choices, [
      'CNY',
      'USD',
      'EUR',
      'JPY',
      'HKD',
      'GBP',
      'KRW',
      'AUD',
      'CAD',
      'SGD',
      'CHF',
    ]);
    await currency.selectOption('USD');
    await page.getByLabel('平台名称', { exact: true }).fill('美元账户');
    await page.getByRole('button', { name: '创建平台', exact: true }).click();
    await page.locator('dialog').waitFor({ state: 'hidden' });
    const create = await page.evaluate(() =>
      window.workflowCalls.find((call) => call.command === 'create_account'),
    );
    assert.equal(create.args.currencyCode, 'USD');

    const row = page.locator('.platform-row').filter({ hasText: '美元账户' });
    assert.ok(await row.getByText('USD', { exact: true }).isVisible());
    await row.getByRole('button', { name: '编辑', exact: true }).click();
    assert.equal(await currency.isDisabled(), true);
    await page.getByRole('button', { name: '关闭', exact: true }).click();

    await page.getByRole('button', { name: '＋ 新建平台' }).click();
    await currency.selectOption('JPY');
    await page.getByLabel('平台名称', { exact: true }).fill('日元账户');
    await page.getByRole('button', { name: '创建平台', exact: true }).click();
    await page.locator('dialog').waitFor({ state: 'hidden' });
    const creates = await page.evaluate(() =>
      window.workflowCalls.filter((call) => call.command === 'create_account'),
    );
    assert.equal(creates[1].args.currencyCode, 'JPY');
  } finally {
    await page.close();
  }
});

test('USD entry uses minor-unit precision and previews the Rust FX quote', async () => {
  const page = await open();
  try {
    await page.getByRole('button', { name: '记一笔', exact: true }).click();
    await page.getByLabel('账户', { exact: true }).selectOption('usd');

    const amount = page.getByLabel('金额（USD）', { exact: true });
    assert.equal(await amount.count(), 1);
    await amount.fill('12.34');
    await page.getByLabel('商户', { exact: true }).fill('Coffee');
    await page.getByRole('button', { name: '查看影响并保存', exact: true }).click();

    await page.waitForFunction(() =>
      window.workflowCalls.some((call) => call.command === 'preview_foreign_transaction'),
    );
    const call = await page.evaluate(() =>
      window.workflowCalls.find((item) => item.command === 'preview_foreign_transaction'),
    );
    assert.equal(call.args.amount, '12.34');
    assert.equal(call.args.quoteCurrency, 'USD');
    assert.equal(call.args.date, await page.getByLabel('日期', { exact: true }).inputValue());
    assert.equal(await page.locator('.fx-preview').count(), 1);
    assert.match(await page.locator('.fx-preview').textContent(), /USD\s*12\.34/u);
    assert.match(await page.locator('.fx-preview').textContent(), /¥\s*9\.80/u);

    await page.getByRole('button', { name: '确认保存修改', exact: true }).click();
    await page.waitForFunction(() => window.savedTransactions.length === 1);
    const saved = await page.evaluate(() => window.savedTransactions[0]);
    assert.equal(saved.edit.amount, '12.34');
    assert.equal(saved.manual_fx_rate, null);
  } finally {
    await page.close();
  }
});

test('JPY rejects fractional amounts and a missing quote requires a manual rate', async () => {
  const page = await open();
  try {
    await page.getByRole('button', { name: '记一笔', exact: true }).click();
    await page.getByLabel('账户', { exact: true }).selectOption('jpy');
    const amount = page.getByLabel('金额（JPY）', { exact: true });
    assert.equal(await amount.count(), 1);
    await amount.fill('12.5');
    await page.getByLabel('商户', { exact: true }).fill('Tokyo cafe');
    await page.getByRole('button', { name: '查看影响并保存', exact: true }).click();
    assert.match(await page.locator('.form-error').textContent(), /最多.*0.*小数|整数/u);

    await page.getByLabel('账户', { exact: true }).selectOption('usd');
    await page.getByLabel('金额（USD）', { exact: true }).fill('1.00');
    await page.evaluate(() => (window.forceManualRate = true));
    await page.getByRole('button', { name: '查看影响并保存', exact: true }).click();
    const manualRate = page.getByLabel(/手动汇率/u);
    assert.equal(await manualRate.count(), 1);
    await manualRate.fill('7.12');
    await page.getByRole('button', { name: '查看影响并保存', exact: true }).click();
    await page.getByRole('button', { name: '确认保存修改', exact: true }).click();
    await page.waitForFunction(() => window.savedTransactions.length === 1);
    const saved = await page.evaluate(() => window.savedTransactions[0]);
    assert.equal(saved.manual_fx_rate, '7.12');
  } finally {
    await page.close();
  }
});

test('transfers allow matching currencies and reject mixed currencies', async () => {
  const page = await open();
  try {
    await page.getByRole('button', { name: '记一笔', exact: true }).click();
    await page.getByLabel('类型', { exact: true }).selectOption('transfer');
    await page.getByLabel('账户', { exact: true }).selectOption('usd');
    await page.getByLabel('转入账户（转账时必填）', { exact: true }).selectOption('usd2');
    await page.getByLabel('金额（USD）', { exact: true }).fill('25.00');
    await page.getByRole('button', { name: '查看影响并保存', exact: true }).click();
    await page.getByRole('button', { name: '确认保存修改', exact: true }).click();
    await page.waitForFunction(() => window.savedTransactions.length === 1);
    assert.equal((await page.evaluate(() => window.savedTransactions[0])).destination_id, 'usd2');

    await page.getByRole('button', { name: '记一笔', exact: true }).click();
    await page.getByLabel('类型', { exact: true }).selectOption('transfer');
    await page.getByLabel('账户', { exact: true }).selectOption('usd');
    await page.getByLabel('转入账户（转账时必填）', { exact: true }).selectOption('jpy');
    await page.getByLabel('金额（USD）', { exact: true }).fill('25.00');
    await page.getByRole('button', { name: '查看影响并保存', exact: true }).click();
    assert.match(await page.locator('.form-error').textContent(), /相同币种/u);
    assert.equal(await page.evaluate(() => window.savedTransactions.length), 1);
  } finally {
    await page.close();
  }
});

test('transaction history shows foreign amount and its CNY snapshot', async () => {
  const page = await open({
    transaction: {
      id: 'tx-1',
      kind: 'expense',
      status: 'confirmed',
      amount: '10.00',
      currency_code: 'USD',
      report_amount_cny: '14.08',
      fx_snapshot: {
        rate: '7.10',
        effective_date: '2026-10-29',
        source: 'reference',
        provider: 'test',
        provider_sources: ['fixture'],
      },
      date: '2026-10-30',
      time: '12:00:00',
      account_id: 'usd',
      destination_id: null,
      category_id: null,
      merchant: '商户',
      description: '境外消费',
      original_transaction_id: null,
      edit_version: 0,
      reversal_id: null,
      reversal_of: null,
      linked_ids: [],
      sources: [],
      history: [['created', 1760445000000000]],
    },
  });
  try {
    await page.locator('#account').selectOption('usd');
    await navigate(page, 'records');
    await page.locator('.entry').waitFor();
    await page.locator('.entry').click();
    const dialog = page.locator('dialog[open]');
    assert.match(await dialog.textContent(), /USD\s*10\.00/u);
    assert.match(await dialog.textContent(), /14\.08|¥\s*14\.08/u);
    assert.match(await dialog.textContent(), /7\.10/u);
    assert.match(await dialog.textContent(), /2026-10-29/u);

    await dialog.getByRole('button', { name: '编辑交易', exact: true }).click();
    await dialog.getByLabel('金额（USD）', { exact: true }).fill('11.00');
    await dialog.getByRole('button', { name: '查看影响并保存', exact: true }).click();
    const impactPreview = await dialog.locator('.impact-preview').textContent();
    assert.match(impactPreview, /USD 11\.00/u);
    assert.match(impactPreview, /¥ 9\.80/u);
    assert.doesNotMatch(impactPreview, /余额(增加|减少) ¥/u);
  } finally {
    await page.close();
  }
});

test('pending confirmation and report balances stay in their supported currencies', async () => {
  const page = await open({
    pending: [
      {
        id: 'pending-1',
        status: 'pending',
        kind: 'expense',
        occurred_at: '2026-10-14T12:00:00Z',
        amount: '12.30',
        currency_code: 'CNY',
        merchant: '待确认商户',
        description: '',
        edit_version: 0,
        reversal_of: null,
        reversal_id: null,
        account_id: 'wallet',
        destination_id: null,
        category_id: null,
        original_transaction_id: null,
        refund_matches: [],
        sources: ['bank'],
        external_ids: [],
      },
    ],
  });
  try {
    await navigate(page, 'pending');
    await page.getByRole('button', { name: '编辑 / 确认', exact: true }).click();
    const account = page.getByLabel('记入平台', { exact: true });
    const values = await account
      .locator('option')
      .evaluateAll((options) => options.map((option) => option.value));
    assert.ok(values.includes('wallet'));
    assert.ok(!values.includes('usd'));
    assert.ok(!values.includes('jpy'));
    await page.getByRole('button', { name: '关闭', exact: true }).click();

    await navigate(page, 'reports');
    const balances = page.locator('.analytics-section').filter({
      has: page.getByRole('heading', { name: '期末平台余额', exact: true }),
    });
    assert.ok(await balances.getByText('¥ 100.00', { exact: true }).isVisible());
    assert.ok(await balances.getByText('USD 20.00', { exact: true }).isVisible());
  } finally {
    await page.close();
  }
});

test('daily balance and reference input use the account currency precision', async () => {
  const page = await open();
  try {
    await page.locator('#account').selectOption('jpy');
    await page.getByRole('button', { name: '设置参考余额', exact: true }).click();
    const amount = page.getByLabel('当日日末余额（JPY）', { exact: true });
    assert.equal(await amount.count(), 1);
    await amount.fill('100.5');
    await page.getByRole('button', { name: '保存参考余额', exact: true }).click();
    assert.match(await page.locator('.form-error').textContent(), /0 位小数/u);
    await amount.fill('100');
    await page.getByRole('button', { name: '保存参考余额', exact: true }).click();
    await page.waitForFunction(() =>
      window.workflowCalls.some((call) => call.command === 'set_reference'),
    );
    const call = await page.evaluate(() =>
      window.workflowCalls.find((item) => item.command === 'set_reference'),
    );
    assert.equal(call.args.amount, '100');
  } finally {
    await page.close();
  }
});
