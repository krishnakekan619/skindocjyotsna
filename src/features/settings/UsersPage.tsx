import { useState } from 'react';
import {
  Button,
  Card,
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
import { api, errorField, type Role, type UserSummary } from '../../api';
import { useApp } from '../../app/AppContext';
import { ErrorAlert, Loading, PageHeader, StatusChip, useLoader } from '../../components/common';
import { t } from '../../i18n/en';
import { formatDateTime } from '../../lib/dates';
import { AccountFields, accountProblem, emptyAccount, toAccount } from '../auth/AccountFields';

const ROLES: Role[] = ['RECEPTIONIST', 'ADMIN'];

/** Users and roles (admin). Users are deactivated, never deleted, so the audit trail stays readable. */
export function UsersPage() {
  const { notify } = useApp();
  const { data, error, loading, reload } = useLoader(() => api.listUsers(), []);
  const [adding, setAdding] = useState(false);
  const [editing, setEditing] = useState<UserSummary | null>(null);
  const [resetting, setResetting] = useState<UserSummary | null>(null);
  const done = (message: string) => {
    setAdding(false);
    setEditing(null);
    setResetting(null);
    notify(message);
    reload();
  };

  return (
    <>
      <PageHeader title={t.settings.usersTitle} actions={<Button variant="contained" onClick={() => setAdding(true)}>{t.settings.addUser}</Button>} />
      <ErrorAlert error={error} />
      <Card variant="outlined">
        {loading && !data ? (
          <Loading />
        ) : (
          <Table size="small">
            <TableHead>
              <TableRow>
                <TableCell>{t.account.fullName}</TableCell>
                <TableCell>{t.account.username}</TableCell>
                <TableCell>{t.settings.role}</TableCell>
                <TableCell>{t.settings.status}</TableCell>
                <TableCell>{t.settings.lastLogin}</TableCell>
                <TableCell />
              </TableRow>
            </TableHead>
            <TableBody>
              {(data ?? []).map((u) => (
                <TableRow key={u.id}>
                  <TableCell>{u.fullName}</TableCell>
                  <TableCell>{u.username}</TableCell>
                  <TableCell>{t.roles[u.role]}</TableCell>
                  <TableCell>
                    <Stack direction="row" spacing={0.5}>
                      <StatusChip label={u.isActive ? t.common.active : t.common.inactive} color={u.isActive ? 'success' : 'default'} />
                      {u.isLocked && <StatusChip label={t.settings.locked} color="error" />}
                      {u.hasPin && <StatusChip label={t.settings.hasPin} color="info" />}
                    </Stack>
                  </TableCell>
                  <TableCell>{formatDateTime(u.lastLoginAt)}</TableCell>
                  <TableCell align="right">
                    <Button size="small" onClick={() => setEditing(u)}>
                      {t.common.edit}
                    </Button>
                    <Button size="small" onClick={() => setResetting(u)}>
                      {t.settings.resetPassword}
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </Card>
      {adding && <AddUserDialog onClose={() => setAdding(false)} onSaved={(u) => done(`${u.fullName} ${t.common.saved.toLowerCase()}`)} />}
      {editing && <EditUserDialog user={editing} onClose={() => setEditing(null)} onSaved={() => done(t.common.saved)} />}
      {resetting && <ResetPasswordDialog user={resetting} onClose={() => setResetting(null)} onSaved={() => done(t.common.saved)} />}
    </>
  );
}

function AddUserDialog({ onSaved, onClose }: { onSaved: (u: UserSummary) => void; onClose: () => void }) {
  const [account, setAccount] = useState(emptyAccount);
  const [role, setRole] = useState<Role>('RECEPTIONIST');
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  const problem = accountProblem(account);
  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      onSaved(await api.createUser(toAccount(account), role));
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog open onClose={busy ? undefined : onClose} maxWidth="sm" fullWidth>
      <DialogTitle>{t.settings.addUser}</DialogTitle>
      <DialogContent>
        <Stack spacing={2} sx={{ pt: 1 }}>
          <TextField select label={t.settings.role} value={role} onChange={(e) => setRole(e.target.value as Role)}>
            {ROLES.map((r) => (
              <MenuItem key={r} value={r}>
                {t.roles[r]}
              </MenuItem>
            ))}
          </TextField>
          <AccountFields value={account} onChange={setAccount} errorField={errorField(error)} />
          <ErrorAlert error={error} />
        </Stack>
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose} disabled={busy}>
          {t.common.cancel}
        </Button>
        <Button variant="contained" onClick={save} disabled={busy || problem !== null || !account.username.trim() || !account.fullName.trim() || !account.password}>
          {t.common.save}
        </Button>
      </DialogActions>
    </Dialog>
  );
}

function EditUserDialog({ user, onSaved, onClose }: { user: UserSummary; onSaved: () => void; onClose: () => void }) {
  const [fullName, setFullName] = useState(user.fullName);
  const [role, setRole] = useState<Role>(user.role);
  const [isActive, setIsActive] = useState(user.isActive);
  const [error, setError] = useState<unknown>(null);
  const save = async () => {
    try {
      await api.updateUser(user.id, { fullName: fullName.trim(), role, isActive });
      onSaved();
    } catch (e) {
      setError(e);
    }
  };
  return (
    <Dialog open onClose={onClose} maxWidth="xs" fullWidth>
      <DialogTitle>
        {t.common.edit}: {user.username}
      </DialogTitle>
      <DialogContent>
        <Stack spacing={2} sx={{ pt: 1 }}>
          <TextField label={t.account.fullName} value={fullName} onChange={(e) => setFullName(e.target.value)} />
          <TextField select label={t.settings.role} value={role} onChange={(e) => setRole(e.target.value as Role)}>
            {ROLES.map((r) => (
              <MenuItem key={r} value={r}>
                {t.roles[r]}
              </MenuItem>
            ))}
          </TextField>
          <FormControlLabel control={<Checkbox checked={isActive} onChange={(e) => setIsActive(e.target.checked)} />} label={t.common.active} />
          <ErrorAlert error={error} />
        </Stack>
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose}>{t.common.cancel}</Button>
        <Button variant="contained" onClick={save} disabled={!fullName.trim()}>
          {t.common.save}
        </Button>
      </DialogActions>
    </Dialog>
  );
}

function ResetPasswordDialog({ user, onSaved, onClose }: { user: UserSummary; onSaved: () => void; onClose: () => void }) {
  const [password, setPassword] = useState('');
  const [confirm, setConfirm] = useState('');
  const [error, setError] = useState<unknown>(null);
  const mismatch = confirm.length > 0 && confirm !== password;
  const save = async () => {
    try {
      await api.resetUserPassword(user.id, password);
      onSaved();
    } catch (e) {
      setError(e);
    }
  };
  return (
    <Dialog open onClose={onClose} maxWidth="xs" fullWidth>
      <DialogTitle>{t.settings.resetFor(user.fullName)}</DialogTitle>
      <DialogContent>
        <Stack spacing={2} sx={{ pt: 1 }}>
          <TextField label={t.account.newPassword} type="password" value={password} onChange={(e) => setPassword(e.target.value)} helperText={t.account.passwordHint} autoFocus />
          <TextField label={t.account.confirmPassword} type="password" value={confirm} onChange={(e) => setConfirm(e.target.value)} error={mismatch} helperText={mismatch ? t.account.passwordsDiffer : ' '} />
          <ErrorAlert error={error} />
        </Stack>
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose}>{t.common.cancel}</Button>
        <Button variant="contained" onClick={save} disabled={!password || password !== confirm}>
          {t.settings.resetPassword}
        </Button>
      </DialogActions>
    </Dialog>
  );
}
