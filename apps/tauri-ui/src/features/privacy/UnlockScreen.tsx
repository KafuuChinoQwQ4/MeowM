import { useState } from 'react';
import { privacyApi, type PrivacyStatus } from '../../api/privacy';
import { Field, FormError } from '../../components/Fields';
import { errorText } from '../../shared/errors';
export function UnlockScreen({ onUnlocked }: { onUnlocked: (status: PrivacyStatus) => void }) {
  const [password, setPassword] = useState('');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  return (
    <main className="privacy-lock-screen">
      <section className="data-card" aria-label="解锁账本">
        <h1>应用已锁定</h1>
        <p>输入本机应用锁密码，继续查看账本。备份密码与应用锁密码分别管理。</p>
        <form
          onSubmit={async (e) => {
            e.preventDefault();
            setBusy(true);
            setError('');
            try {
              onUnlocked(await privacyApi.unlock(password));
            } catch (cause) {
              setError(errorText(cause));
            } finally {
              setPassword('');
              setBusy(false);
            }
          }}
        >
          <fieldset disabled={busy}>
            <Field
              label="解锁密码"
              type="password"
              value={password}
              required
              onChange={(e) => setPassword(e.target.value)}
            />
            <button className="button button-primary" type="submit">
              解锁账本
            </button>
          </fieldset>
        </form>
        <FormError message={error} />
      </section>
    </main>
  );
}
