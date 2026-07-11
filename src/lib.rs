mod cas;
mod lua_host;

use std::sync::Arc;

use cas::CasStore;
use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let store = Arc::new(CasStore::open(data_dir.join("cas"))?);
            store.ingest_embedded_seed()?;
            app.manage(Arc::clone(&store));

            let ui_dir = data_dir.join("ui");
            let ui_file = lua_host::run_main(Arc::clone(&store), &ui_dir)?;

            if let Some(window) = app.get_webview_window("main") {
                let url = tauri::Url::from_file_path(&ui_file)
                    .map_err(|()| anyhow::anyhow!("failed to build UI file URL"))?;
                window.navigate(url)?;
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run tauri application");
}
