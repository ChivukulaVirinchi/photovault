//! Assistant credentials belong to the OS store, not config.json.

use std::sync::Mutex;

// Serialize native-store access and avoid a DBus/Keychain roundtrip on
// every config read. Never cache errors or include them in logs: some
// credential errors carry the secret bytes.
static CACHED: Mutex<Option<Option<String>>> = Mutex::new(None);

fn entry() -> keyring::Result<keyring::Entry> {
    keyring::Entry::new("in.smriti.app", "assistant-api-key")
}

pub fn load() -> Option<String> {
    let mut cache = CACHED.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(value) = &*cache {
        return value.clone();
    }
    let value = match entry().and_then(|entry| entry.get_password()) {
        Ok(value) => Some(value),
        Err(keyring::Error::NoEntry) => None,
        Err(_) => return None,
    };
    *cache = Some(value.clone());
    value
}

pub fn store(value: Option<&str>) -> std::io::Result<()> {
    let mut cache = CACHED.lock().unwrap_or_else(|error| error.into_inner());
    if cache
        .as_ref()
        .is_some_and(|cached| cached.as_deref() == value)
    {
        return Ok(());
    }
    write_entry(&entry().map_err(|_| unavailable())?, value)?;
    *cache = Some(value.map(str::to_owned));
    Ok(())
}

fn unavailable() -> std::io::Error {
    std::io::Error::other("Cannot access the OS credential store. Unlock your keychain or enable a Secret Service on Linux, then retry.")
}

fn write_entry(entry: &keyring::Entry, value: Option<&str>) -> std::io::Result<()> {
    let result = match value {
        Some(value) => entry.set_password(value),
        None => entry.delete_credential(),
    };
    match result {
        Ok(()) | Err(keyring::Error::NoEntry) if value.is_none() => Ok(()),
        Ok(()) => Ok(()),
        Err(_) => Err(unavailable()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_set_replace_clear_and_redacted_failure() {
        let credential = keyring::mock::default_credential_builder()
            .build(None, "test-smriti", "test-key")
            .unwrap();
        let entry = keyring::Entry::new_with_credential(credential);
        write_entry(&entry, Some("first-secret")).unwrap();
        assert_eq!(entry.get_password().unwrap(), "first-secret");
        write_entry(&entry, Some("replacement")).unwrap();
        assert_eq!(entry.get_password().unwrap(), "replacement");
        write_entry(&entry, None).unwrap();
        assert!(matches!(entry.get_password(), Err(keyring::Error::NoEntry)));
        write_entry(&entry, None).unwrap();
        let mock = entry
            .get_credential()
            .downcast_ref::<keyring::mock::MockCredential>()
            .unwrap();
        mock.set_error(keyring::Error::BadEncoding(b"must-not-leak".to_vec()));
        assert!(!write_entry(&entry, Some("key"))
            .unwrap_err()
            .to_string()
            .contains("must-not-leak"));
    }
}
