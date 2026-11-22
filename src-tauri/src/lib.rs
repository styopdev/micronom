
use std::path::PathBuf;

#[tauri::command]
fn create_service_folder(name: String) -> Result<String, String> {
    // Only allow safe folder names.
    if name.is_empty()
        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("Invalid service name. Use letters, numbers, hyphens, or underscores.".into());
    }

    let home_dir = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .ok_or_else(|| "Could not determine the user's home directory".to_string())?;

    let app_dir = PathBuf::from(home_dir).join("app");
    let service_dir = app_dir.join(&name);

    std::fs::create_dir_all(&app_dir)
        .map_err(|e| format!("Could not create app directory: {e}"))?;

    // Don't overwrite an existing service directory.
    if service_dir.exists() {
        return Err(format!("A service named '{name}' already exists."));
    }

    std::fs::create_dir(&service_dir)
        .map_err(|e| format!("Could not create service directory: {e}"))?;

    Ok(service_dir.to_string_lossy().into_owned())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![create_service_folder])
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}
