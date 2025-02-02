use gettextrs::gettext;
use glib::types::StaticType;
use gtk::{
    gio::ListStore,
    prelude::{FileExt, SettingsExtManual},
    DialogError, FileDialog, FileFilter,
};
use std::path::PathBuf;

use crate::{app::CarteroApplication, error::CarteroError, win::CarteroWindow};

fn get_cartero_file_filter() -> FileFilter {
    let filter = FileFilter::new();
    filter.add_pattern("*.cartero");
    filter.add_mime_type("application/cartero");
    filter.add_suffix("cartero");
    filter.set_name(Some(&gettext("Request (.cartero)")));
    filter
}

pub async fn open_file(win: &CarteroWindow) -> Result<PathBuf, CarteroError> {
    let filters = ListStore::with_type(FileFilter::static_type());
    let cartero = get_cartero_file_filter();
    filters.append(&cartero);

    let dialog = FileDialog::builder()
        .accept_label(gettext("Open"))
        .title(gettext("Open request"))
        .filters(&filters)
        .default_filter(&cartero)
        .modal(true)
        .build();

    let app = CarteroApplication::get();
    let settings = app.settings();
    if let Some(dir) = settings.get::<Option<String>>("last-open-dir") {
        let path = PathBuf::from(&dir);
        let file = gtk::gio::File::for_path(path);
        dialog.set_initial_folder(Some(&file));
    }

    let file = dialog.open_future(Some(win)).await.map_err(|e| {
        if let Some(file_error) = e.kind::<DialogError>() {
            match file_error {
                DialogError::Dismissed => CarteroError::NoFilePicked,
                _ => CarteroError::FileDialogError,
            }
        } else {
            CarteroError::FileDialogError
        }
    })?;
    let path = file.path().ok_or(CarteroError::FileDialogError)?;
    if let Some(folder) = path.parent() {
        let location = folder.to_str().ok_or(CarteroError::FileDialogError)?;
        settings
            .set("last-open-dir", Some(location))
            .map_err(|_| CarteroError::FileDialogError)?;
    }
    Ok(path)
}

pub async fn save_file(win: &CarteroWindow) -> Result<PathBuf, CarteroError> {
    let filters = ListStore::with_type(FileFilter::static_type());
    let cartero = get_cartero_file_filter();
    filters.append(&cartero);

    let dialog = FileDialog::builder()
        .accept_label(gettext("Save"))
        .title(gettext("Save request"))
        .modal(true)
        .filters(&filters)
        .default_filter(&cartero)
        .initial_name("request.cartero")
        .build();

    let app = CarteroApplication::get();
    let settings = app.settings();
    if let Some(dir) = settings.get::<Option<String>>("last-save-dir") {
        let path = PathBuf::from(&dir);
        let file = gtk::gio::File::for_path(path);
        dialog.set_initial_folder(Some(&file));
    }

    let file = dialog.save_future(Some(win)).await.map_err(|e| {
        if let Some(file_error) = e.kind::<DialogError>() {
            match file_error {
                DialogError::Dismissed => CarteroError::NoFilePicked,
                _ => CarteroError::FileDialogError,
            }
        } else {
            CarteroError::FileDialogError
        }
    })?;
    let path = file.path().ok_or(CarteroError::FileDialogError)?;
    if let Some(folder) = path.parent() {
        let location = folder.to_str().ok_or(CarteroError::FileDialogError)?;
        settings
            .set("last-save-dir", Some(location))
            .map_err(|_| CarteroError::FileDialogError)?;
    }
    Ok(path)
}
