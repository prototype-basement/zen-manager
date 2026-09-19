#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod fonts;
mod library;
mod music;
mod mtp;
mod queue;
mod settings;
mod worker;

use settings::{LANGUAGES, Settings, Theme};
use slint::ComponentHandle;
use std::cell::RefCell;
use std::rc::Rc;
use slint::winit_030::{EventResult, WinitWindowAccessor, winit};
use std::path::PathBuf;
use worker::Command;

slint::include_modules!();

/// Release codename. Bumped alongside the major version, not per patch.
const CODENAME: &str = "Awakening";

fn main() -> Result<(), slint::PlatformError> {
    // Both must happen before the window exists: fonts so the first frame is
    // laid out with them, the language because bundled translations are picked
    // up as the UI is built.
    fonts::install();

    let settings = Rc::new(RefCell::new(Settings::load()));

    let ui = AppWindow::new()?;

    // Must come after the first component exists, otherwise there is no global
    // context yet and the call fails with NoTranslationsBundled.
    apply_language(&settings.borrow().language);

    ui.set_language_names(slint::ModelRc::new(slint::VecModel::from(
        LANGUAGES
            .iter()
            .map(|l| slint::SharedString::from(l.name))
            .collect::<Vec<_>>(),
    )));
    ui.set_app_version(env!("CARGO_PKG_VERSION").into());
    ui.set_app_codename(CODENAME.into());
    ui.set_theme_mode(settings.borrow().theme.code());
    ui.set_language_index(settings.borrow().language_index());
    ui.invoke_apply_theme();

    ui.on_set_theme({
        let ui = ui.as_weak();
        let settings = Rc::clone(&settings);
        move |code| {
            let Some(ui) = ui.upgrade() else { return };
            settings.borrow_mut().theme = Theme::from_code(code);
            settings.borrow().save();
            ui.set_theme_mode(code);
            ui.invoke_apply_theme();
        }
    });

    ui.on_set_language({
        let ui = ui.as_weak();
        let settings = Rc::clone(&settings);
        move |index| {
            let Some(ui) = ui.upgrade() else { return };
            let language = Settings::language_from_index(index);
            settings.borrow_mut().language = language.clone();
            settings.borrow().save();
            ui.set_language_index(index);
            // Switching marks every @tr binding dirty, so the UI re-renders in
            // the new language straight away, with no restart.
            apply_language(&language);
        }
    });
    let handle = worker::spawn(ui.as_weak());

    install_file_drop(&ui, handle.sender());
    install_os_theme(&ui);

    if let Some(folder) = std::env::args().nth(1).map(PathBuf::from) {
        handle.sender().send(Command::AddPaths(vec![folder])).ok();
    }

    // Two separate dialogs because a native file picker cannot offer files and
    // folders at once: choosing a folder queues everything inside it.
    ui.on_browse_folders({
        let tx = handle.sender();
        move || {
            if let Some(paths) = rfd::FileDialog::new()
                .set_title("Add music folders")
                .pick_folders()
            {
                let _ = tx.send(Command::AddPaths(paths));
            }
        }
    });

    ui.on_browse_files({
        let tx = handle.sender();
        move || {
            if let Some(paths) = rfd::FileDialog::new()
                .set_title("Add music files")
                .add_filter("Audio", library::AUDIO_EXTENSIONS)
                .pick_files()
            {
                let _ = tx.send(Command::AddPaths(paths));
            }
        }
    });

    ui.on_refresh_device({
        let tx = handle.sender();
        move || {
            let _ = tx.send(Command::RefreshDevice);
        }
    });

    ui.on_transfer({
        let tx = handle.sender();
        move || {
            let _ = tx.send(Command::Transfer);
        }
    });

    ui.on_clear_queue({
        let tx = handle.sender();
        move || {
            let _ = tx.send(Command::ClearQueue);
        }
    });

    ui.on_remove_queued({
        let tx = handle.sender();
        move |index| {
            let _ = tx.send(Command::RemoveAt(index as usize));
        }
    });

    ui.on_toggle_expanded({
        let tx = handle.sender();
        move |index| {
            let _ = tx.send(Command::ToggleExpanded(index as usize));
        }
    });

    ui.on_set_checked({
        let tx = handle.sender();
        move |index, checked| {
            let _ = tx.send(Command::SetChecked(index as usize, checked));
        }
    });

    ui.on_set_all_checked({
        let tx = handle.sender();
        move |checked| {
            let _ = tx.send(Command::SetAllChecked(checked));
        }
    });

    ui.on_set_all_expanded({
        let tx = handle.sender();
        move |expanded| {
            let _ = tx.send(Command::SetAllExpanded(expanded));
        }
    });

    ui.on_delete_checked({
        let tx = handle.sender();
        move || {
            let _ = tx.send(Command::DeleteChecked);
        }
    });

    ui.on_cancel_transfer({
        let cancel = handle.canceller();
        move || cancel()
    });

    ui.run()
}

fn apply_language(language: &str) {
    // An empty code means "follow the system", which is what Slint does for an
    // empty string, so this is safe to call unconditionally.
    if let Err(e) = slint::select_bundled_translation(language) {
        eprintln!("Could not switch to language {language:?}: {e}");
    }
}

/// The public `Palette` global exposes no OS colour scheme, so the system
/// light/dark preference is read from the winit window instead — both at
/// startup and whenever the OS switches while the app is open.
fn install_os_theme(ui: &AppWindow) {
    let apply = |ui: &AppWindow, theme: Option<winit::window::Theme>| {
        ui.set_os_dark(matches!(theme, Some(winit::window::Theme::Dark)));
        ui.invoke_apply_theme();
    };

    let initial = ui.window().with_winit_window(|window| window.theme()).flatten();
    apply(ui, initial);

    let weak = ui.as_weak();
    ui.window().on_winit_window_event(move |_, event| {
        if let winit::event::WindowEvent::ThemeChanged(theme) = event
            && let Some(ui) = weak.upgrade()
        {
            apply(&ui, Some(*theme));
        }
        EventResult::Propagate
    });
}

/// Slint's own `DropArea` only handles drags that start inside the app, so
/// accepting files from Finder means reading winit's window events directly.
/// winit reports one `DroppedFile` per file with no cursor position, so a drop
/// anywhere on the window counts.
fn install_file_drop(ui: &AppWindow, tx: std::sync::mpsc::Sender<Command>) {
    let weak = ui.as_weak();

    ui.window().on_winit_window_event(move |_, event| {
        let Some(ui) = weak.upgrade() else {
            return EventResult::Propagate;
        };

        match event {
            winit::event::WindowEvent::HoveredFile(_) => {
                ui.set_drop_hover(true);
            }
            winit::event::WindowEvent::HoveredFileCancelled => {
                ui.set_drop_hover(false);
            }
            winit::event::WindowEvent::DroppedFile(path) => {
                ui.set_drop_hover(false);
                let _ = tx.send(Command::AddPaths(vec![path.clone()]));
            }
            _ => {}
        }

        EventResult::Propagate
    });
}
