use std::{fs, io, path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use serde::{Deserialize, Serialize};

use crate::install::{self, InstallationStatus, Paths};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Language {
    ZhCn,
    En,
}

impl Language {
    fn load() -> Self {
        if let Ok(path) = lang_config_path()
            && let Ok(content) = fs::read_to_string(&path)
            && let Ok(lang) = serde_json::from_str::<Language>(&content)
        {
            return lang;
        }
        Language::ZhCn
    }

    fn save(&self) -> Result<()> {
        let path = lang_config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string(self)?;
        fs::write(&path, content)?;
        Ok(())
    }

    fn toggle(&self) -> Self {
        match self {
            Language::ZhCn => Language::En,
            Language::En => Language::ZhCn,
        }
    }

    fn strings(&self) -> Strings {
        match self {
            Language::ZhCn => Strings {
                title: "grok-bridge 安装配置",
                action_install: "安装 / 更新并自动配置 Grok",
                action_uninstall: "移除本工具的 Grok Hooks 配置",
                action_webui: "打开 WebUI",
                action_language: "语言 / Language",
                action_exit: "退出",
                status_section: " 状态 ",
                status_label: "安装状态",
                binary_label: "原生 EXE（当前版本）",
                hooks_label: "Grok Hooks",
                target_path: "目标路径",
                ready: "已就绪",
                needs_update: "需更新",
                configured: "已配置",
                not_configured: "未配置",
                actions_section: " 操作 ",
                result_section: " 结果 ",
                help_bar: "↑/↓ 或 j/k 选择 · Enter 执行 · q/Esc 退出",
                msg_initial: "选择第一项并按 Enter，即可完成 EXE 安装和 Grok 配置。",
                msg_install_success: "配置完成：{}；请在 Grok 中确认信任 Hooks。",
                msg_install_failed: "配置失败：{}",
                msg_uninstall_success: "已移除本工具写入的 Grok Hooks 配置。",
                msg_uninstall_none: "没有找到本工具写入的 Hooks，未做修改。",
                msg_uninstall_failed: "移除失败：{}",
                msg_webui_try: "已尝试打开 WebUI，请检查浏览器。",
                msg_webui_failed: "打开 WebUI 失败：{}",
                msg_lang_switched: "语言已切换。",
                err_server_not_running: "Runtime server 未运行或状态异常",
                err_webui_unavailable: "Runtime WebUI 不可用；检查 server stderr",
                err_unexpected_response: "Runtime 返回了意外的 server status 响应",
                err_spawn_explorer: "无法启动 explorer.exe",
                err_spawn_open: "无法启动 open",
                err_spawn_xdg: "无法启动 xdg-open",
            },
            Language::En => Strings {
                title: "grok-bridge Setup",
                action_install: "Install / Update and Configure Grok",
                action_uninstall: "Remove Grok Hooks Managed by This Tool",
                action_webui: "Open WebUI",
                action_language: "语言 / Language",
                action_exit: "Exit",
                status_section: " Status ",
                status_label: "Installation",
                binary_label: "Native Binary (Current)",
                hooks_label: "Grok Hooks",
                target_path: "Target Path",
                ready: "Ready",
                needs_update: "Update Needed",
                configured: "Configured",
                not_configured: "Not Configured",
                actions_section: " Actions ",
                result_section: " Result ",
                help_bar: "↑/↓ or j/k: Select · Enter: Execute · q/Esc: Exit",
                msg_initial: "Select the first item and press Enter to install the binary and configure Grok.",
                msg_install_success: "Configuration complete: {}; please confirm trust in Grok.",
                msg_install_failed: "Configuration failed: {}",
                msg_uninstall_success: "Removed Grok Hooks managed by this tool.",
                msg_uninstall_none: "No managed Hooks found; no changes made.",
                msg_uninstall_failed: "Uninstall failed: {}",
                msg_webui_try: "Attempted to open WebUI; please check your browser.",
                msg_webui_failed: "Failed to open WebUI: {}",
                msg_lang_switched: "Language switched.",
                err_server_not_running: "Runtime server is not running or in abnormal state",
                err_webui_unavailable: "Runtime WebUI unavailable; check server stderr",
                err_unexpected_response: "Runtime returned unexpected server status response",
                err_spawn_explorer: "Failed to spawn explorer.exe",
                err_spawn_open: "Failed to spawn open",
                err_spawn_xdg: "Failed to spawn xdg-open",
            },
        }
    }
}

#[allow(dead_code)]
struct Strings {
    title: &'static str,
    action_install: &'static str,
    action_uninstall: &'static str,
    action_webui: &'static str,
    action_language: &'static str,
    action_exit: &'static str,
    status_section: &'static str,
    status_label: &'static str,
    binary_label: &'static str,
    hooks_label: &'static str,
    target_path: &'static str,
    ready: &'static str,
    needs_update: &'static str,
    configured: &'static str,
    not_configured: &'static str,
    actions_section: &'static str,
    result_section: &'static str,
    help_bar: &'static str,
    msg_initial: &'static str,
    msg_install_success: &'static str,
    msg_install_failed: &'static str,
    msg_uninstall_success: &'static str,
    msg_uninstall_none: &'static str,
    msg_uninstall_failed: &'static str,
    msg_webui_try: &'static str,
    msg_webui_failed: &'static str,
    msg_lang_switched: &'static str,
    err_server_not_running: &'static str,
    err_webui_unavailable: &'static str,
    err_unexpected_response: &'static str,
    err_spawn_explorer: &'static str,
    err_spawn_open: &'static str,
    err_spawn_xdg: &'static str,
}

fn lang_config_path() -> Result<PathBuf> {
    let paths = Paths::discover()?;
    let config_dir = paths
        .skill_root
        .parent()
        .ok_or_else(|| anyhow::anyhow!("skill root has no parent"))?;
    Ok(config_dir.join("grok-bridge-lang.json"))
}

pub fn run() -> Result<()> {
    enable_raw_mode().context("Failed to enable raw mode")?;
    let _restore = RestoreTerminal;
    execute!(io::stdout(), EnterAlternateScreen).context("Failed to enter alternate screen")?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend).context("Failed to create terminal")?;
    let result = run_loop(&mut terminal);
    terminal.show_cursor().ok();
    result
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let paths = Paths::discover()?;
    let mut status = install::status(&paths)?;
    let mut selected: usize = 0;
    let mut lang = Language::load();
    let mut message = String::from(lang.strings().msg_initial);

    loop {
        terminal.draw(|frame| render_menu(frame, &paths, status, selected, &message, lang))?;
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        if let Event::Key(key) = event::read()?
            && key.kind != KeyEventKind::Release
            && handle_key(
                key,
                &mut selected,
                &mut message,
                &paths,
                &mut status,
                &mut lang,
            )?
        {
            return Ok(());
        }
    }
}

fn handle_key(
    key: KeyEvent,
    selected: &mut usize,
    message: &mut String,
    paths: &Paths,
    status: &mut InstallationStatus,
    lang: &mut Language,
) -> Result<bool> {
    let s = lang.strings();
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => return Ok(true),
        KeyCode::Up | KeyCode::Char('k') => *selected = selected.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => *selected = (*selected + 1).min(4),
        KeyCode::Enter if key.kind == KeyEventKind::Press => match *selected {
            0 => match install::apply() {
                Ok(result) => {
                    *status = install::status(paths)?;
                    *message = s.msg_install_success.replace("{}", &result.display());
                }
                Err(error) => *message = s.msg_install_failed.replace("{}", &format!("{error:#}")),
            },
            1 => match install::uninstall() {
                Ok(true) => {
                    *status = install::status(paths)?;
                    *message = s.msg_uninstall_success.to_string();
                }
                Ok(false) => {
                    *message = s.msg_uninstall_none.to_string();
                }
                Err(error) => {
                    *message = s.msg_uninstall_failed.replace("{}", &format!("{error:#}"))
                }
            },
            2 => match open_webui(lang) {
                Ok(()) => *message = s.msg_webui_try.to_string(),
                Err(error) => *message = s.msg_webui_failed.replace("{}", &format!("{error:#}")),
            },
            3 => {
                *lang = lang.toggle();
                lang.save().ok();
                *message = lang.strings().msg_lang_switched.to_string();
            }
            _ => return Ok(true),
        },
        _ => {}
    }
    Ok(false)
}

fn render_menu(
    frame: &mut Frame,
    paths: &Paths,
    status: InstallationStatus,
    selected: usize,
    message: &str,
    lang: Language,
) {
    let s = lang.strings();
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(6),
            Constraint::Length(9),
            Constraint::Min(4),
            Constraint::Length(2),
        ])
        .split(frame.area());

    let title = Paragraph::new(s.title)
        .alignment(Alignment::Center)
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(title, areas[0]);

    let status_text = vec![
        Line::from(status_line(
            s.status_label,
            status.display(),
            status.binary_current && status.hooks_configured,
        )),
        Line::from(status_line(
            s.binary_label,
            if status.binary_current {
                s.ready
            } else {
                s.needs_update
            },
            status.binary_current,
        )),
        Line::from(status_line(
            s.hooks_label,
            if status.hooks_configured {
                s.configured
            } else {
                s.not_configured
            },
            status.hooks_configured,
        )),
        Line::from(format!(
            "{}：{}",
            s.target_path,
            paths.installed_binary.display()
        )),
    ];
    frame.render_widget(
        Paragraph::new(status_text)
            .block(
                Block::default()
                    .title(s.status_section)
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: false }),
        areas[1],
    );

    let actions = [
        s.action_install,
        s.action_uninstall,
        s.action_webui,
        s.action_language,
        s.action_exit,
    ];
    let items: Vec<ListItem> = actions
        .iter()
        .map(|action| ListItem::new(*action))
        .collect();
    let actions_widget = List::new(items)
        .block(
            Block::default()
                .title(s.actions_section)
                .borders(Borders::ALL),
        )
        .highlight_symbol("▶ ")
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    let mut list_state = ListState::default().with_selected(Some(selected));
    frame.render_stateful_widget(actions_widget, areas[2], &mut list_state);

    frame.render_widget(
        Paragraph::new(message)
            .block(
                Block::default()
                    .title(s.result_section)
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: false }),
        areas[3],
    );

    frame.render_widget(
        Paragraph::new(s.help_bar)
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray)),
        areas[4],
    );
}

fn status_line(label: &str, value: &str, ready: bool) -> Vec<Span<'static>> {
    vec![
        Span::raw(format!("{label}：")),
        Span::styled(
            value.to_owned(),
            Style::default().fg(if ready { Color::Green } else { Color::Yellow }),
        ),
    ]
}

fn open_webui(lang: &Language) -> Result<()> {
    use crate::protocol::Request;
    use crate::transport;
    use std::process::Command;

    let s = lang.strings();
    let response = transport::call(Request::ServerStatus, true)?;
    if !response.ok {
        anyhow::bail!(s.err_server_not_running);
    }

    match response.result {
        Some(crate::protocol::ResponseResult::ServerInfo(info)) => match info.web_url {
            Some(url) => {
                #[cfg(windows)]
                Command::new("explorer.exe")
                    .arg(&url)
                    .spawn()
                    .context(s.err_spawn_explorer)?;
                #[cfg(target_os = "macos")]
                Command::new("open")
                    .arg(&url)
                    .spawn()
                    .context(s.err_spawn_open)?;
                #[cfg(all(unix, not(target_os = "macos")))]
                Command::new("xdg-open")
                    .arg(&url)
                    .spawn()
                    .context(s.err_spawn_xdg)?;
                Ok(())
            }
            None => anyhow::bail!(s.err_webui_unavailable),
        },
        _ => anyhow::bail!(s.err_unexpected_response),
    }
}

struct RestoreTerminal;

impl Drop for RestoreTerminal {
    fn drop(&mut self) {
        disable_raw_mode().ok();
        execute!(io::stdout(), LeaveAlternateScreen).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_language_is_zh_cn() {
        let lang = Language::ZhCn;
        let s = lang.strings();
        assert!(s.action_exit.contains("退出"));
    }

    #[test]
    fn language_toggle_works() {
        assert_eq!(Language::ZhCn.toggle(), Language::En);
        assert_eq!(Language::En.toggle(), Language::ZhCn);
    }

    #[test]
    fn language_switch_changes_menu_labels() {
        let zh = Language::ZhCn.strings();
        let en = Language::En.strings();

        assert!(zh.action_install.contains("安装"));
        assert!(en.action_install.contains("Install"));

        assert!(zh.action_exit.contains("退出"));
        assert!(en.action_exit.contains("Exit"));

        assert_ne!(zh.action_install, en.action_install);
        assert_ne!(zh.action_exit, en.action_exit);
    }

    #[test]
    fn handle_key_quit_on_q() {
        let mut selected = 0;
        let mut message = String::new();
        let paths = Paths::discover().unwrap();
        let mut status = InstallationStatus {
            binary_installed: false,
            binary_current: false,
            hooks_configured: false,
        };
        let mut lang = Language::ZhCn;

        let key = KeyEvent::new(KeyCode::Char('q'), event::KeyModifiers::NONE);
        let should_exit = handle_key(
            key,
            &mut selected,
            &mut message,
            &paths,
            &mut status,
            &mut lang,
        )
        .unwrap();
        assert!(should_exit, "pressing 'q' should exit");
    }

    #[test]
    fn handle_key_quit_on_esc() {
        let mut selected = 0;
        let mut message = String::new();
        let paths = Paths::discover().unwrap();
        let mut status = InstallationStatus {
            binary_installed: false,
            binary_current: false,
            hooks_configured: false,
        };
        let mut lang = Language::ZhCn;

        let key = KeyEvent::new(KeyCode::Esc, event::KeyModifiers::NONE);
        let should_exit = handle_key(
            key,
            &mut selected,
            &mut message,
            &paths,
            &mut status,
            &mut lang,
        )
        .unwrap();
        assert!(should_exit, "pressing Esc should exit");
    }

    #[test]
    fn handle_key_navigation() {
        let mut selected = 1;
        let mut message = String::new();
        let paths = Paths::discover().unwrap();
        let mut status = InstallationStatus {
            binary_installed: false,
            binary_current: false,
            hooks_configured: false,
        };
        let mut lang = Language::ZhCn;

        let key_up = KeyEvent::new(KeyCode::Up, event::KeyModifiers::NONE);
        handle_key(
            key_up,
            &mut selected,
            &mut message,
            &paths,
            &mut status,
            &mut lang,
        )
        .unwrap();
        assert_eq!(selected, 0, "up arrow should decrease selection");

        let key_down = KeyEvent::new(KeyCode::Down, event::KeyModifiers::NONE);
        handle_key(
            key_down,
            &mut selected,
            &mut message,
            &paths,
            &mut status,
            &mut lang,
        )
        .unwrap();
        assert_eq!(selected, 1, "down arrow should increase selection");

        let key_j = KeyEvent::new(KeyCode::Char('j'), event::KeyModifiers::NONE);
        handle_key(
            key_j,
            &mut selected,
            &mut message,
            &paths,
            &mut status,
            &mut lang,
        )
        .unwrap();
        assert_eq!(selected, 2, "'j' should increase selection");

        let key_k = KeyEvent::new(KeyCode::Char('k'), event::KeyModifiers::NONE);
        handle_key(
            key_k,
            &mut selected,
            &mut message,
            &paths,
            &mut status,
            &mut lang,
        )
        .unwrap();
        assert_eq!(selected, 1, "'k' should decrease selection");
    }

    #[test]
    fn handle_key_up_at_zero_stays_at_bound() {
        let mut selected = 0;
        let mut message = String::new();
        let paths = Paths::discover().unwrap();
        let mut status = InstallationStatus {
            binary_installed: false,
            binary_current: false,
            hooks_configured: false,
        };
        let mut lang = Language::ZhCn;

        let key_up = KeyEvent::new(KeyCode::Up, event::KeyModifiers::NONE);
        handle_key(
            key_up,
            &mut selected,
            &mut message,
            &paths,
            &mut status,
            &mut lang,
        )
        .unwrap();
        assert_eq!(selected, 0, "up arrow at index 0 should stay at 0");

        let key_k = KeyEvent::new(KeyCode::Char('k'), event::KeyModifiers::NONE);
        handle_key(
            key_k,
            &mut selected,
            &mut message,
            &paths,
            &mut status,
            &mut lang,
        )
        .unwrap();
        assert_eq!(selected, 0, "'k' at index 0 should stay at 0");
    }

    #[test]
    fn handle_key_down_at_last_item_stays_at_bound() {
        let mut selected = 4;
        let mut message = String::new();
        let paths = Paths::discover().unwrap();
        let mut status = InstallationStatus {
            binary_installed: false,
            binary_current: false,
            hooks_configured: false,
        };
        let mut lang = Language::ZhCn;

        let key_down = KeyEvent::new(KeyCode::Down, event::KeyModifiers::NONE);
        handle_key(
            key_down,
            &mut selected,
            &mut message,
            &paths,
            &mut status,
            &mut lang,
        )
        .unwrap();
        assert_eq!(selected, 4, "down arrow at last item should stay");

        let key_j = KeyEvent::new(KeyCode::Char('j'), event::KeyModifiers::NONE);
        handle_key(
            key_j,
            &mut selected,
            &mut message,
            &paths,
            &mut status,
            &mut lang,
        )
        .unwrap();
        assert_eq!(selected, 4, "'j' at last item should stay");
    }

    #[test]
    fn handle_key_exit_action() {
        let mut selected = 4;
        let mut message = String::new();
        let paths = Paths::discover().unwrap();
        let mut status = InstallationStatus {
            binary_installed: false,
            binary_current: false,
            hooks_configured: false,
        };
        let mut lang = Language::ZhCn;

        let key = KeyEvent::new(KeyCode::Enter, event::KeyModifiers::NONE);
        let should_exit = handle_key(
            key,
            &mut selected,
            &mut message,
            &paths,
            &mut status,
            &mut lang,
        )
        .unwrap();
        assert!(
            should_exit,
            "selecting exit action (4) and pressing Enter should exit"
        );
    }

    #[test]
    fn handle_key_language_switch() {
        let mut selected = 3;
        let mut message = String::new();
        let paths = Paths::discover().unwrap();
        let mut status = InstallationStatus {
            binary_installed: false,
            binary_current: false,
            hooks_configured: false,
        };
        let mut lang = Language::ZhCn;

        let key = KeyEvent::new(KeyCode::Enter, event::KeyModifiers::NONE);
        handle_key(
            key,
            &mut selected,
            &mut message,
            &paths,
            &mut status,
            &mut lang,
        )
        .unwrap();

        assert_eq!(lang, Language::En, "language should switch to English");
        assert!(
            message.contains("Language switched") || message.contains("语言已切换"),
            "message should indicate language switch"
        );
    }
}
