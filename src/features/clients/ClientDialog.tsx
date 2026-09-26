import { useState } from 'react';
import {
  Alert,
  Box,
  Button,
  Checkbox,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  FormControlLabel,
  MenuItem,
  Stack,
  TextField,
  Typography,
} from '@mui/material';
import { api, errorField, isCommandError, type ClientInput, type ClientRow, type DuplicateMatch, type Gender } from '../../api';
import { ErrorAlert, FormGrid } from '../../components/common';
import { t } from '../../i18n/en';
import { formatDateTime } from '../../lib/dates';

const GENDERS: Gender[] = ['UNDISCLOSED', 'FEMALE', 'MALE', 'OTHER'];

function draftOf(client: ClientRow | null, initialName: string): ClientInput {
  return client
    ? { id: client.id, fullName: client.fullName, phone: client.phone, email: client.email, dateOfBirth: client.dateOfBirth, gender: client.gender, address: client.address, emergencyContact: client.emergencyContact, notes: client.notes, isActive: client.isActive }
    : { id: null, fullName: initialName, phone: '', email: '', dateOfBirth: null, gender: 'UNDISCLOSED', address: '', emergencyContact: '', notes: '', isActive: true };
}

/** Shows a phone number with the middle digits hidden: 98XXXXXX12. */
function maskedPhone(phone: string): string {
  const digits = phone.replace(/\D/g, '');
  return digits.length >= 6 ? `${digits.slice(0, 2)}${'X'.repeat(digits.length - 4)}${digits.slice(-2)}` : phone;
}

/**
 * Add or edit a client. Only the name is required (data minimisation, brief §15). Before a new
 * client is saved, possible existing clients are shown so the receptionist can pick one instead
 * of creating a duplicate (DEC-032). `onSaved` receives the new client or the chosen existing one.
 */
export function ClientDialog({
  client,
  initialName = '',
  onSaved,
  onClose,
}: {
  client: ClientRow | null;
  initialName?: string;
  onSaved: (c: ClientRow) => void;
  onClose: () => void;
}) {
  const [draft, setDraft] = useState<ClientInput>(() => draftOf(client, initialName));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [matches, setMatches] = useState<DuplicateMatch[] | null>(null);
  const set = (patch: Partial<ClientInput>) => {
    setDraft({ ...draft, ...patch });
    setMatches(null); // details changed: check again before saving
  };
  const field = errorField(error);

  const save = async (allowDuplicate: boolean) => {
    setBusy(true);
    setError(null);
    try {
      if (!client && !allowDuplicate) {
        const found = await api.checkClientDuplicates({ fullName: draft.fullName, phone: draft.phone, dateOfBirth: draft.dateOfBirth, excludeId: null });
        if (found.length > 0) {
          setMatches(found);
          return;
        }
      }
      onSaved(await api.saveClient({ ...draft, allowDuplicate }));
    } catch (e) {
      if (isCommandError(e) && e.code === 'POSSIBLE_DUPLICATE') {
        setMatches(await api.checkClientDuplicates({ fullName: draft.fullName, phone: draft.phone, dateOfBirth: draft.dateOfBirth, excludeId: null }).catch(() => []));
      }
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
          {matches && matches.length > 0 ? (
            <>
              <Alert severity="warning">
                <strong>{t.clients.possibleTitle}</strong>
                <br />
                {t.clients.possibleIntro}
              </Alert>
              <Stack spacing={1}>
                {matches.map((m) => (
                  <Box key={m.client.id} sx={{ display: 'flex', alignItems: 'center', gap: 2, p: 1.5, border: 1, borderColor: 'divider', borderRadius: 2, bgcolor: 'background.paper' }}>
                    <Box sx={{ flexGrow: 1 }}>
                      <Typography sx={{ fontWeight: 600 }}>
                        {m.client.fullName} · {m.client.clientCode}
                      </Typography>
                      <Typography variant="body2" color="text.secondary">
                        {[m.client.phone && `${t.clients.phone}: ${maskedPhone(m.client.phone)}`, m.client.lastVisitAt && `${t.clients.lastVisit}: ${formatDateTime(m.client.lastVisitAt)}`, t.clients.reasons[m.reason]]
                          .filter(Boolean)
                          .join(' · ')}
                      </Typography>
                    </Box>
                    <Button variant="contained" onClick={() => onSaved(m.client)}>
                      {t.clients.useThis}
                    </Button>
                  </Box>
                ))}
              </Stack>
            </>
          ) : (
            <>
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
                {client && (
                  <>
                    <TextField label={t.clients.email} value={draft.email} onChange={(e) => set({ email: e.target.value })} error={field === 'email'} />
                    <TextField label={t.clients.emergency} value={draft.emergencyContact} onChange={(e) => set({ emergencyContact: e.target.value })} />
                  </>
                )}
              </FormGrid>
              {client && (
                <>
                  <TextField label={t.clients.address} value={draft.address} onChange={(e) => set({ address: e.target.value })} error={field === 'address'} />
                  <TextField label={t.common.notes} value={draft.notes} onChange={(e) => set({ notes: e.target.value })} helperText={t.clients.notesHint} multiline minRows={2} error={field === 'notes'} />
                  <FormControlLabel control={<Checkbox checked={draft.isActive} onChange={(e) => set({ isActive: e.target.checked })} />} label={t.common.active} />
                </>
              )}
            </>
          )}
          <ErrorAlert error={matches && matches.length > 0 ? null : error} />
        </Stack>
      </DialogContent>
      <DialogActions>
        <Button onClick={matches && matches.length > 0 ? () => setMatches(null) : onClose} disabled={busy}>
          {matches && matches.length > 0 ? t.common.back : t.common.cancel}
        </Button>
        {matches && matches.length > 0 ? (
          <Button onClick={() => void save(true)} disabled={busy}>
            {t.clients.createAnyway}
          </Button>
        ) : (
          <Button variant="contained" onClick={() => void save(false)} disabled={busy || !draft.fullName.trim()}>
            {t.common.save}
          </Button>
        )}
      </DialogActions>
    </Dialog>
  );
}
