//! Configuration parsed from the plugin context at load time.
//!
//! The host strips the `plugin.<name>.` prefix before calling
//! [`mumble_plugin_api::PluginContext::get_config`], so we look up
//! *short* keys (`"greeting_template"`) rather than fully qualified ones.
//!
//! # INI example
//!
//! ```ini
//! plugin.fancy-greeter.enabled=true
//! plugin.fancy-greeter.greeting_template=Welcome, {username}!
//! plugin.fancy-greeter.farewell_template=Goodbye, {username}.
//! ```
//!
//! **Do not read `enabled` here.** The host reads it before calling
//! `on_load`; by the time our code runs, the plugin is confirmed enabled.

/// Default greeting sent to every new connection.
const DEFAULT_GREETING: &str = "Welcome to the server, {username}!";

/// Default farewell logged when a user disconnects.
const DEFAULT_FAREWELL: &str = "Goodbye, {username}.";

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
}

impl GreeterConfig {
    /// Read all configuration keys via the given lookup closure.
    ///
    /// `lookup` is typically a thin wrapper around
    /// [`mumble_plugin_api::PluginContext::get_config`], but tests pass
    /// a `HashMap`-backed stub.
    pub fn from_lookup<F>(lookup: F) -> Self
    where
        F: Fn(&str) -> Option<String>,
    {
        let greeting_template =
            lookup("greeting_template").unwrap_or_else(|| DEFAULT_GREETING.to_owned());

        let farewell_template =
            lookup("farewell_template").unwrap_or_else(|| DEFAULT_FAREWELL.to_owned());

        Self {
            greeting_template,
            farewell_template,
        }
    }
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
        let cfg = GreeterConfig::from_lookup(lookup_from(&[]));
        assert_eq!(cfg.greeting_template, DEFAULT_GREETING);
        assert_eq!(cfg.farewell_template, DEFAULT_FAREWELL);
    }

    #[test]
    fn custom_greeting_is_loaded() {
        let cfg =
            GreeterConfig::from_lookup(lookup_from(&[("greeting_template", "Hey {username}!")]));
        assert_eq!(cfg.greeting_template, "Hey {username}!");
    }

    #[test]
    fn expand_replaces_username() {
        assert_eq!(
            expand_template("Hello {username}!", "Alice"),
            "Hello Alice!"
        );
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
