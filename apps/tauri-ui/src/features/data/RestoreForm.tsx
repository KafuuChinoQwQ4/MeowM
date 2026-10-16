import { useState } from 'react';
import { dataApi, type RestorePreview } from '../../api/data';
import { Field, FormError } from '../../components/Fields';
import { errorText } from '../../shared/errors';
import { isAndroid, pickBackup, readFile } from './files';
import { RestoreCounts } from './RestoreCounts';
export function RestoreForm({
  busy,
  onBusy,
  onRestored,
}: {
  busy: boolean;
  onBusy: (v: boolean) => void;
  onRestored: () => void;
}) {
  const [data, setData] = useState('');
  const [password, setPassword] = useState('');
  const [preview, setPreview] = useState<RestorePreview | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [error, setError] = useState('');
  const reset = () => {
    setPreview(null);
    setConfirmed(false);
    setError('');
  };
  async function choose(load: () => Promise<string>) {
    reset();
    setData('');
    setPassword('');
    onBusy(true);
    try {
      setData(await load());
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      onBusy(false);
    }
  }
  return (
    <section className="data-card" aria-label="恢复备份">
      <h3>恢复备份</h3>
      <p>
        先检查文件和密码，再预览替换范围。恢复会替换当前全部账本；请先保存当前账本的备份。仅支持
        MeowM 加密备份，文件不超过 32 MB。
      </p>
      <fieldset disabled={busy}>
        {isAndroid() ? (
          <button className="button" onClick={() => choose(async () => (await pickBackup()).data)}>
            选择备份文件
          </button>
        ) : (
          <Field
            label="选择备份文件"
            type="file"
            accept=".meowm,application/octet-stream"
            onChange={(e) => {
              const f = e.target.files?.[0];
              reset();
              if (f) void choose(() => readFile(f));
              else setData('');
            }}
          />
        )}
        {data && <p>文件已读取，等待密码校验。</p>}
        <form
          onSubmit={async (e) => {
            e.preventDefault();
            reset();
            onBusy(true);
            try {
              setPreview(await dataApi.inspect(data, password));
              setData('');
            } catch (cause) {
              setError(errorText(cause));
            } finally {
              setPassword('');
              onBusy(false);
            }
          }}
        >
          <Field
            label="恢复密码"
            type="password"
            minLength={8}
            maxLength={256}
            required
            value={password}
            onChange={(e) => {
              setPassword(e.target.value);
              reset();
            }}
          />
          <button className="button" disabled={!data} type="submit">
            检查备份并预览
          </button>
        </form>
        {preview && (
          <div className="restore-preview">
            <RestoreCounts preview={preview} />
            <p>
              恢复为完整替换，不合并。设备权限和本机安全设置保留；密码和通知原文不在备份内。预览有效期
              10 分钟，期间账本有变化需重新检查。
            </p>
            <label>
              <input
                type="checkbox"
                checked={confirmed}
                onChange={(e) => setConfirmed(e.target.checked)}
              />
              我确认用此备份替换当前全部账本
            </label>
            <div className="data-actions">
              <button
                className="button"
                onClick={() => {
                  reset();
                  setData('');
                }}
              >
                取消恢复
              </button>
              <button
                className="button button-primary"
                disabled={!confirmed}
                onClick={async () => {
                  onBusy(true);
                  setError('');
                  try {
                    await dataApi.restore(preview.id);
                    reset();
                    setData('');
                    onRestored();
                  } catch (cause) {
                    setPreview(null);
                    setConfirmed(false);
                    setError(errorText(cause));
                  } finally {
                    onBusy(false);
                  }
                }}
              >
                确认替换并恢复
              </button>
            </div>
          </div>
        )}
      </fieldset>
      <FormError message={error} />
    </section>
  );
}
