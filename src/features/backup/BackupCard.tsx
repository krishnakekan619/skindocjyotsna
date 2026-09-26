import { useCallback, useEffect, useState } from 'react';
import {
  Alert,
  Button,
  Card,
  CardContent,
  CardHeader,
  Chip,
  Dialog,
  DialogActions,
  DialogContent,
  DialogContentText,
  DialogTitle,
  LinearProgress,
  Stack,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableRow,
  TextField,
  Typography,
} from '@mui/material';
import { api, errorMessage, type BackupFile } from '../../api';

const KIND_LABEL: Record<string, string> = { manual: 'Manual', auto: 'Automatic', 'pre-restore': 'Before restore' };
const CONFIRM_WORD = 'RESTORE';

function formatSize(bytes: number): string {
  return bytes < 1024 * 1024 ? `${Math.max(1, Math.round(bytes / 1024))} KB` : `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function formatWhen(createdUtc: string | null): string {
  if (!createdUtc) return '—';
  const date = new Date(createdUtc);
  return Number.isNaN(date.getTime()) ? createdUtc : date.toLocaleString();
}

type Notice = { severity: 'success' | 'error'; text: string } | null;

/**
 * Manual backup, list and restore (admin). A restore signs everyone out; `onRestored` runs a few
 * seconds later (after the result was shown) so the app can return to the sign-in screen.
 */
export function BackupCard({ onRestored }: { onRestored?: (() => void) | undefined }) {
  const [backups, setBackups] = useState<BackupFile[]>([]);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<Notice>(null);
  const [restoreTarget, setRestoreTarget] = useState<BackupFile | null>(null);
  // After a restore everyone is signed out: freeze the card until the app returns to sign-in.
  const [restored, setRestored] = useState(false);

  const refresh = useCallback(() => {
    api
      .listBackups()
      .then(setBackups)
      .catch((error: unknown) => setNotice({ severity: 'error', text: errorMessage(error, 'Could not read the backups folder.') }));
  }, []);

  useEffect(refresh, [refresh]);

  const run = async (action: () => Promise<string>) => {
    setBusy(true);
    setNotice(null);
    try {
      setNotice({ severity: 'success', text: await action() });
    } catch (error) {
      setNotice({ severity: 'error', text: errorMessage(error, 'The operation failed. Nothing was changed.') });
    } finally {
      setBusy(false);
      refresh();
    }
  };

  const backUpNow = () => run(async () => `Backup saved: ${(await api.createBackup()).fileName}`);

  const restore = (file: BackupFile) => {
    setRestoreTarget(null);
    return run(async () => {
      const result = await api.restoreBackup(file.fileName);
      setRestored(true);
      if (onRestored) window.setTimeout(onRestored, 3000);
      return `Restored ${result.restoredFrom.fileName}. The previous data was saved first as ${result.safetyBackup.fileName}.`;
    });
  };

  return (
    <Card variant="outlined">
      <CardHeader
        title="Backup & restore"
        action={
          <Button variant="contained" onClick={backUpNow} disabled={busy || restored}>
            Back up now
          </Button>
        }
      />
      {busy && <LinearProgress />}
      <CardContent>
        <Stack spacing={2}>
          {notice && <Alert severity={notice.severity}>{notice.text}</Alert>}
          {backups.length === 0 ? (
            <Typography color="text.secondary">No backups yet. Click “Back up now” to create the first one.</Typography>
          ) : (
            <Table size="small">
              <TableHead>
                <TableRow>
                  <TableCell>Date</TableCell>
                  <TableCell>Type</TableCell>
                  <TableCell align="right">Size</TableCell>
                  <TableCell />
                </TableRow>
              </TableHead>
              <TableBody>
                {backups.map((file) => (
                  <TableRow key={file.fileName}>
                    <TableCell>{formatWhen(file.createdUtc)}</TableCell>
                    <TableCell>
                      {file.problem ? (
                        <Chip size="small" color="error" label="Unusable" title={file.problem} />
                      ) : (
                        KIND_LABEL[file.kind ?? ''] ?? file.kind
                      )}
                    </TableCell>
                    <TableCell align="right">{formatSize(file.sizeBytes)}</TableCell>
                    <TableCell align="right">
                      <Button size="small" onClick={() => setRestoreTarget(file)} disabled={busy || restored || file.problem !== null}>
                        Restore…
                      </Button>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </Stack>
      </CardContent>
      {restoreTarget && (
        <RestoreDialog file={restoreTarget} onCancel={() => setRestoreTarget(null)} onConfirm={() => restore(restoreTarget)} />
      )}
    </Card>
  );
}

function RestoreDialog({ file, onCancel, onConfirm }: { file: BackupFile; onCancel: () => void; onConfirm: () => void }) {
  const [typed, setTyped] = useState('');
  return (
    <Dialog open onClose={onCancel} maxWidth="xs" fullWidth>
      <DialogTitle>Restore this backup?</DialogTitle>
      <DialogContent>
        <Stack spacing={2}>
          <DialogContentText>
            All current data will be replaced with the backup from <strong>{formatWhen(file.createdUtc)}</strong>. Anything entered
            after that time will no longer be shown.
          </DialogContentText>
          <DialogContentText>A safety backup of the current data is made first, so this can be undone.</DialogContentText>
          <TextField
            autoFocus
            label={`Type ${CONFIRM_WORD} to confirm`}
            value={typed}
            onChange={(event) => setTyped(event.target.value)}
          />
        </Stack>
      </DialogContent>
      <DialogActions>
        <Button onClick={onCancel}>Cancel</Button>
        <Button color="error" variant="contained" disabled={typed.trim().toUpperCase() !== CONFIRM_WORD} onClick={onConfirm}>
          Restore
        </Button>
      </DialogActions>
    </Dialog>
  );
}
