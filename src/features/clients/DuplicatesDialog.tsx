import { useState } from 'react';
import {
  Alert,
  Box,
  Button,
  Card,
  CardContent,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  FormControlLabel,
  Radio,
  RadioGroup,
  Stack,
  Typography,
} from '@mui/material';
import { api, type DuplicateGroup } from '../../api';
import { useApp } from '../../app/AppContext';
import { EmptyState, ErrorAlert, Loading, useLoader } from '../../components/common';
import { t } from '../../i18n/en';
import { formatDateTime } from '../../lib/dates';

type GroupClient = DuplicateGroup['clients'][number];

function describe(c: GroupClient): string {
  return [c.clientCode, c.phone, t.clients.bills(c.billCount), c.lastVisitAt && `${t.clients.lastVisit}: ${formatDateTime(c.lastVisitAt)}`].filter(Boolean).join(' · ');
}

/** Clients → Find duplicates: groups sharing a phone or a name; administrators can merge them. */
export function DuplicatesDialog({ onClose, onMerged }: { onClose: () => void; onMerged: () => void }) {
  const { isAdmin } = useApp();
  const { data, error, loading, reload } = useLoader(() => api.findDuplicateClients(), []);
  const [merging, setMerging] = useState<DuplicateGroup | null>(null);

  return (
    <Dialog open onClose={onClose} maxWidth="md" fullWidth>
      <DialogTitle>{t.clients.duplicatesTitle}</DialogTitle>
      <DialogContent>
        <Stack spacing={2}>
          <Typography color="text.secondary">{t.clients.duplicatesIntro}</Typography>
          {!isAdmin && <Alert severity="info">{t.clients.mergeAdminOnly}</Alert>}
          <ErrorAlert error={error} />
          {loading && !data ? (
            <Loading />
          ) : !data || data.length === 0 ? (
            <EmptyState text={t.clients.noDuplicates} />
          ) : (
            data.map((group, index) => (
              <Card variant="outlined" key={index}>
                <CardContent>
                  <Stack direction="row" sx={{ alignItems: 'center', justifyContent: 'space-between', mb: 1 }}>
                    <Typography variant="overline" color="text.secondary" sx={{ fontWeight: 700 }}>
                      {t.clients.reasons[group.reason]}
                    </Typography>
                    {isAdmin && (
                      <Button variant="outlined" onClick={() => setMerging(group)}>
                        {t.clients.merge}
                      </Button>
                    )}
                  </Stack>
                  {group.clients.map((c) => (
                    <Box key={c.id} sx={{ py: 0.5 }}>
                      <Typography sx={{ fontWeight: 600 }}>{c.fullName}</Typography>
                      <Typography variant="body2" color="text.secondary">
                        {describe(c)}
                      </Typography>
                    </Box>
                  ))}
                </CardContent>
              </Card>
            ))
          )}
        </Stack>
      </DialogContent>
      <DialogActions>
        <Button variant="contained" onClick={onClose}>
          {t.common.close}
        </Button>
      </DialogActions>
      {merging && (
        <MergeDialog
          group={merging}
          onClose={() => setMerging(null)}
          onMerged={() => {
            setMerging(null);
            reload();
            onMerged();
          }}
        />
      )}
    </Dialog>
  );
}

function MergeDialog({ group, onClose, onMerged }: { group: DuplicateGroup; onClose: () => void; onMerged: () => void }) {
  const { notify } = useApp();
  // Suggest keeping the record with the most bills.
  const suggested = [...group.clients].sort((a, b) => b.billCount - a.billCount)[0]?.id ?? group.clients[0]?.id ?? 0;
  const [primaryId, setPrimaryId] = useState<number>(suggested);
  const others = group.clients.filter((c) => c.id !== primaryId);
  const [secondaryId, setSecondaryId] = useState<number>(others[0]?.id ?? 0);
  const secondary = others.find((c) => c.id === secondaryId) ?? others[0];
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);

  const merge = async () => {
    if (!secondary) return;
    setBusy(true);
    setError(null);
    try {
      const result = await api.mergeClients(primaryId, secondary.id);
      notify(t.clients.mergedDone(result.client.fullName, result.movedBills));
      onMerged();
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open onClose={busy ? undefined : onClose} maxWidth="sm" fullWidth>
      <DialogTitle>{t.clients.mergeTitle}</DialogTitle>
      <DialogContent>
        <Stack spacing={2} sx={{ pt: 1 }}>
          <Typography sx={{ fontWeight: 600 }}>{t.clients.keep}</Typography>
          <RadioGroup
            value={primaryId}
            onChange={(e) => {
              const id = Number(e.target.value);
              setPrimaryId(id);
              if (id === secondaryId) setSecondaryId(group.clients.find((c) => c.id !== id)?.id ?? 0);
            }}
          >
            {group.clients.map((c) => (
              <FormControlLabel key={c.id} value={c.id} control={<Radio />} label={<ClientLabel client={c} />} />
            ))}
          </RadioGroup>
          {others.length > 1 && (
            <>
              <Typography sx={{ fontWeight: 600 }}>{t.clients.merge.replace('…', '')}</Typography>
              <RadioGroup value={secondary?.id ?? 0} onChange={(e) => setSecondaryId(Number(e.target.value))}>
                {others.map((c) => (
                  <FormControlLabel key={c.id} value={c.id} control={<Radio />} label={<ClientLabel client={c} />} />
                ))}
              </RadioGroup>
            </>
          )}
          <Alert severity="info">
            {t.clients.mergeWill}
            <ul style={{ margin: '4px 0 0', paddingLeft: 20 }}>
              {t.clients.mergeKeeps.map((line) => (
                <li key={line}>{line}</li>
              ))}
            </ul>
          </Alert>
          <Alert severity="warning">{t.clients.mergeCannotUndo}</Alert>
          <ErrorAlert error={error} />
        </Stack>
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose} disabled={busy}>
          {t.common.cancel}
        </Button>
        <Button variant="contained" color="warning" onClick={() => void merge()} disabled={busy || !secondary}>
          {t.clients.mergeConfirm}
        </Button>
      </DialogActions>
    </Dialog>
  );
}

function ClientLabel({ client }: { client: GroupClient }) {
  return (
    <Box>
      <Typography sx={{ fontWeight: 600 }}>{client.fullName}</Typography>
      <Typography variant="body2" color="text.secondary">
        {describe(client)}
      </Typography>
    </Box>
  );
}
