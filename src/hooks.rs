use std::{
    env,
    ffi::OsString,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
#[cfg(windows)]
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::{
    protocol::{HookEvent, HookEventKind, Request},
    transport,
};

const HOOK_FILE_NAME: &str = "grok-bridge.json";
const MANAGED_ENV_NAME: &str = "GROK_BRIDGE_MANAGED";
const MANAGED_ENV_VALUE: &str = "1";
const PROTOCOL_ENV_NAME: &str = "GROK_BRIDGE_HOOK_PROTOCOL";
const PROTOCOL_ENV_VALUE: &str = "3";
const PROVIDER_SESSION_ENV_NAME: &str = "GROK_SESSION_ID";
const MAX_STDIN_BYTES: usize = 256 * 1024;
const MAX_PROVIDER_SESSION_ID_BYTES: usize = 256;
const MAX_CWD_BYTES: usize = 4096;
const MAX_SHORT_TEXT_BYTES: usize = 128;
const MAX_MESSAGE_BYTES: usize = 1024;
const HOOK_TIMEOUT_SECONDS: u64 = 2;

const HOOK_EVENTS: [HookSpec; 9] = [
    HookSpec::lifecycle("SessionStart"),
    HookSpec::lifecycle("UserPromptSubmit"),
    HookSpec::lifecycle("Stop"),
    HookSpec::lifecycle("StopFailure"),
    HookSpec::lifecycle("SessionEnd"),
    HookSpec::tool("PreToolUse"),
    HookSpec::tool("PostToolUse"),
    HookSpec::tool("PostToolUseFailure"),
    HookSpec::lifecycle("Notification"),
];

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct HookInstallStatus {
    pub(crate) path: String,
    pub(crate) installed: bool,
    pub(crate) changed: bool,
    pub(crate) events_found: usize,
    pub(crate) events_expected: usize,
}

#[derive(Clone, Copy)]
struct HookSpec {
    name: &'static str,
    tool_event: bool,
}

impl HookSpec {
    const fn lifecycle(name: &'static str) -> Self {
        Self {
            name,
            tool_event: false,
        }
    }

    const fn tool(name: &'static str) -> Self {
        Self {
            name,
            tool_event: true,
        }
    }
}

pub(crate) fn run_ingress_fail_open() {
    let Ok(input) = read_limited(io::stdin().lock()) else {
        return;
    };
    if input.overflowed {
        return;
    }

    let provider_session_id = env::var(PROVIDER_SESSION_ENV_NAME).ok();
    let Some((provider_session_id, event)) =
        parse_event(&input.bytes, provider_session_id.as_deref())
    else {
        return;
    };

    let _ = transport::call(
        Request::HookEvent {
            provider_session_id,
            event,
        },
        false,
    );
}

pub(crate) fn install() -> Result<HookInstallStatus> {
    let path = hook_file_path()?;
    let command = current_hook_command()?;
    install_at(&path, &command)
}

pub(crate) fn status() -> Result<HookInstallStatus> {
    let path = hook_file_path()?;
    let command = current_hook_command()?;
    status_at(&path, &command)
}

pub(crate) fn uninstall() -> Result<HookInstallStatus> {
    let path = hook_file_path()?;
    let command = current_hook_command()?;
    uninstall_at(&path, &command)
}

struct LimitedInput {
    bytes: Vec<u8>,
    overflowed: bool,
}

fn read_limited(mut reader: impl Read) -> io::Result<LimitedInput> {
    let mut bytes = Vec::with_capacity(8192);
    let mut buffer = [0_u8; 8192];
    let mut overflowed = false;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let remaining = MAX_STDIN_BYTES.saturating_sub(bytes.len());
        let retained = read.min(remaining);
        bytes.extend_from_slice(&buffer[..retained]);
        overflowed |= retained != read;
    }
    Ok(LimitedInput { bytes, overflowed })
}

fn parse_event(
    input: &[u8],
    provider_session_from_env: Option<&str>,
) -> Option<(String, HookEvent)> {
    let value: Value = serde_json::from_slice(input).ok()?;
    let object = value.as_object()?;
    let event_name = string_field(object, "hookEventName", "hook_event_name")?;
    let kind = parse_event_kind(event_name)?;
    let provider_session_id = provider_session_from_env
        .and_then(|value| sanitize_text(value, MAX_PROVIDER_SESSION_ID_BYTES))
        .or_else(|| {
            string_field(object, "sessionId", "session_id")
                .and_then(|value| sanitize_text(value, MAX_PROVIDER_SESSION_ID_BYTES))
        })?
        .to_ascii_lowercase();

    Some((
        provider_session_id,
        HookEvent {
            kind,
            cwd: object
                .get("cwd")
                .and_then(Value::as_str)
                .and_then(|value| sanitize_text(value, MAX_CWD_BYTES)),
            tool_name: string_field(object, "toolName", "tool_name")
                .and_then(|value| sanitize_text(value, MAX_SHORT_TEXT_BYTES)),
            message: object
                .get("message")
                .and_then(Value::as_str)
                .and_then(|value| sanitize_text(value, MAX_MESSAGE_BYTES)),
            notification_type: string_field(object, "notificationType", "notification_type")
                .and_then(|value| sanitize_text(value, MAX_SHORT_TEXT_BYTES)),
            level: object
                .get("level")
                .and_then(Value::as_str)
                .and_then(|value| sanitize_text(value, MAX_SHORT_TEXT_BYTES)),
        },
    ))
}

fn string_field<'a>(
    object: &'a Map<String, Value>,
    camel_case: &str,
    snake_case: &str,
) -> Option<&'a str> {
    object
        .get(camel_case)
        .and_then(Value::as_str)
        .or_else(|| object.get(snake_case).and_then(Value::as_str))
}

fn parse_event_kind(value: &str) -> Option<HookEventKind> {
    match value {
        "SessionStart" | "session_start" => Some(HookEventKind::SessionStart),
        "UserPromptSubmit" | "user_prompt_submit" => Some(HookEventKind::UserPromptSubmit),
        "Stop" | "stop" => Some(HookEventKind::Stop),
        "StopFailure" | "stop_failure" => Some(HookEventKind::StopFailure),
        "SessionEnd" | "session_end" => Some(HookEventKind::SessionEnd),
        "PreToolUse" | "pre_tool_use" => Some(HookEventKind::PreToolUse),
        "PostToolUse" | "post_tool_use" => Some(HookEventKind::PostToolUse),
        "PostToolUseFailure" | "post_tool_use_failure" => Some(HookEventKind::PostToolUseFailure),
        "Notification" | "notification" => Some(HookEventKind::Notification),
        _ => None,
    }
}

fn sanitize_text(value: &str, max_bytes: usize) -> Option<String> {
    let mut sanitized = String::with_capacity(value.len().min(max_bytes));
    for character in value.chars().filter(|character| !character.is_control()) {
        if sanitized.len() + character.len_utf8() > max_bytes {
            break;
        }
        sanitized.push(character);
    }
    (!sanitized.is_empty()).then_some(sanitized)
}

fn hook_file_path() -> Result<PathBuf> {
    hook_file_path_from(
        env::var_os("GROK_HOME"),
        env::var_os("HOME"),
        env::var_os("USERPROFILE"),
    )
}

fn hook_file_path_from(
    grok_home: Option<OsString>,
    home: Option<OsString>,
    user_profile: Option<OsString>,
) -> Result<PathBuf> {
    let grok_home = non_empty(grok_home);
    let home = non_empty(home);
    let user_profile = non_empty(user_profile);
    let root = if let Some(grok_home) = grok_home {
        PathBuf::from(grok_home)
    } else {
        #[cfg(windows)]
        let home = user_profile.or(home);
        #[cfg(not(windows))]
        let home = home.or(user_profile);
        PathBuf::from(home.context("GROK_HOME, HOME, and USERPROFILE are not set")?).join(".grok")
    };
    Ok(root.join("hooks").join(HOOK_FILE_NAME))
}

fn non_empty(value: Option<OsString>) -> Option<OsString> {
    value.filter(|value| !value.is_empty())
}

fn current_hook_command() -> Result<String> {
    let executable = env::current_exe().context("failed to locate the current executable")?;
    if !executable.is_absolute() {
        bail!("current executable path is not absolute");
    }
    hook_command(&executable)
}

#[cfg(windows)]
fn hook_command(executable: &Path) -> Result<String> {
    let executable = executable
        .to_str()
        .context("current executable path is not valid UTF-8")?;
    if executable.chars().any(char::is_control) {
        bail!("current executable path cannot be represented in a hook command");
    }
    let shell_path = executable.replace('\\', "/");
    if shell_path.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'/' | b'~' | b'-')
    }) {
        return Ok(format!("{shell_path} __hook"));
    }

    let quoted = executable.replace('\'', "''");
    let script = format!(
        "if (Test-Path -LiteralPath '{quoted}' -PathType Leaf) {{ & '{quoted}' __hook; exit $LASTEXITCODE }}; [Console]::In.ReadToEnd() | Out-Null; exit 0"
    );
    let mut encoded = Vec::with_capacity(script.len() * 2);
    for unit in script.encode_utf16() {
        encoded.extend_from_slice(&unit.to_le_bytes());
    }
    let system_root = env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_owned());
    let powershell = format!(
        "{}/System32/WindowsPowerShell/v1.0/powershell.exe",
        system_root.replace('\\', "/").trim_end_matches('/')
    );
    if !powershell.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'/' | b'~' | b'-')
    }) {
        bail!("Windows PowerShell path cannot be represented in a hook command");
    }
    Ok(format!(
        "{powershell} -NoProfile -ExecutionPolicy Bypass -EncodedCommand {}",
        BASE64.encode(encoded)
    ))
}

#[cfg(not(windows))]
fn hook_command(executable: &Path) -> Result<String> {
    let executable = executable
        .to_str()
        .context("current executable path is not valid UTF-8")?;
    if executable.chars().any(char::is_control) {
        bail!("current executable path cannot be represented in a hook command");
    }
    let quoted = executable.replace('\'', r#"'"'"'"#);
    Ok(format!("'{quoted}' __hook"))
}

fn install_at(path: &Path, command: &str) -> Result<HookInstallStatus> {
    let mut document = load_document(path)?.unwrap_or_else(|| json!({}));
    validate_document(&document)?;
    let current = inspect_document(path, &document, command);
    if current.installed {
        return Ok(current);
    }

    let original = document.clone();
    remove_managed_hooks(&mut document)?;
    add_managed_hooks(&mut document, command)?;
    let changed = document != original;
    if changed {
        write_document(path, &document)?;
    }
    let mut result = inspect_document(path, &document, command);
    result.changed = changed;
    Ok(result)
}

fn status_at(path: &Path, command: &str) -> Result<HookInstallStatus> {
    let Some(document) = load_document(path)? else {
        return Ok(empty_status(path));
    };
    validate_document(&document)?;
    Ok(inspect_document(path, &document, command))
}

fn uninstall_at(path: &Path, command: &str) -> Result<HookInstallStatus> {
    let Some(mut document) = load_document(path)? else {
        return Ok(empty_status(path));
    };
    validate_document(&document)?;
    let changed = remove_managed_hooks(&mut document)? != 0;
    if changed {
        write_document(path, &document)?;
    }
    let mut result = inspect_document(path, &document, command);
    result.changed = changed;
    Ok(result)
}

fn empty_status(path: &Path) -> HookInstallStatus {
    HookInstallStatus {
        path: path.display().to_string(),
        installed: false,
        changed: false,
        events_found: 0,
        events_expected: HOOK_EVENTS.len(),
    }
}

fn load_document(path: &Path) -> Result<Option<Value>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", path.display()));
        }
    };
    let document = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(Some(document))
}

fn validate_document(document: &Value) -> Result<()> {
    let object = document
        .as_object()
        .context("Grok hook configuration root must be a JSON object")?;
    if object.get("hooks").is_some_and(|hooks| !hooks.is_object()) {
        bail!("Grok hook configuration 'hooks' must be a JSON object");
    }
    Ok(())
}

fn write_document(path: &Path, document: &Value) -> Result<()> {
    let parent = path
        .parent()
        .context("Grok hook configuration path has no parent directory")?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    let mut bytes = serde_json::to_vec_pretty(document)
        .context("failed to serialize Grok hook configuration")?;
    bytes.push(b'\n');
    fs::write(path, bytes).with_context(|| format!("failed to write {}", path.display()))
}

fn inspect_document(path: &Path, document: &Value, command: &str) -> HookInstallStatus {
    let hooks = document.get("hooks").and_then(Value::as_object);
    let managed_handlers = hooks.map_or(0, count_managed_handlers);
    let events_found = hooks.map_or(0, |hooks| {
        HOOK_EVENTS
            .iter()
            .filter(|spec| event_has_expected_hook(hooks, **spec, command))
            .count()
    });
    HookInstallStatus {
        path: path.display().to_string(),
        installed: events_found == HOOK_EVENTS.len() && managed_handlers == HOOK_EVENTS.len(),
        changed: false,
        events_found,
        events_expected: HOOK_EVENTS.len(),
    }
}

fn count_managed_handlers(hooks: &Map<String, Value>) -> usize {
    hooks
        .values()
        .filter_map(Value::as_array)
        .flatten()
        .filter_map(|group| group.get("hooks").and_then(Value::as_array))
        .flatten()
        .filter(|handler| is_managed_handler(handler))
        .count()
}

fn event_has_expected_hook(hooks: &Map<String, Value>, spec: HookSpec, command: &str) -> bool {
    hooks
        .get(spec.name)
        .and_then(Value::as_array)
        .is_some_and(|groups| {
            groups.iter().any(|group| {
                let Some(group) = group.as_object() else {
                    return false;
                };
                if spec.tool_event {
                    if group.get("matcher").and_then(Value::as_str) != Some(".*") {
                        return false;
                    }
                } else if group.contains_key("matcher") {
                    return false;
                }
                group
                    .get("hooks")
                    .and_then(Value::as_array)
                    .is_some_and(|handlers| {
                        handlers
                            .iter()
                            .any(|handler| is_expected_handler(handler, command))
                    })
            })
        })
}

fn is_managed_handler(handler: &Value) -> bool {
    handler
        .get("env")
        .and_then(Value::as_object)
        .and_then(|env| env.get(MANAGED_ENV_NAME))
        .and_then(Value::as_str)
        == Some(MANAGED_ENV_VALUE)
}

fn is_expected_handler(handler: &Value, command: &str) -> bool {
    is_managed_handler(handler)
        && handler
            .get("env")
            .and_then(Value::as_object)
            .and_then(|env| env.get(PROTOCOL_ENV_NAME))
            .and_then(Value::as_str)
            == Some(PROTOCOL_ENV_VALUE)
        && has_expected_runtime_identity(handler)
        && handler.get("type").and_then(Value::as_str) == Some("command")
        && handler.get("command").and_then(Value::as_str) == Some(command)
        && handler.get("timeout").and_then(Value::as_u64) == Some(HOOK_TIMEOUT_SECONDS)
}

#[cfg(windows)]
fn has_expected_runtime_identity(handler: &Value) -> bool {
    let Some(handler_env) = handler.get("env").and_then(Value::as_object) else {
        return false;
    };
    let username = env::var("USERNAME").unwrap_or_else(|_| "default".to_owned());
    let domain = env::var("USERDOMAIN").unwrap_or_default();
    handler_env.get("USERNAME").and_then(Value::as_str) == Some(username.as_str())
        && handler_env.get("USERDOMAIN").and_then(Value::as_str) == Some(domain.as_str())
}

#[cfg(not(windows))]
fn has_expected_runtime_identity(_: &Value) -> bool {
    true
}

fn remove_managed_hooks(document: &mut Value) -> Result<usize> {
    let Some(hooks) = document.get_mut("hooks") else {
        return Ok(0);
    };
    let hooks = hooks
        .as_object_mut()
        .context("Grok hook configuration 'hooks' must be a JSON object")?;
    let event_names = hooks.keys().cloned().collect::<Vec<_>>();
    let mut removed_total = 0;
    for event_name in event_names {
        let Some(groups) = hooks.get_mut(&event_name).and_then(Value::as_array_mut) else {
            continue;
        };
        let mut removed_from_event = 0;
        groups.retain_mut(|group| {
            let Some(handlers) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
                return true;
            };
            let before = handlers.len();
            handlers.retain(|handler| !is_managed_handler(handler));
            let removed = before - handlers.len();
            removed_from_event += removed;
            removed_total += removed;
            removed == 0 || !handlers.is_empty()
        });
        if removed_from_event != 0 && groups.is_empty() {
            hooks.remove(&event_name);
        }
    }
    Ok(removed_total)
}

fn add_managed_hooks(document: &mut Value, command: &str) -> Result<()> {
    let root = document
        .as_object_mut()
        .context("Grok hook configuration root must be a JSON object")?;
    if !root.contains_key("hooks") {
        root.insert("hooks".to_owned(), json!({}));
    }
    let hooks = root
        .get_mut("hooks")
        .and_then(Value::as_object_mut)
        .context("Grok hook configuration 'hooks' must be a JSON object")?;
    for spec in HOOK_EVENTS {
        let groups = hooks
            .entry(spec.name.to_owned())
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .with_context(|| format!("Grok hook event '{}' must be a JSON array", spec.name))?;
        let mut handler_env = Map::new();
        handler_env.insert(
            MANAGED_ENV_NAME.to_owned(),
            Value::String(MANAGED_ENV_VALUE.to_owned()),
        );
        handler_env.insert(
            PROTOCOL_ENV_NAME.to_owned(),
            Value::String(PROTOCOL_ENV_VALUE.to_owned()),
        );
        #[cfg(windows)]
        {
            handler_env.insert(
                "USERNAME".to_owned(),
                Value::String(env::var("USERNAME").unwrap_or_else(|_| "default".to_owned())),
            );
            handler_env.insert(
                "USERDOMAIN".to_owned(),
                Value::String(env::var("USERDOMAIN").unwrap_or_default()),
            );
        }
        let handler = json!({
            "type": "command",
            "command": command,
            "timeout": HOOK_TIMEOUT_SECONDS,
            "env": handler_env,
        });
        let group = if spec.tool_event {
            json!({
                "matcher": ".*",
                "hooks": [handler],
            })
        } else {
            json!({
                "hooks": [handler],
            })
        };
        groups.push(group);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        io::Cursor,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    static NEXT_TEMP_DIR: AtomicU64 = AtomicU64::new(1);

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let sequence = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("hook-tests")
                .join(format!(
                    "grok-bridge-hooks-test-{}-{timestamp}-{sequence}",
                    std::process::id()
                ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn hook_file(&self) -> PathBuf {
            self.0.join("hooks").join(HOOK_FILE_NAME)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn drains_oversized_input_without_retaining_more_than_the_limit() {
        let data = vec![b'x'; MAX_STDIN_BYTES + 137];
        let mut cursor = Cursor::new(data);
        let input = read_limited(&mut cursor).unwrap();
        assert_eq!(input.bytes.len(), MAX_STDIN_BYTES);
        assert!(input.overflowed);
        assert_eq!(cursor.position(), (MAX_STDIN_BYTES + 137) as u64);
    }

    #[test]
    fn parses_all_supported_event_names() {
        let cases = [
            ("SessionStart", HookEventKind::SessionStart),
            ("UserPromptSubmit", HookEventKind::UserPromptSubmit),
            ("Stop", HookEventKind::Stop),
            ("StopFailure", HookEventKind::StopFailure),
            ("SessionEnd", HookEventKind::SessionEnd),
            ("PreToolUse", HookEventKind::PreToolUse),
            ("PostToolUse", HookEventKind::PostToolUse),
            ("PostToolUseFailure", HookEventKind::PostToolUseFailure),
            ("Notification", HookEventKind::Notification),
        ];
        for (name, expected) in cases {
            let payload = serde_json::to_vec(&json!({
                "hookEventName": name,
                "sessionId": "019e37f4-5135-7b63-a4ab-6d13aa6bf528",
            }))
            .unwrap();
            assert_eq!(parse_event(&payload, None).unwrap().1.kind, expected);
        }
        assert!(parse_event(br#"{"hookEventName":"SubagentStart"}"#, None).is_none());
        assert!(parse_event(br#"{"hookEventName":"Stop"}"#, None).is_none());
    }

    #[test]
    fn accepts_snake_case_fields_and_prefers_provider_session_environment() {
        let payload = serde_json::to_vec(&json!({
            "hook_event_name": "notification",
            "session_id": "json-session",
            "cwd": "repo\nroot",
            "tool_name": "read\u{0}file",
            "message": "message\r\nbody",
            "notification_type": "permission\trequest",
            "level": "warn\n",
        }))
        .unwrap();
        let (provider_session_id, event) =
            parse_event(&payload, Some("019e37f4-5135-7b63-a4ab-6d13aa6bf528\n")).unwrap();
        assert_eq!(provider_session_id, "019e37f4-5135-7b63-a4ab-6d13aa6bf528");
        assert_eq!(event.cwd.as_deref(), Some("reporoot"));
        assert_eq!(event.tool_name.as_deref(), Some("readfile"));
        assert_eq!(event.message.as_deref(), Some("messagebody"));
        assert_eq!(
            event.notification_type.as_deref(),
            Some("permissionrequest")
        );
        assert_eq!(event.level.as_deref(), Some("warn"));
    }

    #[test]
    fn truncates_text_on_utf8_boundaries() {
        let sanitized = sanitize_text(&"中".repeat(100), MAX_SHORT_TEXT_BYTES).unwrap();
        assert!(sanitized.len() <= MAX_SHORT_TEXT_BYTES);
        assert!(sanitized.is_char_boundary(sanitized.len()));
        assert_eq!(sanitized.chars().count(), MAX_SHORT_TEXT_BYTES / 3);
    }

    #[test]
    fn resolves_grok_home_before_platform_home() {
        let path = hook_file_path_from(
            Some(OsString::from("custom-grok-home")),
            Some(OsString::from("home")),
            Some(OsString::from("profile")),
        )
        .unwrap();
        assert_eq!(
            path,
            PathBuf::from("custom-grok-home")
                .join("hooks")
                .join(HOOK_FILE_NAME)
        );
    }

    #[cfg(windows)]
    #[test]
    fn builds_fast_and_encoded_windows_hook_commands() {
        assert_eq!(
            hook_command(Path::new(r"C:\tools\grok-bridge.exe")).unwrap(),
            "C:/tools/grok-bridge.exe __hook"
        );

        let command =
            hook_command(Path::new(r"C:\Program Files\Grok Bridge\grok-bridge.exe")).unwrap();
        assert!(command.contains(" -NoProfile -ExecutionPolicy Bypass -EncodedCommand "));
        assert!(!command.contains("Program Files"));
        let encoded = command.split_whitespace().next_back().unwrap();
        let bytes = base64::Engine::decode(&BASE64, encoded).unwrap();
        let utf16 = bytes
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>();
        let script = String::from_utf16(&utf16).unwrap();
        assert!(script.contains("& 'C:\\Program Files\\Grok Bridge\\grok-bridge.exe' __hook"));
        assert!(script.contains("[Console]::In.ReadToEnd() | Out-Null"));
    }

    #[test]
    fn installation_preserves_foreign_hooks_and_is_idempotent() {
        let directory = TestDir::new();
        let path = directory.hook_file();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let foreign = json!({
            "metadata": { "owner": "user" },
            "hooks": {
                "PreToolUse": [{
                    "matcher": "Bash",
                    "hooks": [{
                        "type": "command",
                        "command": "foreign-command",
                        "timeout": 5,
                    }],
                }],
                "CustomEvent": [{
                    "hooks": [{
                        "type": "command",
                        "command": "custom-command",
                    }],
                }],
            },
        });
        write_document(&path, &foreign).unwrap();

        let first = install_at(&path, r#""C:\Program Files\grok-bridge.exe" __hook"#).unwrap();
        assert!(first.installed);
        assert!(first.changed);
        assert_eq!(first.events_found, HOOK_EVENTS.len());
        let first_bytes = fs::read(&path).unwrap();

        let second = install_at(&path, r#""C:\Program Files\grok-bridge.exe" __hook"#).unwrap();
        assert!(second.installed);
        assert!(!second.changed);
        assert_eq!(fs::read(&path).unwrap(), first_bytes);

        let document = load_document(&path).unwrap().unwrap();
        assert_eq!(document["metadata"]["owner"], "user");
        assert_eq!(
            document["hooks"]["CustomEvent"][0]["hooks"][0]["command"],
            "custom-command"
        );
        let pre_tool_groups = document["hooks"]["PreToolUse"].as_array().unwrap();
        assert!(
            pre_tool_groups
                .iter()
                .any(|group| group["matcher"] == "Bash")
        );
        assert!(pre_tool_groups.iter().any(|group| {
            group["matcher"] == ".*"
                && group["hooks"][0]["env"][MANAGED_ENV_NAME] == MANAGED_ENV_VALUE
                && group["hooks"][0]["env"][PROTOCOL_ENV_NAME] == PROTOCOL_ENV_VALUE
        }));
        #[cfg(windows)]
        {
            let managed = pre_tool_groups
                .iter()
                .find(|group| group["matcher"] == ".*")
                .unwrap();
            let username = env::var("USERNAME").unwrap_or_else(|_| "default".to_owned());
            let domain = env::var("USERDOMAIN").unwrap_or_default();
            assert_eq!(
                managed["hooks"][0]["env"]["USERNAME"].as_str(),
                Some(username.as_str())
            );
            assert_eq!(
                managed["hooks"][0]["env"]["USERDOMAIN"].as_str(),
                Some(domain.as_str())
            );
        }
    }

    #[test]
    fn uninstall_removes_only_managed_hooks_and_is_idempotent() {
        let directory = TestDir::new();
        let path = directory.hook_file();
        let command = "'/opt/grok bridge/grok-bridge' __hook";
        let foreign = json!({
            "hooks": {
                "Stop": [{
                    "hooks": [{ "type": "command", "command": "foreign" }],
                }],
                "OldEvent": [{
                    "hooks": [{
                        "type": "command",
                        "command": "old-managed-command",
                        "env": { MANAGED_ENV_NAME: MANAGED_ENV_VALUE },
                    }],
                }],
            },
        });
        write_document(&path, &foreign).unwrap();
        install_at(&path, command).unwrap();

        let first = uninstall_at(&path, command).unwrap();
        assert!(!first.installed);
        assert!(first.changed);
        assert_eq!(first.events_found, 0);
        let document = load_document(&path).unwrap().unwrap();
        assert_eq!(
            document["hooks"]["Stop"][0]["hooks"][0]["command"],
            "foreign"
        );
        assert!(document["hooks"].get("OldEvent").is_none());

        let second = uninstall_at(&path, command).unwrap();
        assert!(!second.installed);
        assert!(!second.changed);
    }
}
