//! Local account management. Import never switches the running Codex session.

use super::process::ensure_codex_not_running;
use crate::auth::persistence::read_limited;
use crate::auth::{
    add_account, ensure_chatgpt_tokens_fresh_locked, get_active_account, import_from_auth_json,
    load_accounts, read_current_auth, remove_account, switch_to_account,
    sync_active_account_tokens, update_accounts, AUTH_OPERATION_LOCK,
};
use crate::types::{
    AccountInfo, AccountsStore, AuthData, AuthDotJson, AuthMode, ImportAccountsSummary,
};
use anyhow::{Context, Result};
use std::collections::HashSet;

#[tauri::command]
pub async fn list_accounts() -> Result<Vec<AccountInfo>, String> {
    let store = load_accounts().map_err(|e| e.to_string())?;
    Ok(store
        .accounts
        .iter()
        .map(|account| {
            let mut info = AccountInfo::from_stored(account, store.active_account_id.as_deref());
            super::usage::apply_cached_account_metadata(&mut info);
            info
        })
        .collect())
}

#[tauri::command]
pub async fn get_active_account_info() -> Result<Option<AccountInfo>, String> {
    let store = load_accounts().map_err(|e| e.to_string())?;
    Ok(get_active_account()
        .map_err(|e| e.to_string())?
        .map(|account| {
            let mut info = AccountInfo::from_stored(&account, store.active_account_id.as_deref());
            super::usage::apply_cached_account_metadata(&mut info);
            info
        }))
}

#[tauri::command]
pub async fn add_account_from_file(path: String, name: String) -> Result<AccountInfo, String> {
    let account = import_from_auth_json(&path, name).map_err(|e| e.to_string())?;
    let stored = add_account(account).map_err(|e| e.to_string())?;
    let store = load_accounts().map_err(|e| e.to_string())?;
    Ok(AccountInfo::from_stored(
        &stored,
        store.active_account_id.as_deref(),
    ))
}

#[tauri::command]
pub async fn switch_account(account_id: String) -> Result<(), String> {
    switch_account_by_id(&account_id).await
}

pub async fn switch_account_by_id(account_id: &str) -> Result<(), String> {
    let _auth_guard = AUTH_OPERATION_LOCK.lock().await;
    let store = load_accounts().map_err(|e| e.to_string())?;
    if store.active_account_id.as_deref() == Some(account_id) {
        return Ok(());
    }
    ensure_codex_not_running()?;
    let target = update_accounts(|store| {
        if let Some(auth) = read_current_auth()? {
            sync_active_account_tokens(store, &auth);
        }
        store
            .accounts
            .iter()
            .find(|account| account.id == account_id)
            .cloned()
            .context("Account not found")
    })
    .map_err(|e| e.to_string())?;
    ensure_chatgpt_tokens_fresh_locked(&target)
        .await
        .map_err(|e| e.to_string())?;
    // The app may have reopened while the token refresh was in flight.
    ensure_codex_not_running()?;
    update_accounts(|store| {
        let account = store
            .accounts
            .iter_mut()
            .find(|account| account.id == account_id)
            .context("Account was removed while switching")?;
        switch_to_account(account)?;
        account.last_used_at = Some(chrono::Utc::now());
        store.active_account_id = Some(account_id.to_string());
        Ok(())
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_account(account_id: String) -> Result<(), String> {
    remove_account(&account_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn rename_account(account_id: String, new_name: String) -> Result<(), String> {
    crate::auth::storage::update_account_metadata(&account_id, Some(new_name), None, None, None)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn import_legacy_accounts_file(path: String) -> Result<ImportAccountsSummary, String> {
    import_legacy_accounts(std::path::Path::new(&path)).map_err(|e| format!("{e:#}"))
}

pub fn import_legacy_accounts(path: &std::path::Path) -> Result<ImportAccountsSummary> {
    let bytes = read_limited(path).context("Could not read the existing account file")?;
    let imported: AccountsStore = serde_json::from_slice(&bytes)
        .context("Select the original Codex Switcher accounts.json file")?;
    validate_imported_store(&imported)?;
    let previous_active = imported.active_account_id.clone();
    update_accounts(|current| {
        let summary = merge_accounts(current, imported);
        if current.active_account_id.is_none() {
            if let (Some(active_id), Some(auth)) = (previous_active, read_current_auth()?) {
                if current
                    .accounts
                    .iter()
                    .any(|account| account.id == active_id)
                {
                    current.active_account_id = Some(active_id);
                    let synced = sync_active_account_tokens(current, &auth);
                    let matches = current
                        .accounts
                        .iter()
                        .find(|account| {
                            Some(account.id.as_str()) == current.active_account_id.as_deref()
                        })
                        .is_some_and(|account| credentials_match(&account.auth_data, &auth));
                    if !synced && !matches {
                        current.active_account_id = None;
                    }
                }
            }
        }
        Ok(summary)
    })
}

fn credentials_match(data: &AuthData, auth: &AuthDotJson) -> bool {
    match data {
        AuthData::ApiKey { key } => auth.openai_api_key.as_deref() == Some(key.as_str()),
        AuthData::ChatGPT {
            id_token,
            access_token,
            refresh_token,
            ..
        } => auth.tokens.as_ref().is_some_and(|tokens| {
            tokens.id_token == *id_token
                && tokens.access_token == *access_token
                && tokens.refresh_token == *refresh_token
        }),
    }
}

fn validate_imported_store(store: &AccountsStore) -> Result<()> {
    anyhow::ensure!(store.version == 1, "Unsupported account file version");
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for account in &store.accounts {
        anyhow::ensure!(
            !account.id.trim().is_empty() && ids.insert(&account.id),
            "Missing or duplicate account ID"
        );
        anyhow::ensure!(
            !account.name.trim().is_empty() && names.insert(&account.name),
            "Missing or duplicate account name"
        );
        let valid = match &account.auth_data {
            AuthData::ApiKey { key } => {
                account.auth_mode == AuthMode::ApiKey && !key.trim().is_empty()
            }
            AuthData::ChatGPT {
                id_token,
                access_token,
                refresh_token,
                ..
            } => {
                account.auth_mode == AuthMode::ChatGPT
                    && !id_token.trim().is_empty()
                    && !access_token.trim().is_empty()
                    && !refresh_token.trim().is_empty()
            }
        };
        anyhow::ensure!(
            valid,
            "An imported account has missing or inconsistent credentials"
        );
    }
    Ok(())
}

fn merge_accounts(current: &mut AccountsStore, imported: AccountsStore) -> ImportAccountsSummary {
    let total_in_payload = imported.accounts.len();
    let mut imported_count = 0;
    for account in imported.accounts {
        if current
            .accounts
            .iter()
            .any(|existing| existing.id == account.id || existing.name == account.name)
        {
            continue;
        }
        current.accounts.push(account);
        imported_count += 1;
    }
    ImportAccountsSummary {
        total_in_payload,
        imported_count,
        skipped_count: total_in_payload - imported_count,
    }
}

#[tauri::command]
pub async fn get_masked_account_ids() -> Result<Vec<String>, String> {
    crate::auth::storage::get_masked_account_ids().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_masked_account_ids(ids: Vec<String>) -> Result<(), String> {
    crate::auth::storage::set_masked_account_ids(ids).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{get_accounts_file, get_codex_auth_file, test_support::TestHome};

    #[test]
    fn migration_preserves_source_live_auth_and_newer_existing_tokens() {
        let _home = TestHome::new();
        let old =
            crate::types::StoredAccount::new_api_key("existing".into(), "synthetic-old".into());
        let source = AccountsStore {
            accounts: vec![old.clone()],
            active_account_id: Some(old.id.clone()),
            ..AccountsStore::default()
        };
        let path = crate::auth::get_config_dir()
            .unwrap()
            .with_extension("legacy.json");
        let bytes = serde_json::to_vec(&source).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        switch_to_account(&old).unwrap();
        let auth_path = get_codex_auth_file().unwrap();
        let auth_before = std::fs::read(&auth_path).unwrap();

        let result = import_legacy_accounts(&path).unwrap();
        assert_eq!(result.imported_count, 1);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(std::fs::read(&auth_path).unwrap(), auth_before);
        assert_eq!(
            load_accounts().unwrap().active_account_id,
            Some(old.id.clone())
        );

        update_accounts(|store| {
            store.accounts[0].auth_data = AuthData::ApiKey {
                key: "synthetic-newer".into(),
            };
            Ok(())
        })
        .unwrap();
        assert_eq!(import_legacy_accounts(&path).unwrap().skipped_count, 1);
        let store = load_accounts().unwrap();
        assert!(
            matches!(&store.accounts[0].auth_data, AuthData::ApiKey { key } if key == "synthetic-newer")
        );

        let before = std::fs::read(get_accounts_file().unwrap()).unwrap();
        std::fs::write(&path, br#"{"version":1,"accounts":[{}]}"#).unwrap();
        assert!(import_legacy_accounts(&path).is_err());
        assert_eq!(std::fs::read(get_accounts_file().unwrap()).unwrap(), before);
        assert_eq!(std::fs::read(auth_path).unwrap(), auth_before);
        #[cfg(windows)]
        assert!(!before
            .windows(b"synthetic-newer".len())
            .any(|window| window == b"synthetic-newer"));
    }

    #[test]
    fn importing_does_not_activate_an_unrelated_account() {
        let _home = TestHome::new();
        let source =
            crate::types::StoredAccount::new_api_key("source".into(), "synthetic-source".into());
        let imported = AccountsStore {
            active_account_id: Some(source.id.clone()),
            accounts: vec![source],
            ..AccountsStore::default()
        };
        let path = crate::auth::get_config_dir()
            .unwrap()
            .with_extension("legacy.json");
        std::fs::write(&path, serde_json::to_vec(&imported).unwrap()).unwrap();
        switch_to_account(&crate::types::StoredAccount::new_api_key(
            "other".into(),
            "synthetic-other".into(),
        ))
        .unwrap();
        import_legacy_accounts(&path).unwrap();
        assert!(load_accounts().unwrap().active_account_id.is_none());
    }

    #[test]
    fn migration_captures_rotated_active_tokens_without_changing_codex() {
        let _home = TestHome::new();
        let old = crate::types::StoredAccount::new_chatgpt(
            "active".into(),
            None,
            None,
            None,
            "synthetic-id".into(),
            "synthetic-access-old".into(),
            "synthetic-refresh-old".into(),
            Some("synthetic-workspace".into()),
        );
        let imported = AccountsStore {
            active_account_id: Some(old.id.clone()),
            accounts: vec![old.clone()],
            ..AccountsStore::default()
        };
        let mut live = old;
        if let AuthData::ChatGPT {
            access_token,
            refresh_token,
            ..
        } = &mut live.auth_data
        {
            *access_token = "synthetic-access-new".into();
            *refresh_token = "synthetic-refresh-new".into();
        }
        switch_to_account(&live).unwrap();
        let before = std::fs::read(get_codex_auth_file().unwrap()).unwrap();
        let path = crate::auth::get_config_dir()
            .unwrap()
            .with_extension("legacy.json");
        std::fs::write(&path, serde_json::to_vec(&imported).unwrap()).unwrap();
        import_legacy_accounts(&path).unwrap();
        let current = load_accounts().unwrap();
        assert_eq!(current.active_account_id, Some(live.id));
        assert!(
            matches!(&current.accounts[0].auth_data, AuthData::ChatGPT { refresh_token, .. } if refresh_token == "synthetic-refresh-new")
        );
        assert_eq!(
            std::fs::read(get_codex_auth_file().unwrap()).unwrap(),
            before
        );
    }

    #[test]
    fn corrupt_encrypted_store_is_not_overwritten_by_import() {
        let _home = TestHome::new();
        std::fs::create_dir_all(crate::auth::get_config_dir().unwrap()).unwrap();
        let destination = get_accounts_file().unwrap();
        std::fs::write(&destination, b"corrupt-store").unwrap();
        let path = crate::auth::get_config_dir()
            .unwrap()
            .with_extension("legacy.json");
        std::fs::write(
            &path,
            serde_json::to_vec(&AccountsStore::default()).unwrap(),
        )
        .unwrap();
        assert!(import_legacy_accounts(&path).is_err());
        assert_eq!(std::fs::read(destination).unwrap(), b"corrupt-store");
    }
}
