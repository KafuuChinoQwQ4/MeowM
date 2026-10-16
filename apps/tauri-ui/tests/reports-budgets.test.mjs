import assert from 'node:assert/strict';
import { test } from 'node:test';
import { browserSuite } from './helpers/browser.mjs';
const { url, newPage } = browserSuite('1432');
async function install(page) {
  await page.addInitScript(() => {
    let budgets = [];
    window.analyticsCalls = [];
    window.__TAURI_INTERNALS__ = {
      invoke: async (command, args) => {
        if (command === 'privacy_status')
          return { locked: false, lock_enabled: false, hide_amounts: false, auto_lock_minutes: 5 };
        window.analyticsCalls.push({ command, args });
        if (command === 'recurring_state')
          return { templates: [], reminders: [], generated_count: 0, has_more: false };
        if (command === 'list_accounts')
          return [{ id: 'wallet', name: '钱包', category: 'wallet' }];
        if (command === 'list_categories') return [{ id: 'food', name: '餐饮' }];
        if (command === 'get_day')
          return {
            date: args.date,
            income: '0.00',
            expense: '0.00',
            net: '0.00',
            balance: null,
            reference: null,
            entries: [],
          };
        if (command === 'report_summary')
          return {
            income: '100.00',
            expense: '12.30',
            net: '87.70',
            transaction_count: 2,
            categories: [{ id: 'food', name: '餐饮', expense: '12.30', share_percent: 100 }],
            merchants: [
              {
                name: args.query.merchant_offset ? '商户二' : '面包店',
                income: '0.00',
                expense: '12.30',
              },
            ],
            merchant_total: 11,
            trend: [{ month: '2026-10', income: '100.00', expense: '12.30' }],
            balances: [{ account_id: 'wallet', name: '钱包', amount: '87.70' }],
          };
        if (command === 'list_budgets') return budgets;
        if (command === 'budget_reminders')
          return budgets
            .filter((b) => b.enabled)
            .map((b) => ({
              id: 'reminder-' + b.id,
              budget_id: b.id,
              level: 'exceeded',
              spent: b.spent,
              amount: b.amount,
            }));
        if (command === 'write_budget') {
          if (window.rejectBudget) throw Error('同一周期及范围已存在预算');
          const request = args.request;
          const b = {
            ...request,
            id: request.id ?? 'budget-1',
            category_name: '餐饮',
            spent: '12.30',
            remaining: '-2.30',
            percent: '123',
            exceeded: true,
          };
          budgets = budgets.filter((x) => x.id !== b.id).concat(b);
          return b.id;
        }
        if (command === 'archive_budget') {
          budgets = budgets.map((b) => (b.id === args.id ? { ...b, enabled: false } : b));
          return;
        }
        throw Error(command);
      },
    };
  });
  await page.goto(url);
}
async function nav(page, view) {
  await page.locator('#menu').click();
  await page.locator('#nav-' + view).click();
}
test('reports expose aggregates and preserve filters across merchant pages', async () => {
  const page = await newPage({ viewport: { width: 390, height: 844 } });
  try {
    await install(page);
    await nav(page, 'reports');
    await page.getByRole('heading', { name: '报表与预算总览' }).waitFor();
    await page.getByText('面包店', { exact: true }).waitFor();
    assert.match(await page.locator('.report-summary').innerText(), /12\.30/);
    await page.getByLabel('报表平台').selectOption('wallet');
    await page.getByLabel('报表分类').selectOption('food');
    await page.getByRole('button', { name: '下一页', exact: true }).click();
    await page.getByText('商户二', { exact: true }).waitFor();
    const call = await page.evaluate(() =>
      window.analyticsCalls.filter((c) => c.command === 'report_summary').at(-1),
    );
    assert.equal(call.args.query.account_id, 'wallet');
    assert.equal(call.args.query.category_id, 'food');
    assert.equal(call.args.query.merchant_offset, 10);
    await page.getByLabel('报表周期').selectOption('custom');
    await page.getByLabel('开始日期').fill('2026-10-10');
    await page.getByLabel('结束日期（不含）').fill('2026-10-20');
    await page.waitForFunction(() =>
      window.analyticsCalls.some(
        (c) =>
          c.command === 'report_summary' &&
          c.args.query.from_date === '2026-10-10' &&
          c.args.query.until_date === '2026-10-20',
      ),
    );
    assert.equal(
      await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth),
      false,
    );
  } finally {
    await page.close();
  }
});
test('budget create edit archive refresh progress and reminders; failures keep form open', async () => {
  const page = await newPage({ viewport: { width: 390, height: 844 } });
  try {
    await install(page);
    await nav(page, 'budgets');
    await page.getByRole('button', { name: '新增预算' }).click();
    await page.getByLabel('预算分类').selectOption('food');
    await page.getByLabel('预算额度（元）').fill('10.00');
    await page.evaluate(() => (window.rejectBudget = true));
    await page.getByRole('button', { name: '保存预算', exact: true }).click();
    await page.getByRole('alert').getByText('同一周期及范围已存在预算').waitFor();
    assert.equal(await page.locator('dialog[open]').count(), 1);
    await page.evaluate(() => (window.rejectBudget = false));
    await page.getByRole('button', { name: '保存预算', exact: true }).click();
    await page.locator('dialog[open]').waitFor({ state: 'hidden' });
    await page.getByText('已超支', { exact: true }).waitFor();
    assert.match(await page.locator('.budget-card').innerText(), /12\.30/);
    await page.getByRole('button', { name: '编辑预算', exact: true }).click();
    await page.getByLabel('预算额度（元）').fill('20.00');
    await page.getByRole('button', { name: '保存预算', exact: true }).click();
    await page.locator('dialog[open]').waitFor({ state: 'hidden' });
    assert.match(await page.locator('.budget-card').innerText(), /20\.00/);
    await nav(page, 'reports');
    await page.getByText('预算已超支', { exact: false }).waitFor();
    await nav(page, 'budgets');
    page.once('dialog', (dialog) => dialog.accept());
    await page.getByRole('button', { name: '停用预算', exact: true }).click();
    await page.getByText('已停用', { exact: true }).waitFor();
    await nav(page, 'reports');
    await page.getByRole('heading', { name: '报表与预算总览' }).waitFor();
    await page.waitForFunction(() => !document.querySelector('.budget-reminder'));
  } finally {
    await page.close();
  }
});
