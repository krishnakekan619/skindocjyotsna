import { useState } from 'react';
import { Alert, Box, Button, Card, CardContent, Checkbox, Container, FormControlLabel, Stack, Step, StepLabel, Stepper, TextField, Typography } from '@mui/material';
import { api, errorField, type AppStatus, type ClinicSettings } from '../../api';
import { ErrorAlert, FormGrid } from '../../components/common';
import { t } from '../../i18n/en';
import { AccountFields, accountProblem, emptyAccount, toAccount } from './AccountFields';

const DEFAULT_CLINIC: ClinicSettings = {
  name: "Dr Jyotsna's SkinDoc Clinic",
  addressLines: ['', ''],
  phone: '',
  email: '',
  gstin: '',
  receiptFooter: 'Continue your Skincare Journey with the SkinDoc. Thank you.',
  idleLockMinutes: 15,
  invoicePrefix: 'INV',
  receptionistDiscountCapPercent: 10,
  roundToRupee: true,
  utcOffsetMinutes: 330,
  returnWindowDays: 7,
  // Empty = the standard SkinDoc message (filled in by the app).
  whatsappMessage: '',
  defaultMedicineDiscountPercent: 10,
};

/** First run only: clinic details, the owner account and (optionally) a second admin (DEC-025). */
export function SetupWizard({ onDone }: { onDone: (status: AppStatus) => void }) {
  const [step, setStep] = useState(0);
  const [clinic, setClinic] = useState<ClinicSettings>(DEFAULT_CLINIC);
  const [admin, setAdmin] = useState(emptyAccount());
  const [withSecond, setWithSecond] = useState(true);
  const [second, setSecond] = useState(emptyAccount());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);

  const setAddress = (index: number, text: string) => setClinic({ ...clinic, addressLines: clinic.addressLines.map((l, i) => (i === index ? text : l)) });
  const adminProblem = accountProblem(admin);
  const secondProblem = withSecond ? accountProblem(second) : null;

  const finish = async () => {
    setBusy(true);
    setError(null);
    try {
      onDone(await api.completeSetup({ clinic, admin: toAccount(admin), secondAdmin: withSecond ? toAccount(second) : null }));
    } catch (e) {
      setError(e);
      const field = errorField(e) ?? '';
      setStep(field.startsWith('secondAdmin') ? 2 : ['username', 'fullName', 'password', 'pin'].includes(field) ? 1 : 0);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Container maxWidth="md" sx={{ py: 5 }}>
      <Typography variant="h4" sx={{ fontWeight: 700 }}>
        {t.setup.title}
      </Typography>
      <Typography color="text.secondary" sx={{ mb: 3 }}>
        {t.setup.intro}
      </Typography>
      <Stepper activeStep={step} sx={{ mb: 3 }}>
        {[t.setup.stepClinic, t.setup.stepAdmin, t.setup.stepSecond].map((label) => (
          <Step key={label}>
            <StepLabel>{label}</StepLabel>
          </Step>
        ))}
      </Stepper>
      <Card variant="outlined">
        <CardContent>
          <Stack spacing={2}>
            <ErrorAlert error={error} />
            {step === 0 && (
              <FormGrid>
                <TextField label={t.setup.clinicName} value={clinic.name} onChange={(e) => setClinic({ ...clinic, name: e.target.value })} required autoFocus />
                <TextField label={t.setup.phone} value={clinic.phone} onChange={(e) => setClinic({ ...clinic, phone: e.target.value })} />
                <TextField label={t.setup.address1} value={clinic.addressLines[0] ?? ''} onChange={(e) => setAddress(0, e.target.value)} />
                <TextField label={t.setup.address2} value={clinic.addressLines[1] ?? ''} onChange={(e) => setAddress(1, e.target.value)} />
                <TextField label={t.setup.email} value={clinic.email} onChange={(e) => setClinic({ ...clinic, email: e.target.value })} />
                <TextField label={t.setup.gstin} value={clinic.gstin} onChange={(e) => setClinic({ ...clinic, gstin: e.target.value.toUpperCase() })} />
              </FormGrid>
            )}
            {step === 1 && (
              <>
                <Alert severity="info">{t.setup.adminIntro}</Alert>
                <AccountFields value={admin} onChange={setAdmin} errorField={errorField(error)} />
                {adminProblem && <Alert severity="warning">{adminProblem}</Alert>}
              </>
            )}
            {step === 2 && (
              <>
                <Alert severity="info">{t.setup.secondIntro}</Alert>
                <FormControlLabel control={<Checkbox checked={withSecond} onChange={(e) => setWithSecond(e.target.checked)} />} label={t.setup.addSecond} />
                {withSecond && <AccountFields value={second} onChange={setSecond} withPin={false} />}
                {secondProblem && <Alert severity="warning">{secondProblem}</Alert>}
              </>
            )}
            <Box sx={{ display: 'flex', justifyContent: 'space-between' }}>
              <Button disabled={step === 0 || busy} onClick={() => setStep(step - 1)}>
                {t.common.back}
              </Button>
              {step < 2 ? (
                <Button variant="contained" disabled={(step === 0 && !clinic.name.trim()) || (step === 1 && (!admin.password || adminProblem !== null))} onClick={() => setStep(step + 1)}>
                  {t.setup.next}
                </Button>
              ) : (
                <Button variant="contained" disabled={busy || secondProblem !== null} onClick={finish}>
                  {t.setup.finish}
                </Button>
              )}
            </Box>
          </Stack>
        </CardContent>
      </Card>
    </Container>
  );
}
