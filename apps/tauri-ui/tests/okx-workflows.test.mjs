import assert from 'node:assert/strict';
import { test } from 'node:test';
import { browserSuite } from './helpers/browser.mjs';
import { installWorkflowIpc } from './helpers/workflow-ipc.mjs';
import { navigate } from './helpers/navigation.mjs';
const { url, newPage } = browserSuite('1447');

async function open(
  configured = false,
  instruments = ['BTC-USDT', 'ETH-USDT', 'BTC-USDC', 'ETH-BTC'],
  candleTimes = [1708580000000],
) {
  const page = await newPage({ viewport: { width: 390, height: 844 } });
  await installWorkflowIpc(page);
  const marketData = { instruments, candleTimes };
  await page.addInitScript(({ instruments, candleTimes }) => {
    const original = window.__TAURI_INTERNALS__.invoke;
    let watchlist = [];
    const records = [];
    const ticker = (instrument) => ({
      instrument,
      last: '98765.43210001',
      open_24h: '98000',
      high_24h: '99000',
      low_24h: '97000',
      volume_24h: '123.4',
      timestamp: 1708587373361,
    });
    const candle = (timestamp) => ({
      timestamp,
      open: '98000',
      high: '99000',
      low: '97000',
      close: '98765',
      volume: '1.00000001',
      confirmed: true,
    });
    window.nextOrderResult = null;
    window.nextOrderError = null;
    window.nextCancelError = null;
    window.orderDetailResult = null;
    window.orderHistoryResult = null;
    window.tradeContextDelay = 0;
    window.tradeContextCallsFinished = 0;
    window.orderHistoryDelay = 0;
    window.orderHistoryCallsFinished = 0;
    window.__TAURI_INTERNALS__.invoke = async (command, args) => {
      if (!command.startsWith('okx_')) return original(command, args);
      window.workflowCalls.push({ command, args });
      if (command === 'okx_tickers') return instruments.map(ticker);
      if (command === 'okx_watchlist') return watchlist;
      if (command === 'okx_save_watchlist') {
        watchlist = args.instruments;
        return;
      }
      if (command === 'okx_candles')
        return args.after ? [candle(1708570000000)] : candleTimes.map(candle);
      if (command === 'okx_assets')
        return {
          balances: [
            {
              account: 'trading',
              asset: 'BTC',
              quantity: '0.00000001',
              available: null,
              frozen: '0',
              equity: '0.00000001',
              equity_usd: '0.000987654321',
            },
            {
              account: 'funding',
              asset: 'USDT',
              quantity: '12.34000000',
              available: '10.34000000',
              frozen: '2.00000000',
              equity: null,
              equity_usd: null,
            },
          ],
          valuation: {
            total_usd: '12.340987654321',
            trading_usd: '0.000987654321',
            funding_usd: '12.34',
            earn_usd: '0',
            timestamp: 1708587373361,
          },
          fetched_at: 1708587373361,
        };
      if (command === 'okx_records') {
        const rows = records.filter((r) => !args.status || args.status === r.status);
        return { rows: rows.slice(args.offset, args.offset + 50), total: rows.length };
      }
      if (command === 'okx_import_history') {
        const existing = records.length > 0;
        if (!existing)
          records.push({
            status: 'pending',
            record: {
              key: 'okx:123:fill:456',
              account_uid: '123',
              source: 'fill',
              source_id: '456',
              occurred_at: 1708587373361,
              instrument: 'BTC-USDT',
              legs: [
                { asset: 'BTC', quantity: '0.00192834' },
                { asset: 'USDT', quantity: '-99.99985572' },
              ],
              fee: { asset: 'BTC', quantity: '-0.00000192834' },
              price: '51858',
              platform_state: 'filled',
            },
          });
        return {
          inserted: existing ? 0 : 1,
          duplicates: existing ? 1 : 0,
          fetched: 1,
          next_after: null,
          account_uid: '123',
        };
      }
      if (command === 'okx_review_records') {
        for (const r of records) if (args.keys.includes(r.record.key)) r.status = args.status;
        return;
      }
      if (command === 'okx_positions') {
        const position = {
          position_id: '789',
          instrument: 'BTC-USDT-SWAP',
          side: 'long',
          margin_mode: 'cross',
          currency: 'USDT',
          quantity: '2',
          open_price: '98000',
          close_price: '99000',
          unrealized_pnl: '1.25',
          realized_pnl: '0.001',
          pnl: '0.0011',
          fee: '-0.0001',
          funding_fee: '0',
          liquidation_penalty: '0',
          updated_at: 1708587373361,
        };
        return {
          current: [position],
          history: [position],
          next_after: null,
          fetched_at: 1708587373361,
        };
      }
      if (command === 'okx_trade_context') {
        if (window.tradeContextDelay)
          await new Promise((resolve) => setTimeout(resolve, window.tradeContextDelay));
        window.tradeContextCallsFinished += 1;
        const contract = args.instType !== 'SPOT';
        return {
          instrument: contract
            ? {
                inst_type: args.instType,
                inst_id: args.instId,
                state: 'live',
                tick_sz: '0.1',
                lot_sz: '1',
                min_sz: '1',
                ct_val: args.instId.includes('-USD-') ? '100' : '0.01',
                ct_val_ccy: args.instId.includes('-USD-') ? 'USD' : 'BTC',
                ct_type: args.instId.includes('-USD-') ? 'inverse' : 'linear',
                settle_ccy: args.instId.includes('-USD-') ? 'BTC' : 'USDT',
                base_ccy: null,
                quote_ccy: null,
              }
            : {
                inst_type: 'SPOT',
                inst_id: args.instId,
                state: 'live',
                tick_sz: '0.1',
                lot_sz: '0.0001',
                min_sz: '0.0005',
                ct_val: null,
                ct_val_ccy: null,
                ct_type: null,
                settle_ccy: null,
                base_ccy: 'BTC',
                quote_ccy: 'USDT',
              },
          account: {
            acct_lv: '2',
            pos_mode: 'net_mode',
            positions: [
              {
                inst_id: 'BTC-USDT-SWAP',
                pos_side: 'net',
                quantity: '2',
                margin_mode: 'cross',
              },
            ],
            pending_orders: [
              {
                inst_id: 'BTC-USDT-SWAP',
                ord_id: '987654',
                state: 'partially_filled',
                side: 'sell',
                pos_side: 'net',
                margin_mode: 'cross',
                reduce_only: true,
                remaining_size: '1',
              },
            ],
            leverage: [
              { inst_id: 'BTC-USDT-SWAP', margin_mode: 'cross', pos_side: 'net', leverage: '5' },
            ],
          },
        };
      }
      if (command === 'okx_place_order' && window.nextOrderError) {
        const error = window.nextOrderError;
        window.nextOrderError = null;
        throw new Error(error);
      }
      if (command === 'okx_place_order')
        return (
          window.nextOrderResult ?? {
            status: 'accepted',
            client_order_id: args.draft.cl_ord_id,
            acknowledgement: {
              order_id: '123456',
              client_order_id: args.draft.cl_ord_id,
              code: '0',
              message: '',
            },
            error_code: null,
          }
        );
      if (command === 'okx_order_detail')
        return window.orderDetailResult
          ? { ...window.orderDetailResult, client_order_id: args.clOrdId }
          : null;
      if (command === 'okx_order_history') {
        if (window.orderHistoryDelay)
          await new Promise((resolve) => setTimeout(resolve, window.orderHistoryDelay));
        window.orderHistoryCallsFinished += 1;
        return window.orderHistoryResult ?? [];
      }
      if (command === 'okx_cancel_order' && window.nextCancelError) {
        const error = window.nextCancelError;
        window.nextCancelError = null;
        throw new Error(error);
      }
      if (command === 'okx_cancel_order')
        return {
          status: 'accepted',
          order_id: args.ordId,
          client_order_id: null,
          code: '0',
          message: '',
          error_code: null,
        };
      throw Error('Unexpected OKX command');
    };
  }, marketData);
  await page.goto(url);
  await page.locator('.balance-value').waitFor();
  await navigate(page, 'markets');
  await page.getByRole('button', { name: '打开欧易', exact: true }).click();
  if (configured) {
    await page.getByRole('button', { name: 'API 凭据设置', exact: true }).click();
    const card = page.locator('[data-provider="okx"]');
    await card.getByLabel('API Key', { exact: true }).fill('test-key');
    await card.getByLabel('Secret Key', { exact: true }).fill('test-secret');
    await card.getByLabel('Passphrase', { exact: true }).fill('test-passphrase');
    await card.getByRole('button', { name: '保存凭据', exact: true }).click();
    await page.getByRole('tab', { name: 'OKX 行情', exact: true }).waitFor();
  }
  return page;
}

async function saveOkxTradeKey(page, provider) {
  await page.getByRole('button', { name: 'API 凭据设置', exact: true }).click();
  const card = page.locator(`[data-provider="${provider}"]`);
  await card.getByLabel('API Key', { exact: true }).fill(`${provider}-key`);
  await card.getByLabel('Secret Key', { exact: true }).fill(`${provider}-secret`);
  await card.getByLabel('Passphrase', { exact: true }).fill(`${provider}-passphrase`);
  await card.getByRole('button', { name: '保存凭据', exact: true }).click();
  await page.getByRole('tab', { name: '交易下单', exact: true }).waitFor();
}

test('configured OKX opens its workspace with public quotes, persistent watchlist and earlier candles on mobile', async () => {
  const page = await open(true);
  try {
    await page.getByRole('tab', { name: 'OKX 行情', exact: true }).click();
    await page.getByRole('img', { name: /BTC-USDT K 线/ }).waitFor();
    await page.getByRole('button', { name: '收藏 BTC-USDT', exact: true }).click();
    await page.getByRole('button', { name: '取消收藏 BTC-USDT', exact: true }).waitFor();
    await page.getByRole('button', { name: '加载更早 K 线', exact: true }).click();
    assert.equal(await page.locator('[data-candle]').count(), 2);
    await page.getByRole('button', { name: '返回上一级', exact: true }).click();
    await page.getByRole('heading', { name: '交易所', exact: true }).waitFor();
    assert.equal(await page.getByRole('tablist', { name: 'OKX 功能' }).count(), 0);
    await page.getByRole('button', { name: '打开币安', exact: true }).click();
    await page.getByRole('tab', { name: '币安行情', exact: true }).waitFor();
    assert.equal(await page.getByLabel('API Key', { exact: true }).count(), 0);
    assert.equal(await page.getByRole('tablist', { name: 'OKX 功能' }).count(), 0);
    await page.getByRole('button', { name: '返回上一级', exact: true }).click();
    await page.getByRole('button', { name: '打开欧易', exact: true }).click();
    await page.getByRole('button', { name: '取消收藏 BTC-USDT', exact: true }).waitFor();
    assert.equal(
      await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
      true,
    );
    const calls = await page.evaluate(() => window.workflowCalls);
    assert.equal(
      calls.some((c) => ['okx_assets', 'okx_positions', 'okx_import_history'].includes(c.command)),
      false,
    );
  } finally {
    await page.close();
  }
});

test('OKX candle slider reaches both ends of its visible track', async () => {
  const page = await open(false, ['BTC-USDT'], [1708580000000, 1708583600000, 1708587200000]);
  try {
    const slider = page.getByRole('slider', { name: '查看 K 线', exact: true });
    await slider.waitFor();
    // A fixed accent lets us measure the painted thumb without a screenshot baseline.
    await slider.evaluate((element) => {
      element.style.accentColor = 'rgb(0, 120, 255)';
    });
    const detail = page.locator('.okx-candle-detail strong');
    const latest = await detail.textContent();
    await slider.scrollIntoViewIfNeeded();
    const box = await slider.boundingBox();
    const y = box.y + box.height / 2;
    await page.mouse.move(box.x + box.width - 24, y);
    await page.mouse.down();
    await page.mouse.move(box.x + 1, y, { steps: 12 });
    await page.mouse.up();
    assert.equal(await slider.inputValue(), '0');
    assert.notEqual(await detail.textContent(), latest);

    const paintedThumb = async () => {
      const bytes = [...(await slider.screenshot())];
      return page.evaluate(async (bytes) => {
        const image = await createImageBitmap(
          new Blob([new Uint8Array(bytes)], { type: 'image/png' }),
        );
        const canvas = new OffscreenCanvas(image.width, image.height);
        const context = canvas.getContext('2d');
        context.drawImage(image, 0, 0);
        const pixels = context.getImageData(0, 0, image.width, image.height).data;
        let left = image.width,
          right = -1;
        for (let y = 0; y < image.height; y++) {
          for (let x = 0; x < image.width; x++) {
            const index = (y * image.width + x) * 4;
            if (pixels[index] < 50 && pixels[index + 1] < 180 && pixels[index + 2] > 150) {
              left = Math.min(left, x);
              right = Math.max(right, x);
            }
          }
        }
        image.close();
        return { left, right, width: canvas.width };
      }, bytes);
    };
    const first = await paintedThumb();
    assert.ok(first.left <= 3, `first candle leaves ${first.left}px before the thumb`);
    await page.mouse.move(box.x + 24, y);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width - 1, y, { steps: 12 });
    await page.mouse.up();
    assert.equal(await slider.inputValue(), '2');
    assert.equal(await detail.textContent(), latest);
    const last = await paintedThumb();
    const gap = last.width - last.right - 1;
    assert.ok(gap <= 3, `last candle leaves ${gap}px after the thumb`);
  } finally {
    await page.close();
  }
});

test('OKX market lists every spot pair even after saving a favorite', async () => {
  const page = await open(false);
  try {
    const list = page.locator('.okx-quote-list');
    await list.getByRole('button', { name: /BTC-USDT/ }).waitFor();
    assert.equal(await list.getByRole('button').count(), 4);
    await list.getByRole('button', { name: /BTC-USDC/ }).waitFor();
    await list.getByRole('button', { name: /ETH-BTC/ }).waitFor();

    await page.getByRole('button', { name: '收藏 BTC-USDT', exact: true }).click();
    await page.getByRole('button', { name: '取消收藏 BTC-USDT', exact: true }).waitFor();
    assert.equal(await list.getByRole('button').count(), 4);
  } finally {
    await page.close();
  }
});

test('OKX market search matches currencies and pairs and clears an empty result', async () => {
  const page = await open(false);
  try {
    const list = page.locator('.okx-quote-list');
    const search = page.getByLabel('搜索现货交易对', { exact: true });
    await page.getByRole('img', { name: /BTC-USDT K 线/ }).waitFor();
    await search.fill('  usdc  ');
    assert.equal(await list.getByRole('button').count(), 1);
    await list.getByRole('button', { name: /BTC-USDC/ }).waitFor();

    await search.fill('eth');
    assert.equal(await list.getByRole('button').count(), 2);
    await list.getByRole('button', { name: /ETH-BTC/ }).click();
    assert.equal(await page.getByLabel('现货交易对', { exact: true }).inputValue(), 'ETH-BTC');
    await page.getByRole('img', { name: /ETH-BTC K 线/ }).waitFor();

    await search.fill('eth-btc');
    assert.equal(await list.getByRole('button').count(), 1);
    await search.fill('NO-SUCH-COIN');
    assert.equal(await list.getByRole('button').count(), 0);
    await page.getByText('未找到匹配的现货交易对。', { exact: true }).waitFor();
    await page.getByRole('button', { name: '清空搜索', exact: true }).click();
    assert.equal(await search.inputValue(), '');
    assert.equal(await list.getByRole('button').count(), 4);
    assert.equal(await page.getByLabel('现货交易对', { exact: true }).inputValue(), 'ETH-BTC');
  } finally {
    await page.close();
  }
});

test('OKX market scroll reaches later spot pairs on mobile', async () => {
  const instruments = [
    'BTC-USDT',
    ...Array.from({ length: 60 }, (_, index) => `COIN${index}-USDT`),
    'LAST-BTC',
  ];
  const page = await open(false, instruments);
  try {
    const list = page.locator('.okx-quote-list');
    await list.getByRole('button', { name: /BTC-USDT/ }).waitFor();
    const scroll = await list.evaluate((element) => {
      element.scrollTop = element.scrollHeight;
      return {
        top: element.scrollTop,
        height: element.clientHeight,
        content: element.scrollHeight,
      };
    });
    assert.ok(scroll.content > scroll.height);
    assert.ok(scroll.top > 0);
    await list.getByRole('button', { name: /LAST-BTC/ }).click();
    await page.getByRole('img', { name: /LAST-BTC K 线/ }).waitFor();
    assert.equal(await page.getByLabel('现货交易对', { exact: true }).inputValue(), 'LAST-BTC');
    assert.equal(
      await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
      true,
    );
  } finally {
    await page.close();
  }
});

test('OKX public markets open without credentials while private data links to credential settings', async () => {
  const page = await open(false);
  try {
    await page.getByRole('tab', { name: 'OKX 行情', exact: true }).waitFor();
    await page.getByRole('img', { name: /BTC-USDT K 线/ }).waitFor();
    assert.equal(
      await page.evaluate(() =>
        window.workflowCalls.some((call) => call.command === 'okx_tickers'),
      ),
      true,
    );
    assert.equal(await page.getByLabel('API Key', { exact: true }).count(), 0);

    await page.getByRole('tab', { name: '资产总览', exact: true }).click();
    await page.getByRole('button', { name: '配置私有凭据', exact: true }).waitFor();
    await page.getByRole('button', { name: '配置私有凭据', exact: true }).click();
    await page.getByRole('heading', { name: '只读 API', exact: true }).waitFor();
    assert.equal(await page.getByLabel('API Key', { exact: true }).count(), 4);
    assert.equal(
      await page.evaluate(() =>
        window.workflowCalls.some((call) => call.command === 'okx_place_order'),
      ),
      false,
    );
  } finally {
    await page.close();
  }
});

test('OKX trading tab guides users to trade credentials without sending exchange requests', async () => {
  const page = await open(false);
  try {
    await page.getByRole('tab', { name: '交易下单', exact: true }).click();
    await page.getByText(/请配置 OKX 模拟盘交易 API/).waitFor();
    const calls = await page.evaluate(() => window.workflowCalls);
    assert.equal(
      calls.some((call) => call.command === 'okx_trade_context'),
      false,
    );
    assert.equal(
      calls.some((call) => call.command === 'okx_place_order'),
      false,
    );
    assert.equal(
      calls.some((call) => call.command === 'okx_order_history'),
      false,
    );
  } finally {
    await page.close();
  }
});

test('completed OKX orders load only on request and render their snake-case snapshot fields', async () => {
  const page = await open(false);
  try {
    await saveOkxTradeKey(page, 'okx_trade_demo');
    await page.getByRole('tab', { name: '交易下单', exact: true }).click();
    assert.equal(
      await page.evaluate(() =>
        window.workflowCalls.some((call) => call.command === 'okx_order_history'),
      ),
      false,
    );
    await page.evaluate(() => {
      window.orderHistoryResult = [
        {
          inst_id: 'BTC-USDT',
          order_id: '123456',
          client_order_id: 'history-client-123',
          inst_type: 'SPOT',
          side: 'buy',
          order_type: 'market',
          trade_mode: 'cash',
          sz: '0.001',
          price: '',
          average_price: '50000',
          accumulated_fill_size: '0.001',
          state: 'filled',
          position_side: 'net',
          reduce_only: 'false',
          target_currency: 'base_ccy',
          created_at: '1708587373361',
          updated_at: '1708587373361',
        },
      ];
    });
    await page.getByRole('button', { name: '读取历史订单', exact: true }).click();
    const history = page.getByRole('region', { name: '历史订单', exact: true });
    await history.getByText('filled', { exact: true }).waitFor();
    await history.getByText(/123456 .*history-client-123/).waitFor();
    const request = await page.evaluate(() =>
      window.workflowCalls.find((call) => call.command === 'okx_order_history'),
    );
    assert.equal(request.args.instType, 'SPOT');
    assert.equal(request.args.environment, 'demo');
  } finally {
    await page.close();
  }
});

test('Demo spot preview waits for second confirmation and submits one alphanumeric client order ID', async () => {
  const page = await open(false);
  try {
    await saveOkxTradeKey(page, 'okx_trade_demo');
    await page.getByRole('tab', { name: '交易下单', exact: true }).click();
    await page.getByLabel('交易环境', { exact: true }).selectOption('demo');
    await page.getByLabel('产品类型', { exact: true }).selectOption('SPOT');
    await page.getByLabel('交易品种', { exact: true }).fill('BTC-USDT');
    await page.getByLabel('交易方向', { exact: true }).selectOption('buy');
    await page.getByLabel('订单类型', { exact: true }).selectOption('market');
    await page.getByLabel(/委托数量.*BTC/).fill('0.001');
    await page.getByRole('button', { name: '读取交易上下文', exact: true }).click();
    await page.getByText('最新交易上下文 · BTC-USDT', { exact: true }).waitFor();
    await page.getByRole('button', { name: '预览订单', exact: true }).click();
    await page.getByRole('dialog', { name: '确认 OKX 订单' }).waitFor();
    await page
      .getByRole('dialog', { name: '确认 OKX 订单' })
      .getByText(/滑点上限 1%/)
      .waitFor();
    assert.equal(await page.getByLabel('最大滑点（%）').inputValue(), '1');
    assert.equal(
      await page.evaluate(
        () => window.workflowCalls.filter((call) => call.command === 'okx_place_order').length,
      ),
      0,
    );
    await page.getByRole('button', { name: '确认并下单', exact: true }).click();
    await page.getByText('订单已提交，尚未确认成交', { exact: true }).waitFor();
    await page.getByText(/OKX 订单号 123456 .*sCode 0/).waitFor();
    const call = await page.evaluate(() =>
      window.workflowCalls.find((item) => item.command === 'okx_place_order'),
    );
    assert.equal(call.args.draft.environment, 'demo');
    assert.equal(call.args.draft.inst_type, 'SPOT');
    assert.equal(call.args.draft.inst_id, 'BTC-USDT');
    assert.equal(call.args.draft.side, 'buy');
    assert.equal(call.args.draft.ord_type, 'market');
    assert.equal(call.args.draft.size, '0.001');
    assert.equal(call.args.draft.slippage_pct, '0.01');
    assert.equal(call.args.draft.cl_ord_id.length, 32);
    assert.match(call.args.draft.cl_ord_id, /^[A-Za-z0-9]{32}$/);
    assert.equal(
      await page.evaluate(
        () => window.workflowCalls.filter((item) => item.command === 'okx_place_order').length,
      ),
      1,
    );
  } finally {
    await page.close();
  }
});

test('editing a draft while fresh context is pending cannot reopen a stale order review', async () => {
  const page = await open(false);
  try {
    await saveOkxTradeKey(page, 'okx_trade_demo');
    await page.getByRole('tab', { name: '交易下单', exact: true }).click();
    await page.getByLabel(/委托数量.*BTC/).fill('0.001');
    await page.evaluate(() => {
      window.tradeContextDelay = 500;
    });
    await page.getByRole('button', { name: '预览订单', exact: true }).click();
    await page.waitForFunction(() =>
      window.workflowCalls.some((call) => call.command === 'okx_trade_context'),
    );
    await page.getByLabel(/委托数量.*BTC/).fill('0.002');
    await page.waitForFunction(() => window.tradeContextCallsFinished === 1);
    await page.waitForFunction(() =>
      Array.from(document.querySelectorAll('button')).some(
        (button) => button.textContent.trim() === '预览订单' && !button.disabled,
      ),
    );
    assert.equal(await page.getByRole('dialog', { name: '确认 OKX 订单' }).count(), 0);
    assert.equal(await page.getByLabel(/委托数量.*BTC/).inputValue(), '0.002');
    assert.equal(
      await page.evaluate(() =>
        window.workflowCalls.some((call) => call.command === 'okx_place_order'),
      ),
      false,
    );
  } finally {
    await page.close();
  }
});

test('changing environment while context is pending discards old-mode pending orders', async () => {
  const page = await open(false);
  try {
    await saveOkxTradeKey(page, 'okx_trade_demo');
    await saveOkxTradeKey(page, 'okx_trade_live');
    await page.getByRole('tab', { name: '交易下单', exact: true }).click();
    await page.getByLabel('产品类型', { exact: true }).selectOption('SWAP');
    await page.getByLabel('交易品种', { exact: true }).fill('BTC-USDT-SWAP');
    await page.evaluate(() => {
      window.tradeContextDelay = 500;
    });
    await page.getByRole('button', { name: '读取交易上下文', exact: true }).click();
    await page.waitForFunction(() =>
      window.workflowCalls.some((call) => call.command === 'okx_trade_context'),
    );
    await page.getByLabel('交易环境', { exact: true }).selectOption('production');
    await page.waitForFunction(() => window.tradeContextCallsFinished === 1);
    await page.waitForFunction(() =>
      Array.from(document.querySelectorAll('button')).some(
        (button) => button.textContent.trim() === '读取交易上下文',
      ),
    );
    assert.equal(await page.getByRole('region', { name: '最新交易上下文' }).count(), 0);
    assert.equal(await page.getByRole('button', { name: '撤单', exact: true }).count(), 0);
    const contextCall = await page.evaluate(() =>
      window.workflowCalls.find((call) => call.command === 'okx_trade_context'),
    );
    assert.equal(contextCall.args.environment, 'demo');
    assert.equal(await page.getByText(/当前环境：实盘/).count(), 1);
    await page.evaluate(() => {
      window.orderHistoryDelay = 500;
    });
    await page.getByRole('button', { name: '读取历史订单', exact: true }).click();
    await page.waitForFunction(() =>
      window.workflowCalls.some((call) => call.command === 'okx_order_history'),
    );
    await page.getByLabel('交易环境', { exact: true }).selectOption('demo');
    await page.waitForFunction(() => window.orderHistoryCallsFinished === 1);
    await page.waitForFunction(() =>
      Array.from(document.querySelectorAll('button')).some(
        (button) => /^(读取|刷新)历史订单$/.test(button.textContent.trim()) && !button.disabled,
      ),
    );
    assert.equal(await page.getByRole('region', { name: '历史订单', exact: true }).count(), 0);
  } finally {
    await page.close();
  }
});

test('spot market sell uses quote-currency size and production requires its separate key', async () => {
  const page = await open(false);
  try {
    await saveOkxTradeKey(page, 'okx_trade_demo');
    await page.getByRole('tab', { name: '交易下单', exact: true }).click();
    await page.getByLabel('交易环境', { exact: true }).selectOption('production');
    await page.getByText(/请配置 OKX 实盘交易 API/).waitFor();
    assert.equal(
      await page.evaluate(() =>
        window.workflowCalls.some((call) => call.command === 'okx_trade_context'),
      ),
      false,
    );
    await saveOkxTradeKey(page, 'okx_trade_live');
    await page.getByRole('tab', { name: '交易下单', exact: true }).click();
    await page.getByLabel('交易环境', { exact: true }).selectOption('production');
    await page.getByText(/实盘.*真实资产/).waitFor();
    await page.getByLabel('产品类型', { exact: true }).selectOption('SPOT');
    await page.getByLabel('交易品种', { exact: true }).fill('BTC-USDT');
    await page.getByLabel('交易方向', { exact: true }).selectOption('sell');
    await page.getByLabel('订单类型', { exact: true }).selectOption('market');
    await page.getByLabel(/委托数量.*USDT/).fill('10');
    await page.getByRole('button', { name: '读取交易上下文', exact: true }).click();
    await page.getByRole('button', { name: '预览订单', exact: true }).click();
    const review = page.getByRole('dialog', { name: '确认 OKX 订单' });
    await review
      .getByText('实盘订单会使用真实资产下单，请核对全部字段后再确认。', { exact: true })
      .waitFor();
    await review.getByText(/10 USDT.*市价卖出收到的报价币数量/).waitFor();
    await page.getByRole('button', { name: '确认并下单', exact: true }).click();
    const order = await page.evaluate(() =>
      window.workflowCalls.find((call) => call.command === 'okx_place_order'),
    );
    assert.equal(order.args.draft.environment, 'production');
    assert.equal(order.args.draft.side, 'sell');
    assert.equal(order.args.draft.size, '10');
  } finally {
    await page.close();
  }
});

test('contract order confirmation sends the selected action, margin mode, size and mapped side', async () => {
  const page = await open(false);
  try {
    await saveOkxTradeKey(page, 'okx_trade_demo');
    await page.getByRole('tab', { name: '交易下单', exact: true }).click();
    await page.getByLabel('产品类型', { exact: true }).selectOption('SWAP');
    await page.getByLabel('交易品种', { exact: true }).fill('BTC-USDT-SWAP');
    await page.getByLabel('合约操作', { exact: true }).selectOption('open_short');
    await page.getByLabel('保证金模式', { exact: true }).selectOption('isolated');
    await page.getByLabel('订单类型', { exact: true }).selectOption('market');
    await page.getByLabel('委托数量（张）', { exact: true }).fill('3');
    await page.getByRole('button', { name: '读取交易上下文', exact: true }).click();
    await page.getByRole('button', { name: '预览订单', exact: true }).click();
    await page.getByRole('button', { name: '确认并下单', exact: true }).click();
    await page.getByText('订单已提交，尚未确认成交', { exact: true }).waitFor();
    const call = await page.evaluate(() =>
      window.workflowCalls.find((item) => item.command === 'okx_place_order'),
    );
    assert.equal(call.args.draft.inst_type, 'SWAP');
    assert.equal(call.args.draft.inst_id, 'BTC-USDT-SWAP');
    assert.equal(call.args.draft.position_action, 'open_short');
    assert.equal(call.args.draft.td_mode, 'isolated');
    assert.equal(call.args.draft.side, 'sell');
    assert.equal(call.args.draft.size, '3');
    assert.equal(call.args.draft.slippage_pct, null);
  } finally {
    await page.close();
  }
});

test('inverse contract limit prices use the quote currency, not settlement currency', async () => {
  const page = await open(false);
  try {
    await saveOkxTradeKey(page, 'okx_trade_demo');
    await page.getByRole('tab', { name: '交易下单', exact: true }).click();
    await page.getByLabel('产品类型', { exact: true }).selectOption('SWAP');
    await page.getByLabel('交易品种', { exact: true }).fill('BTC-USD-SWAP');
    await page.getByLabel('订单类型', { exact: true }).selectOption('limit');
    await page.getByLabel(/委托数量.*张/).fill('1');
    await page.getByLabel('委托价格（USD）').fill('10000');
    await page.getByRole('button', { name: '读取交易上下文', exact: true }).click();
    await page.getByRole('button', { name: '预览订单', exact: true }).click();
    const review = page.getByRole('dialog', { name: '确认 OKX 订单' });
    await review.waitFor();
    await review.getByText('10000 USD', { exact: true }).waitFor();
    assert.match(await review.innerText(), /100 USD 合约面值/);
    assert.equal(
      await page.evaluate(() =>
        window.workflowCalls.some((call) => call.command === 'okx_place_order'),
      ),
      false,
    );
  } finally {
    await page.close();
  }
});

test('unknown submit keeps its ID and requires same-ID order lookup before another submit', async () => {
  const page = await open(false);
  try {
    await saveOkxTradeKey(page, 'okx_trade_demo');
    await page.evaluate(() => {
      window.nextOrderResult = {
        status: 'unknown',
        client_order_id: '0123456789abcdef0123456789abcdef',
        acknowledgement: null,
        error_code: 'provider_failure',
      };
      window.orderDetailResult = null;
    });
    await page.getByRole('tab', { name: '交易下单', exact: true }).click();
    await page.getByLabel('委托数量（BTC）').fill('0.001');
    await page.getByRole('button', { name: '读取交易上下文', exact: true }).click();
    await page.getByRole('button', { name: '预览订单', exact: true }).click();
    await page.getByRole('button', { name: '确认并下单', exact: true }).click();
    await page.getByText(/结果未确认/).waitFor();
    const firstId = await page.getByLabel('待核对客户订单号').inputValue();
    assert.equal(firstId, '0123456789abcdef0123456789abcdef');
    assert.equal(
      await page.getByRole('button', { name: '预览订单', exact: true }).isDisabled(),
      true,
    );
    await page.getByRole('button', { name: '查询此订单状态', exact: true }).click();
    await page.getByText(/暂未查询到此订单/).waitFor();
    assert.equal(
      await page.getByRole('button', { name: '预览订单', exact: true }).isDisabled(),
      true,
    );
    await page.evaluate(() => {
      window.orderDetailResult = {
        inst_id: 'BTC-USDT',
        order_id: '123456',
        inst_type: 'SPOT',
        side: 'buy',
        order_type: 'market',
        trade_mode: 'cash',
        sz: '0.001',
        price: '',
        average_price: '50000',
        accumulated_fill_size: '0.001',
        state: 'filled',
        position_side: 'net',
        reduce_only: 'false',
        target_currency: 'base_ccy',
        created_at: '1708587373361',
        updated_at: '1708587373361',
      };
    });
    await page.getByRole('button', { name: '查询此订单状态', exact: true }).click();
    await page.getByText('filled', { exact: true }).waitFor();
    await page.getByText(/已成交数量 0\.001 \/ 0\.001/).waitFor();
    const lookup = await page.evaluate(() =>
      window.workflowCalls.find((call) => call.command === 'okx_order_detail'),
    );
    assert.equal(lookup.args.clOrdId, firstId);
    assert.equal(
      await page.getByRole('button', { name: '预览订单', exact: true }).isDisabled(),
      true,
    );
    await page.getByRole('button', { name: '已核对订单状态，允许新订单预览' }).click();
    assert.equal(
      await page.getByRole('button', { name: '预览订单', exact: true }).isDisabled(),
      false,
    );
    assert.equal(
      await page.evaluate(
        () => window.workflowCalls.filter((item) => item.command === 'okx_place_order').length,
      ),
      1,
    );
  } finally {
    await page.close();
  }
});

test('pre-send provider failure remains actionable and is not treated as an unknown submission', async () => {
  const page = await open(false);
  try {
    await saveOkxTradeKey(page, 'okx_trade_demo');
    await page.evaluate(() => {
      window.nextOrderError = 'network_failure';
    });
    await page.getByRole('tab', { name: '交易下单', exact: true }).click();
    await page.getByLabel(/委托数量.*BTC/).fill('0.001');
    await page.getByRole('button', { name: '预览订单', exact: true }).click();
    await page.getByRole('button', { name: '确认并下单', exact: true }).click();
    const review = page.getByRole('dialog', { name: '确认 OKX 订单' });
    await review.getByText(/无法连接 OKX/).waitFor();
    assert.equal(await page.getByLabel('待核对客户订单号').count(), 0);
    assert.equal(await page.getByRole('button', { name: '查询此订单状态' }).count(), 0);
    assert.equal(
      await page.evaluate(
        () => window.workflowCalls.filter((call) => call.command === 'okx_place_order').length,
      ),
      1,
    );
  } finally {
    await page.close();
  }
});

test('selected incomplete contract order needs cancel confirmation and passes its exact IDs', async () => {
  const page = await open(false);
  try {
    await saveOkxTradeKey(page, 'okx_trade_demo');
    await page.getByRole('tab', { name: '交易下单', exact: true }).click();
    await page.getByLabel('产品类型', { exact: true }).selectOption('SWAP');
    await page.getByLabel('交易品种', { exact: true }).fill('BTC-USDT-SWAP');
    await page.getByRole('button', { name: '读取交易上下文', exact: true }).click();
    const pending = page.getByRole('region', { name: '当前挂单', exact: true });
    await pending.getByText(/987654/).waitFor();
    await pending.getByRole('button', { name: '撤单', exact: true }).click();
    await page.getByRole('dialog', { name: '确认撤单' }).waitFor();
    assert.equal(
      await page.evaluate(() =>
        window.workflowCalls.some((call) => call.command === 'okx_cancel_order'),
      ),
      false,
    );
    await page.getByRole('button', { name: '确认撤销订单', exact: true }).click();
    await page.getByText('撤单请求已接受', { exact: true }).waitFor();
    const cancel = await page.evaluate(() =>
      window.workflowCalls.find((call) => call.command === 'okx_cancel_order'),
    );
    assert.equal(cancel.args.instId, 'BTC-USDT-SWAP');
    assert.equal(cancel.args.ordId, '987654');
    assert.equal(
      await page.evaluate(
        () => window.workflowCalls.filter((item) => item.command === 'okx_trade_context').length,
      ),
      2,
    );
  } finally {
    await page.close();
  }
});

test('uncertain cancellation blocks another cancel until refreshed pending orders are read', async () => {
  const page = await open(false);
  try {
    await saveOkxTradeKey(page, 'okx_trade_demo');
    await page.getByRole('tab', { name: '交易下单', exact: true }).click();
    await page.getByLabel('产品类型', { exact: true }).selectOption('SWAP');
    await page.getByLabel('交易品种', { exact: true }).fill('BTC-USDT-SWAP');
    await page.getByRole('button', { name: '读取交易上下文', exact: true }).click();
    const pending = page.getByRole('region', { name: '当前挂单', exact: true });
    await pending.getByText(/987654/).waitFor();
    await page.evaluate(() => {
      window.nextCancelError = 'order_cancel_requires_reconciliation:987654';
    });
    await pending.getByRole('button', { name: '撤单', exact: true }).click();
    await page.getByRole('button', { name: '确认撤销订单', exact: true }).click();
    await page
      .getByRole('region', { name: '待核对撤单' })
      .getByText(/订单 987654/)
      .waitFor();
    assert.equal(await pending.getByRole('button', { name: '撤单', exact: true }).count(), 0);
    assert.equal(
      await page.getByRole('button', { name: '预览订单', exact: true }).isDisabled(),
      true,
    );
    assert.equal(
      await page.evaluate(
        () => window.workflowCalls.filter((call) => call.command === 'okx_cancel_order').length,
      ),
      1,
    );
    await page.getByRole('button', { name: '重新读取交易上下文', exact: true }).click();
    await page.getByRole('button', { name: '撤单', exact: true }).waitFor();
    assert.equal(
      await page.evaluate(
        () => window.workflowCalls.filter((call) => call.command === 'okx_trade_context').length,
      ),
      2,
    );
    assert.equal(
      await page.evaluate(
        () => window.workflowCalls.filter((call) => call.command === 'okx_cancel_order').length,
      ),
      1,
    );
  } finally {
    await page.close();
  }
});

test('OKX assets show separate accounts and preserve tiny quantities and unavailable values', async () => {
  const page = await open(true);
  try {
    await page.getByRole('tab', { name: '资产总览', exact: true }).click();
    await page.getByText('12.340987654321', { exact: true }).waitFor();
    await page.getByText('0.00000001', { exact: true }).first().waitFor();
    await page.getByText('交易账户', { exact: true }).first().waitFor();
    await page.getByText('资金账户', { exact: true }).first().waitFor();
    assert.match(await page.locator('[data-crypto-balance="trading:BTC"]').innerText(), /可用\s*—/);
  } finally {
    await page.close();
  }
});

test('OKX fills import into independent review records with both assets and keep confirmation on reimport', async () => {
  const page = await open(true);
  try {
    await page.getByRole('tab', { name: '加密流水', exact: true }).click();
    await page.getByLabel('流水来源', { exact: true }).selectOption('fills');
    await page.getByRole('button', { name: '读取并导入候选', exact: true }).click();
    const row = page.locator('[data-crypto-key="okx:123:fill:456"]');
    await row.getByText('-99.99985572 USDT', { exact: true }).waitFor();
    await row.getByText('0.00192834 BTC', { exact: true }).waitFor();
    await row.getByRole('button', { name: '确认加密记录', exact: true }).click();
    await page.getByLabel('记录状态', { exact: true }).selectOption('confirmed');
    await row.getByText('已确认', { exact: true }).waitFor();
    await page.getByRole('button', { name: '读取并导入候选', exact: true }).click();
    await row.getByText('已确认', { exact: true }).waitFor();
    assert.equal(
      await page.evaluate(() =>
        window.workflowCalls.some((c) =>
          ['write_transaction', 'confirm_pending', 'save_entry'].includes(c.command),
        ),
      ),
      false,
    );
  } finally {
    await page.close();
  }
});

test('contract performance keeps realized net pnl separate from its included fees', async () => {
  const page = await open(true);
  try {
    await page.getByRole('tab', { name: '合约表现', exact: true }).click();
    const history = page.getByRole('region', { name: '历史持仓', exact: true });
    await history.getByText('0.001 USDT', { exact: true }).waitFor();
    await history.getByText('-0.0001 USDT', { exact: true }).waitFor();
    await page.getByText(/已实现净盈亏已包含手续费/).waitFor();
  } finally {
    await page.close();
  }
});

test('replacing configured OKX credentials resets the old account workspace', async () => {
  const page = await open(true);
  try {
    await page.getByRole('tab', { name: '资产总览', exact: true }).click();
    await page.getByText('12.340987654321', { exact: true }).waitFor();
    await page.getByRole('button', { name: 'API 凭据设置', exact: true }).click();
    assert.equal(await page.getByText('12.340987654321', { exact: true }).count(), 0);
    const card = page.locator('[data-provider="okx"]');
    await card.getByLabel('API Key', { exact: true }).fill('second-account-key');
    await card.getByLabel('Secret Key', { exact: true }).fill('second-account-secret');
    await card.getByLabel('Passphrase', { exact: true }).fill('second-passphrase');
    await card.getByRole('button', { name: '保存凭据', exact: true }).click();
    await page
      .getByRole('tab', { name: 'OKX 行情', exact: true })
      .and(page.locator('[aria-selected="true"]'))
      .waitFor();
    assert.equal(await page.getByText('12.340987654321', { exact: true }).count(), 0);
    await page.getByRole('tab', { name: '资产总览', exact: true }).click();
    await page.getByText('12.340987654321', { exact: true }).waitFor();
    assert.equal(
      await page.evaluate(
        () => window.workflowCalls.filter((c) => c.command === 'okx_assets').length,
      ),
      2,
    );
  } finally {
    await page.close();
  }
});
