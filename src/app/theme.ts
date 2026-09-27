import { alpha, createTheme } from '@mui/material/styles';

// System fonts only: the app must look right with no internet (no web-font downloads).
const systemFontStack = [
  '"Segoe UI Variable"',
  '"Segoe UI"',
  '-apple-system',
  'BlinkMacSystemFont',
  '"Helvetica Neue"',
  'Arial',
  'sans-serif',
].join(',');

/** Warm, calm clinic palette (v0.3 brief §14): faint beige and pink, white cards, charcoal text. */
export const palette = {
  background: '#F8F3EE',
  secondaryBackground: '#F7EDEF',
  rose: '#B7848C',
  roseDark: '#94626B',
  card: '#FFFFFF',
  text: '#2F2A2B',
  mutedText: '#6E6466',
  border: '#EADFD9',
  success: '#5E8C6A',
  warning: '#C08A3E',
  error: '#B4585A',
};

export const theme = createTheme({
  palette: {
    mode: 'light',
    primary: { main: palette.rose, dark: palette.roseDark, contrastText: '#FFFFFF' },
    secondary: { main: '#8C7B75' },
    success: { main: palette.success },
    warning: { main: palette.warning },
    error: { main: palette.error },
    background: { default: palette.background, paper: palette.card },
    text: { primary: palette.text, secondary: palette.mutedText },
    divider: palette.border,
  },
  typography: {
    fontFamily: systemFontStack,
    fontSize: 15,
    button: { textTransform: 'none', fontWeight: 600 },
  },
  shape: { borderRadius: 10 },
  components: {
    // Large click targets for staff who are not used to computers (brief §15).
    MuiButton: { defaultProps: { size: 'large', disableElevation: true }, styleOverrides: { root: { minHeight: 44 } } },
    MuiIconButton: { styleOverrides: { root: { minWidth: 40, minHeight: 40 } } },
    MuiAppBar: { styleOverrides: { root: { backgroundColor: palette.card, color: palette.text, borderBottom: `1px solid ${palette.border}` } } },
    MuiDrawer: { styleOverrides: { paper: { backgroundColor: palette.secondaryBackground, borderRight: `1px solid ${palette.border}` } } },
    MuiListItemButton: {
      styleOverrides: {
        root: {
          borderRadius: 8,
          margin: '0 8px',
          '&.Mui-selected': { backgroundColor: alpha(palette.rose, 0.16) },
          '&.Mui-selected:hover': { backgroundColor: alpha(palette.rose, 0.22) },
        },
      },
    },
    MuiTableCell: { styleOverrides: { head: { fontWeight: 700, color: palette.mutedText } } },
    // Rows and cards opened with the keyboard show where the focus is.
    MuiTableRow: { styleOverrides: { root: { '&[role="button"]:focus-visible': { outline: `2px solid ${palette.rose}`, outlineOffset: -2 } } } },
    MuiCard: { styleOverrides: { root: { borderColor: palette.border, '&[role="button"]:focus-visible': { outline: `2px solid ${palette.rose}`, outlineOffset: 2 } } } },
  },
});
