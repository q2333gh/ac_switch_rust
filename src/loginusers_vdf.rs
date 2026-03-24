use crate::error::Result;
use crate::steam::{format_timestamp, AccountSelector, StartMode, SteamAccount};
use anyhow::{anyhow, bail, Context};

#[derive(Clone, Debug)]
pub struct LoginUsersVdf {
    root: VdfObject,
}

#[derive(Clone, Debug)]
struct VdfObject {
    entries: Vec<VdfEntry>,
}

#[derive(Clone, Debug)]
struct VdfEntry {
    key: String,
    value: VdfValue,
}

#[derive(Clone, Debug)]
enum VdfValue {
    String(String),
    Object(VdfObject),
}

impl LoginUsersVdf {
    pub fn parse(input: &str) -> Result<Self> {
        let stripped = input.strip_prefix('\u{feff}').unwrap_or(input);
        let mut parser = Parser::new(stripped);
        let root = parser.parse_root()?;
        parser.skip_ws();
        if !parser.is_eof() {
            bail!("Unexpected trailing content in loginusers.vdf");
        }
        Ok(Self { root })
    }

    pub fn render(&self) -> String {
        let mut output = String::new();
        render_object(&self.root, 0, &mut output);
        output
    }

    pub fn accounts(&self, auto_login_user: Option<&str>) -> Result<Vec<SteamAccount>> {
        let users = self.users_object()?;
        let mut accounts = Vec::new();
        for entry in &users.entries {
            if let Some(account) = account_from_entry(entry, auto_login_user)? {
                accounts.push(account);
            }
        }
        Ok(accounts)
    }

    pub fn set_active_account(
        &mut self,
        selector: &AccountSelector,
        mode: StartMode,
        auto_login_user: &str,
    ) -> Result<SteamAccount> {
        let target = self
            .accounts(None)?
            .into_iter()
            .find(|account| account.matches(selector))
            .ok_or_else(|| anyhow!("Target account was not found in loginusers.vdf"))?;

        let target_key = target.steam_id64.to_string();
        let users = self.users_object_mut()?;
        for entry in &mut users.entries {
            let VdfValue::Object(user) = &mut entry.value else {
                continue;
            };

            if entry.key == target_key {
                upsert_string(user, "MostRecent", "1");
                match mode {
                    StartMode::Express => {
                        upsert_string(user, "WantsOfflineMode", "0");
                    }
                    StartMode::Offline => {
                        upsert_string(user, "WantsOfflineMode", "1");
                        upsert_string(user, "SkipOfflineModeWarning", "1");
                    }
                }
            } else {
                upsert_string(user, "MostRecent", "0");
            }
        }

        let mut updated = target;
        updated.most_recent = true;
        updated.wants_offline_mode = mode == StartMode::Offline;
        updated.is_auto_login_user = updated.account_name == auto_login_user;
        Ok(updated)
    }

    fn users_object(&self) -> Result<&VdfObject> {
        let entry = self
            .root
            .entries
            .iter()
            .find(|entry| entry.key.eq_ignore_ascii_case("users"))
            .context("Top-level \"users\" object is missing from loginusers.vdf")?;

        match &entry.value {
            VdfValue::Object(value) => Ok(value),
            VdfValue::String(_) => bail!("Top-level \"users\" entry must be an object"),
        }
    }

    fn users_object_mut(&mut self) -> Result<&mut VdfObject> {
        let entry = self
            .root
            .entries
            .iter_mut()
            .find(|entry| entry.key.eq_ignore_ascii_case("users"))
            .context("Top-level \"users\" object is missing from loginusers.vdf")?;

        match &mut entry.value {
            VdfValue::Object(value) => Ok(value),
            VdfValue::String(_) => bail!("Top-level \"users\" entry must be an object"),
        }
    }
}

fn account_from_entry(entry: &VdfEntry, auto_login_user: Option<&str>) -> Result<Option<SteamAccount>> {
    let VdfValue::Object(user) = &entry.value else {
        return Ok(None);
    };

    let steam_id64 = match entry.key.parse::<u64>() {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };

    let account_name = get_string(user, "AccountName").unwrap_or_default();
    let persona_name = get_string(user, "PersonaName").unwrap_or_default();
    let remember_password = get_bool(user, "RememberPassword");
    let most_recent = get_bool(user, "MostRecent");
    let wants_offline_mode = get_bool(user, "WantsOfflineMode");
    let timestamp = get_i64(user, "Timestamp").unwrap_or_default();

    Ok(Some(SteamAccount {
        steam_id64,
        account_name: account_name.clone(),
        persona_name,
        remember_password,
        most_recent,
        wants_offline_mode,
        last_login_timestamp: timestamp,
        last_login_time: format_timestamp(timestamp),
        is_auto_login_user: auto_login_user
            .map(|value| !account_name.is_empty() && value == account_name)
            .unwrap_or(false),
    }))
}

fn get_string(object: &VdfObject, key: &str) -> Option<String> {
    object
        .entries
        .iter()
        .find(|entry| entry.key.eq_ignore_ascii_case(key))
        .and_then(|entry| match &entry.value {
            VdfValue::String(value) => Some(value.clone()),
            VdfValue::Object(_) => None,
        })
}

fn get_bool(object: &VdfObject, key: &str) -> bool {
    get_string(object, key)
        .as_deref()
        .map(parse_bool)
        .unwrap_or(false)
}

fn get_i64(object: &VdfObject, key: &str) -> Option<i64> {
    get_string(object, key).and_then(|value| value.parse::<i64>().ok())
}

fn parse_bool(value: &str) -> bool {
    matches!(value.trim(), "1" | "true" | "True" | "TRUE")
}

fn upsert_string(object: &mut VdfObject, key: &str, value: &str) {
    if let Some(entry) = object
        .entries
        .iter_mut()
        .find(|entry| entry.key.eq_ignore_ascii_case(key))
    {
        entry.value = VdfValue::String(value.to_owned());
        return;
    }

    object.entries.push(VdfEntry {
        key: key.to_owned(),
        value: VdfValue::String(value.to_owned()),
    });
}

fn render_object(object: &VdfObject, depth: usize, output: &mut String) {
    for entry in &object.entries {
        let indent = "\t".repeat(depth);
        output.push_str(&indent);
        output.push('"');
        output.push_str(&escape_string(&entry.key));
        output.push('"');

        match &entry.value {
            VdfValue::String(value) => {
                output.push_str("\t\t\"");
                output.push_str(&escape_string(value));
                output.push_str("\"\n");
            }
            VdfValue::Object(child) => {
                output.push('\n');
                output.push_str(&indent);
                output.push_str("{\n");
                render_object(child, depth + 1, output);
                output.push_str(&indent);
                output.push_str("}\n");
            }
        }
    }
}

fn escape_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

struct Parser<'a> {
    input: &'a [u8],
    position: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
            position: 0,
        }
    }

    fn parse_root(&mut self) -> Result<VdfObject> {
        let mut entries = Vec::new();
        self.skip_ws();
        while !self.is_eof() {
            entries.push(self.parse_entry()?);
            self.skip_ws();
        }
        Ok(VdfObject { entries })
    }

    fn parse_object(&mut self) -> Result<VdfObject> {
        self.expect(b'{')?;
        let mut entries = Vec::new();
        loop {
            self.skip_ws();
            if self.peek() == Some(b'}') {
                self.position += 1;
                break;
            }
            entries.push(self.parse_entry()?);
        }
        Ok(VdfObject { entries })
    }

    fn parse_entry(&mut self) -> Result<VdfEntry> {
        let key = self.parse_string()?;
        self.skip_ws();
        let value = if self.peek() == Some(b'{') {
            VdfValue::Object(self.parse_object()?)
        } else {
            VdfValue::String(self.parse_string()?)
        };
        Ok(VdfEntry { key, value })
    }

    fn parse_string(&mut self) -> Result<String> {
        self.skip_ws();
        self.expect(b'"')?;
        let mut output = String::new();
        while let Some(current) = self.peek() {
            self.position += 1;
            match current {
                b'\\' => {
                    let escaped = self
                        .peek()
                        .context("Unexpected end of file while parsing escaped VDF string")?;
                    self.position += 1;
                    output.push(match escaped {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        other => other as char,
                    });
                }
                b'"' => return Ok(output),
                other => output.push(other as char),
            }
        }

        bail!("Unexpected end of file while parsing VDF string")
    }

    fn skip_ws(&mut self) {
        while let Some(current) = self.peek() {
            if current.is_ascii_whitespace() {
                self.position += 1;
                continue;
            }

            if current == b'/' && self.peek_next() == Some(b'/') {
                self.position += 2;
                while let Some(ch) = self.peek() {
                    self.position += 1;
                    if ch == b'\n' {
                        break;
                    }
                }
                continue;
            }

            break;
        }
    }

    fn expect(&mut self, expected: u8) -> Result<()> {
        self.skip_ws();
        match self.peek() {
            Some(value) if value == expected => {
                self.position += 1;
                Ok(())
            }
            Some(value) => bail!("Expected byte {:?}, found {:?}", expected as char, value as char),
            None => bail!("Unexpected end of file"),
        }
    }

    fn is_eof(&self) -> bool {
        self.position >= self.input.len()
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.position).copied()
    }

    fn peek_next(&self) -> Option<u8> {
        self.input.get(self.position + 1).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MULTI_USER: &str = include_str!("../tests/fixtures/multi_user.vdf");
    const OFFLINE_USER: &str = include_str!("../tests/fixtures/offline_user.vdf");

    #[test]
    fn parses_mixed_case_loginusers() {
        let vdf = LoginUsersVdf::parse(MULTI_USER).expect("fixture should parse");
        let accounts = vdf.accounts(Some("alpha_user")).expect("accounts should parse");

        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0].steam_id64, 76561198000000001);
        assert_eq!(accounts[0].account_name, "alpha_user");
        assert!(accounts[0].remember_password);
        assert!(accounts[0].most_recent);
        assert!(!accounts[0].wants_offline_mode);
        assert!(accounts[0].is_auto_login_user);

        assert_eq!(accounts[1].steam_id64, 76561198000000002);
        assert_eq!(accounts[1].account_name, "beta_user");
        assert!(!accounts[1].remember_password);
    }

    #[test]
    fn rewrites_only_target_fields_and_preserves_unknown_values() {
        let mut vdf = LoginUsersVdf::parse(MULTI_USER).expect("fixture should parse");
        let selected = vdf
            .set_active_account(
                &AccountSelector::AccountName("beta_user".to_owned()),
                StartMode::Offline,
                "beta_user",
            )
            .expect("account should be updated");

        let rendered = vdf.render();
        assert_eq!(selected.account_name, "beta_user");
        assert!(selected.most_recent);
        assert!(selected.wants_offline_mode);
        assert!(rendered.contains("\"CustomValue\"\t\t\"keep-me\""));
        assert!(rendered.contains("\"mostrecent\"\t\t\"0\""));
        assert!(rendered.contains("\"MostRecent\"\t\t\"1\""));
        assert!(rendered.contains("\"WantsOfflineMode\"\t\t\"1\""));
        assert!(rendered.contains("\"SkipOfflineModeWarning\"\t\t\"1\""));
    }

    #[test]
    fn parses_utf8_bom_fixture() {
        let vdf = LoginUsersVdf::parse(OFFLINE_USER).expect("bom fixture should parse");
        let accounts = vdf.accounts(Some("gamma_user")).expect("accounts should parse");

        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].account_name, "gamma_user");
        assert!(accounts[0].most_recent);
        assert!(accounts[0].wants_offline_mode);
        assert!(accounts[0].is_auto_login_user);
    }
}
