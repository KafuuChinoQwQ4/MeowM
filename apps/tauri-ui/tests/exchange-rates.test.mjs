import assert from 'node:assert/strict';
import { test } from 'node:test';
import { browserSuite } from './helpers/browser.mjs';
import { installWorkflowIpc } from './helpers/workflow-ipc.mjs';
import { navigate } from './helpers/navigation.mjs';

const { url, newPage } = browserSuite(process.env.EXCHANGE_RATES_UI_TEST_PORT || '1437');

async function installRates(
  page,
  {
    empty = false,
    failure = false,
    disjoint = false,
    descending = false,
    many = false,
    dated = false,
  } = {},
) {
  await installWorkflowIpc(page);
  await page.addInitScript(
    ({ empty, failure, disjoint, descending, many, dated }) => {
      const original = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        if (command.startsWith('exchange_rates_')) {
          window.workflowCalls.push({ command, args });
          if (command === 'exchange_rates_history' && dated) {
            return [
              '2023-03-03',
              '2023-09-03',
              '2023-09-04',
              '2023-12-02',
              '2023-12-03',
              '2024-01-31',
              '2024-02-01',
              '2024-02-23',
              '2024-02-24',
              '2024-02-29',
              '2024-03-01',
              '2024-03-02',
            ]
              .filter((date) => date >= args.fromDate && date < args.untilDate)
              .map((effective_date, index) => ({
                effective_date,
                quote_currency: args.quoteCurrency,
                rate: (0.14 + index * 0.0001).toFixed(6),
                provider: 'frankfurter',
                provider_sources: ['ECB'],
              }));
          }
          if (command === 'exchange_rates_history')
            return empty
              ? []
              : many
                ? Array.from({ length: 40 }, (_, index) => ({
                    effective_date: new Date(Date.UTC(2026, 8, 21 + index))
                      .toISOString()
                      .slice(0, 10),
                    quote_currency: args.quoteCurrency,
                    rate: descending
                      ? (0.144 - index * 0.0001).toFixed(6)
                      : (0.14 + index * 0.0001).toFixed(6),
                    provider: 'frankfurter',
                    provider_sources: ['ECB'],
                  }))
                : [
                    {
                      effective_date:
                        disjoint && args.quoteCurrency === 'JPY' ? '2026-10-27' : '2026-10-28',
                      quote_currency: args.quoteCurrency,
                      rate: args.quoteCurrency === 'JPY' ? '150' : '0.140123456789',
                      provider: 'frankfurter',
                      provider_sources: ['ECB'],
                    },
                    {
                      effective_date:
                        disjoint && args.quoteCurrency === 'JPY' ? '2026-10-29' : '2026-10-30',
                      quote_currency: args.quoteCurrency,
                      rate: descending
                        ? args.quoteCurrency === 'JPY'
                          ? '149.6'
                          : '0.139'
                        : args.quoteCurrency === 'JPY'
                          ? '150.4'
                          : '0.141',
                      provider: 'frankfurter',
                      provider_sources: ['ECB'],
                    },
                  ];
          if (command === 'exchange_rates_refresh' && failure)
            throw new Error('汇率更新失败，请检查网络后重试。');
          return {
            status: 'success',
            latest_date: '2026-10-30',
            last_attempt_utc: '2026-10-30T12:00:00Z',
            last_success_utc: '2026-10-30T12:00:00Z',
            error_code: null,
          };
        }
        return original(command, args);
      };
    },
    { empty, failure, disjoint, descending, many, dated },
  );
}

test('fiat exchange rates have a dedicated primary navigation destination', async () => {
  const page = await newPage();
  try {
    await installRates(page);
    await page.goto(url);
    await navigate(page, 'exchange');
    await page.getByRole('heading', { name: '法币汇率', exact: true }).waitFor();
    await page.locator('#menu').click();
    assert.equal(await page.locator('#nav-exchange').getAttribute('aria-current'), 'page');
    await page.locator('#nav-finance').click();
    assert.equal(await page.locator('.navigation-directory #nav-exchange').count(), 0);
  } finally {
    await page.close();
  }
});

test('local rates render daily candles and currency/range selectors query the selected data', async () => {
  const page = await newPage({ viewport: { width: 390, height: 844 } });
  try {
    await page.clock.install({ time: new Date('2024-03-01T12:00:00Z') });
    await installRates(page);
    await page.goto(url);
    await navigate(page, 'exchange');
    await page.getByRole('img', { name: 'CNY 至 USD 参考汇率 K 线图，2 根' }).waitFor();
    assert.equal(await page.getByLabel('源币种').inputValue(), 'CNY');
    assert.equal(await page.getByLabel('目标币种').inputValue(), 'USD');
    await page.getByText('USD 0.14', { exact: true }).waitFor();
    const ranges = page.getByRole('group', { name: '时间范围', exact: true });
    assert.equal(
      await ranges.getByRole('button', { name: '近90天' }).getAttribute('aria-pressed'),
      'true',
    );
    const initialQuery = await page.evaluate(
      () => window.workflowCalls.find((call) => call.command === 'exchange_rates_history').args,
    );
    assert.deepEqual([initialQuery.fromDate, initialQuery.untilDate], ['2023-12-03', '2024-03-02']);
    assert.equal(await page.locator('[data-rate-candle]').count(), 2);
    await page.getByRole('button', { name: /2026-10-28.*0.140123456789/ }).click();
    await page.locator('.rate-chart-selected').getByText('2026-10-28', { exact: true }).waitFor();
    assert.equal(
      await page.locator('.rate-chart-selected strong').innerText(),
      '1 CNY = 0.140123456789 USD',
    );
    await page.getByLabel('目标币种').selectOption('JPY');
    await page.getByRole('img', { name: /CNY 至 JPY 参考汇率 K 线图/ }).waitFor();
    for (const days of ['30', '365']) {
      await ranges.getByRole('button', { name: `近${days}天`, exact: true }).click();
      await page.waitForFunction(
        (days) =>
          window.workflowCalls.some(
            (call) =>
              call.command === 'exchange_rates_history' &&
              (Date.parse(call.args.untilDate) - Date.parse(call.args.fromDate)) / 86400000 ===
                Number(days),
          ),
        days,
      );
    }
    const queries = await page.evaluate(() =>
      window.workflowCalls.filter((call) => call.command === 'exchange_rates_history'),
    );
    assert.equal(
      (Date.parse(queries[0].args.untilDate) - Date.parse(queries[0].args.fromDate)) / 86400000,
      90,
    );
    assert(queries.some((call) => call.args.quoteCurrency === 'JPY'));
    assert(
      queries.some(
        (call) => call.args.fromDate === '2023-03-03' && call.args.untilDate === '2024-03-02',
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

test('fiat chart time presets display only observations in the selected calendar range', async () => {
  const page = await newPage({ viewport: { width: 390, height: 844 } });
  try {
    await page.clock.install({ time: new Date('2024-03-01T12:00:00Z') });
    await installRates(page, { dated: true });
    await page.goto(url);
    await navigate(page, 'exchange');
    await page.getByRole('img', { name: 'CNY 至 USD 参考汇率 K 线图，7 根' }).waitFor();
    const ranges = page.getByRole('group', { name: '时间范围', exact: true });
    assert.equal(await ranges.getByRole('button', { name: '近7天', exact: true }).count(), 1);
    for (const [days, fromDate, count] of [
      [7, '2024-02-24', 3],
      [30, '2024-02-01', 5],
      [90, '2023-12-03', 7],
      [180, '2023-09-04', 9],
      [365, '2023-03-03', 11],
      [7, '2024-02-24', 3],
    ]) {
      const button = ranges.getByRole('button', { name: `近${days}天`, exact: true });
      await button.click();
      await page.waitForFunction((fromDate) => {
        const query = window.workflowCalls
          .filter((call) => call.command === 'exchange_rates_history')
          .at(-1);
        return query?.args.fromDate === fromDate && query.args.untilDate === '2024-03-02';
      }, fromDate);
      await page.getByRole('img', { name: `CNY 至 USD 参考汇率 K 线图，${count} 根` }).waitFor();
      assert.equal(await button.getAttribute('aria-pressed'), 'true');
      await page.getByText(`${fromDate} 至 2024-03-01`, { exact: true }).waitFor();
      assert.equal(
        await page.locator('[data-rate-candle]').first().getAttribute('data-rate-date'),
        fromDate,
      );
      assert.equal(
        await page.locator('[data-rate-candle]').last().getAttribute('data-rate-end-date'),
        '2024-03-01',
      );
      if (days === 365) await page.locator('[data-rate-candle]').first().click();
    }
    await page.locator('.rate-chart-selected').getByText('2024-03-01', { exact: true }).waitFor();
    assert.equal(
      await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth),
      false,
    );
  } finally {
    await page.close();
  }
});

test('fiat history candles fit the full range in one chart without a scroll control', async () => {
  const risingPage = await newPage();
  try {
    await installRates(risingPage, { many: true });
    await risingPage.goto(url);
    await navigate(risingPage, 'exchange');
    await risingPage.getByRole('img', { name: 'CNY 至 USD 参考汇率 K 线图，32 根' }).waitFor();
    assert.equal(await risingPage.locator('[data-rate-candle]').count(), 32);
    assert.equal(await risingPage.locator('.rate-chart-grid-line').count(), 6);
    assert.ok(await risingPage.locator('.rate-chart-candle-up').count());
    assert.equal(await risingPage.locator('.rate-chart-range').count(), 0);
    assert.equal(await risingPage.locator('input[type="range"]').count(), 0);
    assert.equal(
      await risingPage.locator('[data-rate-candle]').first().getAttribute('data-rate-date'),
      '2026-09-21',
    );
    assert.equal(
      await risingPage.locator('[data-rate-candle]').last().getAttribute('data-rate-end-date'),
      '2026-10-30',
    );
    assert.equal(
      await risingPage.locator('.rate-chart').getAttribute('aria-label'),
      'CNY 至 USD 参考汇率 K 线图，32 根',
    );
  } finally {
    await risingPage.close();
  }

  const fallingPage = await newPage();
  try {
    await installRates(fallingPage, { descending: true });
    await fallingPage.goto(url);
    await navigate(fallingPage, 'exchange');
    await fallingPage.getByRole('img', { name: 'CNY 至 USD 参考汇率 K 线图，2 根' }).waitFor();
    assert.ok(await fallingPage.locator('.rate-chart-candle-down').count());
  } finally {
    await fallingPage.close();
  }
});

test('exchange page converts arbitrary currency pairs on common observation dates', async () => {
  const page = await newPage({ viewport: { width: 390, height: 844 } });
  try {
    await installRates(page);
    await page.goto(url);
    await navigate(page, 'exchange');
    await page.getByLabel('源币种').selectOption('USD');
    await page.getByLabel('目标币种').selectOption('JPY');
    await page.getByRole('img', { name: 'USD 至 JPY 参考汇率 K 线图，2 根' }).waitFor();
    await page.getByLabel('换算金额').fill('1.23');
    await page.getByText('JPY 1,312', { exact: true }).waitFor();
    assert.match(
      await page.locator('.rate-chart-selected strong').innerText(),
      /^1 USD = 1066\.666/u,
    );
    assert.equal(await page.locator('[data-rate-candle]').count(), 2);
    const queries = await page.evaluate(() =>
      window.workflowCalls.filter((call) => call.command === 'exchange_rates_history'),
    );
    assert(queries.some((call) => call.args.quoteCurrency === 'USD'));
    assert(queries.some((call) => call.args.quoteCurrency === 'JPY'));
  } finally {
    await page.close();
  }
});

test('same-currency conversion is identity', async () => {
  const page = await newPage();
  try {
    await installRates(page);
    await page.goto(url);
    await navigate(page, 'exchange');
    await page.getByLabel('源币种').selectOption('USD');
    await page.getByText('USD 1.00', { exact: true }).waitFor();
    await page.getByText('同币种换算比例为 1').waitFor();
    assert.equal(await page.getByText('同一币种按 1:1 换算，无需汇率曲线。').count(), 1);
    assert.equal(await page.locator('.rate-chart').count(), 0);
  } finally {
    await page.close();
  }
});

test('disjoint histories do not invent a cross rate', async () => {
  const page = await newPage();
  try {
    await installRates(page, { disjoint: true });
    await page.goto(url);
    await navigate(page, 'exchange');
    await page.getByLabel('源币种').selectOption('USD');
    await page.getByLabel('目标币种').selectOption('JPY');
    await page.getByText('所选日期范围没有这组币种的共同汇率观察。').waitFor();
    await page.getByText('所选范围暂无汇率数据').waitFor();
    assert.equal(await page.locator('[data-rate-candle]').count(), 0);
    assert.equal(await page.locator('.exchange-rate-converted strong').innerText(), '—');
  } finally {
    await page.close();
  }
});

test('refresh failure preserves cached chart and offers retry', async () => {
  const page = await newPage();
  try {
    await installRates(page, { failure: true });
    await page.goto(url);
    await navigate(page, 'exchange');
    await page.getByRole('img', { name: 'CNY 至 USD 参考汇率 K 线图，2 根' }).waitFor();
    await page.getByRole('button', { name: '刷新汇率', exact: true }).click();
    await page
      .getByRole('alert')
      .filter({ hasText: /检查网络/ })
      .waitFor();
    assert.equal(await page.locator('[data-rate-candle]').count(), 2);
    assert.equal(await page.getByLabel('目标币种').inputValue(), 'USD');
    assert.equal(
      await page.getByRole('button', { name: '刷新汇率', exact: true }).isEnabled(),
      true,
    );
  } finally {
    await page.close();
  }
});

test('empty local history provides an accessible sync action', async () => {
  const page = await newPage();
  try {
    await installRates(page, { empty: true });
    await page.goto(url);
    await navigate(page, 'exchange');
    await page.getByText('所选范围暂无汇率数据').waitFor();
    assert.equal(await page.getByRole('button', { name: '刷新汇率', exact: true }).count(), 1);
    assert.equal(await page.locator('[data-rate-candle]').count(), 0);
  } finally {
    await page.close();
  }
});
