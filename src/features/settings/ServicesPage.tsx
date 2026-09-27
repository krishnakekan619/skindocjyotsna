import { useState } from 'react';
import {
  Button,
  Card,
  CardContent,
  CardHeader,
  Checkbox,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  FormControlLabel,
  MenuItem,
  Stack,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableRow,
  TextField,
} from '@mui/material';
import { api, errorField, type ServiceInput, type ServiceKind, type ServiceRow } from '../../api';
import { useApp } from '../../app/AppContext';
import { EmptyState, ErrorAlert, Loading, PageHeader, StatusChip, useLoader, rowActions } from '../../components/common';
import { t } from '../../i18n/en';
import { paiseToInput, parseRupees, percentLabel, rupees } from '../../lib/money';

const KINDS: ServiceKind[] = ['CONSULTATION', 'PROCEDURE'];
const GST_RATES = [0, 500, 1200, 1800];

/** Administrators: consultation types and procedures that can be charged on bills (DEC-030). */
export function ServicesPage() {
  const { notify } = useApp();
  const { data, error, loading, reload } = useLoader(() => api.listServices(true), []);
  const [editing, setEditing] = useState<ServiceInput | null>(null);

  const blank = (kind: ServiceKind): ServiceInput => ({
    id: null,
    kind,
    name: '',
    defaultPricePaise: 0,
    gstRateBp: 0,
    discountEligible: false,
    isActive: true,
    sortOrder: ((data ?? []).filter((s) => s.kind === kind).reduce((max, s) => Math.max(max, s.sortOrder), 0) || 0) + 1,
  });

  return (
    <>
      <PageHeader title={t.settings.servicesTitle} subtitle={t.settings.servicesIntro} />
      <ErrorAlert error={error} />
      {loading && !data ? (
        <Loading />
      ) : (
        <Stack spacing={2}>
          {KINDS.map((kind) => {
            const rows = (data ?? []).filter((s) => s.kind === kind);
            return (
              <Card variant="outlined" key={kind}>
                <CardHeader
                  title={kind === 'CONSULTATION' ? t.billing.consultation : t.billing.procedures}
                  action={<Button onClick={() => setEditing(blank(kind))}>{`${t.settings.addService} ${t.settings.kinds[kind].toLowerCase()}`}</Button>}
                />
                <CardContent>
                  {rows.length === 0 ? (
                    <EmptyState text={t.common.none} />
                  ) : (
                    <Table size="small">
                      <TableHead>
                        <TableRow>
                          <TableCell>{t.settings.serviceName}</TableCell>
                          <TableCell align="right">{t.common.price}</TableCell>
                          <TableCell align="right">{t.products.gst}</TableCell>
                          <TableCell>{t.settings.status}</TableCell>
                          <TableCell />
                        </TableRow>
                      </TableHead>
                      <TableBody>
                        {rows.map((s: ServiceRow) => (
                          <TableRow key={s.id} hover sx={{ cursor: 'pointer' }} {...rowActions(() => setEditing({ ...s }))}>
                            <TableCell>{s.name}</TableCell>
                            <TableCell align="right">{rupees(s.defaultPricePaise)}</TableCell>
                            <TableCell align="right">{percentLabel(s.gstRateBp)}</TableCell>
                            <TableCell>
                              <StatusChip label={s.isActive ? t.common.active : t.common.inactive} color={s.isActive ? 'success' : 'default'} />
                            </TableCell>
                            <TableCell align="right">
                              <Button size="small">{t.common.edit}</Button>
                            </TableCell>
                          </TableRow>
                        ))}
                      </TableBody>
                    </Table>
                  )}
                </CardContent>
              </Card>
            );
          })}
        </Stack>
      )}
      {editing && (
        <ServiceDialog
          initial={editing}
          onClose={() => setEditing(null)}
          onSaved={(s) => {
            setEditing(null);
            notify(`${s.name} ${t.common.saved.toLowerCase()}`);
            reload();
          }}
        />
      )}
    </>
  );
}

function ServiceDialog({ initial, onSaved, onClose }: { initial: ServiceInput; onSaved: (s: ServiceRow) => void; onClose: () => void }) {
  const [draft, setDraft] = useState(initial);
  const [price, setPrice] = useState(() => paiseToInput(initial.defaultPricePaise));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const field = errorField(error);
  const pricePaise = parseRupees(price || '0');
  const set = (patch: Partial<ServiceInput>) => setDraft({ ...draft, ...patch });

  const save = async () => {
    if (pricePaise === null) return;
    setBusy(true);
    setError(null);
    try {
      onSaved(await api.saveService({ ...draft, name: draft.name.trim(), defaultPricePaise: pricePaise }));
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open onClose={busy ? undefined : onClose} maxWidth="xs" fullWidth>
      <DialogTitle>{initial.id ? `${t.common.edit}: ${initial.name}` : `${t.settings.addService} ${t.settings.kinds[draft.kind].toLowerCase()}`}</DialogTitle>
      <DialogContent>
        <Stack spacing={2} sx={{ pt: 1 }}>
          <TextField label={t.settings.serviceName} value={draft.name} onChange={(e) => set({ name: e.target.value })} autoFocus required error={field === 'name'} />
          <TextField label={t.settings.standardPrice} value={price} onChange={(e) => setPrice(e.target.value)} error={pricePaise === null || field === 'defaultPricePaise'} />
          <TextField select label={t.settings.gstPercent} value={draft.gstRateBp} onChange={(e) => set({ gstRateBp: Number(e.target.value) })}>
            {GST_RATES.map((bp) => (
              <MenuItem key={bp} value={bp}>
                {percentLabel(bp)}
              </MenuItem>
            ))}
          </TextField>
          <TextField
            label={t.settings.sortOrder}
            value={draft.sortOrder}
            onChange={(e) => set({ sortOrder: Number.parseInt(e.target.value.replace(/\D/g, '') || '0', 10) })}
            slotProps={{ htmlInput: { inputMode: 'numeric' } }}
          />
          <FormControlLabel control={<Checkbox checked={draft.discountEligible} onChange={(e) => set({ discountEligible: e.target.checked })} />} label={t.settings.discountEligible} />
          <FormControlLabel control={<Checkbox checked={draft.isActive} onChange={(e) => set({ isActive: e.target.checked })} />} label={t.common.active} />
          <ErrorAlert error={error} />
        </Stack>
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose} disabled={busy}>
          {t.common.cancel}
        </Button>
        <Button variant="contained" onClick={save} disabled={busy || !draft.name.trim() || pricePaise === null}>
          {t.common.save}
        </Button>
      </DialogActions>
    </Dialog>
  );
}
