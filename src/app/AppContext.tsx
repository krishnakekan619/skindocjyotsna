import { createContext, useContext } from 'react';
import type { AppStatus, BillDetail, Role, Session } from '../api';

/** Every screen of the signed-in app. */
export type Page =
  | { name: 'dashboard' }
  | { name: 'newBill'; clientId?: number; correcting?: BillDetail }
  | { name: 'bills' }
  | { name: 'clients' }
  | { name: 'client'; clientId: number }
  | { name: 'products' }
  | { name: 'expiry' }
  | { name: 'ledger' }
  | { name: 'catalog' }
  | { name: 'reports' }
  | { name: 'clinic' }
  | { name: 'services' }
  | { name: 'users' }
  | { name: 'security' }
  | { name: 'audit' }
  | { name: 'system' };

export type NoticeSeverity = 'success' | 'error' | 'info' | 'warning';

export interface AppContextValue {
  status: AppStatus;
  session: Session;
  isAdmin: boolean;
  /** Replaces the app status (after sign-in, lock, PIN change, ...). */
  setStatus: (status: AppStatus) => void;
  navigate: (page: Page) => void;
  notify: (message: string, severity?: NoticeSeverity) => void;
}

export const AppContext = createContext<AppContextValue | null>(null);

export function useApp(): AppContextValue {
  const value = useContext(AppContext);
  if (!value) throw new Error('useApp must be used inside the signed-in app');
  return value;
}

export function hasRole(role: Role, required: Role | 'ANY'): boolean {
  return required === 'ANY' || role === required;
}
