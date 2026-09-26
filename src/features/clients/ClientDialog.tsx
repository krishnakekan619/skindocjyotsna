import { useState } from 'react';
import { Button, Checkbox, Dialog, DialogActions, DialogContent, DialogTitle, FormControlLabel, MenuItem, Stack, TextField } from '@mui/material';
import { api, errorField, type ClientInput, type ClientRow, type Gender } from '../../api';
import { ErrorAlert, FormGrid } from '../../components/common';
import { t } from '../../i18n/en';

const GENDERS: Gender[] = ['UNDISCLOSED', 'FEMALE', 'MALE', 'OTHER'];

function draftOf(client: ClientRow | null, initialName: string): ClientInput {
  return client
    ? { id: client.id, fullName: client.fullName, phone: client.phone, email: client.email, dateOfBirth: client.dateOfBirth, gender: client.gender, address: client.address, emergencyContact: client.emergencyContact, notes: client.notes, isActive: client.isActive }
    : { id: null, fullName: initialName, phone: '', email: '', dateOfBirth: null, gender: 'UNDISCLOSED', address: '', emergencyContact: '', notes: '', isActive: true };
}

/** Add or edit a client. Only the name is required (data minimisation, brief §15). */
export function ClientDialog({ client, initialName = '', onSaved, onClose }: { client: ClientRow | null; initialName?: string; onSaved: (c: ClientRow) => void; onClose: () => void }) {
  const [draft, setDraft] = useState<ClientInput>(() => draftOf(client, initialName));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const set = (patch: Partial<ClientInput>) => setDraft({ ...draft, ...patch });
  const field = errorField(error);

  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      onSaved(await api.saveClient(draft));
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open onClose={busy ? undefined : onClose} maxWidth="sm" fullWidth>
      <DialogTitle>{client ? `${t.common.edit}: ${client.fullName}` : t.clients.add}</DialogTitle>
      <DialogContent>
        <Stack spacing={2} sx={{ pt: 1 }}>
          <FormGrid>
            <TextField label={t.clients.name} value={draft.fullName} onChange={(e) => set({ fullName: e.target.value })} required autoFocus error={field === 'fullName'} />
            <TextField label={t.clients.phone} value={draft.phone} onChange={(e) => set({ phone: e.target.value })} error={field === 'phone'} slotProps={{ htmlInput: { inputMode: 'tel' } }} />
            <TextField
              label={t.clients.dob}
              type="date"
              value={draft.dateOfBirth ?? ''}
              onChange={(e) => set({ dateOfBirth: e.target.value || null })}
              error={field === 'dateOfBirth'}
              slotProps={{ inputLabel: { shrink: true } }}
            />
            <TextField select label={t.clients.gender} value={draft.gender} onChange={(e) => set({ gender: e.target.value as Gender })}>
              {GENDERS.map((g) => (
                <MenuItem key={g} value={g}>
                  {t.clients.genders[g]}
                </MenuItem>
              ))}
            </TextField>
            <TextField label={t.clients.email} value={draft.email} onChange={(e) => set({ email: e.target.value })} error={field === 'email'} />
            <TextField label={t.clients.emergency} value={draft.emergencyContact} onChange={(e) => set({ emergencyContact: e.target.value })} />
          </FormGrid>
          <TextField label={t.clients.address} value={draft.address} onChange={(e) => set({ address: e.target.value })} error={field === 'address'} />
          <TextField label={t.common.notes} value={draft.notes} onChange={(e) => set({ notes: e.target.value })} helperText={t.clients.notesHint} multiline minRows={2} error={field === 'notes'} />
          {client && <FormControlLabel control={<Checkbox checked={draft.isActive} onChange={(e) => set({ isActive: e.target.checked })} />} label={t.common.active} />}
          <ErrorAlert error={error} />
        </Stack>
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose} disabled={busy}>
          {t.common.cancel}
        </Button>
        <Button variant="contained" onClick={save} disabled={busy || !draft.fullName.trim()}>
          {t.common.save}
        </Button>
      </DialogActions>
    </Dialog>
  );
}
