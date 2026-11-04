import assert from 'node:assert/strict';
import { test } from 'node:test';
import { browserSuite } from './helpers/browser.mjs';
import { installWorkflowIpc } from './helpers/workflow-ipc.mjs';
import { navigate } from './helpers/navigation.mjs';
const { url, newPage } = browserSuite('1447');

async function open(configured = false) {
  const page = await newPage({ viewport: { width: 390, height: 844 } });
  await installWorkflowIpc(page);
  await page.addInitScript(() => {
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
    window.__TAURI_INTERNALS__.invoke = async (command, args) => {
      if (!command.startsWith('okx_')) return original(command, args);
      window.workflowCalls.push({ command, args });
      if (command === 'okx_tickers') return [ticker('BTC-USDT'), ticker('ETH-USDT')];
      if (command === 'okx_watchlist') return watchlist;
      if (command === 'okx_save_watchlist') {
        watchlist = args.instruments;
        return;
      }
      if (command === 'okx_candles') return [candle(args.after ? 1708570000000 : 1708580000000)];
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
      throw Error('Unexpected OKX command');
    };
  });
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
    await page.getByLabel('API Key', { exact: true }).waitFor();
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
    assert.equal(await page.getByLabel('API Key', { exact: true }).count(), 3);
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
