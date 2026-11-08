import assert from 'node:assert/strict';
import { test } from 'node:test';
import { browserSuite } from './helpers/browser.mjs';
import { installWorkflowIpc } from './helpers/workflow-ipc.mjs';
import { navigate } from './helpers/navigation.mjs';

const { url, newPage } = browserSuite('1456');

async function open(options = {}) {
  const page = await newPage({
    viewport: { width: 390, height: 844 },
    timezoneId: options.timezone ?? 'Asia/Shanghai',
  });
  await installWorkflowIpc(page);
  await page.addInitScript((options) => {
    const original = window.__TAURI_INTERNALS__.invoke;
    const products = [
      ['BTCUSDT', 'BTC', 'USDT'],
      ['BTCUSDC', 'BTC', 'USDC'],
      ['ETHBTC', 'ETH', 'BTC'],
      ['BTCEUR', 'BTC', 'EUR'],
      ['币安人生USDT', '币安人生', 'USDT'],
      ...Array.from({ length: 80 }, (_, i) => [`COIN${i}USDC`, `COIN${i}`, 'USDC']),
    ];
    window.marketFailure = '';
    window.ifindSearchShape = 'columns';
    window.__TAURI_INTERNALS__.invoke = async (command, args) => {
      if (command === 'privacy_status')
        return {
          locked: false,
          lock_enabled: false,
          hide_amounts: !!options.hidden,
          auto_lock_minutes: 5,
        };
      if (command === 'provider_connections')
        return (await original(command, args)).map((status) => ({
          ...status,
          configured: !!options[status.provider],
        }));
      if (!['binance_api_request', 'ifind_api_request'].includes(command))
        return original(command, args);
      window.workflowCalls.push({ command, args });
      const { endpoint_id: id, params } = args.request;
      if (window.marketFailure)
        return {
          status: 'failed',
          code: '-1303',
          message: window.marketFailure,
          data: null,
          http_status: 403,
          request_id: args.request.request_id,
        };
      let data;
      if (id.endsWith('api_v3_exchangeinfo'))
        data = {
          symbols: [
            ...products.map(([symbol, baseAsset, quoteAsset]) => ({
              symbol,
              baseAsset,
              quoteAsset,
              status: 'TRADING',
              isSpotTradingAllowed: true,
            })),
            {
              symbol: 'HALTBTC',
              baseAsset: 'HALT',
              quoteAsset: 'BTC',
              status: 'BREAK',
              isSpotTradingAllowed: true,
            },
          ],
        };
      else if (id.endsWith('api_v3_ticker_24hr'))
        data = products.map(([symbol]) => ({
          symbol,
          lastPrice: '101.12340001',
          openPrice: '100.00000000',
          highPrice: '110.00000000',
          lowPrice: '90.00000000',
          volume: '12.00000001',
          priceChangePercent: '1.123',
          closeTime: 1700007200000,
        }));
      else if (id.endsWith('api_v3_klines')) {
        if (window.holdSymbol === params.symbol)
          await new Promise((resolve) => {
            window.releaseCandles = resolve;
          });
        const times = params.endTime
          ? [1699996400000]
          : [1700000000000, 1700003600000, 1700007200000];
        data = times.map((time, i) => [
          time,
          '100.00000000',
          '110.00000000',
          '90.00000000',
          params.symbol === 'ETHBTC' ? '0.12345678' : `${101 + i}.12340001`,
          '12.00000001',
          time + 3599999,
        ]);
      } else if (id.endsWith('api_v3_account'))
        data = {
          accountType: 'SPOT',
          balances: [
            { asset: 'BTC', free: '0.00000001', locked: '2.00000000' },
            { asset: 'USDT', free: '123456789012345678.12345678', locked: '0.00000000' },
            { asset: 'ZERO', free: '0.00000000', locked: '0' },
          ],
        };
      else if (id.endsWith('smart_stock_picking'))
        data = {
          errorcode: 0,
          tables:
            window.ifindSearchShape === 'rows'
              ? [{ table: [{ 股票代码: '300033.SZ', 股票简称: '同花顺' }] }]
              : [
                  {
                    table: {
                      股票代码: ['300033.SZ', '600519.SH'],
                      股票简称: ['同花顺', '贵州茅台'],
                    },
                  },
                ],
        };
      else if (id.endsWith('real_time_quotation'))
        data = {
          errorcode: 0,
          tables: params.codes.split(',').map((thscode) => ({
            thscode,
            time: ['2026-11-06 15:00:00'],
            table: { latest: [123.4567], open: [120.1], high: [125.5], low: [119.8] },
          })),
        };
      else if (id.endsWith('basic_data_service'))
        data = {
          errorcode: 0,
          tables: [{ thscode: params.codes, table: { ths_stock_short_name_stock: ['同花顺'] } }],
        };
      else if (id.endsWith('cmd_history_quotation'))
        data = {
          errorcode: 0,
          tables: [
            {
              thscode: params.codes,
              time: ['2026-11-02', '2026-11-03', '2026-11-04', '2026-11-05'],
              table: {
                open: [120.1, null, 121.2, 122.3],
                high: [122.2, null, 123.3, 124.4],
                low: [119.1, null, 120.2, 121.3],
                close: [121.1234, null, 122.2345, 123.3456],
                volume: [10000, null, 20000, 30000],
              },
            },
          ],
        };
      else throw new Error(`Unexpected market endpoint: ${id}`);
      return {
        status: 'success',
        code: '0',
        message: null,
        data,
        http_status: 200,
        request_id: args.request.request_id,
      };
    };
  }, options);
  await page.goto(url);
  await page.locator('.balance-value').waitFor();
  await navigate(page, 'markets');
  return page;
}

async function openProvider(page, provider) {
  await page.getByRole('button', { name: `打开${provider}`, exact: true }).click();
}

test('Binance opens all active public spot pairs, searches quote currencies and scrolls on mobile', async () => {
  const page = await open();
  try {
    await openProvider(page, '币安');
    await page.getByRole('tab', { name: '币安行情', exact: true }).waitFor();
    const list = page.getByRole('region', { name: '币安现货行情列表', exact: true });
    await list.getByRole('button', { name: /ETH-BTC/ }).waitFor();
    assert.equal(await list.getByRole('button').count(), 85);
    assert.equal(await list.getByText('HALT-BTC').count(), 0);
    assert.equal(await page.getByLabel('API Key', { exact: true }).count(), 0);
    assert.equal(await page.locator('.market-api-workspace').count(), 0);
    assert.ok(await list.evaluate((el) => el.scrollHeight > el.clientHeight));
    await list.evaluate((el) => (el.scrollTop = el.scrollHeight));
    assert.ok(await list.evaluate((el) => el.scrollTop > 0));
    await page.getByLabel('搜索现货交易对', { exact: true }).fill(' btc ');
    assert.equal(await list.getByRole('button').count(), 4);
    assert.equal(await list.evaluate((el) => el.scrollTop), 0);
    await page.getByLabel('搜索现货交易对', { exact: true }).fill('ETH-BTC');
    assert.equal(await list.getByRole('button').count(), 1);
    await page.getByLabel('搜索现货交易对', { exact: true }).fill('USDC');
    assert.equal(await list.getByRole('button').count(), 81);
    await page.getByLabel('搜索现货交易对', { exact: true }).fill('币安人生');
    assert.equal(await list.getByRole('button').count(), 1);
    await page.getByLabel('搜索现货交易对', { exact: true }).fill('no-match');
    await page.getByText('没有匹配的现货交易对。', { exact: true }).waitFor();
    await page.getByRole('button', { name: '清空搜索', exact: true }).click();
    assert.equal(await list.getByRole('button').count(), 85);
    assert.equal(
      await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth),
      false,
    );
    const calls = await page.evaluate(() => window.workflowCalls);
    assert.equal(
      calls.some((c) => c.args?.request?.endpoint_id?.endsWith('api_v3_account')),
      false,
    );
    assert.ok(
      calls
        .filter((c) => c.command === 'binance_api_request')
        .every((c) => !c.args.request.confirmed),
    );
  } finally {
    await page.close();
  }
});

test('Binance favorites persist after reload and remain separate from iFinD', async () => {
  const page = await open();
  try {
    await openProvider(page, '币安');
    const list = page.getByRole('region', { name: '币安现货行情列表' });
    await list.getByRole('button', { name: /ETH-BTC/ }).click();
    await page.getByRole('button', { name: '收藏 ETH-BTC', exact: true }).click();
    await page.getByRole('tab', { name: '自选', exact: true }).click();
    assert.equal(await list.getByRole('button').count(), 1);
    await page.reload();
    await page.locator('.balance-value').waitFor();
    await navigate(page, 'markets');
    await openProvider(page, '币安');
    await page.getByRole('tab', { name: '自选', exact: true }).click();
    await list.getByRole('button', { name: /ETH-BTC/ }).waitFor();
    await page.getByRole('img', { name: 'ETH-BTC K 线，3 根', exact: true }).waitFor();
    await page.getByRole('button', { name: '返回上一级', exact: true }).click();
    await openProvider(page, '同花顺 iFinD');
    await page.getByRole('tab', { name: '自选', exact: true }).click();
    await page.getByText('尚未添加自选股票。', { exact: true }).waitFor();
  } finally {
    await page.close();
  }
});

test('Binance candles preserve decimals, select endpoints, change intervals and load earlier history', async () => {
  const page = await open();
  try {
    await openProvider(page, '币安');
    await page.getByRole('img', { name: 'BTC-USDT K 线，3 根', exact: true }).waitFor();
    const range = page.getByRole('slider', { name: '查看 K 线', exact: true });
    assert.equal(await range.inputValue(), '2');
    await range.focus();
    await page.keyboard.press('Home');
    assert.equal(await range.inputValue(), '0');
    await page
      .locator('.okx-candle-detail')
      .getByText(/101\.12340001/)
      .waitFor();
    await page.keyboard.press('End');
    assert.equal(await range.inputValue(), '2');
    await page
      .locator('.okx-candle-detail')
      .getByText(/103\.12340001/)
      .waitFor();
    assert.equal(await range.evaluate((el) => getComputedStyle(el).paddingLeft), '0px');
    await page.getByRole('button', { name: '加载更早 K 线', exact: true }).click();
    await page.getByRole('img', { name: 'BTC-USDT K 线，4 根', exact: true }).waitFor();
    await page.getByLabel('K 线周期', { exact: true }).selectOption('1d');
    await page.waitForFunction(() =>
      window.workflowCalls.some((c) => c.args?.request?.params?.interval === '1d'),
    );
    const request = await page.evaluate(
      () => window.workflowCalls.find((c) => c.args?.request?.params?.endTime)?.args.request,
    );
    assert.equal(request.params.endTime, 1699999999999);
  } finally {
    await page.close();
  }
});

test('Binance account requires read credentials, shows exact spot balances and honors amount hiding', async () => {
  for (const hidden of [false, true]) {
    const page = await open({ binance: true, hidden });
    try {
      await openProvider(page, '币安');
      await page.getByRole('tab', { name: '资产总览', exact: true }).click();
      await page.getByRole('heading', { name: '现货账户余额', exact: true }).waitFor();
      await page.getByText('BTC', { exact: true }).waitFor();
      await page
        .getByText(hidden ? '••••' : '123456789012345678.12345678', { exact: true })
        .first()
        .waitFor();
      assert.equal(await page.getByText('ZERO', { exact: true }).count(), 0);
      if (hidden)
        assert.equal(
          (await page.locator('#day-content').textContent()).includes('0.00000001'),
          false,
        );
      const request = await page.evaluate(
        () =>
          window.workflowCalls.find((c) => c.args?.request?.endpoint_id?.endsWith('api_v3_account'))
            .args.request,
      );
      assert.equal(request.credentials, 'read');
      assert.equal(request.environment, 'production');
      assert.equal(request.confirmed, false);
    } finally {
      await page.close();
    }
  }
  const page = await open();
  try {
    await openProvider(page, '币安');
    await page.getByRole('tab', { name: '资产总览', exact: true }).click();
    await page.getByText(/^请配置币安只读凭据以读取账户数据。/).waitFor();
    assert.equal(
      await page.evaluate(() =>
        window.workflowCalls.some((c) => c.args?.request?.endpoint_id?.endsWith('api_v3_account')),
      ),
      false,
    );
  } finally {
    await page.close();
  }
});

test('iFinD opens its workspace without tokens and guides users to credential settings', async () => {
  const page = await open();
  try {
    await openProvider(page, '同花顺 iFinD');
    await page.getByRole('tab', { name: '股票行情', exact: true }).waitFor();
    await page.getByText(/请配置 iFinD Refresh Token/).waitFor();
    assert.equal(await page.getByLabel('HTTP API Refresh Token', { exact: true }).count(), 0);
    assert.equal(
      await page.evaluate(() =>
        window.workflowCalls.some((c) => c.command === 'ifind_api_request'),
      ),
      false,
    );
    assert.equal(await page.getByRole('tab', { name: '资产总览', exact: true }).count(), 0);
    await page.getByRole('button', { name: 'API 凭据设置', exact: true }).click();
    await page.getByLabel('HTTP API Refresh Token', { exact: true }).waitFor();
  } finally {
    await page.close();
  }
});

test('iFinD searches official stock names, displays quotes and real OHLC history, and saves favorites', async () => {
  const page = await open({ ifind: true, timezone: 'America/Los_Angeles' });
  try {
    await openProvider(page, '同花顺 iFinD');
    await page.getByLabel('搜索股票', { exact: true }).fill('同花顺');
    await page.getByRole('button', { name: '搜索股票', exact: true }).click();
    const list = page.getByRole('region', { name: '股票行情列表', exact: true });
    await list.getByRole('button', { name: /同花顺.*300033.SZ/ }).click();
    await page.getByText('123.4567', { exact: true }).waitFor();
    await page.getByRole('img', { name: '300033.SZ K 线，3 根', exact: true }).waitFor();
    await page.getByRole('slider', { name: '查看 K 线', exact: true }).focus();
    await page.keyboard.press('Home');
    await page
      .locator('.okx-candle-detail')
      .getByText(/121\.1234/)
      .waitFor();
    await page
      .locator('.okx-candle-detail')
      .getByText(/2026\/11\/2/)
      .waitFor();
    await page.getByLabel('K 线周期', { exact: true }).selectOption('W');
    await page.waitForFunction(() =>
      window.workflowCalls.some((c) => c.args?.request?.params?.functionpara?.Interval === 'W'),
    );
    await page.getByRole('button', { name: '收藏 300033.SZ', exact: true }).click();
    await page.getByRole('tab', { name: '自选', exact: true }).click();
    await list.getByRole('button', { name: /300033.SZ/ }).waitFor();
    const calls = await page.evaluate(() =>
      window.workflowCalls.filter((c) => c.command === 'ifind_api_request'),
    );
    assert.ok(
      calls.some(
        (c) =>
          c.args.request.params.searchstring === '同花顺' &&
          c.args.request.params.searchtype === 'stock',
      ),
    );
    assert.ok(calls.every((c) => !c.args.request.confirmed));
    assert.equal(
      await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth),
      false,
    );
    await page.reload();
    await page.locator('.balance-value').waitFor();
    await navigate(page, 'markets');
    await openProvider(page, '同花顺 iFinD');
    await page.getByRole('tab', { name: '自选', exact: true }).click();
    await list.getByRole('button', { name: /同花顺.*300033.SZ/ }).click();
    await page.getByRole('img', { name: '300033.SZ K 线，3 根', exact: true }).waitFor();
    await page.getByRole('button', { name: '取消收藏 300033.SZ', exact: true }).click();
    await page.getByText('尚未添加自选股票。', { exact: true }).waitFor();
    assert.equal(await page.getByRole('img', { name: /300033.SZ K 线/ }).count(), 0);
  } finally {
    await page.close();
  }
});

test('iFinD accepts explicit exchange codes without smart-search permission and surfaces retriable permission errors', async () => {
  const page = await open({ ifind: true });
  try {
    await openProvider(page, '同花顺 iFinD');
    await page.getByLabel('搜索股票', { exact: true }).fill('300033.sz');
    await page.getByRole('button', { name: '搜索股票', exact: true }).click();
    await page.getByText('123.4567', { exact: true }).waitFor();
    assert.equal(
      await page.evaluate(() =>
        window.workflowCalls.some((c) =>
          c.args?.request?.endpoint_id?.endsWith('smart_stock_picking'),
        ),
      ),
      false,
    );
    await page.evaluate(() => (window.marketFailure = '无权限访问该行情服务'));
    await page.getByRole('button', { name: '刷新行情', exact: true }).click();
    await page
      .getByRole('alert')
      .filter({ hasText: /无权限访问该行情服务/ })
      .waitFor();
    await page.evaluate(() => (window.marketFailure = ''));
    await page.getByRole('button', { name: '重试', exact: true }).click();
    await page.getByText('123.4567', { exact: true }).waitFor();
    await page.evaluate(() => (window.ifindSearchShape = 'rows'));
    await page.getByLabel('搜索股票', { exact: true }).fill('同花顺');
    await page.getByRole('button', { name: '搜索股票', exact: true }).click();
    const list = page.getByRole('region', { name: '股票行情列表', exact: true });
    await list.getByRole('button', { name: /同花顺.*300033.SZ/ }).waitFor();
    assert.equal(await list.getByRole('button').count(), 1);
  } finally {
    await page.close();
  }
});

test('late Binance candles cannot replace the newly selected market', async () => {
  const page = await open();
  try {
    await page.evaluate(() => (window.holdSymbol = 'BTCUSDT'));
    await openProvider(page, '币安');
    await page.waitForFunction(() => typeof window.releaseCandles === 'function');
    await page
      .getByRole('region', { name: '币安现货行情列表' })
      .getByRole('button', { name: /ETH-BTC/ })
      .click();
    await page.getByRole('img', { name: 'ETH-BTC K 线，3 根', exact: true }).waitFor();
    await page.evaluate(() => window.releaseCandles());
    await page
      .locator('.okx-candle-detail')
      .getByText(/0\.12345678/)
      .waitFor();
    assert.equal(await page.getByRole('img', { name: /BTC-USDT K 线/ }).count(), 0);
  } finally {
    await page.close();
  }
});
