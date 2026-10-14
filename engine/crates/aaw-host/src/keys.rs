//! The person's keys for outside services, kept in the macOS Keychain: one
//! generic password for the service `AAW` and the account named after the
//! service, `elevenlabs`. The item's access list names the program that wrote
//! it, so `daw` reads it for a request without a prompt once the person has
//! allowed that build, and any other program that asks, the `security` tool
//! included, gets a macOS prompt the person answers: an agent that runs
//! commands as the person can use the key through `daw` but cannot read it
//! without the person seeing. No command prints a key.
//!
//! On another system the Keychain is not there and each function says so;
//! tests point `AAW_GENERATE_URL` at a loopback stand-in and give the key as
//! `AAW_TEST_GENERATE_KEY` instead, which `generate::key` reads.

use crate::generate::Key;

type Result<T> = std::result::Result<T, String>;

/// The Keychain item's service.
pub const KEYCHAIN_SERVICE: &str = "AAW";
/// The accounts a key is kept under.
pub const ACCOUNTS: &[&str] = &["elevenlabs"];

/// Checks that a service is one a key is kept for.
pub fn account(service: &str) -> Result<&'static str> {
    ACCOUNTS
        .iter()
        .copied()
        .find(|a| *a == service)
        .ok_or_else(|| format!("Unknown service {service:?}; keys are kept for {}", ACCOUNTS.join(", ")))
}

#[cfg(target_os = "macos")]
mod store {
    use super::*;
    use security_framework::passwords;

    const NOT_FOUND: i32 = -25300;

    fn problem(verb: &str, e: security_framework::base::Error) -> String {
        format!("The Keychain could not {verb} the key (error {}: {})", e.code(), e.message().unwrap_or_default())
    }

    pub fn read(service: &str) -> Result<Option<Key>> {
        match passwords::get_generic_password(KEYCHAIN_SERVICE, service) {
            Ok(bytes) => Ok(Key::new(&String::from_utf8_lossy(&bytes))),
            Err(e) if e.code() == NOT_FOUND => Ok(None),
            Err(e) => Err(problem("read", e)),
        }
    }

    pub fn write(service: &str, key: &str) -> Result<()> {
        passwords::set_generic_password(KEYCHAIN_SERVICE, service, key.trim().as_bytes()).map_err(|e| problem("keep", e))
    }

    pub fn remove(service: &str) -> Result<bool> {
        match passwords::delete_generic_password(KEYCHAIN_SERVICE, service) {
            Ok(()) => Ok(true),
            Err(e) if e.code() == NOT_FOUND => Ok(false),
            Err(e) => Err(problem("remove", e)),
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod store {
    use super::*;

    fn needs_a_mac() -> String {
        "The key is kept in the macOS Keychain, which needs a Mac; for a test, point AAW_GENERATE_URL at a loopback stand-in and set AAW_TEST_GENERATE_KEY".to_string()
    }

    pub fn read(_service: &str) -> Result<Option<Key>> {
        Err(needs_a_mac())
    }

    pub fn write(_service: &str, _key: &str) -> Result<()> {
        Err(needs_a_mac())
    }

    pub fn remove(_service: &str) -> Result<bool> {
        Err(needs_a_mac())
    }
}

/// The ElevenLabs key, or None when none is kept.
pub fn read() -> Result<Option<Key>> {
    store::read("elevenlabs")
}

/// Whether a key is kept for the service, without reading it out.
pub fn present(service: &str) -> Result<bool> {
    Ok(store::read(account(service)?)?.is_some())
}

/// Keeps a key for the service, in place of any kept before.
pub fn write(service: &str, key: &str) -> Result<()> {
    let service = account(service)?;
    if Key::new(key).is_none() {
        return Err("The key is empty".into());
    }
    store::write(service, key)
}

/// Removes the service's key; false when there was none.
pub fn remove(service: &str) -> Result<bool> {
    store::remove(account(service)?)
}
