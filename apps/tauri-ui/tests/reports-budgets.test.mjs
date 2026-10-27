import assert from 'node:assert/strict';
import { test } from 'node:test';
import { browserSuite } from './helpers/browser.mjs';
import { navigate as nav } from './helpers/navigation.mjs';
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
        if (command === 'report_summary') {
          const [year, month] = args.query.from_date.slice(0, 7).split('-').map(Number);
          const until = args.query.until_date.slice(0, 7);
          const trend = [];
          let cursor = new Date(Date.UTC(year, month - 1, 1));
          while (cursor < new Date(`${until}-01T00:00:00Z`)) {
            const label = `${cursor.getUTCFullYear()}-${String(cursor.getUTCMonth() + 1).padStart(2, '0')}`;
            trend.push({
              month: label,
              income: '100.00',
              expense: window.expenseDominates ? '200.00' : '12.30',
            });
            cursor.setUTCMonth(cursor.getUTCMonth() + 1);
          }
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
            trend,
            balances: [{ account_id: 'wallet', name: '钱包', amount: '87.70' }],
          };
        }
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
test('reports show a pie chart plus vertical month and year views', async () => {
  const page = await newPage({ viewport: { width: 390, height: 844 } });
  try {
    await install(page);
    await nav(page, 'reports');
    await page.locator('[aria-label="分类支出饼图"]').waitFor();
    await page.locator('.vertical-chart[aria-label="月度收支柱状图"]').waitFor();
    await page.locator('.vertical-chart[aria-label="近6年收支柱状图"]').waitFor();
    assert.equal(await page.locator('.report-pie').count(), 1);
    assert.equal(await page.locator('.vertical-chart').count(), 2);
    assert.equal(await page.locator('.monthly-chart .column-item').count(), 6);
    assert.equal(await page.locator('.yearly-chart .column-item').count(), 6);
  } finally {
    await page.close();
  }
});
test('monthly columns share one baseline on a phone and the largest expense stays inside the plot', async () => {
  const page = await newPage({ viewport: { width: 390, height: 844 } });
  try {
    await page.clock.install({ time: new Date('2026-10-27T12:00:00+08:00') });
    await page.addInitScript(() => {
      window.expenseDominates = true;
    });
    await install(page);
    await nav(page, 'reports');
    await page.locator('.monthly-chart .column-item').nth(5).waitFor();
    await page.evaluate(() => document.fonts.ready);
    const geometry = await page.locator('.monthly-chart .column-bars').evaluateAll((plots) =>
      plots.map((plot) => ({
        bottom: plot.getBoundingClientRect().bottom,
        height: plot.getBoundingClientRect().height,
        expenseHeight: plot.querySelector('.column-expense').getBoundingClientRect().height,
      })),
    );
    assert.ok(
      Math.max(...geometry.map((g) => g.bottom)) - Math.min(...geometry.map((g) => g.bottom)) < 1,
      'all months must share a baseline',
    );
    for (const g of geometry)
      assert.ok(g.expenseHeight <= g.height, 'bars must fit the shared scale');
    assert.ok(Math.abs(geometry[0].expenseHeight - geometry[0].height) < 1);
  } finally {
    await page.close();
  }
});

test('month and year charts switch independently to expense or income pies without losing filters', async () => {
  const page = await newPage({ viewport: { width: 390, height: 844 } });
  try {
    await install(page);
    await nav(page, 'reports');
    await page.getByLabel('报表平台', { exact: true }).selectOption('wallet');
    const month = page.getByRole('region', { name: '月度收支图表', exact: true });
    const year = page.getByRole('region', { name: '近6年收支图表', exact: true });
    await month.getByRole('button', { name: '饼图', exact: true }).click();
    await month.getByRole('img', { name: /^\d{4}年支出占比饼图$/ }).waitFor();
    assert.equal(await month.locator('.vertical-chart').count(), 0);
    assert.equal(await year.locator('.vertical-chart').count(), 1);
    assert.match(await month.locator('.pie-total').innerText(), /147\.60/);
    assert.equal(await month.locator('.pie-legend-row').count(), 12);
    await month.getByLabel('饼图统计', { exact: true }).selectOption('income');
    await month.getByRole('img', { name: /^\d{4}年收入占比饼图$/ }).waitFor();
    assert.match(await month.locator('.pie-total').innerText(), /1,200\.00/);
    await year.getByRole('button', { name: '饼图', exact: true }).click();
    assert.equal(await year.locator('.pie-legend-row').count(), 6);
    assert.equal(
      await month.getByRole('button', { name: '饼图', exact: true }).getAttribute('aria-pressed'),
      'true',
    );
    await month.getByRole('button', { name: '柱形图', exact: true }).click();
    assert.equal(await month.locator('.column-item').count(), 6);
    assert.equal(await page.getByLabel('报表平台', { exact: true }).inputValue(), 'wallet');
    assert.equal(
      await page.evaluate(() => document.documentElement.scrollWidth > innerWidth),
      false,
    );
  } finally {
    await page.close();
  }
});

test('pie charts show empty periods and refund net negatives without invalid slices', async () => {
  const page = await newPage();
  try {
    await install(page);
    await page.evaluate(() => {
      const invoke = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        const result = await invoke(command, args);
        if (command === 'report_summary')
          result.trend = result.trend.map((row, i) => ({
            ...row,
            income: '0.00',
            expense: i === 0 ? '-3.00' : '0.00',
          }));
        return result;
      };
    });
    await nav(page, 'reports');
    const month = page.getByRole('region', { name: '月度收支图表', exact: true });
    await month.getByRole('button', { name: '饼图', exact: true }).click();
    await month.getByText('暂无正支出可绘制饼图。', { exact: true }).waitFor();
    assert.match(await month.locator('.pie-negatives').innerText(), /-3\.00/);
    assert.equal(await month.getByRole('img').count(), 0);
    await month.getByLabel('饼图统计', { exact: true }).selectOption('income');
    await month.getByText('暂无正收入可绘制饼图。', { exact: true }).waitFor();
  } finally {
    await page.close();
  }
});
test('bar charts show yuan ticks and reveal exact income and expense when a period is selected', async () => {
  const page = await newPage({ viewport: { width: 390, height: 844 } });
  try {
    await page.clock.install({ time: new Date('2026-10-27T12:00:00+08:00') });
    await install(page);
    await nav(page, 'reports');
    const month = page.getByRole('region', { name: '月度收支图表', exact: true });
    const year = page.getByRole('region', { name: '近6年收支图表', exact: true });
    await month.locator('.chart-axis').waitFor();
    assert.deepEqual(await month.locator('.chart-axis span').allTextContents(), [
      '¥100.00',
      '¥75.00',
      '¥50.00',
      '¥25.00',
      '¥0.00',
    ]);
    await month.getByRole('button', { name: '查看2026年10月收支金额', exact: true }).click();
    assert.match(await month.getByRole('status').innerText(), /2026年10月/);
    assert.match(await month.getByRole('status').innerText(), /收入 ¥100\.00/);
    assert.match(await month.getByRole('status').innerText(), /支出 ¥12\.30/);
    await year.getByRole('button', { name: '查看2026年收支金额', exact: true }).focus();
    await page.keyboard.press('Enter');
    assert.match(await year.getByRole('status').innerText(), /收入 ¥1,200\.00/);
    assert.match(await year.getByRole('status').innerText(), /支出 ¥147\.60/);
    assert.match(await month.getByRole('status').innerText(), /2026年10月/);
    await month.getByRole('button', { name: '饼图', exact: true }).click();
    await month.getByRole('button', { name: '柱形图', exact: true }).click();
    assert.match(await month.getByRole('status').innerText(), /支出 ¥12\.30/);
    assert.equal(
      await page.evaluate(() => document.documentElement.scrollWidth > innerWidth),
      false,
    );
  } finally {
    await page.close();
  }
});
test('selecting zero and refund periods shows the actual amount and updates after filtering', async () => {
  const page = await newPage({ viewport: { width: 390, height: 844 } });
  try {
    await page.clock.install({ time: new Date('2026-10-27T12:00:00+08:00') });
    await install(page);
    await page.evaluate(() => {
      const invoke = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        const result = await invoke(command, args);
        if (command === 'report_summary')
          result.trend = result.trend.map((row) => ({
            ...row,
            income: '0.00',
            expense: args.query.account_id ? '0.00' : '-3.15',
          }));
        return result;
      };
    });
    await nav(page, 'reports');
    const month = page.getByRole('region', { name: '月度收支图表', exact: true });
    await month.getByRole('button', { name: '查看2026年10月收支金额', exact: true }).click();
    assert.match(await month.getByRole('status').innerText(), /收入 ¥0\.00/);
    assert.match(await month.getByRole('status').innerText(), /支出 ¥-3\.15/);
    await page.getByLabel('报表平台', { exact: true }).selectOption('wallet');
    await month.getByRole('status').getByText('支出 ¥0.00', { exact: true }).waitFor();
    assert.doesNotMatch(await month.locator('.chart-axis').innerText(), /NaN|Infinity/);
  } finally {
    await page.close();
  }
});
test('monthly bars switch between six calendar months and keep the chosen half across chart types', async () => {
  const page = await newPage({ viewport: { width: 390, height: 844 } });
  try {
    await page.clock.install({ time: new Date('2026-10-27T12:00:00+08:00') });
    await install(page);
    await nav(page, 'reports');
    const month = page.getByRole('region', { name: '月度收支图表', exact: true });
    await month.getByRole('heading', { name: '2026年月度收支', exact: true }).waitFor();
    await month.getByRole('button', { name: '下半年', exact: true }).waitFor();
    assert.equal(
      await month.getByRole('button', { name: '下半年', exact: true }).getAttribute('aria-pressed'),
      'true',
    );
    assert.deepEqual(await month.locator('.column-item > span').allTextContents(), [
      '07月',
      '08月',
      '09月',
      '10月',
      '11月',
      '12月',
    ]);
    assert.equal(
      await page.evaluate(() =>
        window.analyticsCalls.some(
          (call) =>
            call.command === 'report_summary' &&
            call.args.query.from_date === '2026-01-01' &&
            call.args.query.until_date === '2027-01-01',
        ),
      ),
      true,
    );
    await month.getByRole('button', { name: '上半年', exact: true }).click();
    assert.deepEqual(await month.locator('.column-item > span').allTextContents(), [
      '01月',
      '02月',
      '03月',
      '04月',
      '05月',
      '06月',
    ]);
    const geometry = await month.locator('.column-item').evaluateAll((items) =>
      items.map((item) => {
        const bounds = item.getBoundingClientRect();
        return { left: bounds.left, right: bounds.right, width: bounds.width };
      }),
    );
    for (let i = 0; i < geometry.length; i++) {
      assert.ok(geometry[i].width >= 30, 'six months must have readable widths on a phone');
      if (i) assert.ok(geometry[i].left - geometry[i - 1].right >= 7, 'months need clear spacing');
    }
    await month.getByRole('button', { name: '查看2026年06月收支金额', exact: true }).click();
    assert.match(await month.getByRole('status').innerText(), /收入 ¥100\.00/);
    await month.getByRole('button', { name: '下半年', exact: true }).click();
    assert.equal(await month.getByRole('status').count(), 0);
    await month.getByRole('button', { name: '上半年', exact: true }).click();
    await month.getByRole('button', { name: '饼图', exact: true }).click();
    assert.equal(await month.locator('.pie-legend-row').count(), 12);
    await month.getByRole('button', { name: '柱形图', exact: true }).click();
    assert.equal(
      await month.getByRole('button', { name: '上半年', exact: true }).getAttribute('aria-pressed'),
      'true',
    );
    assert.equal(await page.locator('.yearly-chart .column-item').count(), 6);
    assert.equal(
      await page.evaluate(() => document.documentElement.scrollWidth > innerWidth),
      false,
    );
    await page.setViewportSize({ width: 320, height: 844 });
    assert.equal(
      await month
        .locator('.monthly-chart')
        .evaluate((plot) => plot.scrollWidth <= plot.clientWidth),
      true,
      'all six months must fit the plot on a narrow phone',
    );
  } finally {
    await page.close();
  }
});

test('monthly amount ticks rescale for each half and account, including 4000 and 12000 yuan', async () => {
  const page = await newPage({ viewport: { width: 390, height: 844 } });
  try {
    await page.clock.install({ time: new Date('2026-10-27T12:00:00+08:00') });
    await install(page);
    await page.evaluate(() => {
      const invoke = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        const result = await invoke(command, args);
        if (command === 'report_summary')
          result.trend = result.trend.map((row) => ({
            ...row,
            income: Number(row.month.slice(5)) <= 6 ? '4000.00' : '12000.00',
            expense: args.query.account_id ? '80000.00' : '0.00',
          }));
        return result;
      };
    });
    await nav(page, 'reports');
    const month = page.getByRole('region', { name: '月度收支图表', exact: true });
    await month
      .locator('.chart-axis span')
      .first()
      .getByText('¥12,000.00', { exact: true })
      .waitFor();
    assert.deepEqual(await month.locator('.chart-axis span').allTextContents(), [
      '¥12,000.00',
      '¥9,000.00',
      '¥6,000.00',
      '¥3,000.00',
      '¥0.00',
    ]);
    await month.getByRole('button', { name: '上半年', exact: true }).click();
    assert.deepEqual(await month.locator('.chart-axis span').allTextContents(), [
      '¥4,000.00',
      '¥3,000.00',
      '¥2,000.00',
      '¥1,000.00',
      '¥0.00',
    ]);
    await page.getByLabel('报表平台', { exact: true }).selectOption('wallet');
    await month
      .locator('.chart-axis span')
      .first()
      .getByText('¥80,000.00', { exact: true })
      .waitFor();
    await month.getByRole('button', { name: '查看2026年06月收支金额', exact: true }).click();
    assert.match(await month.getByRole('status').innerText(), /支出 ¥80,000\.00/);
    const bounds = await month.locator('.bar-chart-layout').evaluate((layout) => {
      const axis = layout.querySelector('.chart-axis').getBoundingClientRect();
      const labels = [...layout.querySelectorAll('.chart-axis span')].map((label) =>
        label.getBoundingClientRect(),
      );
      const plot = layout.querySelector('.vertical-chart').getBoundingClientRect();
      return {
        axisLeft: axis.left,
        plotLeft: plot.left,
        labels: labels.map((r) => ({ left: r.left, right: r.right })),
      };
    });
    for (const label of bounds.labels) {
      assert.ok(label.left >= bounds.axisLeft - 1, 'large tick labels must stay inside the axis');
      assert.ok(label.right < bounds.plotLeft, 'tick labels must not overlap the bars');
    }
    assert.equal(
      await page.evaluate(() => document.documentElement.scrollWidth > innerWidth),
      false,
    );
  } finally {
    await page.close();
  }
});

test('monthly bars default to January through June in the first local half of the year', async () => {
  const page = await newPage({ viewport: { width: 390, height: 844 } });
  try {
    await page.clock.install({ time: new Date('2026-06-30T23:30:00+08:00') });
    await install(page);
    await nav(page, 'reports');
    const month = page.getByRole('region', { name: '月度收支图表', exact: true });
    await month.getByRole('button', { name: '上半年', exact: true }).waitFor();
    assert.equal(
      await month.getByRole('button', { name: '上半年', exact: true }).getAttribute('aria-pressed'),
      'true',
    );
    assert.deepEqual(await month.locator('.column-item > span').allTextContents(), [
      '01月',
      '02月',
      '03月',
      '04月',
      '05月',
      '06月',
    ]);
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
