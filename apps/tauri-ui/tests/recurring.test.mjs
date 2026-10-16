import assert from 'node:assert/strict';
import { test } from 'node:test';
import { browserSuite } from './helpers/browser.mjs';
const { url, newPage } = browserSuite('1434');
async function install(page) {
  await page.addInitScript(() => {
    window.recurringCalls = [];
    const read = () => JSON.parse(localStorage.getItem('recurringFixture') ?? '[]');
    const save = (templates) => localStorage.setItem('recurringFixture', JSON.stringify(templates));
    window.__TAURI_INTERNALS__ = {
      invoke: async (command, args) => {
        if (command === 'privacy_status')
          return { locked: false, lock_enabled: false, hide_amounts: false, auto_lock_minutes: 5 };
        window.recurringCalls.push({ command, args });
        if (command === 'list_accounts')
          return [{ id: 'wallet', name: '钱包', category: 'wallet' }];
        if (command === 'list_categories') return [{ id: 'rent', name: '居住' }];
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
        if (command === 'recurring_state')
          return {
            templates: read(),
            reminders: read()
              .filter((t) => t.enabled)
              .map((t) => ({
                template_id: t.id,
                due_date: t.next_due_date,
                transaction_id: 'candidate',
                account_id: t.account_id,
                amount: '12.34',
                merchant: t.merchant,
                description: t.description,
                overdue: true,
              })),
            generated_count: 0,
            has_more: false,
          };
        if (command === 'write_recurring') {
          if (window.rejectRecurring) throw Error('记录已变化，请刷新后重试');
          const t = {
            ...args.request,
            id: args.request.id ?? 'template-1',
            version: 1,
            ended: false,
            category_name: '居住',
          };
          save(
            read()
              .filter((x) => x.id !== t.id)
              .concat(t),
          );
          return t.id;
        }
        if (command === 'set_recurring_enabled') {
          save(
            read().map((t) =>
              t.id === args.id ? { ...t, enabled: args.enabled, version: t.version + 1 } : t,
            ),
          );
          return;
        }
        if (command === 'skip_recurring') {
          save(
            read().map((t) =>
              t.id === args.id ? { ...t, next_due_date: '2026-03-31', version: t.version + 1 } : t,
            ),
          );
          return;
        }
        if (command === 'recurring_instances')
          return [
            {
              due_date: '2026-01-31',
              transaction_id: 'candidate',
              status: 'pending',
              amount: '12.34',
            },
            { due_date: '2026-02-28', transaction_id: null, status: 'skipped', amount: null },
          ];
        if (command === 'query_entries') return { items: [], total: 0, offset: 0, limit: 20 };
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
test('recurring create edit pause resume skip and persisted history; conflicts keep draft', async () => {
  const page = await newPage({ viewport: { width: 390, height: 844 } });
  try {
    await install(page);
    await nav(page, 'recurring');
    await page.getByRole('button', { name: '新增周期账单', exact: true }).click();
    await page.getByLabel('周期金额（元）').fill('12.34');
    await page.getByLabel('周期商户').fill('房租');
    await page.getByLabel('周期备注').fill('固定开销');
    await page.getByLabel('周期分类').selectOption('rent');
    await page.getByLabel('下一次到期日').fill('2026-02-28');
    await page.evaluate(() => (window.rejectRecurring = true));
    await page.getByRole('button', { name: '保存周期账单', exact: true }).click();
    await page.getByRole('alert').getByText('记录已变化，请刷新后重试').waitFor();
    assert.equal(await page.getByLabel('周期金额（元）').inputValue(), '12.34');
    await page.evaluate(() => (window.rejectRecurring = false));
    await page.getByRole('button', { name: '保存周期账单', exact: true }).click();
    await page.locator('dialog[open]').waitFor({ state: 'hidden' });
    await page.locator('.recurring-card').getByText('房租', { exact: true }).waitFor();
    const request = await page.evaluate(
      () =>
        window.recurringCalls.filter((c) => c.command === 'write_recurring').at(-1).args.request,
    );
    assert.equal(request.account_id, 'wallet');
    assert.equal(request.category_id, 'rent');
    assert.equal(request.amount, '12.34');
    await page.getByRole('button', { name: '编辑未来账单' }).click();
    await page.getByLabel('周期金额（元）').fill('20.00');
    await page.getByRole('button', { name: '保存周期账单', exact: true }).click();
    await page.locator('dialog[open]').waitFor({ state: 'hidden' });
    await page.waitForFunction(() =>
      document.querySelector('.recurring-card')?.textContent.includes('20.00'),
    );
    await page.getByRole('button', { name: '暂停', exact: true }).click();
    await page.getByRole('button', { name: '恢复', exact: true }).waitFor();
    await page.reload();
    await nav(page, 'recurring');
    await page.getByRole('button', { name: '恢复', exact: true }).click();
    await page.getByRole('button', { name: '暂停', exact: true }).waitFor();
    page.once('dialog', (d) => d.accept());
    await page.getByRole('button', { name: '跳过下次' }).click();
    await page
      .locator('.recurring-card')
      .getByText(/2026-03-31/)
      .waitFor();
    await page.getByRole('button', { name: '查看实例' }).click();
    await page.getByText('已跳过', { exact: true }).waitFor();
    assert.equal(
      await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth),
      false,
    );
  } finally {
    await page.close();
  }
});
test('home reminders retain candidate amount and open inbox with recurring source filter', async () => {
  const page = await newPage();
  try {
    await install(page);
    await page.evaluate(() =>
      localStorage.setItem(
        'recurringFixture',
        JSON.stringify([
          {
            id: 'template-1',
            version: 1,
            account_id: 'wallet',
            kind: 'expense',
            amount: '20.00',
            merchant: '房租',
            description: '',
            cadence: 'monthly',
            interval: 1,
            next_due_date: '2026-02-28',
            end_date: null,
            enabled: true,
            ended: false,
            category_id: null,
            category_name: '未分类',
          },
        ]),
      ),
    );
    await page.reload();
    await page.getByRole('heading', { name: '周期账单提醒' }).waitFor();
    assert.match(await page.locator('.recurring-reminders').innerText(), /12\.34/);
    await page.getByRole('button', { name: '查看待确认' }).click();
    await page.getByRole('heading', { name: '待确认收件箱' }).waitFor();
    assert.equal(await page.getByLabel('来源').inputValue(), 'recurring');
    await page.waitForFunction(() =>
      window.recurringCalls.some(
        (c) => c.command === 'query_entries' && c.args.query.source === 'recurring',
      ),
    );
  } finally {
    await page.close();
  }
});
