import type { Account } from '../api/accounts';
import { ALL_ACCOUNTS } from '../api/ledger';
import { moveDate, today, validDate } from '../shared/format';
import {
  platformThemeStyle,
  resolvePlatformTheme,
  TOTAL_PLATFORM_THEME,
} from '../shared/platformThemes';
export function Toolbar({
  accounts,
  accountId,
  date,
  hidden,
  ready,
  onAccount,
  onDate,
  onAdd,
}: {
  accounts: Account[];
  accountId: string;
  date: string;
  hidden: boolean;
  ready: boolean;
  onAccount: (id: string) => void;
  onDate: (date: string) => void;
  onAdd: () => void;
}) {
  const account = accounts.find((a) => a.id === accountId);
  const theme =
    accountId === ALL_ACCOUNTS
      ? TOTAL_PLATFORM_THEME
      : resolvePlatformTheme(account?.name ?? '', account?.category);
  return (
    <>
      <div className={'ledger-toolbar' + (hidden ? ' platform-mode' : '')}>
        <div className="account-field">
          <label htmlFor="account">当前平台</label>
          <div
            className="account-select-wrap"
            data-platform={theme.id}
            style={platformThemeStyle(theme)}
          >
            <select
              id="account"
              aria-label="当前平台"
              disabled={!accounts.length}
              value={accountId}
              onChange={(e) => onAccount(e.target.value)}
            >
              {!accounts.length && <option value="">尚无平台</option>}
              {!!accounts.length && <option value={ALL_ACCOUNTS}>总和</option>}
              {accounts.map((a) => (
                <option key={a.id} value={a.id}>
                  {a.name}
                </option>
              ))}
            </select>
          </div>
        </div>
        <div className="toolbar-actions">
          <button
            className="button button-primary compact-add"
            id="add-entry"
            disabled={!ready}
            onClick={onAdd}
          >
            <span aria-hidden="true">＋</span> 记一笔
          </button>
        </div>
      </div>
      <div className={'date-bar' + (hidden ? ' platform-mode' : '')}>
        <div className="date-controls">
          <button
            className="icon-button"
            id="previous"
            aria-label="前一天"
            disabled={date <= '0001-01-01'}
            onClick={() => onDate(moveDate(date, -1))}
          >
            <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
              <path d="m15 6-6 6 6 6" />
            </svg>
          </button>
          <label className="date-label">
            <span className="sr-only">记账日期</span>
            <span className="date-value" aria-hidden="true">
              {date.split('-').map((part, index) => (
                <span key={index}>{part}</span>
              ))}
            </span>
            <input
              type="date"
              id="date"
              min="0001-01-01"
              max="9999-12-31"
              value={date}
              onChange={(e) => {
                if (validDate(e.target.value)) onDate(e.target.value);
              }}
            />
          </label>
          <button
            className="icon-button"
            id="next"
            aria-label="后一天"
            disabled={date >= '9999-12-31'}
            onClick={() => onDate(moveDate(date, 1))}
          >
            <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
              <path d="m9 6 6 6-6 6" />
            </svg>
          </button>
        </div>
        <button className="today-button" id="today" onClick={() => onDate(today())}>
          回到今天
        </button>
      </div>
    </>
  );
}
