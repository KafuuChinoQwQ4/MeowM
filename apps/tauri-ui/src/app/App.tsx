import { useCallback, useState } from 'react';
import type { Account } from '../api/accounts';
import { ALL_ACCOUNTS } from '../api/ledger';
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
import { ReportsPage } from '../features/reports/ReportsPage';
import { BudgetsPage } from '../features/budgets/BudgetsPage';
import { NotificationsPage } from '../features/notifications/NotificationsPage';
import { RecurringPage } from '../features/recurring/RecurringPage';
import { RecurringReminders } from '../features/recurring/RecurringReminders';
import { useRecurring } from '../features/recurring/useRecurring';
import { today } from '../shared/format';
import { ReconciliationPage } from '../features/reconciliation/ReconciliationPage';
import { SearchPage } from '../features/search/SearchPage';
import { Navigation, NavigationDirectory, parentView, type View } from './Navigation';
import { DataManagementPage } from '../features/data/DataManagementPage';
import { Toolbar } from './Toolbar';
import { usePrivacyGate } from '../features/privacy/usePrivacyGate';
import { UnlockScreen } from '../features/privacy/UnlockScreen';
import { PrivacyPage } from '../features/privacy/PrivacyPage';
import type { PrivacyStatus } from '../api/privacy';
import { setAmountsHidden } from '../shared/format';
type Modal =
  | { kind: 'account'; account?: Account }
  | { kind: 'reference' | 'clear' | 'import' }
  | { kind: 'transaction'; id?: string }
  | { kind: 'pending'; entry: PageEntry };
export function App() {
  const privacy = usePrivacyGate();
  if (!privacy.status)
    return (
      <main className="privacy-lock-screen">
        <p role="status">正在检查隐私设置…</p>
        {privacy.error && (
          <button onClick={() => void privacy.reload()}>重试：{privacy.error}</button>
        )}
      </main>
    );
  if (privacy.status.locked) return <UnlockScreen onUnlocked={privacy.setStatus} />;
  setAmountsHidden(privacy.status.hide_amounts);
  return (
    <UnlockedApp
      privacyStatus={privacy.status}
      onPrivacyStatus={privacy.setStatus}
      onPrivacyReload={privacy.reload}
    />
  );
}
function UnlockedApp({
  privacyStatus,
  onPrivacyStatus,
  onPrivacyReload,
}: {
  privacyStatus: PrivacyStatus;
  onPrivacyStatus: (status: PrivacyStatus) => void;
  onPrivacyReload: () => Promise<void>;
}) {
  const accountsState = useAccounts();
  const accounts = accountsState.data ?? [];
  const [selected, setAccount] = useState('');
  const accountId =
    (selected === ALL_ACCOUNTS && accounts.length > 0) || accounts.some((a) => a.id === selected)
      ? selected
      : (accounts[0]?.id ?? '');
  const allAccounts = accountId === ALL_ACCOUNTS;
  const entryAccountId = allAccounts ? (accounts[0]?.id ?? '') : accountId;
  const [date, setDate] = useState(today);
  const [view, setView] = useState<View>('ledger');
  const parent = parentView(view);
  const [reconciliationId, setReconciliationId] = useState('');
  const [notice, setNotice] = useState('');
  const [navOpen, setNavOpen] = useState(false);
  const [revision, setRevision] = useState(0);
  const [generatedRevision, setGeneratedRevision] = useState(0);
  const generated = useCallback(() => setGeneratedRevision((value) => value + 1), []);
  const recurringState = useRecurring(revision, generated);
  const [pendingSource, setPendingSource] = useState<'recurring' | undefined>();
  const [modal, setModal] = useState<Modal | null>(null);
  const dayState = useDay(accountId, date, revision + generatedRevision);
  const day = dayState.data;
  function refresh() {
    setRevision((value) => value + 1);
  }
  function selectView(next: View) {
    if (next === 'pending') setPendingSource(undefined);
    setReconciliationId('');
    setNotice('');
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
        <section className="ledger" aria-label="日常账本">
          <Navigation view={view} open={navOpen} onOpen={setNavOpen} onSelect={selectView} />
          <button
            className="back-button"
            aria-label="返回上一级"
            disabled={!navOpen && !parent && !(view === 'reconciliation' && reconciliationId)}
            onClick={() => {
              if (navOpen) setNavOpen(false);
              else if (view === 'reconciliation' && reconciliationId) setReconciliationId('');
              else if (parent) selectView(parent);
            }}
          >
            <svg className="back-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
              <path d="m14 5-7 7 7 7" />
            </svg>
          </button>
          <Toolbar
            accounts={accounts}
            accountId={accountId}
            date={date}
            hidden={view !== 'ledger' && view !== 'records'}
            ready={!!day && !loading}
            onAccount={setAccount}
            onDate={setDate}
            onAdd={() => setModal({ kind: 'transaction' })}
          />
          {notice && <p role="status">{notice}</p>}
          <div key={view} id="day-content" aria-busy={loading}>
            <LoadState
              loading={accountsState.loading}
              error={accountsState.error}
              reload={accountsState.reload}
            />
            {!accountsState.loading && !accountsState.error && (
              <>
                <NavigationDirectory view={view} onSelect={selectView} />
                {view === 'data' && (
                  <DataManagementPage
                    accounts={accounts}
                    onRestored={() => {
                      setNotice('备份恢复成功，账本已替换。');
                      setModal(null);
                      setAccount('');
                      accountsState.reload();
                      refresh();
                      void onPrivacyReload();
                    }}
                  />
                )}
                {view === 'privacy' && (
                  <PrivacyPage
                    status={privacyStatus}
                    onStatus={onPrivacyStatus}
                    onNavigate={(next) => selectView(next)}
                    onDeleted={(scope) => {
                      accountsState.reload();
                      refresh();
                      setModal(null);
                      if (scope === 'all') {
                        setAccount('');
                        selectView('ledger');
                      }
                    }}
                  />
                )}
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
                {view === 'notifications' && (
                  <NotificationsPage
                    accounts={accounts}
                    onPending={() => {
                      selectView('pending');
                      refresh();
                    }}
                  />
                )}
                {view === 'reports' && <ReportsPage accounts={accounts} revision={revision} />}
                {view === 'budgets' && (
                  <BudgetsPage accounts={accounts} revision={revision} onChanged={refresh} />
                )}
                {view === 'search' && (
                  <SearchPage
                    accounts={accounts}
                    revision={revision}
                    onOpen={(entry) =>
                      setModal(
                        entry.status === 'pending'
                          ? { kind: 'pending', entry }
                          : { kind: 'transaction', id: entry.id },
                      )
                    }
                  />
                )}
                {view === 'reconciliation' && (
                  <ReconciliationPage
                    accounts={accounts}
                    revision={revision}
                    sessionId={reconciliationId}
                    onSession={setReconciliationId}
                    onChanged={refresh}
                    onReference={(id) => {
                      setAccount(id);
                      selectView('ledger');
                    }}
                    onPending={() => selectView('pending')}
                    onEntry={(id) => setModal({ kind: 'transaction', id })}
                  />
                )}
                {view === 'recurring' && (
                  <RecurringPage accounts={accounts} state={recurringState} onChanged={refresh} />
                )}
                {view === 'pending' && (
                  <PendingPage
                    key={pendingSource ?? 'all'}
                    initialSource={pendingSource}
                    accounts={accounts}
                    revision={revision + generatedRevision}
                    onEdit={(entry) => setModal({ kind: 'pending', entry })}
                    onImport={() => setModal({ kind: 'import' })}
                    onChanged={refresh}
                  />
                )}
                {view === 'ledger' && (
                  <RecurringReminders
                    state={recurringState}
                    accounts={accounts}
                    onManage={() => selectView('recurring')}
                    onPending={() => {
                      selectView('pending');
                      setPendingSource('recurring');
                      refresh();
                    }}
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
                        allAccounts={allAccounts}
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
            ? `${allAccounts ? '总和' : accounts.find((a) => a.id === accountId)?.name} · ${date}`
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
          {(modal.kind === 'reference' || modal.kind === 'clear') && day && !allAccounts && (
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
              accountId={entryAccountId}
              date={date}
              onLink={(id) => setModal({ kind: 'transaction', id })}
              onSaved={refresh}
            />
          )}
          {modal.kind === 'pending' && (
            <PendingEditor
              entry={modal.entry}
              accounts={accounts}
              accountId={entryAccountId}
              onSaved={refresh}
            />
          )}
          {modal.kind === 'import' && (
            <ImportPanel
              accounts={accounts}
              accountId={entryAccountId}
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
