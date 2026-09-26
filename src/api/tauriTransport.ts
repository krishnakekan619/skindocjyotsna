import { invoke } from '@tauri-apps/api/core';
import type { ClinicApi } from './clinicApi';

// Argument names are camelCase here; Tauri maps them to the Rust snake_case parameters.
const call = <R>(command: string, args?: Record<string, unknown>) => invoke<R>(command, args);

/** ClinicApi implementation that calls the Rust shell over Tauri IPC. */
export const tauriApi: ClinicApi = {
  getAppStatus: () => call('get_app_status'),
  completeSetup: (input) => call('complete_setup', { input }),
  login: (username, password) => call('login', { username, password }),
  logout: () => call('logout'),
  lockScreen: () => call('lock_screen'),
  unlockWithPin: (pin) => call('unlock_with_pin', { pin }),
  unlockWithPassword: (password) => call('unlock_with_password', { password }),
  heartbeat: () => call('heartbeat'),
  changeOwnPassword: (currentPassword, newPassword) => call('change_own_password', { currentPassword, newPassword }),
  setOwnPin: (currentPassword, pin) => call('set_own_pin', { currentPassword, pin }),

  listUsers: () => call('list_users'),
  createUser: (account, role) => call('create_user', { account, role }),
  updateUser: (userId, change) => call('update_user', { userId, change }),
  resetUserPassword: (userId, newPassword) => call('reset_user_password', { userId, newPassword }),
  getClinicSettings: () => call('get_clinic_settings'),
  updateClinicSettings: (settings) => call('update_clinic_settings', { settings }),
  listAudit: (limit, beforeId) => call('list_audit', { limit, beforeId }),

  listCategories: () => call('list_categories'),
  saveCategory: (input) => call('save_category', { input }),
  listSuppliers: () => call('list_suppliers'),
  saveSupplier: (input) => call('save_supplier', { input }),
  listProducts: (filter) => call('list_products', { filter }),
  getProduct: (productId) => call('get_product', { productId }),
  saveProduct: (input) => call('save_product', { input }),
  stockIn: (input) => call('stock_in', { input }),
  adjustStock: (input) => call('adjust_stock', { input }),
  listStockLedger: (productId, limit) => call('list_stock_ledger', { productId, limit }),
  listExpiring: (withinDays) => call('list_expiring', { withinDays }),
  searchProductsForSale: (text) => call('search_products_for_sale', { text }),

  searchClients: (text, includeInactive) => call('search_clients', { text, includeInactive }),
  saveClient: (input) => call('save_client', { input }),
  getClientProfile: (clientId, fromDate, toDate) => call('get_client_profile', { clientId, fromDate, toDate }),

  quoteBill: (lines, discount, correctingBillId) => call('quote_bill', { lines, discount, correctingBillId }),
  finalizeBill: (input) => call('finalize_bill', { input }),
  listBills: (filter) => call('list_bills', { filter }),
  getBill: (billId) => call('get_bill', { billId }),
  cancelBill: (billId, reason) => call('cancel_bill', { billId, reason }),
  returnBillItems: (input) => call('return_bill_items', { input }),
  correctBill: (input) => call('correct_bill', { input }),
  getReceipt: (billId) => call('get_receipt', { billId }),
  exportReceiptPdf: (billId) => call('export_receipt_pdf', { billId }),
  openExport: (fileName) => call('open_export', { fileName }),

  getDashboard: () => call('get_dashboard'),
  salesReport: (range) => call('sales_report', { range }),
  productSalesReport: (range) => call('product_sales_report', { range }),
  stockReport: () => call('stock_report'),

  getSystemInfo: () => call('get_system_info'),
  getHealth: () => call('get_health'),
  listBackups: () => call('list_backups'),
  createBackup: () => call('create_backup'),
  restoreBackup: (fileName) => call('restore_backup', { fileName }),
  openFolder: (folder) => call('open_folder', { folder }),
};
