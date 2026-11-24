use super::*;

#[test]
fn a_url_with_a_query_becomes_the_address_alone() {
    assert_eq!(
        url("https://example.com/a/b.fantome?token=secret#frag"),
        "https://example.com/a/b.fantome"
    );
    assert_eq!(
        url("vinciskin://install?url=https://x/y?token=1"),
        "vinciskin://install"
    );
}

#[test]
fn a_url_without_either_is_answered_as_it_stands() {
    assert_eq!(
        url("https://raw.githubusercontent.com/a/b/Annie.fantome"),
        "https://raw.githubusercontent.com/a/b/Annie.fantome"
    );
    assert_eq!(url(""), "");
    assert_eq!(url("not a url"), "not a url");
}

#[test]
fn a_registered_path_is_replaced_in_both_spellings() {
    register(r"C:\Tools\redact-showcase", "%SHOWCASE%");

    assert_eq!(
        text(r"reading C:\Tools\redact-showcase\mods\Annie.fantome"),
        r"reading %SHOWCASE%\mods\Annie.fantome"
    );
    assert_eq!(
        text(r#"failed to open "C:\\Tools\\redact-showcase\\mods""#),
        r#"failed to open "%SHOWCASE%\\mods""#
    );
}

#[test]
fn the_longer_registered_path_wins_over_the_folder_it_sits_in() {
    register(r"C:\Users\showcase", PROFILE);
    register(r"C:\Users\showcase\AppData\Roaming", "%APPDATA%");

    assert_eq!(
        text(r"C:\Users\showcase\AppData\Roaming\com.vinciskin.app"),
        r"%APPDATA%\com.vinciskin.app"
    );
}

#[test]
fn another_accounts_profile_path_is_replaced_by_shape() {
    assert_eq!(
        text(r"opened C:\Users\someone-else\Documents\settings.json"),
        r"opened %USERPROFILE%\Documents\settings.json"
    );
    assert_eq!(
        text(r#"opened "D:/Users/someone-else/x""#),
        r#"opened "%USERPROFILE%/x""#
    );
}

#[test]
fn a_link_loses_its_query_where_it_stands() {
    assert_eq!(
        text("deep link vinciskin://install?url=https://a/b?token=1 opened"),
        "deep link vinciskin://install opened"
    );
    assert_eq!(
        text("GET https://api.example.com/v1/mods?page=2 200"),
        "GET https://api.example.com/v1/mods 200"
    );
}

#[test]
fn an_address_is_replaced() {
    assert_eq!(
        text("signed in as someone@example.com"),
        "signed in as <email>"
    );
}

#[test]
fn a_line_with_nothing_private_is_answered_borrowed() {
    let line = "patcher: injected into League of Legends.exe";
    assert!(matches!(text(line), Cow::Borrowed(same) if same == line));
}

#[test]
fn a_value_too_short_to_be_private_is_ignored() {
    register("C:", "%DRIVE%");
    assert_eq!(text("drive C: is full"), "drive C: is full");
}

#[test]
fn each_rule_counts_what_it_replaced() {
    register(r"D:\Games\report-showcase", "%SHOWCASE%");

    let before = counts().get("%SHOWCASE%").copied().unwrap_or(0);

    /* Only the registered value is counted exactly, because the other rules are shared with
    every test running beside this one. */
    let _ = text(r"D:\Games\report-showcase\a.txt and D:\Games\report-showcase\b.txt");
    let _ = text("wrote to C:\\Users\\counted-elsewhere\\x and mailed someone@example.com");

    let after = counts();

    assert_eq!(after.get("%SHOWCASE%").copied().unwrap_or(0) - before, 2);
    assert!(after.get(PROFILE).copied().unwrap_or(0) > 0);
    assert!(after.get("<email>").copied().unwrap_or(0) > 0);
}
