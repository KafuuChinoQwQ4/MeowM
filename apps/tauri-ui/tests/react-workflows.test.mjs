import assert from 'node:assert/strict';
import { test } from 'node:test';
import { browserSuite } from './helpers/browser.mjs';
import { installWorkflowIpc } from './helpers/workflow-ipc.mjs';
import { navigate } from './helpers/navigation.mjs';
const { url, newPage } = browserSuite(process.env.REACT_WORKFLOW_TEST_PORT || '1428');
async function open() {
  const page = await newPage({ viewport: { width: 430, height: 820 } });
  await installWorkflowIpc(page);
  await page.goto(url);
  await page.locator('.balance-value').waitFor();
  return page;
}
test('ledger dates stop at the local current day and advance after midnight', async () => {
  const page = await newPage({ timezoneId: 'Asia/Shanghai' });
  try {
    await page.clock.setFixedTime(new Date('2026-10-26T17:00:00Z'));
    await installWorkflowIpc(page);
    await page.goto(url);
    await page.locator('.balance-value').waitFor();
    const date = page.locator('#date');
    const next = page.getByRole('button', { name: '后一天', exact: true });
    assert.equal(await date.inputValue(), '2026-10-27');
    assert.equal(await next.isDisabled(), true);
    assert.equal(await date.getAttribute('max'), '2026-10-27');
    await next.evaluate((button) => button.click());
    await date.fill('2026-10-28');
    assert.equal(await date.inputValue(), '2026-10-27');
    assert.equal(
      await page.evaluate(() =>
        window.workflowCalls.some((c) => c.command === 'get_day' && c.args.date > '2026-10-27'),
      ),
      false,
    );
    await page.getByRole('button', { name: '前一天', exact: true }).click();
    assert.equal(await date.inputValue(), '2026-10-26');
    assert.equal(await next.isDisabled(), false);
    await next.click();
    assert.equal(await date.inputValue(), '2026-10-27');
    assert.equal(await next.isDisabled(), true);
    await date.fill('2026-10-01');
    await page.getByRole('button', { name: '回到今天', exact: true }).click();
    assert.equal(await date.inputValue(), '2026-10-27');
    await page.clock.setFixedTime(new Date('2026-10-27T17:00:00Z'));
    await page.evaluate(() => window.dispatchEvent(new Event('focus')));
    await page.waitForFunction(() => document.querySelector('#date').max === '2026-10-28');
    assert.equal(await next.isDisabled(), false);
    await next.click();
    assert.equal(await date.inputValue(), '2026-10-28');
    assert.equal(await next.isDisabled(), true);
  } finally {
    await page.close();
  }
});

test('navigation directories return to their parent without crossing navigation roots', async () => {
  const page = await open();
  try {
    const back = page.getByRole('button', { name: '返回上一级', exact: true });
    await navigate(page, 'platforms');
    await page.getByRole('heading', { name: '平台管理', exact: true }).waitFor();
    await back.click();
    await page.getByRole('heading', { name: '平台与采集', exact: true }).waitFor();
    await back.click();
    await page.getByRole('heading', { name: '设置', exact: true }).waitFor();
    assert.equal(await back.isDisabled(), true);
    await page.locator('#nav-security').click();
    await page.locator('#nav-privacy').click();
    await page.getByRole('heading', { name: '隐私与数据', exact: true }).waitFor();
    await back.click();
    await page.getByRole('heading', { name: '安全与数据', exact: true }).waitFor();
    // Switching roots must discard the previous directory's parent chain.
    for (const view of ['finance', 'records']) {
      await navigate(page, view);
      assert.equal(await back.isDisabled(), true);
      await back.evaluate((button) => button.click());
      assert.equal(await page.locator('#nav-' + view).getAttribute('class'), 'nav-item active');
    }
    await page.getByRole('button', { name: '记一笔' }).click();
    await page.getByLabel('备注', { exact: true }).waitFor();
    await page.getByRole('button', { name: '关闭', exact: true }).click();
    await page.locator('dialog').waitFor({ state: 'hidden' });
    assert.equal(await page.locator('#nav-records').getAttribute('class'), 'nav-item active');
  } finally {
    await page.close();
  }
});

test('all-platform totals follow the date and keep entry creation on a real platform', async () => {
  const page = await open();
  try {
    await page.evaluate(() => {
      const invoke = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        if (command === 'get_day' && args.accountId === null) {
          window.workflowCalls.push({ command, args });
          return {
            date: args.date,
            income: '30.30',
            expense: '12.10',
            net: '18.20',
            balance: null,
            reference: null,
            entries: [],
          };
        }
        return invoke(command, args);
      };
    });
    assert.ok((await page.locator('#account option').allTextContents()).includes('总和'));
    await page.getByLabel('当前平台').selectOption({ label: '总和' });
    await page.waitForFunction(
      () =>
        document.querySelector('.summary-card.income .summary-amount')?.textContent === '¥30.30',
    );
    assert.equal(
      await page.locator('.summary-card.expense .summary-amount').textContent(),
      '¥12.10',
    );
    assert.equal(await page.locator('.summary-card.net .summary-amount').textContent(), '¥18.20');
    assert.equal(await page.locator('.balance-panel').count(), 0);
    assert.match(await page.locator('.dock-context').textContent(), /^总和 · /);
    const previousDate = await page.locator('#date').inputValue();
    await page.getByRole('button', { name: '前一天', exact: true }).click();
    await page.waitForFunction(
      (date) =>
        window.workflowCalls.some(
          (c) => c.command === 'get_day' && c.args.accountId === null && c.args.date !== date,
        ),
      previousDate,
    );
    await page.getByRole('button', { name: '记一笔' }).click();
    assert.equal(await page.getByLabel('账户', { exact: true }).inputValue(), 'wallet');
    await page.getByRole('button', { name: '关闭', exact: true }).click();
    await page.getByLabel('当前平台').selectOption('bank');
    await page.locator('.balance-value').waitFor();
    assert.equal(
      await page.locator('.summary-card.expense .summary-amount').textContent(),
      '¥10.00',
    );
  } finally {
    await page.close();
  }
});

test('platform name suggestions filter aliases, select brands and keep custom names editable', async () => {
  const page = await open();
  try {
    await navigate(page, 'platforms');
    await page.getByRole('button', { name: '＋ 新建平台' }).click();
    const name = page.getByRole('combobox', { name: '平台名称', exact: true });
    await name.fill('biNAN');
    assert.equal(await page.getByRole('option', { name: /币安/ }).count(), 1);
    assert.equal(await page.getByRole('option', { name: /微信/ }).count(), 0);
    await page.getByRole('option', { name: /币安/ }).click();
    assert.equal(await name.inputValue(), '币安');
    assert.equal(
      await page.locator('dialog').getByLabel('平台分类', { exact: true }).inputValue(),
      'exchange',
    );
    assert.equal(await page.getByRole('listbox', { name: '平台候选' }).count(), 0);
    await name.fill('贝宝');
    await name.press('ArrowDown');
    await name.press('Enter');
    assert.equal(await name.inputValue(), 'PayPal');
    assert.equal(
      await page.locator('dialog').getByLabel('平台分类', { exact: true }).inputValue(),
      'wallet',
    );
    await page.getByRole('button', { name: '展开平台列表', exact: true }).click();
    assert.ok(await page.getByRole('option', { name: /微信/ }).isVisible());
    await name.press('Escape');
    assert.equal(await page.locator('dialog[open]').count(), 1);
    assert.equal(await page.getByRole('listbox', { name: '平台候选' }).count(), 0);
    await name.fill('小众测试App');
    await page.getByRole('option', { name: '使用“小众测试App”', exact: true }).click();
    assert.equal(await name.inputValue(), '小众测试App');
    await page.getByRole('button', { name: '创建平台', exact: true }).click();
    await page.locator('dialog').waitFor({ state: 'hidden' });
    let row = page.locator('.platform-row').filter({ hasText: '小众测试App' });
    assert.equal(await row.locator('.platform-badge').getAttribute('data-platform'), 'default');
    await row.getByRole('button', { name: '编辑', exact: true }).click();
    await name.fill('欧');
    await page.getByRole('option', { name: /欧易/ }).click();
    await page.getByRole('button', { name: '保存平台', exact: true }).click();
    await page.locator('dialog').waitFor({ state: 'hidden' });
    row = page.locator('.platform-row').filter({ hasText: '欧易' });
    assert.equal(await row.locator('.platform-badge').getAttribute('data-platform'), 'okx');
  } finally {
    await page.close();
  }
});

test('brand colors preview live and follow platform creation, selection and renaming', async () => {
  const page = await open();
  try {
    await navigate(page, 'platforms');
    await page.getByRole('button', { name: '＋ 新建平台' }).click();
    for (const [name, theme] of [
      ['币安现货', 'binance'],
      ['OKEx 资金', 'okx'],
      ['同花顺', 'ths'],
      ['ＰａｙＰａｌ 美元', 'paypal'],
      ['微信钱包', 'wechat'],
      ['支付宝余额', 'alipay'],
      ['Wise EUR', 'wise'],
      ['未知小众钱包', 'default'],
      ['现金', 'cash'],
    ]) {
      await page.getByLabel('平台名称').fill(name);
      assert.equal(
        await page.locator('.platform-color-preview .platform-badge').getAttribute('data-platform'),
        theme,
      );
    }
    await page.getByLabel('平台名称').fill('币安现货');
    await page.getByRole('button', { name: '创建平台', exact: true }).click();
    await page.locator('dialog').waitFor({ state: 'hidden' });
    let row = page.locator('.platform-row').filter({ hasText: '币安现货' });
    assert.equal(await row.locator('.platform-badge').getAttribute('data-platform'), 'binance');
    await row.getByRole('button', { name: '进入', exact: true }).click();
    assert.equal(
      await page.locator('.account-select-wrap').getAttribute('data-platform'),
      'binance',
    );
    assert.deepEqual(
      await page.locator('.account-select-wrap').evaluate((el) => {
        const style = getComputedStyle(el);
        return [style.backgroundColor, style.color, el.getBoundingClientRect().height];
      }),
      ['rgb(24, 26, 32)', 'rgb(240, 185, 11)', 36],
    );
    await navigate(page, 'platforms');
    row = page.locator('.platform-row').filter({ hasText: '币安现货' });
    await row.getByRole('button', { name: '编辑', exact: true }).click();
    await page.getByLabel('平台名称').fill('PayPal 美元');
    await page.getByRole('button', { name: '保存平台', exact: true }).click();
    await page.locator('dialog').waitFor({ state: 'hidden' });
    row = page.locator('.platform-row').filter({ hasText: 'PayPal 美元' });
    assert.equal(await row.locator('.platform-badge').getAttribute('data-platform'), 'paypal');
    await row.getByRole('button', { name: '进入', exact: true }).click();
    assert.equal(
      await page.locator('.account-select-wrap').getAttribute('data-platform'),
      'paypal',
    );
    await page.getByLabel('当前平台').selectOption('wallet');
    assert.equal(
      await page
        .locator('.account-select-wrap')
        .evaluate((el) => getComputedStyle(el).backgroundColor),
      'rgb(255, 255, 255)',
    );
    await page.getByLabel('当前平台').selectOption({ label: '总和' });
    assert.match(
      await page
        .locator('.account-select-wrap')
        .evaluate((el) => getComputedStyle(el).backgroundImage),
      /linear-gradient/,
    );
  } finally {
    await page.close();
  }
});

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
    await page.locator('#previous').click();
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
