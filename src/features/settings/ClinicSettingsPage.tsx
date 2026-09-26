import { useEffect, useState } from 'react';
import { Button, Card, CardContent, Checkbox, FormControlLabel, Stack, TextField } from '@mui/material';
import { api, errorField, type ClinicSettings } from '../../api';
import { useApp } from '../../app/AppContext';
import { ErrorAlert, FormGrid, Loading, PageHeader, useLoader } from '../../components/common';
import { t } from '../../i18n/en';

const int = (text: string) => Number.parseInt(text.replace(/[^\d-]/g, '') || '0', 10);

/** Clinic details printed on receipts, plus billing and security settings (admin). */
export function ClinicSettingsPage() {
  const { notify, status, setStatus } = useApp();
  const { data, error: loadError } = useLoader(() => api.getClinicSettings(), []);
  const [draft, setDraft] = useState<ClinicSettings | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  useEffect(() => {
    if (data) setDraft({ ...data, addressLines: [data.addressLines[0] ?? '', data.addressLines[1] ?? '', data.addressLines[2] ?? ''] });
  }, [data]);

  if (!draft) return loadError ? <ErrorAlert error={loadError} /> : <Loading />;
  const set = (patch: Partial<ClinicSettings>) => setDraft({ ...draft, ...patch });
  const setAddress = (index: number, text: string) => set({ addressLines: draft.addressLines.map((l, i) => (i === index ? text : l)) });
  const field = errorField(error);

  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      const saved = await api.updateClinicSettings({ ...draft, addressLines: draft.addressLines.map((l) => l.trim()).filter(Boolean) });
      setStatus({ ...status, clinicName: saved.name, idleLockMinutes: saved.idleLockMinutes });
      notify(t.common.saved);
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <PageHeader title={t.settings.clinicTitle} />
      <Card variant="outlined">
        <CardContent>
          <Stack spacing={2}>
            <FormGrid>
              <TextField label={t.setup.clinicName} value={draft.name} onChange={(e) => set({ name: e.target.value })} required error={field === 'name'} />
              <TextField label={t.setup.phone} value={draft.phone} onChange={(e) => set({ phone: e.target.value })} error={field === 'phone'} />
              {draft.addressLines.map((line, i) => (
                <TextField key={i} label={`${t.clients.address} ${i + 1}`} value={line} onChange={(e) => setAddress(i, e.target.value)} error={field === 'addressLines'} />
              ))}
              <TextField label={t.setup.email} value={draft.email} onChange={(e) => set({ email: e.target.value })} error={field === 'email'} />
              <TextField label={t.setup.gstin} value={draft.gstin} onChange={(e) => set({ gstin: e.target.value.toUpperCase() })} error={field === 'gstin'} />
            </FormGrid>
            <TextField label={t.settings.receiptFooter} value={draft.receiptFooter} onChange={(e) => set({ receiptFooter: e.target.value })} error={field === 'receiptFooter'} />
            <FormGrid>
              <TextField label={t.settings.invoicePrefix} value={draft.invoicePrefix} onChange={(e) => set({ invoicePrefix: e.target.value.toUpperCase() })} error={field === 'invoicePrefix'} />
              <TextField label={t.settings.discountCap} value={draft.receptionistDiscountCapPercent} onChange={(e) => set({ receptionistDiscountCapPercent: int(e.target.value) })} error={field === 'receptionistDiscountCapPercent'} />
              <TextField label={t.settings.idleLock} value={draft.idleLockMinutes} onChange={(e) => set({ idleLockMinutes: int(e.target.value) })} error={field === 'idleLockMinutes'} />
              <TextField label={t.settings.returnWindow} value={draft.returnWindowDays} onChange={(e) => set({ returnWindowDays: int(e.target.value) })} error={field === 'returnWindowDays'} />
              <TextField label={t.settings.utcOffset} value={draft.utcOffsetMinutes} onChange={(e) => set({ utcOffsetMinutes: int(e.target.value) })} error={field === 'utcOffsetMinutes'} />
            </FormGrid>
            <FormControlLabel control={<Checkbox checked={draft.roundToRupee} onChange={(e) => set({ roundToRupee: e.target.checked })} />} label={t.settings.roundToRupee} />
            <ErrorAlert error={error} />
            <Stack direction="row">
              <Button variant="contained" onClick={save} disabled={busy || !draft.name.trim()}>
                {t.common.save}
              </Button>
            </Stack>
          </Stack>
        </CardContent>
      </Card>
    </>
  );
}
