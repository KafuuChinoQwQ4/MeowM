import assert from 'node:assert/strict';
import { test } from 'node:test';
import { browserSuite } from './helpers/browser.mjs';
import { installWorkflowIpc } from './helpers/workflow-ipc.mjs';
const { url, newPage } = browserSuite(process.env.REACT_WORKFLOW_TEST_PORT || '1428');
async function open() {
  const page = await newPage({ viewport: { width: 430, height: 820 } });
  await installWorkflowIpc(page);
  await page.goto(url);
  await page.locator('.balance-value').waitFor();
  return page;
}
async function navigate(page, view) {
  await page.locator('#menu').click();
  await page.locator('#nav-' + view).click();
}

test('platform creation and editing preserve selection and reference balances', async () => {
  const page = await open();
  try {
    await navigate(page, 'platforms');
    await page.getByRole('button', { name: '＋ 新建平台' }).click();
    await page.getByLabel('平台名称').fill('测试现金');
    await page.locator('dialog').getByLabel('平台分类', { exact: true }).selectOption('cash');
    await page.getByRole('button', { name: '创建平台', exact: true }).click();
    await page.locator('dialog').waitFor({ state: 'hidden' });
    const row = page.locator('.platform-row').filter({ hasText: '测试现金' });
    await row.getByRole('button', { name: '编辑', exact: true }).click();
    await page.getByLabel('平台名称').fill('现金钱包');
    await page.getByRole('button', { name: '保存平台' }).click();
    await page.locator('dialog').waitFor({ state: 'hidden' });
    await page
      .locator('.platform-row')
      .filter({ hasText: '现金钱包' })
      .getByRole('button', { name: '进入', exact: true })
      .click();
    assert.equal(await page.getByLabel('当前平台').inputValue(), 'created');
    await page.getByRole('button', { name: '设置参考余额' }).click();
    await page.getByLabel('当日日末余额（元）').fill('-12.34');
    await page.getByRole('button', { name: '保存参考余额' }).click();
    await page.waitForFunction(
      () => document.querySelector('.balance-value')?.textContent === '¥ -12.34',
    );
    await page.getByRole('button', { name: '清除', exact: true }).click();
    await page.getByRole('button', { name: '确认清除' }).click();
    await page.getByRole('button', { name: '设置参考余额' }).waitFor();
    const calls = await page.evaluate(() => window.workflowCalls);
    assert.equal(calls.find((c) => c.command === 'update_platform').args.accountId, 'created');
    assert.equal(calls.find((c) => c.command === 'set_reference').args.amount, '-12.34');
  } finally {
    await page.close();
  }
});

test('bill preview retains revisions, duplicate selection rules and selected account', async () => {
  const page = await open();
  try {
    await navigate(page, 'pending');
    await page.getByRole('button', { name: '导入账单', exact: true }).click();
    await page.locator('dialog').getByLabel('来源', { exact: true }).selectOption('bank');
    await page.locator('input[type=file]').setInputFiles({
      name: 'bank.csv',
      mimeType: 'text/csv',
      buffer: Buffer.from('日期,金额\n2026-10-14,-12.30'),
    });
    await page.getByRole('button', { name: '加入待确认收件箱' }).waitFor();
    const rows = page.locator('dialog article');
    assert.equal(await rows.nth(0).getByRole('checkbox').isChecked(), true);
    assert.equal(await rows.nth(1).getByRole('checkbox').isDisabled(), true);
    assert.equal(await rows.nth(2).getByRole('checkbox').isChecked(), false);
    await rows.nth(2).getByRole('checkbox').check();
    await rows.nth(0).getByText('修订金额、日期、商户或备注').click();
    await rows.nth(0).getByLabel('金额（元）').fill('15.67');
    await rows.nth(0).getByLabel('商户', { exact: true }).fill('<修订商户>');
    await page.getByLabel('记入平台').selectOption('bank');
    await page.getByRole('button', { name: '加入待确认收件箱' }).click();
    await page.locator('dialog').waitFor({ state: 'hidden' });
    await page.getByText('<修订商户>', { exact: true }).waitFor();
    const calls = await page.evaluate(() => window.workflowCalls);
    const stage = calls.find((c) => c.command === 'stage_bill').args;
    assert.deepEqual(stage.ids, ['new', 'suspected']);
    assert.equal(stage.accountId, 'bank');
    assert.equal(stage.edits[0].account_id, 'bank');
    assert.equal(stage.edits[0].amount, '15.67');
    assert.equal(calls.find((c) => c.command === 'preview_bill').args.provider, 'bank');
  } finally {
    await page.close();
  }
});

test('dialog ignores Escape and browser back during save and prevents repeat submission', async () => {
  const page = await open();
  try {
    await navigate(page, 'platforms');
    await page.evaluate(() => {
      window.holdSave = true;
    });
    await page.getByRole('button', { name: '＋ 新建平台' }).click();
    await page.getByLabel('平台名称').fill('保存中的平台');
    await page.getByRole('button', { name: '创建平台', exact: true }).click();
    await page.waitForFunction(() => !!window.releaseSave);
    await page.keyboard.press('Escape');
    await page.evaluate(() => history.back());
    await page.waitForFunction(() => history.state?.meowmSheet === true);
    assert.equal(await page.locator('dialog').isVisible(), true);
    assert.equal(await page.getByRole('button', { name: '关闭', exact: true }).isDisabled(), true);
    assert.equal(
      await page.evaluate(
        () => window.workflowCalls.filter((c) => c.command === 'create_account').length,
      ),
      1,
    );
    await page.evaluate(() => window.releaseSave());
    await page.locator('dialog').waitFor({ state: 'hidden' });
    await page.getByRole('button', { name: '＋ 新建平台' }).click();
    await page.keyboard.press('Escape');
    await page.locator('dialog').waitFor({ state: 'hidden' });
    assert.equal(
      await page
        .getByRole('button', { name: '＋ 新建平台' })
        .evaluate((el) => el === document.activeElement),
      true,
    );
  } finally {
    await page.close();
  }
});

test('reversal refreshes totals and exposes the linked reversal without edit actions', async () => {
  const page = await open();
  try {
    await navigate(page, 'records');
    await page.getByRole('button', { name: /原消费/ }).click();
    page.on('dialog', (dialog) =>
      dialog.accept(dialog.type() === 'prompt' ? '重复记账' : undefined),
    );
    await page.getByRole('button', { name: '撤销交易', exact: true }).click();
    await page.locator('dialog').waitFor({ state: 'hidden' });
    await navigate(page, 'ledger');
    await page.waitForFunction(() =>
      document.querySelector('.summary-card.expense')?.textContent.includes('0.00'),
    );
    await navigate(page, 'records');
    await page.getByRole('button', { name: /原消费/ }).click();
    await page.getByRole('button', { name: '打开关联记录 reverse-' }).click();
    await page.getByText('此记录不可编辑。').waitFor();
    assert.equal(await page.getByRole('button', { name: '编辑交易', exact: true }).count(), 0);
    const call = await page.evaluate(() =>
      window.workflowCalls.find((c) => c.command === 'reverse_transaction'),
    );
    assert.equal(call.args.reason, '重复记账');
  } finally {
    await page.close();
  }
});

test('an old account response cannot overwrite the newly selected account', async () => {
  const page = await open();
  try {
    await page.evaluate(() => {
      window.delayWallet = true;
    });
    await page.locator('#next').click();
    await page.waitForFunction(
      () => window.workflowCalls.filter((c) => c.command === 'get_day').length >= 2,
    );
    await page.getByLabel('当前平台').selectOption('bank');
    await page.waitForFunction(
      () => document.querySelector('.balance-value')?.textContent === '¥ 200.00',
    );
    await page.waitForFunction(() => window.oldDayFinished);
    assert.equal(await page.locator('.balance-value').textContent(), '¥ 200.00');
  } finally {
    await page.close();
  }
});
