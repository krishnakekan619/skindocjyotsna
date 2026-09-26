import { useEffect, useState } from 'react';
import { Button, Card, Stack, Table, TableBody, TableCell, TableHead, TableRow } from '@mui/material';
import { api, type AuditEntry } from '../../api';
import { EmptyState, ErrorAlert, Loading, PageHeader } from '../../components/common';
import { t } from '../../i18n/en';
import { formatDateTime } from '../../lib/dates';

const PAGE = 100;

function detailsText(details: unknown): string {
  if (details === null || details === undefined) return '';
  if (typeof details !== 'object') return String(details);
  return Object.entries(details as Record<string, unknown>)
    .map(([k, v]) => `${k}: ${typeof v === 'object' ? JSON.stringify(v) : String(v)}`)
    .join(' · ');
}

/** Who did what and when (admin). Read-only: the log cannot be edited from the app. */
export function AuditLogPage() {
  const [rows, setRows] = useState<AuditEntry[] | null>(null);
  const [more, setMore] = useState(true);
  const [error, setError] = useState<unknown>(null);

  const load = (beforeId: number | null) =>
    api
      .listAudit(PAGE, beforeId)
      .then((page) => {
        setRows((current) => (beforeId === null ? page : [...(current ?? []), ...page]));
        setMore(page.length === PAGE);
      })
      .catch(setError);
  useEffect(() => {
    void load(null);
  }, []);

  return (
    <>
      <PageHeader title={t.settings.auditTitle} />
      <ErrorAlert error={error} />
      <Card variant="outlined">
        {!rows ? (
          <Loading />
        ) : rows.length === 0 ? (
          <EmptyState text={t.common.none} />
        ) : (
          <Table size="small">
            <TableHead>
              <TableRow>
                <TableCell>{t.common.date}</TableCell>
                <TableCell>{t.settings.who}</TableCell>
                <TableCell>{t.settings.action}</TableCell>
                <TableCell>{t.settings.details}</TableCell>
              </TableRow>
            </TableHead>
            <TableBody>
              {rows.map((r) => (
                <TableRow key={r.id}>
                  <TableCell sx={{ whiteSpace: 'nowrap' }}>{formatDateTime(r.occurredAt)}</TableCell>
                  <TableCell>{r.username ?? 'system'}</TableCell>
                  <TableCell>{r.action}</TableCell>
                  <TableCell sx={{ wordBreak: 'break-word' }}>{[r.entityType && `${r.entityType} ${r.entityId ?? ''}`, detailsText(r.details)].filter(Boolean).join(' · ')}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </Card>
      {rows && more && rows.length > 0 && (
        <Stack direction="row" sx={{ mt: 2, justifyContent: 'center' }}>
          <Button onClick={() => void load(rows[rows.length - 1]?.id ?? null)}>{t.settings.more}</Button>
        </Stack>
      )}
    </>
  );
}
