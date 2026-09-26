import { useCallback, useEffect, useState } from 'react';
import { Box, Container, Typography } from '@mui/material';
import { api, type AppStatus } from '../api';
import { ErrorAlert, Loading } from '../components/common';
import { LockScreen } from '../features/auth/LockScreen';
import { LoginScreen } from '../features/auth/LoginScreen';
import { SetupWizard } from '../features/auth/SetupWizard';
import { t } from '../i18n/en';
import { MainLayout } from './MainLayout';

/** Decides what to show: first-run setup, sign-in, lock screen, or the app itself. */
export function App() {
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [error, setError] = useState<unknown>(null);

  const refresh = useCallback(() => {
    api.getAppStatus().then(setStatus).catch(setError);
  }, []);
  useEffect(refresh, [refresh]);

  if (!status) {
    return (
      <Container maxWidth="sm" sx={{ py: 8 }}>
        <Typography variant="h4" sx={{ fontWeight: 700, mb: 2 }}>
          {t.app.name}
        </Typography>
        {error ? <ErrorAlert error={error} /> : <Loading />}
      </Container>
    );
  }
  if (status.needsSetup) return <SetupWizard onDone={setStatus} />;
  if (!status.session) return <LoginScreen status={status} onSignedIn={setStatus} />;
  if (status.locked) return <LockScreen status={status} onUnlocked={setStatus} />;
  return (
    <Box sx={{ minHeight: '100vh', bgcolor: 'background.default' }}>
      <MainLayout status={status} session={status.session} setStatus={setStatus} />
    </Box>
  );
}
