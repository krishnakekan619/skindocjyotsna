import { useState, type FormEvent } from 'react';
import { Button, Card, CardContent, Container, Stack, TextField, Typography } from '@mui/material';
import { api, isCommandError, type AppStatus } from '../../api';
import { ErrorAlert } from '../../components/common';
import { t } from '../../i18n/en';

/** Idle lock (DEC-023/024): unlock with the personal PIN, or the full password. */
export function LockScreen({ status, onUnlocked }: { status: AppStatus; onUnlocked: (s: AppStatus) => void }) {
  const session = status.session;
  const [usePassword, setUsePassword] = useState(!session?.hasPin);
  const [secret, setSecret] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      onUnlocked(await (usePassword ? api.unlockWithPassword(secret) : api.unlockWithPin(secret)));
    } catch (e) {
      setError(e);
      setSecret('');
      if (isCommandError(e) && (e.code === 'PIN_LOCKED' || e.code === 'PIN_NOT_SET')) setUsePassword(true);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Container maxWidth="xs" sx={{ py: 10 }}>
      <Card variant="outlined">
        <CardContent>
          <form onSubmit={submit}>
            <Stack spacing={2}>
              <Typography variant="h5" sx={{ fontWeight: 700, textAlign: 'center' }}>
                {t.lock.title}
              </Typography>
              <Typography color="text.secondary" sx={{ textAlign: 'center' }}>
                {session?.fullName} · {t.lock.subtitle}
              </Typography>
              <TextField
                key={usePassword ? 'password' : 'pin'}
                label={usePassword ? t.account.password : t.lock.pin}
                type="password"
                value={secret}
                onChange={(e) => setSecret(usePassword ? e.target.value : e.target.value.replace(/\D/g, '').slice(0, 6))}
                autoFocus
                slotProps={{ htmlInput: usePassword ? {} : { inputMode: 'numeric' } }}
              />
              <ErrorAlert error={error} />
              <Button type="submit" variant="contained" disabled={busy || !secret}>
                {t.lock.unlock}
              </Button>
              {session?.hasPin && (
                <Button onClick={() => { setUsePassword(!usePassword); setSecret(''); setError(null); }}>
                  {usePassword ? t.lock.usePin : t.lock.usePassword}
                </Button>
              )}
              <Button color="secondary" onClick={() => api.logout().then(onUnlocked).catch(setError)}>
                {t.lock.otherUser}
              </Button>
            </Stack>
          </form>
        </CardContent>
      </Card>
    </Container>
  );
}
