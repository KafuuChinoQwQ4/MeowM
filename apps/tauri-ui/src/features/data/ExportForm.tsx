import { useCallback, useState } from 'react';
import type { Account } from '../../api/accounts';
import type { PageQuery } from '../../api/entries';
import { dataApi } from '../../api/data';
import { searchApi } from '../../api/search';
import { FormError, SelectField } from '../../components/Fields';
import { LoadState } from '../../components/LoadState';
import { useResource } from '../../shared/useResource';
import { errorText } from '../../shared/errors';
import { SearchFilters } from '../search/SearchFilters';
import { saveFile } from './files';
export function ExportForm({
  accounts,
  busy,
  onBusy,
}: {
  accounts: Account[];
  busy: boolean;
  onBusy: (v: boolean) => void;
}) {
  const [query, setQuery] = useState<PageQuery>({ status: 'confirmed', offset: 0, limit: 200 });
  const [format, setFormat] = useState<'csv' | 'json'>('csv');
  const [bom, setBom] = useState(true);
  const [error, setError] = useState('');
  const [message, setMessage] = useState('');
  const categories = useResource(useCallback(() => searchApi.state(), []));
  return (
    <section className="data-card" aria-label="筛选导出">
      <h3>筛选导出</h3>
      <p>
        默认导出已确认交易。修改状态可明确包含待确认或已忽略记录。导出涵盖筛选后的全部结果，金额使用精确十进制字符串，保留退款及冲销关联，不改变账本。
      </p>
      <LoadState {...categories} />
      <form
        onSubmit={async (e) => {
          e.preventDefault();
          onBusy(true);
          setError('');
          setMessage('');
          try {
            const file = await dataApi.export(query, format, bom);
            await saveFile(file);
            setMessage(`已导出 ${file.count ?? 0} 条记录，文件已交给系统保存或浏览器下载。`);
          } catch (cause) {
            setError(errorText(cause));
          } finally {
            onBusy(false);
          }
        }}
      >
        <fieldset disabled={busy || categories.loading || !!categories.error}>
          <SearchFilters
            query={query}
            accounts={accounts}
            categories={categories.data?.categories ?? []}
            onChange={(q) => {
              setQuery(q);
              setMessage('');
            }}
          />
          <SelectField
            label="导出格式"
            value={format}
            options={[
              ['csv', 'CSV（表格）'],
              ['json', 'JSON（结构化）'],
            ]}
            onChange={(e) => setFormat(e.target.value as 'csv' | 'json')}
          />
          {format === 'csv' && (
            <label>
              <input type="checkbox" checked={bom} onChange={(e) => setBom(e.target.checked)} />
              UTF-8 BOM（便于表格软件识别中文）
            </label>
          )}
          <p className="field-hint">
            导出文件未加密，包含你填写的商户和备注，不含外部流水号、密码或原始通知正文。
          </p>
          <button className="button button-primary" type="submit">
            导出全部筛选结果
          </button>
        </fieldset>
      </form>
      <FormError message={error} />
      {message && <p role="status">{message}</p>}
    </section>
  );
}
