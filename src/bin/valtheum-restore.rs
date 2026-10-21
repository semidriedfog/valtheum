#![cfg_attr(windows, windows_subsystem = "windows")]
#[path = "../archive_crypto.rs"]
#[allow(dead_code)]
mod archive_crypto;
#[path = "../language.rs"]
mod language;
#[path = "../restore_ui.rs"]
mod restore_ui;
#[path = "../restore_workflow.rs"]
mod restore_workflow;
fn main() {
    if let Err(error) = restore_ui::run() {
        rfd::MessageDialog::new()
            .set_title(crate::language::t("valtheum 복원 도구"))
            .set_description(format!("복원 화면을 열 수 없습니다.\n{error:#}"))
            .set_level(rfd::MessageLevel::Error)
            .show();
    }
}
