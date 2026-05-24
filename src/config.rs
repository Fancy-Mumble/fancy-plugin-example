//! Configuration parsed from the plugin context at load time.
//!
//! The host strips the `plugin.<name>.` prefix before calling
//! [`mumble_plugin_api::PluginContext::get_config`], so we look up
//! *short* keys (`"http_port"`) rather than fully qualified ones.
//!
//! # INI example
//!
//! ```ini
//! plugin.fancy-greeter.enabled=true
//! plugin.fancy-greeter.greeting_template=Welcome, {username}!
//! plugin.fancy-greeter.farewell_template=Goodbye, {username}.
//! plugin.fancy-greeter.http_port=64741
//! ```
//!
//! **Do not read `enabled` here.** The host reads it before calling
//! `on_load`; by the time our code runs, the plugin is confirmed enabled.

use thiserror::Error;

/// Default greeting sent to every new connection.
const DEFAULT_GREETING: &str = "Welcome to the server, {username}!";

/// Default farewell logged when a user disconnects.
const DEFAULT_FAREWELL: &str = "Goodbye, {username}.";

/// Default TCP port for the HTTP status page.
const DEFAULT_HTTP_PORT: u16 = 64741;

/// All runtime configuration for the `fancy-greeter` plugin.
#[derive(Debug, Clone)]
pub struct GreeterConfig {
    /// Template used to build the welcome message.  `{username}` is
    /// replaced with the connecting user's display name.
    pub greeting_template: String,
    /// Template logged server-side when a user disconnects.  Users are
    /// already gone by the time `on_client_disconnected` fires, so this
    /// is only logged, not sent.
    pub farewell_template: String,
    /// TCP port for the HTTP status page (`GET /status`).
    pub http_port: u16,
}

/// Errors that can occur while parsing plugin configuration.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// The value for a known key could not be parsed as the expected type.
    #[error("invalid value for config key {key}: {value}")]
    Parse {
        /// Short key name (without the `plugin.fancy-greeter.` prefix).
        key: &'static str,
        /// Raw value string that failed to parse.
        value: String,
    },
}

impl GreeterConfig {
    /// Read all configuration keys via the given lookup closure.
    ///
    /// `lookup` is typically a thin wrapper around
    /// [`mumble_plugin_api::PluginContext::get_config`], but tests pass
    /// a `HashMap`-backed stub.
    pub fn from_lookup<F>(lookup: F) -> Result<Self, ConfigError>
    where
        F: Fn(&str) -> Option<String>,
    {
        let http_port = parse_optional::<u16>(&lookup, "http_port")?.unwrap_or(DEFAULT_HTTP_PORT);

        let greeting_template = lookup("greeting_template")
            .unwrap_or_else(|| DEFAULT_GREETING.to_owned());

        let farewell_template = lookup("farewell_template")
            .unwrap_or_else(|| DEFAULT_FAREWELL.to_owned());

        Ok(Self { greeting_template, farewell_template, http_port })
    }
}

fn parse_optional<T: std::str::FromStr>(
    lookup: &dyn Fn(&str) -> Option<String>,
    key: &'static str,
) -> Result<Option<T>, ConfigError> {
    let Some(raw) = lookup(key) else { return Ok(None) };
    raw.parse::<T>()
        .map(Some)
        .map_err(|_| ConfigError::Parse { key, value: raw })
}

/// Expand `{username}` in a template string.
///
/// # Example
///
/// ```
/// # use fancy_greeter::config::expand_template;
/// assert_eq!(expand_template("Hello {username}!", "Alice"), "Hello Alice!");
/// ```
pub fn expand_template(template: &str, username: &str) -> String {
    template.replace("{username}", username)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn lookup_from(pairs: &[(&'static str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |key: &str| map.get(key).cloned()
    }

    #[test]
    fn defaults_when_no_config() {
        let cfg = GreeterConfig::from_lookup(lookup_from(&[])).expect("loads defaults");
        assert_eq!(cfg.http_port, DEFAULT_HTTP_PORT);
        assert_eq!(cfg.greeting_template, DEFAULT_GREETING);
        assert_eq!(cfg.farewell_template, DEFAULT_FAREWELL);
    }

    #[test]
    fn custom_port_is_loaded() {
        let cfg = GreeterConfig::from_lookup(lookup_from(&[("http_port", "9999")]))
            .expect("loads");
        assert_eq!(cfg.http_port, 9999);
    }

    #[test]
    fn invalid_port_is_rejected() {
        let err = GreeterConfig::from_lookup(lookup_from(&[("http_port", "not-a-number")]));
        assert!(err.is_err());
    }

    #[test]
    fn custom_greeting_is_loaded() {
        let cfg = GreeterConfig::from_lookup(lookup_from(&[(
            "greeting_template",
            "Hey {username}!",
        )]))
        .expect("loads");
        assert_eq!(cfg.greeting_template, "Hey {username}!");
    }

    #[test]
    fn expand_replaces_username() {
        assert_eq!(expand_template("Hello {username}!", "Alice"), "Hello Alice!");
        assert_eq!(expand_template("No placeholder", "Bob"), "No placeholder");
    }

    #[test]
    fn expand_replaces_all_occurrences() {
        assert_eq!(
            expand_template("{username} and {username}", "Eve"),
            "Eve and Eve"
        );
    }
}
