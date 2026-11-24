//! Taking a private value out of text before the text reaches a log or a report.
//!
//! This is not a security boundary and does not pretend to be one. What it is is what keeps the
//! files a user sends with a bug report - a log kept for days, a report read by a maintainer -
//! from naming their account, their folders and the one-time code in a shared link.
//!
//! The values the app knows about are registered at startup and whenever settings change. The
//! patterns catch what the registry does not: another account's profile path, an address, a URL's
//! query. Both run at the writer, so every line is covered and no call site has to remember.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::{LazyLock, OnceLock};

use parking_lot::{Mutex, RwLock};
use regex::Regex;

/// One registered value, and the placeholder that stands for it.
struct Known {
    value: String,
    placeholder: &'static str,
}

/// The registered values, longest first so a nested path wins over the folder it sits in.
static KNOWN: RwLock<Vec<Known>> = RwLock::new(Vec::new());

/// What each rule has replaced since the process started, as `manifest.json` reports it.
static COUNTS: LazyLock<Mutex<BTreeMap<String, u64>>> = LazyLock::new(Default::default);

/// The shortest value worth registering, below which a replacement would redact too much.
const MIN_KNOWN_LEN: usize = 4;

/// The placeholder the profile-path pattern writes.
const PROFILE: &str = "%USERPROFILE%";

/// The rule name a stripped URL query is counted under.
const URL_QUERY: &str = "url-query";

/// Register `value` to be written as `placeholder` wherever it appears.
///
/// Both spellings are registered, because a path logged through `{:?}` carries doubled
/// separators. Registering the same value twice, as a settings save does, replaces the entry
/// rather than adding a second one.
pub fn register(value: &str, placeholder: &'static str) {
    let value = value.trim_end_matches(['\\', '/']);
    if value.len() < MIN_KNOWN_LEN {
        return;
    }

    let escaped = value.replace('\\', "\\\\");

    let mut known = KNOWN.write();

    for spelling in [value.to_owned(), escaped] {
        known.retain(|entry| entry.value != spelling);
        known.push(Known {
            value: spelling,
            placeholder,
        });
    }

    known.sort_by(|a, b| b.value.len().cmp(&a.value.len()));
}

/// Register a path to be written as `placeholder`.
pub fn register_path(path: &std::path::Path, placeholder: &'static str) {
    register(&path.to_string_lossy(), placeholder);
}

/// Register the folders a settings document names, each under its own placeholder.
pub fn register_settings(
    league: Option<&std::path::Path>,
    storage: Option<&std::path::Path>,
    workshop: Option<&std::path::Path>,
) {
    for (path, placeholder) in [
        (league, "%LEAGUE%"),
        (storage, "%STORAGE%"),
        (workshop, "%WORKSHOP%"),
    ] {
        if let Some(path) = path {
            register_path(path, placeholder);
        }
    }
}

/// `text` with every registered value and every known pattern replaced.
#[must_use]
pub fn text(text: &str) -> Cow<'_, str> {
    if !is_private(text) {
        return Cow::Borrowed(text);
    }

    let mut out = apply_known(text);
    out = apply_profile_paths(out);
    out = apply_urls(out);
    out = apply_email(out);

    Cow::Owned(out)
}

/// `url` with its query and fragment removed, keeping `scheme://host/path`.
///
/// A value that is not a URL is answered as it stands: the whole point is that this can be
/// applied at a log site without knowing what arrived.
#[must_use]
pub fn url(url: &str) -> &str {
    let end = url.find(['?', '#']).unwrap_or(url.len());
    &url[..end]
}

/// How many replacements each rule has made since the process started.
#[must_use]
pub fn counts() -> BTreeMap<String, u64> {
    COUNTS.lock().clone()
}

/// Add `times` to the tally of `rule`.
fn count(rule: &str, times: u64) {
    if times == 0 {
        return;
    }

    *COUNTS.lock().entry(rule.to_owned()).or_default() += times;
}

/// Whether `text` holds anything the patterns or the registry would change.
fn is_private(text: &str) -> bool {
    if KNOWN
        .read()
        .iter()
        .any(|entry| text.contains(entry.value.as_str()))
    {
        return true;
    }

    if profile_path().is_match(text) || email().is_match(text) {
        return true;
    }

    url_pattern()
        .find(text)
        .is_some_and(|found| url(found.as_str()).len() < found.as_str().len())
}

/// `text` with every registered value replaced.
fn apply_known(text: &str) -> String {
    let mut out = text.to_owned();

    let known = KNOWN.read();

    for entry in known.iter() {
        let hits = out.matches(entry.value.as_str()).count() as u64;
        if hits > 0 {
            out = out.replace(entry.value.as_str(), entry.placeholder);
            count(entry.placeholder, hits);
        }
    }

    out
}

/// `text` with every profile path replaced by its placeholder.
fn apply_profile_paths(text: String) -> String {
    let hits = profile_path().find_iter(&text).count() as u64;
    if hits == 0 {
        return text;
    }

    count(PROFILE, hits);
    profile_path().replace_all(&text, PROFILE).into_owned()
}

/// `text` with every URL's query and fragment removed.
fn apply_urls(text: String) -> String {
    let mut stripped = 0u64;

    let out = url_pattern().replace_all(&text, |captures: &regex::Captures<'_>| {
        let address = url(&captures[0]);
        if address.len() < captures[0].len() {
            stripped += 1;
        }

        address.to_owned()
    });

    count(URL_QUERY, stripped);
    out.into_owned()
}

/// `text` with every address replaced.
fn apply_email(text: String) -> String {
    let hits = email().find_iter(&text).count() as u64;
    if hits == 0 {
        return text;
    }

    count("<email>", hits);
    email().replace_all(&text, "<email>").into_owned()
}

/// Every `C:\Users\<name>` and `C:\\Users\\<name>`, whichever account it names.
fn profile_path() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();

    RE.get_or_init(|| {
        Regex::new(r#"(?i)(?:[a-z]:|\\\\\?\\[a-z]:)[\\/]{1,2}users[\\/]{1,2}[^\\/\s"'<>;,]+"#)
            .unwrap()
    })
}

/// Every `http(s)://` and `vinciskin://` address, which the writer strips the query off.
fn url_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();

    RE.get_or_init(|| Regex::new(r#"(?i)\b(?:https?|vinciskin)://[^\s"'<>]+"#).unwrap())
}

/// Every address in text, as a mail client would recognise one.
fn email() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();

    RE.get_or_init(|| Regex::new(r"(?i)\b[a-z0-9._%+\-]+@[a-z0-9.\-]+\.[a-z]{2,}\b").unwrap())
}

#[cfg(test)]
mod tests;
