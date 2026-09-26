import { TextField } from '@mui/material';
import type { NewAccount } from '../../api';
import { FormGrid } from '../../components/common';
import { t } from '../../i18n/en';

export interface AccountDraft extends NewAccount {
  confirm: string;
}

export const emptyAccount = (): AccountDraft => ({ username: '', fullName: '', password: '', confirm: '', pin: '' });

/** Checks what can be checked before asking the backend (which re-checks everything). */
export function accountProblem(a: AccountDraft): string | null {
  if (!a.username.trim() || !a.fullName.trim() || !a.password) return null;
  return a.password !== a.confirm ? t.account.passwordsDiffer : null;
}

export function toAccount(a: AccountDraft): NewAccount {
  return { username: a.username.trim(), fullName: a.fullName.trim(), password: a.password, pin: a.pin?.trim() ? a.pin.trim() : null };
}

/** Username, name, password twice and optional PIN. `errorField` highlights a backend error. */
export function AccountFields({ value, onChange, withPin = true, errorField }: { value: AccountDraft; onChange: (a: AccountDraft) => void; withPin?: boolean; errorField?: string | undefined }) {
  const set = (patch: Partial<AccountDraft>) => onChange({ ...value, ...patch });
  const mismatch = value.confirm.length > 0 && value.password !== value.confirm;
  return (
    <FormGrid>
      <TextField label={t.account.fullName} value={value.fullName} onChange={(e) => set({ fullName: e.target.value })} required error={errorField === 'fullName'} />
      <TextField
        label={t.account.username}
        value={value.username}
        onChange={(e) => set({ username: e.target.value })}
        required
        error={errorField === 'username'}
        slotProps={{ htmlInput: { autoCapitalize: 'none', spellCheck: false } }}
      />
      <TextField label={t.account.password} type="password" value={value.password} onChange={(e) => set({ password: e.target.value })} required helperText={t.account.passwordHint} error={errorField === 'password'} />
      <TextField label={t.account.confirmPassword} type="password" value={value.confirm} onChange={(e) => set({ confirm: e.target.value })} required error={mismatch} helperText={mismatch ? t.account.passwordsDiffer : ' '} />
      {withPin && (
        <TextField
          label={t.account.pin}
          type="password"
          value={value.pin ?? ''}
          onChange={(e) => set({ pin: e.target.value.replace(/\D/g, '').slice(0, 6) })}
          error={errorField === 'pin'}
          slotProps={{ htmlInput: { inputMode: 'numeric' } }}
        />
      )}
    </FormGrid>
  );
}
