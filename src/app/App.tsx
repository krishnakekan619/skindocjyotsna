import { AppBar, Box, Container, Stack, Toolbar, Typography } from '@mui/material';
import { BackupCard } from '../features/backup/BackupCard';
import { ReceiptCard } from '../features/receipt/ReceiptCard';
import { SystemStatusCard } from '../features/system/SystemStatusCard';

export function App() {
  return (
    <Box sx={{ minHeight: '100vh', bgcolor: 'background.default' }}>
      <AppBar position="static" color="primary" elevation={0}>
        <Toolbar>
          <Typography variant="h6" component="h1" sx={{ fontWeight: 700 }}>
            SkinDocJyotsna
          </Typography>
          <Typography variant="body2" sx={{ ml: 2, opacity: 0.85 }}>
            Phase 0 technical preview
          </Typography>
        </Toolbar>
      </AppBar>
      <Container maxWidth="md" sx={{ py: 4 }}>
        <Stack spacing={3}>
          <SystemStatusCard />
          <BackupCard />
          <ReceiptCard />
        </Stack>
      </Container>
    </Box>
  );
}
