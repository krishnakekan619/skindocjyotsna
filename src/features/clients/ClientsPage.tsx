import { useEffect, useState } from 'react';
import { Button, Card, Checkbox, FormControlLabel, Stack, Table, TableBody, TableCell, TableHead, TableRow, TextField } from '@mui/material';
import { api, type ClientRow } from '../../api';
import { useApp } from '../../app/AppContext';
import { EmptyState, ErrorAlert, Loading, PageHeader, StatusChip, useLoader } from '../../components/common';
import { t } from '../../i18n/en';
import { formatDateTime } from '../../lib/dates';
import { ClientDialog } from './ClientDialog';
import { DuplicatesDialog } from './DuplicatesDialog';

export function ClientsPage() {
  const { navigate, notify } = useApp();
  const [text, setText] = useState('');
  const [query, setQuery] = useState('');
  const [includeInactive, setIncludeInactive] = useState(false);
  const [editing, setEditing] = useState<ClientRow | 'new' | null>(null);
  const [duplicates, setDuplicates] = useState(false);

  // Search as you type, 250 ms after the last key (brief §35: debounced search).
  useEffect(() => {
    const timer = window.setTimeout(() => setQuery(text), 250);
    return () => window.clearTimeout(timer);
  }, [text]);
  const { data, error, loading, reload } = useLoader(() => api.searchClients(query, includeInactive), [query, includeInactive]);

  return (
    <>
      <PageHeader
        title={t.clients.title}
        actions={
          <>
            <Button variant="outlined" onClick={() => setDuplicates(true)}>
              {t.clients.findDuplicates}
            </Button>
            <Button variant="contained" onClick={() => setEditing('new')}>
              {t.clients.add}
            </Button>
          </>
        }
      />
      <Stack direction="row" spacing={2} sx={{ mb: 2, alignItems: 'center' }}>
        <TextField label={t.common.search} placeholder={t.clients.searchHint} value={text} onChange={(e) => setText(e.target.value)} autoFocus sx={{ flexGrow: 1, maxWidth: 480 }} />
        <FormControlLabel control={<Checkbox checked={includeInactive} onChange={(e) => setIncludeInactive(e.target.checked)} />} label={t.common.showInactive} />
      </Stack>
      <ErrorAlert error={error} />
      <Card variant="outlined">
        {loading && !data ? (
          <Loading />
        ) : !data || data.length === 0 ? (
          <EmptyState text={t.clients.empty} />
        ) : (
          <Table size="small">
            <TableHead>
              <TableRow>
                <TableCell>{t.clients.code}</TableCell>
                <TableCell>{t.clients.name}</TableCell>
                <TableCell>{t.clients.phone}</TableCell>
                <TableCell>{t.clients.lastVisit}</TableCell>
                <TableCell />
              </TableRow>
            </TableHead>
            <TableBody>
              {data.map((c) => (
                <TableRow key={c.id} hover sx={{ cursor: 'pointer' }} onClick={() => navigate({ name: 'client', clientId: c.id })}>
                  <TableCell>{c.clientCode}</TableCell>
                  <TableCell>
                    {c.fullName} {!c.isActive && <StatusChip label={t.common.inactive} color="default" />}
                  </TableCell>
                  <TableCell>{c.phone || '—'}</TableCell>
                  <TableCell>{formatDateTime(c.lastVisitAt)}</TableCell>
                  <TableCell align="right" onClick={(e) => e.stopPropagation()}>
                    {c.isActive && (
                      <Button size="small" onClick={() => navigate({ name: 'newBill', clientId: c.id })}>
                        {t.nav.newBill}
                      </Button>
                    )}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </Card>
      {editing && (
        <ClientDialog
          client={editing === 'new' ? null : editing}
          initialName={editing === 'new' ? text : ''}
          onClose={() => setEditing(null)}
          onSaved={(c) => {
            // A new client, or an existing one picked instead of creating a duplicate: open it.
            if (editing === 'new') {
              setEditing(null);
              navigate({ name: 'client', clientId: c.id });
              return;
            }
            setEditing(null);
            notify(`${c.fullName} (${c.clientCode}) ${t.common.saved.toLowerCase()}`);
            reload();
          }}
        />
      )}
      {duplicates && <DuplicatesDialog onClose={() => setDuplicates(false)} onMerged={reload} />}
    </>
  );
}
