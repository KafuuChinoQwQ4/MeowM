import { useState } from 'react';
import type { Account } from '../api/accounts';
import type { PageEntry } from '../api/entries';
import { Dialog } from '../components/Dialog';
import { LoadState } from '../components/LoadState';
import { useAccounts } from '../features/accounts/useAccounts';
import { AccountsPage } from '../features/accounts/AccountsPage';
import { AccountForm } from '../features/accounts/AccountForm';
import { useDay } from '../features/ledger/useDay';
import { LedgerPage } from '../features/ledger/LedgerPage';
import { ReferenceForm } from '../features/ledger/ReferenceForm';
import { RecordsPage } from '../features/transactions/RecordsPage';
import { TransactionPanel } from '../features/transactions/TransactionPanel';
import { PendingPage } from '../features/pending/PendingPage';
import { PendingEditor } from '../features/pending/PendingEditor';
import { ImportPanel } from '../features/import/ImportPanel';
import { today } from '../shared/format';
import { Navigation, type View } from './Navigation';
import { Toolbar } from './Toolbar';
type Modal =
  | { kind: 'account'; account?: Account }
  | { kind: 'reference' | 'clear' | 'import' }
  | { kind: 'transaction'; id?: string }
  | { kind: 'pending'; entry: PageEntry };
export function App() {
  const accountsState = useAccounts();
  const accounts = accountsState.data ?? [];
  const [selected, setAccount] = useState('');
  const accountId = accounts.some((a) => a.id === selected) ? selected : (accounts[0]?.id ?? '');
  const [date, setDate] = useState(today);
  const [view, setView] = useState<View>('ledger');
  const [navOpen, setNavOpen] = useState(false);
  const [revision, setRevision] = useState(0);
  const [modal, setModal] = useState<Modal | null>(null);
  const dayState = useDay(accountId, date, revision);
  const day = dayState.data;
  function refresh() {
    setRevision((value) => value + 1);
  }
  function selectView(next: View) {
    setView(next);
    setNavOpen(false);
    if (next === 'ledger' || next === 'records') refresh();
  }
  const titles = {
    account: modal?.kind === 'account' && modal.account ? '编辑平台' : '新建平台',
    reference: '设置参考余额',
    clear: '清除参考余额？',
    import: '导入账单',
    transaction: modal?.kind === 'transaction' && modal.id ? '交易详情' : '记一笔',
    pending: '检查候选记录',
  };
  const loading =
    accountsState.loading || ((view === 'ledger' || view === 'records') && dayState.loading);
  return (
    <>
      <main className="app-shell">
        <Navigation view={view} open={navOpen} onOpen={setNavOpen} onSelect={selectView} />
        <section className="ledger" aria-label="日常账本">
          <button
            className="back-button"
            aria-label="返回上一级"
            disabled={!navOpen && view === 'ledger'}
            onClick={() => (navOpen ? setNavOpen(false) : selectView('ledger'))}
          >
            ‹
          </button>
          <Toolbar
            accounts={accounts}
            accountId={accountId}
            date={date}
            hidden={view === 'platforms' || view === 'pending'}
            ready={!!day && !loading}
            onAccount={setAccount}
            onDate={setDate}
            onAdd={() => setModal({ kind: 'transaction' })}
          />
          <div id="day-content" aria-busy={loading}>
            <LoadState
              loading={accountsState.loading}
              error={accountsState.error}
              reload={accountsState.reload}
            />
            {!accountsState.loading && !accountsState.error && (
              <>
                {view === 'platforms' && (
                  <AccountsPage
                    accounts={accounts}
                    onEdit={(account) => setModal({ kind: 'account', account })}
                    onEnter={(id) => {
                      setAccount(id);
                      selectView('ledger');
                    }}
                  />
                )}
                {view === 'pending' && (
                  <PendingPage
                    accounts={accounts}
                    revision={revision}
                    onEdit={(entry) => setModal({ kind: 'pending', entry })}
                    onImport={() => setModal({ kind: 'import' })}
                    onChanged={refresh}
                  />
                )}
                {(view === 'ledger' || view === 'records') && (
                  <>
                    <LoadState {...dayState} />
                    {!accountId && !dayState.loading && (
                      <div className="empty-state">
                        <span className="empty-art">＋</span>
                        <h2>从第一个平台开始</h2>
                        <p>为微信、支付宝或银行卡建一个独立账本。</p>
                        <button
                          className="button button-primary"
                          onClick={() => setModal({ kind: 'account' })}
                        >
                          创建我的第一个平台
                        </button>
                      </div>
                    )}
                    {day && view === 'ledger' && (
                      <LedgerPage
                        day={day}
                        onReference={() => setModal({ kind: 'reference' })}
                        onClear={() => setModal({ kind: 'clear' })}
                      />
                    )}
                    {day && view === 'records' && (
                      <RecordsPage
                        key={accountId + date}
                        entries={day.entries}
                        onOpen={(id) => setModal({ kind: 'transaction', id })}
                      />
                    )}
                  </>
                )}
              </>
            )}
          </div>
        </section>
      </main>
      <div className="action-dock">
        <span className="dock-context">
          {accountId
            ? `${accounts.find((a) => a.id === accountId)?.name} · ${date}`
            : '先创建一个平台，开始记录'}
        </span>
      </div>
      {modal && (
        <Dialog title={titles[modal.kind]} onClose={() => setModal(null)}>
          {modal.kind === 'account' && (
            <AccountForm
              account={modal.account}
              onSaved={(id) => {
                if (id) setAccount(id);
                accountsState.reload();
                refresh();
              }}
            />
          )}
          {(modal.kind === 'reference' || modal.kind === 'clear') && day && (
            <ReferenceForm
              accountId={accountId}
              date={date}
              day={day}
              clear={modal.kind === 'clear'}
              onSaved={refresh}
            />
          )}
          {modal.kind === 'transaction' && (
            <TransactionPanel
              id={modal.id}
              accountId={accountId}
              date={date}
              onLink={(id) => setModal({ kind: 'transaction', id })}
              onSaved={refresh}
            />
          )}
          {modal.kind === 'pending' && (
            <PendingEditor
              entry={modal.entry}
              accounts={accounts}
              accountId={accountId}
              onSaved={refresh}
            />
          )}
          {modal.kind === 'import' && (
            <ImportPanel
              accounts={accounts}
              accountId={accountId}
              onSaved={() => {
                setView('pending');
                refresh();
              }}
            />
          )}
        </Dialog>
      )}
    </>
  );
}
