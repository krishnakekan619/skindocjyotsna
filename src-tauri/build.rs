/// Commands the UI may call. Tauri generates an `allow-<command>` permission for each;
/// the UI can use only those granted in capabilities/default.json (deny by default).
const COMMANDS: &[&str] = &[
    "get_system_info",
    "list_backups",
    "create_backup",
    "restore_backup",
    "get_sample_receipt",
    "export_sample_receipt_pdf",
];

fn main() {
    let attributes = tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(COMMANDS));
    if let Err(error) = tauri_build::try_build(attributes) {
        panic!("tauri-build failed: {error:#}");
    }
}
