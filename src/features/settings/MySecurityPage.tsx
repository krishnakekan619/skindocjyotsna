import { useState } from 'react';
import { Box, Button, Card, CardContent, CardHeader, Stack, TextField, Typography } from '@mui/material';
import { api } from '../../api';
import { useApp } from '../../app/AppContext';
import { ErrorAlert, PageHeader } from '../../components/common';
import { t } from '../../i18n/en';

/** Change your own password and unlock PIN (every user). */
export function MySecurityPage() {
  return (
    <>
      <PageHeader title={t.settings.securityTitle} />
      <Box sx={{ display: 'grid', gridTemplateColumns: { xs: '1fr', md: '1fr 1fr' }, gap: 2 }}>
        <PasswordCard />
        <PinCard />
      </Box>
    </>
  );
}

function PasswordCard() {
  const { notify } = useApp();
  const [current, setCurrent] = useState('');
  const [next, setNext] = useState('');
  const [confirm, setConfirm] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const mismatch = confirm.length > 0 && confirm !== next;
  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      await api.changeOwnPassword(current, next);
      setCurrent('');
      setNext('');
      setConfirm('');
      notify(t.common.saved);
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card variant="outlined">
      <CardHeader title={t.settings.changePassword} />
      <CardContent>
        <Stack spacing={2}>
          <TextField label={t.account.currentPassword} type="password" value={current} onChange={(e) => setCurrent(e.target.value)} />
          <TextField label={t.account.newPassword} type="password" value={next} onChange={(e) => setNext(e.target.value)} helperText={t.account.passwordHint} />
          <TextField label={t.account.confirmPassword} type="password" value={confirm} onChange={(e) => setConfirm(e.target.value)} error={mismatch} helperText={mismatch ? t.account.passwordsDiffer : ' '} />
          <ErrorAlert error={error} />
          <Stack direction="row">
            <Button variant="contained" onClick={save} disabled={busy || !current || !next || next !== confirm}>
              {t.settings.changePassword}
            </Button>
          </Stack>
        </Stack>
      </CardContent>
    </Card>
  );
}

function PinCard() {
  const { session, setStatus, notify } = useApp();
  const [current, setCurrent] = useState('');
  const [pin, setPin] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const save = async (value: string | null) => {
    setBusy(true);
    setError(null);
    try {
      setStatus(await api.setOwnPin(current, value));
      setCurrent('');
      setPin('');
      notify(t.common.saved);
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card variant="outlined">
      <CardHeader title={t.settings.pinTitle} />
      <CardContent>
        <Stack spacing={2}>
          <Typography color="text.secondary">{session.hasPin ? t.settings.pinSet : t.settings.pinNotSet}</Typography>
          <TextField label={t.account.currentPassword} type="password" value={current} onChange={(e) => setCurrent(e.target.value)} />
          <TextField
            label={t.account.pin}
            type="password"
            value={pin}
            onChange={(e) => setPin(e.target.value.replace(/\D/g, '').slice(0, 6))}
            slotProps={{ htmlInput: { inputMode: 'numeric' } }}
          />
          <ErrorAlert error={error} />
          <Stack direction="row" spacing={1}>
            <Button variant="contained" onClick={() => save(pin)} disabled={busy || !current || pin.length < 4}>
              {t.settings.setPin}
            </Button>
            {session.hasPin && (
              <Button color="error" onClick={() => save(null)} disabled={busy || !current}>
                {t.settings.removePin}
              </Button>
            )}
          </Stack>
        </Stack>
      </CardContent>
    </Card>
  );
}
