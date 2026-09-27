import { useCallback, useEffect, useState, type KeyboardEvent, type ReactNode } from 'react';
import {
  Alert,
  Box,
  Button,
  Chip,
  CircularProgress,
  Dialog,
  DialogActions,
  DialogContent,
  DialogContentText,
  DialogTitle,
  Stack,
  TextField,
  Typography,
  type ButtonProps,
} from '@mui/material';
import { errorMessage } from '../api';
import { palette } from '../app/theme';
import { t } from '../i18n/en';

export function PageHeader({ title, subtitle, actions }: { title: string; subtitle?: string | undefined; actions?: ReactNode }) {
  return (
    <Stack direction="row" spacing={2} sx={{ mb: 2, flexWrap: 'wrap', rowGap: 1, alignItems: 'center', justifyContent: 'space-between' }}>
      <Box>
        <Typography variant="h5" component="h2" sx={{ fontWeight: 700 }}>
          {title}
        </Typography>
        {subtitle && <Typography color="text.secondary">{subtitle}</Typography>}
      </Box>
      {actions && <Stack direction="row" spacing={1}>{actions}</Stack>}
    </Stack>
  );
}

export function EmptyState({ text }: { text: string }) {
  return (
    <Typography color="text.secondary" sx={{ py: 3, textAlign: 'center' }}>
      {text}
    </Typography>
  );
}

export function Loading() {
  return (
    <Box sx={{ py: 4, display: 'flex', justifyContent: 'center' }}>
      <CircularProgress aria-label={t.common.loading} />
    </Box>
  );
}

export function ErrorAlert({ error }: { error: unknown }) {
  return error ? <Alert severity="error" sx={{ whiteSpace: 'pre-line' }}>{errorMessage(error, t.common.somethingWrong)}</Alert> : null;
}

export function StatusChip({ label, color }: { label: string; color: 'success' | 'error' | 'warning' | 'default' | 'info' }) {
  return <Chip size="small" label={label} color={color} variant={color === 'default' ? 'outlined' : 'filled'} />;
}

/**
 * Loads data with `load`, re-running when `deps` change. Returns data, error, a reload function
 * and a loading flag. The last request wins if several overlap.
 */
export function useLoader<T>(load: () => Promise<T>, deps: unknown[]) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [loading, setLoading] = useState(true);
  const [version, setVersion] = useState(0);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const stableLoad = useCallback(load, deps);
  useEffect(() => {
    let current = true;
    setLoading(true);
    stableLoad()
      .then((value) => {
        if (current) {
          setData(value);
          setError(null);
        }
      })
      .catch((e: unknown) => current && setError(e))
      .finally(() => current && setLoading(false));
    return () => {
      current = false;
    };
  }, [stableLoad, version]);
  const reload = useCallback(() => setVersion((v) => v + 1), []);
  return { data, error, loading, reload };
}

/**
 * Asks before a destructive action. With `requireReason`, the user must type a reason (min. 3
 * characters) which is passed to `onConfirm`.
 */
export function ConfirmDialog({
  open,
  title,
  text,
  confirmLabel,
  danger,
  requireReason,
  onConfirm,
  onClose,
}: {
  open: boolean;
  title: string;
  text: string;
  confirmLabel: string;
  danger?: boolean;
  requireReason?: boolean;
  onConfirm: (reason: string) => Promise<void> | void;
  onClose: () => void;
}) {
  const [reason, setReason] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  useEffect(() => {
    if (open) {
      setReason('');
      setError(null);
    }
  }, [open]);
  const confirm = async () => {
    setBusy(true);
    setError(null);
    try {
      await onConfirm(reason.trim());
      onClose();
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };
  const ready = !requireReason || reason.trim().length >= 3;
  return (
    <Dialog open={open} onClose={busy ? undefined : onClose} maxWidth="xs" fullWidth>
      <DialogTitle>{title}</DialogTitle>
      <DialogContent>
        <Stack spacing={2}>
          <DialogContentText>{text}</DialogContentText>
          {requireReason && (
            <TextField autoFocus label={t.common.reason} value={reason} onChange={(e) => setReason(e.target.value)} multiline minRows={2} />
          )}
          <ErrorAlert error={error} />
        </Stack>
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose} disabled={busy}>
          {t.common.cancel}
        </Button>
        <Button variant="contained" color={danger ? 'error' : 'primary'} disabled={!ready || busy} onClick={confirm}>
          {confirmLabel}
        </Button>
      </DialogActions>
    </Dialog>
  );
}

/** Two-column responsive form grid (no Grid component needed). */
export function FormGrid({ children }: { children: ReactNode }) {
  return <Box sx={{ display: 'grid', gridTemplateColumns: { xs: '1fr', sm: '1fr 1fr' }, gap: 2 }}>{children}</Box>;
}

/**
 * Props for a clickable row or card: it can also be reached with Tab and opened with Enter or
 * Space (keyboard-only use). Keys pressed on buttons inside the row are left to those buttons.
 */
export function rowActions(onActivate: () => void) {
  return {
    tabIndex: 0,
    role: 'button',
    onClick: onActivate,
    onKeyDown: (e: KeyboardEvent) => {
      if (e.target !== e.currentTarget) return;
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        onActivate();
      }
    },
  };
}

/** The green "Add" button used under every input on the bill screen (one look everywhere). */
export function AddButton({ children, ...props }: ButtonProps) {
  return (
    <Button
      variant="contained"
      size="medium"
      {...props}
      sx={{
        bgcolor: palette.add,
        color: '#FFFFFF',
        minHeight: 36,
        minWidth: 96,
        '&:hover': { bgcolor: palette.addHover },
        ...(props.sx as object),
      }}
    >
      {children}
    </Button>
  );
}
