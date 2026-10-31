import assert from 'node:assert/strict';
import { test } from 'node:test';
import { browserSuite } from './helpers/browser.mjs';
import { installWorkflowIpc } from './helpers/workflow-ipc.mjs';
import { navigate } from './helpers/navigation.mjs';

const { url, newPage } = browserSuite(process.env.EXCHANGE_RATES_UI_TEST_PORT || '1437');

async function installRates(page, { empty = false, failure = false } = {}) {
  await installWorkflowIpc(page);
  await page.addInitScript(
    ({ empty, failure }) => {
      const original = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        if (command.startsWith('exchange_rates_')) {
          window.workflowCalls.push({ command, args });
          if (command === 'exchange_rates_history')
            return empty
              ? []
              : [
                  {
                    effective_date: '2026-10-28',
                    quote_currency: args.quoteCurrency,
                    rate: '0.140123456789',
                    provider: 'frankfurter',
                    provider_sources: ['ECB'],
                  },
                  {
                    effective_date: '2026-10-30',
                    quote_currency: args.quoteCurrency,
                    rate: '0.1409',
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
    { empty, failure },
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

test('local rates render chart points and currency/range selectors query the selected data', async () => {
  const page = await newPage({ viewport: { width: 390, height: 844 } });
  try {
    await page.clock.install({ time: new Date('2024-03-01T12:00:00Z') });
    await installRates(page);
    await page.goto(url);
    await navigate(page, 'exchange');
    await page.getByRole('img', { name: 'CNY 至 USD 参考汇率曲线' }).waitFor();
    assert.equal(await page.getByLabel('目标币种').inputValue(), 'USD');
    assert.equal(await page.getByLabel('时间范围').inputValue(), '90');
    const initialQuery = await page.evaluate(
      () => window.workflowCalls.find((call) => call.command === 'exchange_rates_history').args,
    );
    assert.deepEqual([initialQuery.fromDate, initialQuery.untilDate], ['2023-12-03', '2024-03-02']);
    assert.equal(await page.locator('.rate-chart circle').count(), 2);
    await page.getByRole('button', { name: /2026-10-28.*0.140123456789/ }).click();
    await page
      .getByRole('status')
      .filter({ hasText: /2026-10-28.*0.140123456789/ })
      .waitFor();
    await page.getByLabel('目标币种').selectOption('JPY');
    await page.getByRole('img', { name: 'CNY 至 JPY 参考汇率曲线' }).waitFor();
    for (const days of ['30', '365']) {
      await page.getByLabel('时间范围').selectOption(days);
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

test('refresh failure preserves cached chart and offers retry', async () => {
  const page = await newPage();
  try {
    await installRates(page, { failure: true });
    await page.goto(url);
    await navigate(page, 'exchange');
    await page.getByRole('img', { name: 'CNY 至 USD 参考汇率曲线' }).waitFor();
    await page.getByRole('button', { name: '刷新汇率', exact: true }).click();
    await page
      .getByRole('alert')
      .filter({ hasText: /检查网络/ })
      .waitFor();
    assert.equal(await page.locator('.rate-chart circle').count(), 2);
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
    assert.equal(await page.locator('.rate-chart circle').count(), 0);
  } finally {
    await page.close();
  }
});
