import { useState, type FormEvent } from 'react';
import { Button, Card, CardContent, Container, Stack, TextField, Typography } from '@mui/material';
import { api, type AppStatus } from '../../api';
import { ErrorAlert } from '../../components/common';
import { t } from '../../i18n/en';

export function LoginScreen({ status, onSignedIn }: { status: AppStatus; onSignedIn: (s: AppStatus) => void }) {
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      onSignedIn(await api.login(username, password));
    } catch (e) {
      setError(e);
      setPassword('');
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
                {status.clinicName || t.app.name}
              </Typography>
              <Typography color="text.secondary" sx={{ textAlign: 'center' }}>
                {t.login.title}
              </Typography>
              <TextField label={t.account.username} value={username} onChange={(e) => setUsername(e.target.value)} autoFocus required slotProps={{ htmlInput: { autoCapitalize: 'none', spellCheck: false } }} />
              <TextField label={t.account.password} type="password" value={password} onChange={(e) => setPassword(e.target.value)} required />
              <ErrorAlert error={error} />
              <Button type="submit" variant="contained" disabled={busy || !username || !password}>
                {t.login.submit}
              </Button>
              <Typography variant="caption" color="text.secondary" sx={{ textAlign: 'center' }}>
                {t.app.name} v{status.appVersion}
              </Typography>
            </Stack>
          </form>
        </CardContent>
      </Card>
    </Container>
  );
}
