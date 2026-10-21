use crate::restore_workflow as workflow;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::WindowBuilder,
};
use wry::WebViewBuilder;
use zeroize::{Zeroize, Zeroizing};

#[derive(Deserialize)]
struct Command {
    action: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    language: String,
    #[serde(default)]
    notes_only: bool,
}
impl Drop for Command {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}
enum UiEvent {
    Command(Command),
    Update(Value),
    Finished(Value),
}
#[derive(Default)]
struct Selection {
    key: Option<PathBuf>,
    files: Vec<PathBuf>,
    folder: Option<PathBuf>,
}
impl Selection {
    fn json(&self) -> Value {
        json!({"type":"selection", "key":self.key.as_ref().map(|p| p.display().to_string()), "files":self.files.iter().map(|p|p.display().to_string()).collect::<Vec<_>>(), "folder":self.folder.as_ref().map(|p|p.display().to_string())})
    }
}

pub fn run() -> anyhow::Result<()> {
    let language_path = crate::language::preference_path();
    crate::language::load_preference(&language_path);
    let event_loop = EventLoopBuilder::<UiEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let builder = WindowBuilder::new()
        .with_title(crate::language::t("valtheum · 독립 복원 도구"))
        .with_inner_size(tao::dpi::LogicalSize::new(880.0, 820.0))
        .with_min_inner_size(tao::dpi::LogicalSize::new(600.0, 560.0));
    #[cfg(windows)]
    let builder = {
        use tao::platform::windows::{IconExtWindows, WindowBuilderExtWindows};
        let small = tao::window::Icon::from_resource(1, Some(tao::dpi::PhysicalSize::new(32, 32)))?;
        let large =
            tao::window::Icon::from_resource(1, Some(tao::dpi::PhysicalSize::new(256, 256)))?;
        builder
            .with_window_icon(Some(small))
            .with_taskbar_icon(Some(large))
    };
    let window = builder.build(&event_loop)?;
    let ipc = proxy.clone();
    let first_navigation = std::cell::Cell::new(true);
    let language_runtime = format!(
        "<script data-i18n-runtime>window.__valtheumLanguage={};{}\n{}</script>",
        serde_json::to_string(crate::language::code())?,
        include_str!("../static/i18n-catalog.js"),
        include_str!("../static/i18n.js")
    );
    let html = include_str!("../restore-ui/index.html")
        .replace("<!--i18n-runtime-->", &language_runtime)
        .replace(
            "<!--license-data-->",
            include_str!("../static/licenses.json"),
        )
        .replace(
            "<!--license-style-->",
            &format!("<style>{}</style>", include_str!("../static/licenses.css")),
        )
        .replace(
            "<!--license-script-->",
            &format!("<script>{}</script>", include_str!("../static/licenses.js")),
        )
        .replace(
            "<!--caps-lock-style-->",
            &format!(
                "<style>{}</style>",
                include_str!("../static/password-caps-lock.css")
            ),
        )
        .replace(
            "<!--caps-lock-script-->",
            &format!(
                "<script>{}</script>",
                include_str!("../static/password-caps-lock.js")
            ),
        );
    let webview = WebViewBuilder::new()
        .with_html(&html)
        .with_devtools(false)
        .with_incognito(true)
        // WebView2's initial NavigateToString URI can vary by runtime version.
        // Only the initial embedded document is allowed to navigate.
        .with_navigation_handler(move |_| first_navigation.replace(false))
        .with_new_window_req_handler(|_| false)
        .with_ipc_handler(move |request| {
            let mut body = request.into_body();
            if body.len() <= 8192 {
                if let Ok(command) = serde_json::from_str::<Command>(&body) {
                    let _ = ipc.send_event(UiEvent::Command(command));
                }
            }
            body.zeroize();
        })
        .build(&window)?;
    let send = move |value: Value| {
        let _ = webview.evaluate_script(&format!("window.restoreUI.receive({value});"));
    };
    let mut selection = Selection::default();
    let mut busy = false;
    let mut closing = false;
    let cancel = Arc::new(AtomicBool::new(false));
    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(UiEvent::Command(mut command)) => {
                let password = Zeroizing::new(std::mem::take(&mut command.password));
                if command.action == "cancel" {
                    cancel.store(true, Ordering::Release);
                    send(json!({"type":"status","message":"취소 중… 현재 임시 파일을 정리합니다."}));
                    return;
                }
                if busy { return; }
                match command.action.as_str() {
                    "ready" => send(selection.json()),
                    "language" => {
                        if crate::language::save_preference(&language_path,&command.language).is_ok() {
                            window.set_title(&crate::language::t("valtheum · 독립 복원 도구"));
                            send(json!({"type":"language","language":crate::language::code()}));
                        } else {
                            send(json!({"type":"language","language":crate::language::code()}));
                            send(json!({"type":"error","message":"언어 설정을 저장하지 못했습니다."}));
                        }
                    },
                    "key" => {
                        if let Some(path) = rfd::FileDialog::new().set_parent(&window).set_title(crate::language::t("vault.key 또는 DB·키 백업 선택")).add_filter(crate::language::t("키 파일 / 백업"), &["key", "vtbackup", "zip"]).pick_file() { selection.key = Some(path); }
                        send(selection.json());
                    }
                    "files" => {
                        if let Some(paths) = rfd::FileDialog::new().set_parent(&window).set_title(crate::language::t("복원할 암호화 파일 선택 (여러 개 가능)")).add_filter(crate::language::t("암호화된 미디어·메모 파일"), &["vta", "vtv", "vti", "vtn"]).pick_files() { selection.files = paths; }
                        send(selection.json());
                    }
                    "folder" => {
                        if let Some(path) = rfd::FileDialog::new().set_parent(&window).set_title(crate::language::t("복원 결과를 저장할 폴더 선택")).pick_folder() { selection.folder = Some(path); }
                        send(selection.json());
                    }
                    "start" => {
                        let (Some(key_path), Some(folder)) = (selection.key.clone(), selection.folder.clone()) else {
                            send(json!({"type":"error","message":"키 백업, 암호화 파일과 저장 폴더를 모두 선택해 주세요."})); return;
                        };
                        if selection.files.is_empty() || password.is_empty() || password.len() > 1024 {
                            send(json!({"type":"error","message":"암호화 파일과 올바른 비밀번호를 입력해 주세요."})); return;
                        }
                        busy = true;
                        cancel.store(false, Ordering::Release);
                        send(json!({"type":"started"}));
                        let notes_only=command.notes_only;
                        let files = selection.files.clone();
                        let proxy = proxy.clone();
                        let cancel = cancel.clone();
                        std::thread::spawn(move || {
                            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> anyhow::Result<workflow::Summary> {
                                let key = workflow::read_key_source(&key_path, &password)?;
                                drop(password);
                                workflow::restore_files_mode(&key, &files, &folder, &cancel, notes_only,
                                    |p| { let _ = proxy.send_event(UiEvent::Update(json!({"type":"progress","value":p}))); },
                                    |r| { let _ = proxy.send_event(UiEvent::Update(json!({"type":"file","value":r}))); })
                            }));
                            let value = match result {
                                Ok(Ok(summary)) => json!({"type":"finished","value":summary}),
                                Ok(Err(error)) => json!({"type":"error","message":format!("{error:#}")}),
                                Err(_) => json!({"type":"error","message":"예기치 않은 오류로 복원을 중단했습니다. 완료된 파일은 유지됩니다."}),
                            };
                            let _ = proxy.send_event(UiEvent::Finished(value));
                        });
                    }
                    _ => {}
                }
            }
            Event::UserEvent(UiEvent::Update(value)) => send(value),
            Event::UserEvent(UiEvent::Finished(value)) => {
                busy = false; send(value);
                if closing { *flow = ControlFlow::Exit; }
            }
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                if busy {
                    closing = true; cancel.store(true, Ordering::Release);
                    send(json!({"type":"status","message":"임시 파일 정리 후 창을 닫습니다…"}));
                } else { *flow = ControlFlow::Exit; }
            }
            _ => {}
        }
    });
}
