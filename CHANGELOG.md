# Changelog

## v0.3.0 (unreleased): receptionist-first billing

### New
- **Navigation:** three big buttons at the top of the menu: **+ New Bill** (Ctrl/⌘+N), **Clients**, **Products & stock**.
- **One-screen New Bill:** client, consultation, procedures and medicines on one screen, then payment and Finalize.
  - Type 2 letters of a name, phone or client ID. Suggestions forgive small spelling mistakes.
  - **Recent clients** and **recent products** can be picked with one click.
  - **+ Create "name" as new client** works right on the bill. The new client is selected automatically.
  - **+ Consultation ₹500** takes one click (other consultation types are in the ▾ menu), and **+ Procedure** picks from a list.
  - Consultations and procedures never touch stock.
  - The discount applies to **medicines only** (DEC-030).
  - Charging less than a standard fee needs administrator approval.
  - **Finalize & print** (Ctrl/⌘+Enter) and **Finalize only**. The result shows "Bill … created successfully" with Print, Save PDF, **Send via WhatsApp** and New bill.
  - More shortcuts: F2 jumps to the client search and F4 to the product search.
- **Receipts:** lines are grouped under Consultation / Procedures / Medicines & Products, with section subtotals and "Discount on medicines". Every receipt says it is a computer-generated e-receipt that needs no signature or stamp.
- **WhatsApp handoff** (DEC-031):
  - opens your WhatsApp on the client's chat with the clinic's message typed in (editable in Clinic details);
  - puts the bill PDF on the clipboard, so press Ctrl/⌘+V, then Send;
  - if the client has no mobile number, offers **Add mobile number**.
- **Duplicate clients** (DEC-032):
  - a warning with **Use this client** before creating a client with the same phone or name;
  - **Clients → Find duplicates**;
  - **Merge** for administrators: all bills and payments move to the kept client, and nothing is deleted.
- **Client profile:** total visits, total bills, last visit and total spent, computed over all bills (not only the latest 200). Consultations and procedures appear in the history.
- **Settings → Consultations & procedures** (administrators): names, standard prices, GST, order, active/inactive, and whether the discount may apply.
- **Look and feel:** a warm beige, pink and white theme with a dusty-rose accent, and larger buttons.
- **Safer upgrades:** the app backs up the database automatically before an update changes it (DEC-033).

### Database
- Migration 0005 (additive): the `service` catalog (seeded with General/Follow-up Consultation, Dressing, Injection and Nebulization; edit the prices in Settings), `bill_service_item`, `bill_item.discount_eligible`, client `name_key` / `phone_digits` / `merged_into_client_id`, and the merge exception in the bill lock.

## v0.2.0 (2026-09-26): first feature-complete version

### What the app does
- **Sign-in and security:** first-run setup with an optional second administrator; administrator and receptionist roles; an unlock PIN; an idle lock (15 minutes by default); account lockout after wrong passwords; an audit log.
- **Inventory:** products, categories and suppliers; batches with expiry dates; stock in and adjustments with a reason; an immutable stock ledger; expiry and low-stock lists.
- **Clients:** profiles with their purchase history.
- **Billing:**
  - stock is taken earliest-expiry first and never goes negative;
  - out-of-stock items are recorded as "not supplied";
  - bill-level discount, which an administrator must approve above the receptionist limit;
  - split payments and change;
  - totals rounded to the rupee;
  - the same bill is never saved twice.
- **Receipts:** A5 PDF and printing.
- **After the sale:** returns (partial, restocked or written off); cancellation (administrators); correction (cancel + reissue).
- **Reports:** sales, product sales, stock.
- **Backups:** automatic and manual backups, and restore with a safety backup.

### Fixed after the pre-release review
- **Billing and returns:**
  - A full return never refunds more than was collected; it now includes the round-off.
  - Refunds can never exceed the bill total.
  - A receptionist's discount of exactly the limit (for example 10%) no longer asks for approval.
  - Duplicate lines in one return are refused.
  - The return window counts calendar days.
  - "Cash received" is only accepted for cash payments.
  - A bill key reused for a different bill is refused instead of silently returning the other bill.
- **Database:** new triggers (migration 0004) refuse any change to a finalized bill's amounts or lines, allow a bill to be closed only once, and allow refunds only to grow.
- **Inventory:**
  - An auto-generated SKU skips numbers already typed by hand.
  - Stock-in to an existing batch at different prices is refused instead of silently repricing the shelf.
  - Stock-in prices are capped.
- **Reports:** refunds count on the day of the return, so a closed day's figures never change.
- **Security:**
  - Wrong "current password" attempts count towards the account lockout.
  - Backup and restore re-check the administrator.
  - Everyone is signed out after any restore attempt.
  - An empty database is never backed up.
  - Passwords and PINs are kept out of debug output.
  - The settings audit entry records before and after values.
- **Desktop shell:**
  - Commands run off the UI thread, so the window doesn't freeze during backups or sign-in.
  - The app recovers from an internal panic without a restart.
  - The server-side idle lock has a one-minute grace period.
- **Screens:**
  - A bill being typed survives the idle lock.
  - The app returns to the lock or sign-in screen when the session ends.
  - Wrong administrator approval details can be re-entered.
  - Adding products while correcting a bill counts the original stock.
  - Clearing a quantity no longer deletes the line.
  - F9 follows the same checks as the Finalize button.
  - Cancel and Correct are hidden where they aren't allowed.
  - Smaller fixes to settings, stock adjustments and the audit log.

### Known limitations (planned)
- **Lockout:** it lasts a fixed 5 minutes and doesn't get longer with repeated lockouts.
- **Search:** `%` and `_` in a search are treated as spaces.
- ~~**Client profile:** visit count and total spent are computed from the latest 200 bills.~~ Fixed in v0.3.0.
- **Startup errors:** on Windows, a failure at start-up (for example, the data folder can't be written) closes the app without a message. The reason is written to the log when logging works.
- **Product sales report:** it is by bill date; refunds are subtracted whenever they happened.
- **Drafts:** bills being typed are not saved as drafts; closing the app loses them (DEC-028).
- **Build versions:** Rust libraries resolve to the latest stable release at build time (DEC-029).
