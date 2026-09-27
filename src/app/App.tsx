import { useCallback, useEffect, useState } from 'react';
import { Box, Container, Typography } from '@mui/material';
import { api, SESSION_CHANGED_EVENT, type AppStatus } from '../api';
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
  // The backend locked or ended the session (idle timeout, sleep, restore): show the right screen.
  useEffect(() => {
    window.addEventListener(SESSION_CHANGED_EVENT, refresh);
    return () => window.removeEventListener(SESSION_CHANGED_EVENT, refresh);
  }, [refresh]);

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
  // The lock screen covers the app instead of replacing it, so a bill being typed survives the
  // idle lock. Another user signing in gets a fresh app (keyed by user).
  return (
    <Box sx={{ minHeight: '100vh', bgcolor: 'background.default' }}>
      {/* inert: nothing behind the lock screen can be focused, clicked or typed into. */}
      <Box inert={status.locked}>
        <MainLayout key={status.session.userId} status={status} session={status.session} setStatus={setStatus} />
      </Box>
      {status.locked && (
        <Box sx={{ position: 'fixed', inset: 0, zIndex: (theme) => theme.zIndex.modal + 10, bgcolor: 'background.default', overflow: 'auto' }}>
          <LockScreen status={status} onUnlocked={setStatus} />
        </Box>
      )}
    </Box>
  );
}
