// Mock only the native IPC boundary. Browser tests exercise the real React UI.
export async function installWorkflowIpc(page) {
  await page.addInitScript(() => {
    let accounts = [
      { id: 'wallet', name: '钱包', category: 'wallet' },
      { id: 'bank', name: '银行', category: 'bank' },
    ];
    let reference = null;
    let pending = [];
    let reversed = false;
    window.workflowCalls = [];
    window.__TAURI_INTERNALS__ = {
      invoke: async (command, args) => {
        if (command === 'privacy_status')
          return { locked: false, lock_enabled: false, hide_amounts: false, auto_lock_minutes: 5 };
        window.workflowCalls.push({ command, args });
        if (command === 'list_accounts') return accounts.map((a) => ({ ...a }));
        if (command === 'create_account') {
          if (window.holdSave)
            await new Promise((resolve) => {
              window.releaseSave = resolve;
            });
          const account = { id: 'created', name: args.name, category: args.category };
          accounts.push(account);
          return { ...account };
        }
        if (command === 'update_platform') {
          accounts = accounts.map((a) =>
            a.id === args.accountId ? { ...a, name: args.name, category: args.category } : a,
          );
          return;
        }
        if (command === 'get_day') {
          if (window.delayWallet && args.accountId === 'wallet') {
            await new Promise((resolve) => setTimeout(resolve, 300));
            window.oldDayFinished = true;
          }
          return {
            date: args.date,
            income: '0.00',
            expense: reversed ? '0.00' : '10.00',
            net: reversed ? '0.00' : '-10.00',
            balance: reference?.amount ?? (args.accountId === 'bank' ? '200.00' : '90.00'),
            reference,
            entries: [
              { id: 'tx-1', kind: 'expense', amount: '-10.00', note: '原消费', time: '12:00' },
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
            };
          });
          return args.ids;
        }
        if (command === 'list_categories') return [];
        if (command === 'transaction_detail')
          return {
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
          };
        if (command === 'reverse_transaction') {
          reversed = true;
          return 'reverse-1';
        }
        throw new Error(`Unexpected IPC command: ${command}`);
      },
    };
  });
}
