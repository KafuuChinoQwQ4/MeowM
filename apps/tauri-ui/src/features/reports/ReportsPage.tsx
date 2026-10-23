import { useCallback, useState } from 'react';
import type { Account } from '../../api/accounts';
import { budgetsApi } from '../../api/budgets';
import { entriesApi } from '../../api/entries';
import { reportsApi } from '../../api/reports';
import { LoadState } from '../../components/LoadState';
import { Pager } from '../../components/Pager';
import { monthRange, recentMonthRange, recentYearRange } from '../../shared/calendar';
import { money, today, validDate } from '../../shared/format';
import { useResource } from '../../shared/useResource';
import { BudgetCard } from '../budgets/BudgetCard';
import { BudgetReminders } from '../budgets/BudgetReminders';
import { ReportCharts } from './ReportCharts';
import { ReportFilters, type Filters } from './ReportFilters';
export function ReportsPage({ accounts, revision }: { accounts: Account[]; revision: number }) {
  const [filters, setFilters] = useState<Filters>(() => {
    const month = today().slice(0, 7),
      [from, until] = monthRange(month);
    return { period: 'month', month, from, until, account: '', category: '', merchant: '' };
  });
  const [page, setPage] = useState(0);
  const categories = useResource(useCallback(() => entriesApi.categories(), []));
  const [from, until] =
    filters.period === 'month' ? monthRange(filters.month) : [filters.from, filters.until];
  const report = useResource(
    useCallback(() => {
      if (!validDate(from) || !validDate(until) || from >= until)
        return Promise.reject(new Error('请选择有效的日期范围，结束日期不包含在报表内。'));
      return reportsApi.summary({
        from_date: from,
        until_date: until,
        account_id: filters.account || undefined,
        category_id: filters.category || undefined,
        merchant: filters.merchant || undefined,
        merchant_offset: page * 10,
        merchant_limit: 10,
      });
    }, [from, until, filters.account, filters.category, filters.merchant, page, revision]),
  );
  const [monthFrom, monthUntil] = recentMonthRange(today().slice(0, 7), 12);
  const [yearFrom, yearUntil] = recentYearRange(today(), 6);
  const timelineQuery = useCallback(
    (rangeFrom: string, rangeUntil: string) => {
      if (!validDate(rangeFrom) || !validDate(rangeUntil) || rangeFrom >= rangeUntil)
        return Promise.reject(new Error('无法生成趋势范围'));
      return reportsApi.summary({
        from_date: rangeFrom,
        until_date: rangeUntil,
        account_id: filters.account || undefined,
        category_id: filters.category || undefined,
        merchant: filters.merchant || undefined,
        merchant_offset: 0,
        merchant_limit: 1,
      });
    },
    [filters.account, filters.category, filters.merchant, revision],
  );
  const monthly = useResource(
    useCallback(() => timelineQuery(monthFrom, monthUntil), [timelineQuery, monthFrom, monthUntil]),
    true,
  );
  const yearly = useResource(
    useCallback(() => timelineQuery(yearFrom, yearUntil), [timelineQuery, yearFrom, yearUntil]),
    true,
  );
  const budgetState = useResource(
    useCallback(
      async () => ({ budgets: await budgetsApi.list(), reminders: await budgetsApi.reminders() }),
      [revision],
    ),
  );
  const data = report.data;
  const matching =
    budgetState.data?.budgets.filter(
      (b) =>
        b.enabled &&
        b.from_date < until &&
        b.until_date > from &&
        (!filters.account || !b.account_id || b.account_id === filters.account) &&
        (!filters.category || !b.category_id || b.category_id === filters.category),
    ) ?? [];
  return (
    <div className="analytics-page">
      <h2>报表与预算总览</h2>
      <p className="field-hint">按设备本地日期统计已确认交易，转账不计入收支。</p>
      <ReportFilters
        value={filters}
        accounts={accounts}
        categories={categories.data ?? []}
        onChange={(value) => {
          setFilters(value);
          setPage(0);
        }}
      />
      <LoadState {...categories} />
      <LoadState {...report} />
      {data && (
        <>
          <div className="report-summary" aria-label="本期收支">
            <div>
              <span>收入</span>
              <strong>¥{money(data.income)}</strong>
            </div>
            <div>
              <span>支出</span>
              <strong>¥{money(data.expense)}</strong>
            </div>
            <div>
              <span>结余</span>
              <strong>¥{money(data.net)}</strong>
            </div>
          </div>
          {!data.transaction_count && (
            <p className="analytics-empty">本期没有符合筛选的已确认收支。</p>
          )}
          <ReportCharts report={data} monthly={monthly.data} yearly={yearly.data} />
          <section className="analytics-section">
            <h3>商户排行</h3>
            {!data.merchants.length && <p>暂无商户记录。</p>}
            {data.merchants.map((m) => (
              <div className="analytics-row" key={m.name}>
                <strong>{m.name}</strong>
                <span>
                  支出 ¥{money(m.expense)}
                  <br />
                  收入 ¥{money(m.income)}
                </span>
              </div>
            ))}
            <Pager page={page} pages={Math.ceil(data.merchant_total / 10)} onChange={setPage} />
          </section>
          <section className="analytics-section">
            <h3>期末平台余额</h3>
            <p className="field-hint">截至结束日期前一刻，包含转账；不受分类和商户筛选影响。</p>
            {!data.balances.length && <p>暂无平台。</p>}
            {data.balances.map((b) => (
              <div className="analytics-row" key={b.account_id}>
                <span>{b.name}</span>
                <strong>{b.amount === null ? '未设置参考余额' : `¥${money(b.amount)}`}</strong>
              </div>
            ))}
          </section>
        </>
      )}
      <section className="analytics-section">
        <h3>预算进度</h3>
        <p className="field-hint">显示与所选日期重叠的预算，进度按预算的完整周期计算。</p>
        <LoadState {...budgetState} />
        {budgetState.data && (
          <>
            <BudgetReminders
              budgets={budgetState.data.budgets}
              reminders={budgetState.data.reminders.filter((r) =>
                matching.some((b) => b.id === r.budget_id),
              )}
            />
            {!matching.length && <p>本期暂无启用的预算，可在「预算管理」中新增。</p>}
            {matching.map((b) => (
              <BudgetCard key={b.id} budget={b} accounts={accounts} />
            ))}
          </>
        )}
      </section>
    </div>
  );
}
