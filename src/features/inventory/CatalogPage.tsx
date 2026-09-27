import { useState } from 'react';
import {
  Box,
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
  Stack,
  Table,
  TableBody,
  TableCell,
  TableRow,
  TextField,
} from '@mui/material';
import { api, errorField, type Category, type Supplier } from '../../api';
import { useApp } from '../../app/AppContext';
import { EmptyState, ErrorAlert, PageHeader, StatusChip, useLoader, rowActions } from '../../components/common';
import { t } from '../../i18n/en';

type SupplierDraft = { id: number | null; name: string; phone: string; gstin: string; isActive: boolean };

/** Categories and suppliers (admin). Nothing is deleted: entries are deactivated instead. */
export function CatalogPage() {
  const { notify } = useApp();
  const categories = useLoader(() => api.listCategories(), []);
  const suppliers = useLoader(() => api.listSuppliers(), []);
  const [category, setCategory] = useState<Category | 'new' | null>(null);
  const [supplier, setSupplier] = useState<SupplierDraft | null>(null);

  return (
    <>
      <PageHeader title={t.catalog.title} />
      <Box sx={{ display: 'grid', gridTemplateColumns: { xs: '1fr', md: '1fr 1fr' }, gap: 2 }}>
        <Card variant="outlined">
          <CardHeader title={t.catalog.categories} action={<Button onClick={() => setCategory('new')}>{t.catalog.addCategory}</Button>} />
          <CardContent>
            <ErrorAlert error={categories.error} />
            {(categories.data ?? []).length === 0 ? (
              <EmptyState text={t.common.none} />
            ) : (
              <Table size="small">
                <TableBody>
                  {(categories.data ?? []).map((c) => (
                    <TableRow key={c.id} hover sx={{ cursor: 'pointer' }} {...rowActions(() => setCategory(c))}>
                      <TableCell>{c.name}</TableCell>
                      <TableCell align="right">{!c.isActive && <StatusChip label={t.common.inactive} color="default" />}</TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            )}
          </CardContent>
        </Card>
        <Card variant="outlined">
          <CardHeader
            title={t.catalog.suppliers}
            action={<Button onClick={() => setSupplier({ id: null, name: '', phone: '', gstin: '', isActive: true })}>{t.catalog.addSupplier}</Button>}
          />
          <CardContent>
            <ErrorAlert error={suppliers.error} />
            {(suppliers.data ?? []).length === 0 ? (
              <EmptyState text={t.common.none} />
            ) : (
              <Table size="small">
                <TableBody>
                  {(suppliers.data ?? []).map((s: Supplier) => (
                    <TableRow key={s.id} hover sx={{ cursor: 'pointer' }} {...rowActions(() => setSupplier({ ...s }))}>
                      <TableCell>{s.name}</TableCell>
                      <TableCell>{s.phone}</TableCell>
                      <TableCell>{s.gstin}</TableCell>
                      <TableCell align="right">{!s.isActive && <StatusChip label={t.common.inactive} color="default" />}</TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            )}
          </CardContent>
        </Card>
      </Box>
      {category && (
        <CategoryDialog
          category={category === 'new' ? null : category}
          onClose={() => setCategory(null)}
          onSaved={() => {
            setCategory(null);
            notify(t.common.saved);
            categories.reload();
          }}
        />
      )}
      {supplier && (
        <SupplierDialog
          initial={supplier}
          onClose={() => setSupplier(null)}
          onSaved={() => {
            setSupplier(null);
            notify(t.common.saved);
            suppliers.reload();
          }}
        />
      )}
    </>
  );
}

function CategoryDialog({ category, onSaved, onClose }: { category: Category | null; onSaved: () => void; onClose: () => void }) {
  const [name, setName] = useState(category?.name ?? '');
  const [isActive, setIsActive] = useState(category?.isActive ?? true);
  const [error, setError] = useState<unknown>(null);
  const save = async () => {
    try {
      await api.saveCategory({ id: category?.id ?? null, name: name.trim(), isActive });
      onSaved();
    } catch (e) {
      setError(e);
    }
  };
  return (
    <Dialog open onClose={onClose} maxWidth="xs" fullWidth>
      <DialogTitle>{category ? t.common.edit : t.catalog.addCategory}</DialogTitle>
      <DialogContent>
        <Stack spacing={2} sx={{ pt: 1 }}>
          <TextField label={t.catalog.name} value={name} onChange={(e) => setName(e.target.value)} autoFocus required />
          {category && <FormControlLabel control={<Checkbox checked={isActive} onChange={(e) => setIsActive(e.target.checked)} />} label={t.common.active} />}
          <ErrorAlert error={error} />
        </Stack>
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose}>{t.common.cancel}</Button>
        <Button variant="contained" onClick={save} disabled={!name.trim()}>
          {t.common.save}
        </Button>
      </DialogActions>
    </Dialog>
  );
}

function SupplierDialog({ initial, onSaved, onClose }: { initial: SupplierDraft; onSaved: () => void; onClose: () => void }) {
  const [draft, setDraft] = useState(initial);
  const [error, setError] = useState<unknown>(null);
  const field = errorField(error);
  const save = async () => {
    try {
      await api.saveSupplier({ ...draft, name: draft.name.trim() });
      onSaved();
    } catch (e) {
      setError(e);
    }
  };
  return (
    <Dialog open onClose={onClose} maxWidth="xs" fullWidth>
      <DialogTitle>{initial.id ? t.common.edit : t.catalog.addSupplier}</DialogTitle>
      <DialogContent>
        <Stack spacing={2} sx={{ pt: 1 }}>
          <TextField label={t.catalog.name} value={draft.name} onChange={(e) => setDraft({ ...draft, name: e.target.value })} autoFocus required error={field === 'name'} />
          <TextField label={t.catalog.phone} value={draft.phone} onChange={(e) => setDraft({ ...draft, phone: e.target.value })} error={field === 'phone'} />
          <TextField label={t.catalog.gstin} value={draft.gstin} onChange={(e) => setDraft({ ...draft, gstin: e.target.value })} error={field === 'gstin'} />
          {initial.id && <FormControlLabel control={<Checkbox checked={draft.isActive} onChange={(e) => setDraft({ ...draft, isActive: e.target.checked })} />} label={t.common.active} />}
          <ErrorAlert error={error} />
        </Stack>
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose}>{t.common.cancel}</Button>
        <Button variant="contained" onClick={save} disabled={!draft.name.trim()}>
          {t.common.save}
        </Button>
      </DialogActions>
    </Dialog>
  );
}
