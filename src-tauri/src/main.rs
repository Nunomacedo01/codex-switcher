// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn lock_desktop_instance(directory: &std::path::Path) -> std::io::Result<std::fs::File> {
    std::fs::create_dir_all(directory)?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join("desktop.lock"))?;
    file.try_lock().map_err(std::io::Error::other)?;
    Ok(file)
}

#[cfg(target_os = "linux")]
mod linux_gtk;

fn main() {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if !arguments.is_empty() {
        if arguments.len() != 2 || arguments[0] != "--import-legacy-accounts" {
            eprintln!("Usage: codex-switcher [--import-legacy-accounts <accounts.json>]");
            std::process::exit(2);
        }
        match codex_switcher_lib::commands::import_legacy_accounts(std::path::Path::new(
            &arguments[1],
        )) {
            Ok(summary) => println!(
                "Imported {}, skipped {} accounts.",
                summary.imported_count, summary.skipped_count
            ),
            Err(error) => {
                eprintln!("Account import failed: {error:#}");
                std::process::exit(1);
            }
        }
        return;
    }
    #[cfg(target_os = "linux")]
    for variable in ["GTK_MODULES", "GTK3_MODULES"] {
        if let Some(filtered) = std::env::var_os(variable)
            .as_deref()
            .and_then(linux_gtk::without_appmenu_module)
        {
            unsafe { std::env::set_var(variable, filtered) };
        }
    }

    // A second desktop must not rotate the same single-use OAuth refresh tokens.
    let _instance = match codex_switcher_lib::auth::get_config_dir()
        .and_then(|directory| lock_desktop_instance(&directory).map_err(Into::into))
    {
        Ok(instance) => instance,
        Err(error) => {
            eprintln!("Could not start Codex Switcher Local: {error:#}");
            std::process::exit(1);
        }
    };
    codex_switcher_lib::run()
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_one_desktop_instance_can_hold_the_lock() {
        let path = std::env::temp_dir().join(format!("switcher-instance-{}", uuid::Uuid::new_v4()));
        let first = super::lock_desktop_instance(&path).unwrap();
        assert!(super::lock_desktop_instance(&path).is_err());
        drop(first);
        drop(super::lock_desktop_instance(&path).unwrap());
        std::fs::remove_dir_all(path).unwrap();
    }
}
