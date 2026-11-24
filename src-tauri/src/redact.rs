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
use std::sync::{OnceLock, RwLock};

use regex::Regex;

/// One registered value, and the placeholder that stands for it.
struct Known {
    value: String,
    placeholder: &'static str,
}

/// The registered values, longest first so a nested path wins over the folder it sits in.
static KNOWN: RwLock<Vec<Known>> = RwLock::new(Vec::new());

/// The shortest value worth registering, below which a replacement would redact too much.
const MIN_KNOWN_LEN: usize = 4;

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

    let Ok(mut known) = KNOWN.write() else {
        return;
    };

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
pub fn line(text: &str) -> Cow<'_, str> {
    if !is_private(text) {
        return Cow::Borrowed(text);
    }

    let mut out = apply_known(text);
    out = profile_path()
        .replace_all(&out, "%USERPROFILE%")
        .into_owned();
    out = url_pattern()
        .replace_all(&out, |caps: &regex::Captures<'_>| url(&caps[0]).to_owned())
        .into_owned();
    out = email().replace_all(&out, "<email>").into_owned();

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

/// Whether `text` holds anything the patterns or the registry would change.
fn is_private(text: &str) -> bool {
    if let Ok(known) = KNOWN.read() {
        if known
            .iter()
            .any(|entry| text.contains(entry.value.as_str()))
        {
            return true;
        }
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

    let Ok(known) = KNOWN.read() else {
        return out;
    };

    for entry in known.iter() {
        if out.contains(entry.value.as_str()) {
            out = out.replace(entry.value.as_str(), entry.placeholder);
        }
    }

    out
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
