import { useState } from 'react';
import { Alert, Button, Card, CardContent, CardHeader, Stack } from '@mui/material';
import { api, type AppFolder, type HealthReport } from '../../api';
import { useApp } from '../../app/AppContext';
import { ErrorAlert, PageHeader } from '../../components/common';
import { t } from '../../i18n/en';
import { BackupCard } from '../backup/BackupCard';
import { SystemStatusCard } from '../system/SystemStatusCard';

const FOLDERS: [AppFolder, string][] = [
  ['mirror', 'Second backup (skindocjyotsnaBackup)'],
  ['backups', 'Backups'],
  ['exports', 'Receipts (PDF)'],
  ['logs', 'Logs'],
  ['data', 'Data'],
];

/** Backups, restore, health check and system information (admin). */
export function SystemPage() {
  const { setStatus } = useApp();
  const [health, setHealth] = useState<HealthReport | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [checking, setChecking] = useState(false);

  const check = async () => {
    setChecking(true);
    setError(null);
    try {
      setHealth(await api.getHealth());
    } catch (e) {
      setError(e);
    } finally {
      setChecking(false);
    }
  };

  return (
    <>
      <PageHeader title={t.settings.systemTitle} />
      <Stack spacing={2}>
        <BackupCard onRestored={() => api.getAppStatus().then(setStatus).catch(() => undefined)} />
        <Card variant="outlined">
          <CardHeader
            title="Health check"
            action={
              <Button onClick={check} disabled={checking}>
                Run check
              </Button>
            }
          />
          <CardContent>
            <Stack spacing={1}>
              <ErrorAlert error={error} />
              {health && (
                <>
                  <Alert severity={health.integrityOk ? 'success' : 'error'}>Database integrity: {health.integrityOk ? 'OK' : 'problem found — restore the latest backup and contact support'}</Alert>
                  <Alert severity={health.ledgerMismatchBatchIds.length === 0 ? 'success' : 'warning'}>
                    Stock ledger: {health.ledgerMismatchBatchIds.length === 0 ? 'every batch matches its history' : `${health.ledgerMismatchBatchIds.length} batch(es) do not match (ids ${health.ledgerMismatchBatchIds.join(', ')})`}
                  </Alert>
                </>
              )}
              <Stack direction="row" spacing={1} useFlexGap sx={{ flexWrap: 'wrap' }}>
                {FOLDERS.map(([folder, label]) => (
                  <Button key={folder} size="small" variant="outlined" onClick={() => api.openFolder(folder).catch(setError)}>
                    Open {label} folder
                  </Button>
                ))}
              </Stack>
            </Stack>
          </CardContent>
        </Card>
        <SystemStatusCard />
      </Stack>
    </>
  );
}
