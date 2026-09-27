import { useCallback, useEffect, useState } from 'react';
import {
  Alert,
  Button,
  Card,
  CardContent,
  CardHeader,
  Chip,
  CircularProgress,
  Stack,
  Table,
  TableBody,
  TableCell,
  TableRow,
} from '@mui/material';
import { api, isCommandError, type SystemInfo } from '../../api';

type LoadState =
  | { status: 'loading' }
  | { status: 'error'; message: string }
  | { status: 'ready'; info: SystemInfo };

function toMessage(error: unknown): string {
  return isCommandError(error) ? error.message : 'Unable to read system information.';
}

function StatusChip({ ok, label }: { ok: boolean; label: string }) {
  return <Chip size="small" color={ok ? 'success' : 'error'} label={`${label}: ${ok ? 'OK' : 'Problem'}`} />;
}

/** Phase 0 spike screen: proves UI -> Rust -> SQLite works on this OS. */
export function SystemStatusCard() {
  const [state, setState] = useState<LoadState>({ status: 'loading' });

  const load = useCallback(() => {
    setState({ status: 'loading' });
    api
      .getSystemInfo()
      .then((info) => setState({ status: 'ready', info }))
      .catch((error: unknown) => setState({ status: 'error', message: toMessage(error) }));
  }, []);

  useEffect(load, [load]);

  return (
    <Card variant="outlined">
      <CardHeader
        title="System status"
        action={
          <Button onClick={load} disabled={state.status === 'loading'}>
            Refresh
          </Button>
        }
      />
      <CardContent>
        {state.status === 'loading' && <CircularProgress aria-label="Loading system status" />}
        {state.status === 'error' && <Alert severity="warning">{state.message}</Alert>}
        {state.status === 'ready' && <SystemInfoTable info={state.info} />}
      </CardContent>
    </Card>
  );
}

function SystemInfoTable({ info }: { info: SystemInfo }) {
  const db = info.database;
  const rows: Array<[string, string]> = [
    ['App version', info.appVersion],
    ['Platform', `${info.platform} (${info.arch})`],
    ['SQLite', `${db.sqliteVersion}, schema v${db.schemaVersion}, journal ${db.journalMode}`],
    ['Database file', info.databaseFile],
    ['Backups folder', info.backupDir],
    ['Second backup copy', `${info.mirrorDir} (${info.mirrorBackups} files)`],
    ['Logs folder', info.logDir],
  ];
  return (
    <Stack spacing={2}>
      <Stack direction="row" sx={{ flexWrap: 'wrap', gap: 1 }}>
        <StatusChip ok={db.integrityOk} label="Integrity" />
        <StatusChip ok={db.journalMode.toLowerCase() === 'wal'} label="WAL" />
        <StatusChip ok={db.fts5Available} label="Fast search (FTS5)" />
        <Chip
          size="small"
          color={info.diskEncryption === 'ON' ? 'success' : info.diskEncryption === 'OFF' ? 'error' : 'default'}
          label={`Disk encryption: ${info.diskEncryption === 'ON' ? 'On' : info.diskEncryption === 'OFF' ? 'Off' : 'Unknown'}`}
        />
      </Stack>
      <Table size="small">
        <TableBody>
          {rows.map(([label, value]) => (
            <TableRow key={label}>
              <TableCell component="th" sx={{ fontWeight: 600, width: 160 }}>
                {label}
              </TableCell>
              <TableCell sx={{ wordBreak: 'break-all' }}>{value}</TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </Stack>
  );
}
