// Mock only the native IPC boundary. Browser tests exercise the real React UI.
export async function installWorkflowIpc(page, seed = {}) {
  await page.addInitScript((seedData) => {
    let accounts = seedData.accounts ?? [
      {
        id: 'wallet',
        name: '钱包',
        category: 'wallet',
        currency_code: 'CNY',
        currency_exponent: 2,
      },
      { id: 'bank', name: '银行', category: 'bank', currency_code: 'CNY', currency_exponent: 2 },
    ];
    let reference = null;
    let pending = seedData.pending ?? [];
    let reversed = false;
    const connectionStatus = new Map(
      [
        'wechat_pay',
        'alipay',
        'binance',
        'binance_trade_demo',
        'binance_trade_live',
        'binance_withdraw',
        'okx',
        'okx_trade_demo',
        'okx_trade_live',
        'okx_withdraw',
        'ifind',
      ].map((provider) => [provider, false]),
    );
    window.workflowCalls = [];
    window.savedTransactions = [];
    window.__TAURI_INTERNALS__ = {
      invoke: async (command, args) => {
        if (command === 'privacy_status')
          return { locked: false, lock_enabled: false, hide_amounts: false, auto_lock_minutes: 5 };
        window.workflowCalls.push({ command, args });
        if (command === 'provider_connections')
          return [...connectionStatus].map(([provider, configured]) => ({ provider, configured }));
        if (command === 'save_provider_credentials') {
          const provider = args.input.provider;
          connectionStatus.set(provider, true);
          return { provider, configured: true };
        }
        if (command === 'remove_provider_credentials') {
          connectionStatus.set(args.provider, false);
          return { provider: args.provider, configured: false };
        }
        if (command === 'connections_test') {
          const provider = args.request.provider;
          if (provider === 'wechat_pay' || provider === 'alipay')
            return {
              provider,
              outcome: 'connected',
              summary: {
                kind: 'bill_read',
                data: {
                  bill_date: args.request.bill_date,
                  downloaded_bytes: 1024,
                  hash_verified: true,
                },
              },
              error_code: null,
            };
          if (provider === 'ifind')
            return {
              provider,
              outcome: 'connected',
              summary: {
                kind: 'ifind_quote',
                data: {
                  quote: {
                    symbol: args.request.symbol,
                    quote_time: '2026-10-30 15:00',
                    last: '10.25',
                  },
                },
              },
              error_code: null,
            };
          return {
            provider,
            outcome: 'connected',
            summary: {
              kind: provider === 'binance' ? 'binance_balances' : 'okx_balances',
              data: { balances: [{ asset: 'USDT', free: '12.5', locked: '0' }] },
            },
            error_code: null,
          };
        }
        if (command === 'binance_api_catalogue')
          return [
            {
              id: 'binance.rest.spot.get.api_v3_ticker_price',
              product: 'Spot REST API',
              category: 'Market Data',
              title: 'Ticker Price',
              method: 'GET',
              path: '/api/v3/ticker/price',
              host: 'api',
              transport: 'rest',
              auth: 'public',
              mutating: false,
              params: [
                {
                  name: 'symbol',
                  type: 'string',
                  required: false,
                  location: 'query',
                  description: '',
                  example: 'BTCUSDT',
                },
              ],
              doc_url: 'https://developers.binance.com/zh-CN/docs/catalog',
            },
          ];
        if (command === 'ifind_api_catalogue')
          return [
            {
              id: 'ifind.rest.post.real_time_quotation',
              product: 'iFinD HTTP API',
              category: '官方数据服务',
              title: '实时行情',
              method: 'POST',
              path: '/api/v1/real_time_quotation',
              host: 'quantapi.51ifind.com',
              transport: 'rest',
              auth: 'access_token',
              mutating: false,
              params: [
                {
                  name: 'codes',
                  type: 'string',
                  required: false,
                  location: 'json',
                  description: '',
                  example: '300033.SZ',
                },
              ],
              sample: { codes: '300033.SZ' },
              doc_url:
                'https://quantapi.51ifind.com/gwstatic/static/ds_web/quantapi-web/help-center/manual.html',
            },
          ];
        if (command === 'binance_api_request') {
          const id = args.request.endpoint_id;
          let data = { symbol: 'BTCUSDT', price: '100.00' };
          if (id.endsWith('api_v3_exchangeinfo'))
            data = {
              symbols: [
                {
                  symbol: 'BTCUSDT',
                  baseAsset: 'BTC',
                  quoteAsset: 'USDT',
                  status: 'TRADING',
                  isSpotTradingAllowed: true,
                },
              ],
            };
          if (id.endsWith('api_v3_ticker_24hr'))
            data = [
              {
                symbol: 'BTCUSDT',
                lastPrice: '100.00',
                openPrice: '99.00',
                highPrice: '101.00',
                lowPrice: '98.00',
                volume: '12.5',
                priceChangePercent: '1.01',
                closeTime: 1708587373361,
              },
            ];
          if (id.endsWith('api_v3_klines'))
            data = [[1708580000000, '99', '101', '98', '100', '12.5', 1708583600000]];
          if (id.endsWith('api_v3_account'))
            data = { balances: [{ asset: 'USDT', free: '12.5', locked: '0' }] };
          return {
            status: 'success',
            code: '0',
            message: null,
            data,
            request_id: args.request.request_id,
            http_status: 200,
          };
        }
        if (command === 'ifind_api_request')
          return {
            status: 'success',
            code: '0',
            message: null,
            data: { symbol: 'BTCUSDT', price: '100.00' },
            request_id: args.request.request_id,
            http_status: 200,
          };
        if (command === 'binance_socket_request')
          return {
            status: 'success',
            code: '0',
            message: null,
            data: { result: null },
            request_id: args.request.request_id,
            http_status: 101,
          };
        if (command === 'recurring_state')
          return { templates: [], reminders: [], generated_count: 0, has_more: false };
        if (command === 'list_budgets' || command === 'budget_reminders') return [];
        if (command === 'report_summary')
          return {
            income: '20.00',
            expense: '5.00',
            net: '15.00',
            transaction_count: 1,
            categories: [],
            merchants: [],
            merchant_total: 0,
            trend: [],
            balances: [
              { account_id: 'wallet', name: '人民币账户', amount: '100.00', currency_code: 'CNY' },
              { account_id: 'usd', name: '美元账户', amount: '20.00', currency_code: 'USD' },
            ],
          };
        if (command === 'list_accounts') return accounts.map((a) => ({ ...a }));
        if (command === 'create_account') {
          if (window.holdSave)
            await new Promise((resolve) => {
              window.releaseSave = resolve;
            });
          const currencyCode = args.currencyCode ?? 'CNY';
          const account = {
            id: 'created',
            name: args.name,
            category: args.category,
            currency_code: currencyCode,
            currency_exponent: ['JPY', 'KRW'].includes(currencyCode) ? 0 : 2,
          };
          accounts.push(account);
          return { ...account };
        }
        if (command === 'update_platform') {
          accounts = accounts.map((a) =>
            a.id === args.accountId ? { ...a, name: args.name, category: args.category } : a,
          );
          return;
        }
        if (command === 'delete_platform') {
          accounts = accounts.filter((a) => a.id !== args.accountId);
          return;
        }
        if (command === 'get_day') {
          if (window.delayWallet && args.accountId === 'wallet') {
            await new Promise((resolve) => setTimeout(resolve, 300));
            window.oldDayFinished = true;
          }
          return {
            date: args.date,
            currency_code:
              accounts.find((account) => account.id === args.accountId)?.currency_code ?? null,
            income: '0.00',
            expense: reversed ? '0.00' : '10.00',
            net: reversed ? '0.00' : '-10.00',
            balance: reference?.amount ?? (args.accountId === 'bank' ? '200.00' : '90.00'),
            reference,
            entries: [
              {
                id: 'tx-1',
                kind: 'expense',
                amount: '-10.00',
                currency_code:
                  accounts.find((account) => account.id === args.accountId)?.currency_code ?? 'CNY',
                note: '原消费',
                time: '12:00',
              },
            ],
          };
        }
        if (command === 'set_reference') {
          reference = { date: args.date, amount: args.amount };
          return;
        }
        if (command === 'clear_reference') {
          reference = null;
          return;
        }
        if (command === 'query_entries')
          return { items: pending, total: pending.length, offset: 0, limit: 20 };
        if (command === 'preview_bill')
          return {
            preview_id: 'preview-1',
            rows: [
              {
                id: 'new',
                row: 1,
                date: '2026-10-14',
                amount: '12.30',
                kind: 'expense',
                merchant: '原始商户',
                description: '',
                decision: 'new',
                duplicate_id: null,
                warnings: [],
              },
              {
                id: 'exact',
                row: 2,
                date: '2026-10-14',
                amount: '12.30',
                kind: 'expense',
                merchant: '精确重复商户',
                description: '',
                decision: 'exact_duplicate',
                duplicate_id: 'saved',
                warnings: [],
              },
              {
                id: 'suspected',
                row: 3,
                date: '2026-10-14',
                amount: '12.30',
                kind: 'expense',
                merchant: '疑似商户',
                description: '',
                decision: 'suspected_duplicate',
                duplicate_id: 'maybe',
                warnings: ['需要核对'],
              },
            ],
            errors: [],
            new_count: 1,
            exact_duplicates: 1,
            suspected_duplicates: 1,
            pending_count: 2,
          };
        if (command === 'stage_bill') {
          pending = args.ids.map((id) => {
            const edit = args.edits.find((e) => e.id === id);
            return {
              id,
              status: 'pending',
              kind: 'expense',
              occurred_at: '2026-10-14T12:00:00Z',
              amount: edit?.amount ?? '12.30',
              merchant: edit?.merchant ?? '疑似商户',
              description: '',
              edit_version: 0,
              reversal_of: null,
              account_id: args.accountId,
              category_id: null,
              original_transaction_id: null,
              sources: ['bank'],
              external_ids: [],
            };
          });
          return args.ids;
        }
        if (command === 'list_categories') return [];
        if (command === 'preview_foreign_transaction') {
          if (window.forceManualRate && !args.manualFxRate)
            throw new Error('缺少汇率，请输入手动汇率');
          return {
            amount: args.amount,
            currency_code: args.quoteCurrency,
            report_amount_cny: args.manualFxRate ? '8.23' : '9.80',
            rate: args.manualFxRate ?? '7.10',
            source: args.manualFxRate ? 'manual' : 'reference',
            effective_date: args.manualFxRate ? args.date : '2026-10-29',
            fx_snapshot: {
              rate: args.manualFxRate ?? '7.10',
              effective_date: args.manualFxRate ? args.date : '2026-10-29',
              source: args.manualFxRate ? 'manual' : 'reference',
              provider: args.manualFxRate ? null : 'test',
              provider_sources: args.manualFxRate ? [] : ['fixture'],
            },
          };
        }
        if (command === 'write_transaction') {
          window.savedTransactions.push(args.request);
          return `saved-${window.savedTransactions.length}`;
        }
        if (command === 'transaction_detail')
          return (
            seedData.transaction ?? {
              id: args.id,
              kind: args.id === 'reverse-1' ? 'adjustment' : 'expense',
              status: 'confirmed',
              amount: '10.00',
              date: '2026-10-14',
              account_id: 'wallet',
              destination_id: null,
              category_id: null,
              merchant: '商户',
              description: '',
              original_transaction_id: null,
              edit_version: 0,
              reversal_id: args.id === 'tx-1' && reversed ? 'reverse-1' : null,
              reversal_of: args.id === 'reverse-1' ? 'tx-1' : null,
              linked_ids: reversed ? [args.id === 'tx-1' ? 'reverse-1' : 'tx-1'] : [],
              sources: [],
              history: [[reversed ? 'reversed' : 'created', 1760445000000000]],
            }
          );
        if (command === 'reverse_transaction') {
          reversed = true;
          return 'reverse-1';
        }
        throw new Error(`Unexpected IPC command: ${command}`);
      },
    };
  }, seed);
}
