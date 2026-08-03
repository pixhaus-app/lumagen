//! API-key storage in the OS credential vault. The key itself lives only
//! in the platform vault — never in settings state on disk or in the config file. The
//! settings UI keeps a boolean "is a key set" plus a non-persisted edit buffer instead.
//!
//! Every vault operation is blocking I/O that can pop an OS unlock dialog, so none of it
//! runs on the UI thread: reads/writes go through the app's background worker and return
//! over a channel the frame loop drains (see `state.rs`).

use keyring_core::{Entry, Error};

/// Stable service name — one per app, with each provider key its own entry under it.
const SERVICE: &str = "dev.lumagen.api-keys";

/// The providers Lumagen can hold a key for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Provider {
    /// fal.ai — the default primary image provider.
    Fal,
    /// OpenRouter — alternate image provider.
    OpenRouter,
    /// Legacy OpenAI entry (kept for the settings UI).
    OpenAi,
    /// LLM for the Description Check.
    Llm,
}

impl Provider {
    pub const fn account(self) -> &'static str {
        match self {
            Provider::Fal => "fal",
            Provider::OpenRouter => "openrouter",
            Provider::OpenAi => "openai",
            Provider::Llm => "llm",
        }
    }
}

/// Install the OS credential store as keyring's default. Call once, early in startup,
/// before any `Entry` is created. A headless/locked environment is reported via the
/// logged error, not a panic — the app still runs, keys just won't persist.
pub fn install_credential_store() {
    #[cfg(target_os = "windows")]
    let store = windows_native_keyring_store::Store::new();

    #[cfg(not(target_os = "windows"))]
    {
        // No provider wired for this platform yet; leave the default store unset. Entry
        // operations then return `Error::NoDefaultStore`, which `load`/`store` map to a
        // calm "keychain unavailable" result rather than a crash.
        tracing::warn!("no keyring provider for this platform; API keys will not persist");
        return;
    }

    #[cfg(target_os = "windows")]
    let store = match store {
        Ok(store) => store,
        Err(e) => {
            tracing::error!("failed to open the OS credential store: {e}");
            return;
        }
    };

    keyring_core::set_default_store(store);
}

/// Read a provider's key. `Ok(None)` means "not set yet" (an ordinary state, not an
/// error); vault unavailability also maps to `Ok(None)` so the UI treats it as "no key"
/// rather than surfacing a scary failure. Blocking — call off the UI thread.
pub fn load(provider: Provider) -> Option<String> {
    let entry = match Entry::new(SERVICE, provider.account()) {
        Ok(entry) => entry,
        Err(e) => {
            tracing::warn!("keyring entry unavailable for {}: {e}", provider.account());
            return None;
        }
    };
    match entry.get_password() {
        Ok(key) => Some(key),
        Err(Error::NoEntry) => None,
        Err(e) => {
            tracing::warn!("could not read {} key from the vault: {e}", provider.account());
            None
        }
    }
}

/// Store a provider's key. Returns `false` (and logs) if the vault is unavailable.
/// Blocking — call off the UI thread.
pub fn store(provider: Provider, key: &str) -> bool {
    match Entry::new(SERVICE, provider.account()).and_then(|e| e.set_password(key)) {
        Ok(()) => true,
        Err(e) => {
            tracing::error!("could not store {} key in the vault: {e}", provider.account());
            false
        }
    }
}

/// Remove a provider's key from the vault. `true` when the entry is gone afterwards —
/// deleting a key that was never stored counts as success. Blocking — call off the UI thread.
pub fn delete(provider: Provider) -> bool {
    match Entry::new(SERVICE, provider.account()).and_then(|e| e.delete_credential()) {
        Ok(()) | Err(Error::NoEntry) => true,
        Err(e) => {
            tracing::error!("could not delete {} key from the vault: {e}", provider.account());
            false
        }
    }
}
