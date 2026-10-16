import { useState } from 'react';
import { dataApi } from '../../api/data';
import { Field, FormError } from '../../components/Fields';
import { errorText } from '../../shared/errors';
import { saveFile } from './files';
export function BackupForm({ busy, onBusy }: { busy: boolean; onBusy: (v: boolean) => void }) {
  const [password, setPassword] = useState('');
  const [confirm, setConfirm] = useState('');
  const [error, setError] = useState('');
  const [message, setMessage] = useState('');
  return (
    <section className="data-card" aria-label="创建加密备份">
      <h3>创建加密备份</h3>
      <p>
        保存全部平台、交易、来源摘要、导入批次、分类规则、预算、周期账单和对账审计。密码只用于本次加密，不会保存，遗失后无法找回。
      </p>
      <form
        onSubmit={async (e) => {
          e.preventDefault();
          setError('');
          setMessage('');
          if (password !== confirm) {
            setError('两次密码不一致');
            return;
          }
          onBusy(true);
          try {
            const file = await dataApi.backup(password);
            setPassword('');
            setConfirm('');
            await saveFile(file);
            setMessage('加密备份已交给系统保存或浏览器下载，请保管好文件和密码。');
          } catch (cause) {
            setError(errorText(cause));
          } finally {
            setPassword('');
            setConfirm('');
            onBusy(false);
          }
        }}
      >
        <fieldset disabled={busy}>
          <Field
            label="备份密码"
            type="password"
            minLength={8}
            maxLength={256}
            required
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
          <Field
            label="再次输入备份密码"
            type="password"
            minLength={8}
            maxLength={256}
            required
            value={confirm}
            onChange={(e) => setConfirm(e.target.value)}
          />
          <button className="button button-primary" type="submit">
            创建并保存备份
          </button>
        </fieldset>
      </form>
      <FormError message={error} />
      {message && <p role="status">{message}</p>}
    </section>
  );
}
