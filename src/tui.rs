use std::{io, time::Duration};

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

use crate::install::{self, InstallationStatus, Paths};

const ACTIONS: [&str; 4] = [
    "安装 / 更新并自动配置 Grok",
    "移除本工具的 Grok Hooks 配置",
    "打开 WebUI",
    "退出",
];

pub fn run() -> Result<()> {
    enable_raw_mode().context("启用终端原始模式失败")?;
    let _restore = RestoreTerminal;
    execute!(io::stdout(), EnterAlternateScreen).context("进入 TUI 屏幕失败")?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend).context("创建 TUI 终端失败")?;
    let result = run_loop(&mut terminal);
    terminal.show_cursor().ok();
    result
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let paths = Paths::discover()?;
    let mut status = install::status(&paths)?;
    let mut selected: usize = 0;
    let mut message = String::from("选择第一项并按 Enter，即可完成 EXE 安装和 Grok 配置。");

    loop {
        terminal.draw(|frame| render_menu(frame, &paths, status, selected, &message))?;
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        if let Event::Key(key) = event::read()?
            && key.kind != KeyEventKind::Release
            && handle_key(key, &mut selected, &mut message, &paths, &mut status)?
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
) -> Result<bool> {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => return Ok(true),
        KeyCode::Up | KeyCode::Char('k') => *selected = selected.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => {
            *selected = (*selected + 1).min(ACTIONS.len() - 1);
        }
        KeyCode::Enter => match *selected {
            0 => match install::apply() {
                Ok(result) => {
                    *status = install::status(paths)?;
                    *message = format!(
                        "配置完成：{}；请在 Grok 中确认信任 Hooks。",
                        result.display()
                    );
                }
                Err(error) => *message = format!("配置失败：{error:#}"),
            },
            1 => match install::uninstall() {
                Ok(true) => {
                    *status = install::status(paths)?;
                    *message = "已移除本工具写入的 Grok Hooks 配置。".into();
                }
                Ok(false) => {
                    *message = "没有找到本工具写入的 Hooks，未做修改。".into();
                }
                Err(error) => *message = format!("移除失败：{error:#}"),
            },
            2 => match open_webui() {
                Ok(()) => *message = "已尝试打开 WebUI，请检查浏览器。".into(),
                Err(error) => *message = format!("打开 WebUI 失败：{error:#}"),
            },
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
) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(6),
            Constraint::Length(7),
            Constraint::Min(4),
            Constraint::Length(2),
        ])
        .split(frame.area());

    let title = Paragraph::new(Line::from(vec![
        Span::styled(
            "grok-bridge",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" 安装配置"),
    ]))
    .alignment(Alignment::Center)
    .block(Block::default().borders(Borders::ALL));
    frame.render_widget(title, areas[0]);

    let status_text = vec![
        Line::from(status_line(
            "安装状态",
            status.display(),
            status.binary_current && status.hooks_configured,
        )),
        Line::from(status_line(
            "原生 EXE（当前版本）",
            if status.binary_current {
                "已就绪"
            } else {
                "需更新"
            },
            status.binary_current,
        )),
        Line::from(status_line(
            "Grok Hooks",
            if status.hooks_configured {
                "已配置"
            } else {
                "未配置"
            },
            status.hooks_configured,
        )),
        Line::from(format!("目标路径：{}", paths.installed_binary.display())),
    ];
    frame.render_widget(
        Paragraph::new(status_text)
            .block(Block::default().title(" 状态 ").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        areas[1],
    );

    let items: Vec<ListItem> = ACTIONS
        .iter()
        .map(|action| ListItem::new(*action))
        .collect();
    let actions = List::new(items)
        .block(Block::default().title(" 操作 ").borders(Borders::ALL))
        .highlight_symbol("▶ ")
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    let mut list_state = ListState::default().with_selected(Some(selected));
    frame.render_stateful_widget(actions, areas[2], &mut list_state);

    frame.render_widget(
        Paragraph::new(message)
            .block(Block::default().title(" 结果 ").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        areas[3],
    );

    frame.render_widget(
        Paragraph::new("↑/↓ 或 j/k 选择 · Enter 执行 · q/Esc 退出")
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

fn open_webui() -> Result<()> {
    use crate::protocol::Request;
    use crate::transport;
    use std::process::Command;

    let response = transport::call(Request::ServerStatus, true)?;
    if !response.ok {
        anyhow::bail!("Runtime server 未运行或状态异常");
    }

    match response.result {
        Some(crate::protocol::ResponseResult::ServerInfo(info)) => match info.web_url {
            Some(url) => {
                #[cfg(windows)]
                Command::new("explorer.exe")
                    .arg(&url)
                    .spawn()
                    .context("无法启动 explorer.exe")?;
                #[cfg(target_os = "macos")]
                Command::new("open")
                    .arg(&url)
                    .spawn()
                    .context("无法启动 open")?;
                #[cfg(all(unix, not(target_os = "macos")))]
                Command::new("xdg-open")
                    .arg(&url)
                    .spawn()
                    .context("无法启动 xdg-open")?;
                Ok(())
            }
            None => anyhow::bail!("Runtime WebUI 不可用；检查 server stderr"),
        },
        _ => anyhow::bail!("Runtime 返回了意外的 server status 响应"),
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
    fn handle_key_quit_on_q() {
        let mut selected = 0;
        let mut message = String::new();
        let paths = Paths::discover().unwrap();
        let mut status = InstallationStatus {
            binary_installed: false,
            binary_current: false,
            hooks_configured: false,
        };

        let key = KeyEvent::new(KeyCode::Char('q'), event::KeyModifiers::NONE);
        let should_exit =
            handle_key(key, &mut selected, &mut message, &paths, &mut status).unwrap();
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

        let key = KeyEvent::new(KeyCode::Esc, event::KeyModifiers::NONE);
        let should_exit =
            handle_key(key, &mut selected, &mut message, &paths, &mut status).unwrap();
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

        let key_up = KeyEvent::new(KeyCode::Up, event::KeyModifiers::NONE);
        handle_key(key_up, &mut selected, &mut message, &paths, &mut status).unwrap();
        assert_eq!(selected, 0, "up arrow should decrease selection");

        let key_down = KeyEvent::new(KeyCode::Down, event::KeyModifiers::NONE);
        handle_key(key_down, &mut selected, &mut message, &paths, &mut status).unwrap();
        assert_eq!(selected, 1, "down arrow should increase selection");

        let key_j = KeyEvent::new(KeyCode::Char('j'), event::KeyModifiers::NONE);
        handle_key(key_j, &mut selected, &mut message, &paths, &mut status).unwrap();
        assert_eq!(selected, 2, "'j' should increase selection");

        let key_k = KeyEvent::new(KeyCode::Char('k'), event::KeyModifiers::NONE);
        handle_key(key_k, &mut selected, &mut message, &paths, &mut status).unwrap();
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

        let key_up = KeyEvent::new(KeyCode::Up, event::KeyModifiers::NONE);
        handle_key(key_up, &mut selected, &mut message, &paths, &mut status).unwrap();
        assert_eq!(selected, 0, "up arrow at index 0 should stay at 0");

        let key_k = KeyEvent::new(KeyCode::Char('k'), event::KeyModifiers::NONE);
        handle_key(key_k, &mut selected, &mut message, &paths, &mut status).unwrap();
        assert_eq!(selected, 0, "'k' at index 0 should stay at 0");
    }

    #[test]
    fn handle_key_down_at_last_item_stays_at_bound() {
        let mut selected = ACTIONS.len() - 1;
        let mut message = String::new();
        let paths = Paths::discover().unwrap();
        let mut status = InstallationStatus {
            binary_installed: false,
            binary_current: false,
            hooks_configured: false,
        };

        let key_down = KeyEvent::new(KeyCode::Down, event::KeyModifiers::NONE);
        handle_key(key_down, &mut selected, &mut message, &paths, &mut status).unwrap();
        assert_eq!(
            selected,
            ACTIONS.len() - 1,
            "down arrow at last item should stay"
        );

        let key_j = KeyEvent::new(KeyCode::Char('j'), event::KeyModifiers::NONE);
        handle_key(key_j, &mut selected, &mut message, &paths, &mut status).unwrap();
        assert_eq!(selected, ACTIONS.len() - 1, "'j' at last item should stay");
    }

    #[test]
    fn handle_key_exit_action() {
        let mut selected = 3;
        let mut message = String::new();
        let paths = Paths::discover().unwrap();
        let mut status = InstallationStatus {
            binary_installed: false,
            binary_current: false,
            hooks_configured: false,
        };

        let key = KeyEvent::new(KeyCode::Enter, event::KeyModifiers::NONE);
        let should_exit =
            handle_key(key, &mut selected, &mut message, &paths, &mut status).unwrap();
        assert!(
            should_exit,
            "selecting exit action (3) and pressing Enter should exit"
        );
    }
}
