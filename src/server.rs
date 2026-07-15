use std::{
    env,
    io::{BufRead, BufReader, ErrorKind, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use interprocess::local_socket::{ListenerOptions, Stream, prelude::*};

use crate::{
    protocol::{
        Request, ResponseEnvelope, ResponseResult, ServerInfo, decode_request, decode_write_data,
        validate_owner,
    },
    session::SessionHost,
    transport::{call, read_frame, runtime_name, write_response},
};

pub(crate) fn run() -> Result<()> {
    let name = runtime_name()?;
    let listener = match ListenerOptions::new().name(name).create_sync() {
        Ok(listener) => listener,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::AddrInUse | ErrorKind::PermissionDenied
            ) =>
        {
            if call(Request::ServerStatus, false).is_ok_and(|response| {
                response.ok && matches!(response.result, Some(ResponseResult::ServerInfo(_)))
            }) {
                return Ok(());
            }
            return Err(error).context("runtime pipe name is occupied by another process");
        }
        Err(error) => return Err(error).context("failed to bind the runtime named pipe"),
    };

    let web_listener = bind_web_ui();
    let web_url = web_listener
        .as_ref()
        .and_then(|listener| listener.local_addr().ok())
        .map(|address| format!("http://{address}/"));
    let state = Arc::new(RuntimeState {
        host: SessionHost::new(),
        started_at_ms: now_millis(),
        stopping: AtomicBool::new(false),
        web_url,
    });
    if let Some(listener) = web_listener {
        let web_state = Arc::clone(&state);
        thread::spawn(move || run_web_ui(listener, web_state));
    }

    for connection in listener.incoming() {
        let connection = match connection {
            Ok(connection) => connection,
            Err(error) => {
                if state.stopping.load(Ordering::Acquire) {
                    break;
                }
                eprintln!("grok-bridge server: failed to accept client: {error}");
                continue;
            }
        };
        if state.stopping.load(Ordering::Acquire) {
            break;
        }
        let state = Arc::clone(&state);
        thread::spawn(move || handle_connection(connection, state));
    }

    state.host.shutdown_all()?;
    Ok(())
}

struct RuntimeState {
    host: SessionHost,
    started_at_ms: u64,
    stopping: AtomicBool,
    web_url: Option<String>,
}

fn handle_connection(stream: Stream, state: Arc<RuntimeState>) {
    let mut connection = BufReader::new(stream);
    let frame = match read_frame(&mut connection) {
        Ok(frame) => frame,
        Err(error) => {
            let response =
                ResponseEnvelope::failure("invalid-request", "invalid_frame", format!("{error:#}"));
            let _ = write_response(connection.get_mut(), &response);
            return;
        }
    };
    let envelope = match decode_request(&frame) {
        Ok(envelope) => envelope,
        Err(error) => {
            let response = ResponseEnvelope::failure(
                "invalid-request",
                "invalid_request",
                format!("{error:#}"),
            );
            let _ = write_response(connection.get_mut(), &response);
            return;
        }
    };

    let request_id = envelope.id;
    let (response, stop_after_response) = match dispatch(&state, envelope.request) {
        Ok((result, stop)) => (ResponseEnvelope::success(request_id, result), stop),
        Err(error) => (
            ResponseEnvelope::failure(request_id, "request_failed", format!("{error:#}")),
            false,
        ),
    };
    let _ = write_response(connection.get_mut(), &response);
    if stop_after_response {
        wake_listener();
    }
}

fn dispatch(state: &RuntimeState, request: Request) -> Result<(ResponseResult, bool)> {
    let result = match request {
        Request::ServerStatus => ResponseResult::ServerInfo(state.server_info()),
        Request::ServerStop => {
            state.stopping.store(true, Ordering::Release);
            state.host.shutdown_all()?;
            return Ok((ResponseResult::Accepted { accepted: true }, true));
        }
        Request::Create {
            cwd,
            prompt,
            model,
            owner,
            always_approve,
        } => ResponseResult::Session(state.host.create(
            &cwd,
            prompt,
            model,
            owner,
            always_approve,
        )?),
        Request::List => ResponseResult::Sessions {
            sessions: state.host.list()?,
        },
        Request::Show { session } => ResponseResult::Session(state.host.show(&session)?),
        Request::Read {
            session,
            cursor,
            limit,
            wait_ms,
        } => ResponseResult::Read(state.host.read(
            &session,
            cursor.unwrap_or(0),
            limit.unwrap_or(4096) as usize,
            wait_ms.unwrap_or(0),
        )?),
        Request::Send { session, input } => {
            ResponseResult::Session(state.host.send(&session, input)?)
        }
        Request::Write {
            session,
            data_base64,
        } => {
            state
                .host
                .write_raw(&session, decode_write_data(&data_base64)?)?;
            ResponseResult::Accepted { accepted: true }
        }
        Request::Resize {
            session,
            cols,
            rows,
        } => {
            state.host.resize(&session, cols, rows)?;
            ResponseResult::Accepted { accepted: true }
        }
        Request::Wait {
            session,
            for_condition,
            timeout_ms,
        } => ResponseResult::Wait(state.host.wait(
            &session,
            for_condition,
            timeout_ms.unwrap_or(300_000),
        )?),
        Request::Close { session } => ResponseResult::Accepted {
            accepted: state.host.close(&session)?,
        },
        Request::HookEvent {
            provider_session_id,
            event,
        } => ResponseResult::Accepted {
            accepted: state.host.apply_hook_event(&provider_session_id, event)?,
        },
    };
    Ok((result, false))
}

impl RuntimeState {
    fn server_info(&self) -> ServerInfo {
        ServerInfo {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            process_id: std::process::id(),
            started_at_ms: self.started_at_ms,
            active_sessions: self.host.active_count(),
            web_url: self.web_url.clone(),
            stopping: self.stopping.load(Ordering::Acquire),
        }
    }
}

fn bind_web_ui() -> Option<TcpListener> {
    let address = env::var("GROK_BRIDGE_WEB_ADDR").unwrap_or_else(|_| "127.0.0.1:47653".to_owned());
    match TcpListener::bind(&address) {
        Ok(listener) => Some(listener),
        Err(error) => {
            eprintln!("grok-bridge server: WebUI unavailable at {address}: {error}");
            None
        }
    }
}

fn run_web_ui(listener: TcpListener, state: Arc<RuntimeState>) {
    for connection in listener.incoming() {
        if state.stopping.load(Ordering::Acquire) {
            break;
        }
        match connection {
            Ok(stream) => {
                let state = Arc::clone(&state);
                thread::spawn(move || handle_web_connection(stream, state));
            }
            Err(error) => eprintln!("grok-bridge server: WebUI accept failed: {error}"),
        }
    }
}

fn handle_web_connection(mut stream: TcpStream, state: Arc<RuntimeState>) {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
    let request = match read_http_request(&mut stream) {
        Ok(request) => request,
        Err(error) => {
            let _ = write_http(
                &mut stream,
                "400 Bad Request",
                "text/plain; charset=utf-8",
                &error,
            );
            return;
        }
    };
    let (method, path, bridge_header) = request;
    match (method.as_str(), path.as_str()) {
        ("GET", "/") => {
            let _ = write_http(&mut stream, "200 OK", "text/html; charset=utf-8", WEB_UI);
        }
        ("GET", "/api/sessions") => match state.host.list_web().and_then(|sessions| {
            serde_json::to_string(&sessions).context("failed to encode WebUI sessions")
        }) {
            Ok(body) => {
                let _ = write_http(&mut stream, "200 OK", "application/json", &body);
            }
            Err(error) => {
                let _ = write_http(
                    &mut stream,
                    "500 Internal Server Error",
                    "text/plain; charset=utf-8",
                    &format!("{error:#}"),
                );
            }
        },
        ("POST", path) if path.starts_with("/api/owners/") => {
            let Some(encoded_owner) = close_path_segment(path, "/api/owners/") else {
                let _ = write_http(
                    &mut stream,
                    "404 Not Found",
                    "text/plain; charset=utf-8",
                    "not found",
                );
                return;
            };
            if !bridge_header {
                let _ = write_http(
                    &mut stream,
                    "403 Forbidden",
                    "text/plain; charset=utf-8",
                    "missing WebUI request header",
                );
                return;
            }
            let owner = match percent_decode_path_segment(encoded_owner) {
                Ok(owner) => owner,
                Err(error) => {
                    let _ = write_http(
                        &mut stream,
                        "400 Bad Request",
                        "text/plain; charset=utf-8",
                        &error,
                    );
                    return;
                }
            };
            if let Err(error) = validate_owner(&owner) {
                let _ = write_http(
                    &mut stream,
                    "400 Bad Request",
                    "text/plain; charset=utf-8",
                    &format!("{error:#}"),
                );
                return;
            }
            match state.host.close_owner(&owner) {
                Ok(result) => {
                    let body = serde_json::json!({
                        "matched": result.matched,
                        "closed": result.closed,
                        "failures": result.failures,
                    })
                    .to_string();
                    let _ = write_http(&mut stream, "200 OK", "application/json", &body);
                }
                Err(error) => {
                    let _ = write_http(
                        &mut stream,
                        "500 Internal Server Error",
                        "text/plain; charset=utf-8",
                        &format!("{error:#}"),
                    );
                }
            }
        }
        ("POST", path) if path.starts_with("/api/sessions/") => {
            let Some(handle) = close_path_segment(path, "/api/sessions/") else {
                let _ = write_http(
                    &mut stream,
                    "404 Not Found",
                    "text/plain; charset=utf-8",
                    "not found",
                );
                return;
            };
            if !bridge_header {
                let _ = write_http(
                    &mut stream,
                    "403 Forbidden",
                    "text/plain; charset=utf-8",
                    "missing WebUI request header",
                );
                return;
            }
            match state.host.close(handle) {
                Ok(closed) => {
                    let body = format!(r#"{{"accepted":{closed}}}"#);
                    let _ = write_http(&mut stream, "200 OK", "application/json", &body);
                }
                Err(error) => {
                    let _ = write_http(
                        &mut stream,
                        "404 Not Found",
                        "text/plain; charset=utf-8",
                        &format!("{error:#}"),
                    );
                }
            }
        }
        _ => {
            let _ = write_http(
                &mut stream,
                "404 Not Found",
                "text/plain; charset=utf-8",
                "not found",
            );
        }
    }
}

fn close_path_segment<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    let segment = path.strip_prefix(prefix)?.strip_suffix("/close")?;
    if segment.contains('/') || segment.contains('?') {
        return None;
    }
    Some(segment)
}

fn percent_decode_path_segment(value: &str) -> std::result::Result<String, String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err("owner contains an incomplete percent escape".to_owned());
            }
            let high = hex_value(bytes[index + 1])
                .ok_or_else(|| "owner contains an invalid percent escape".to_owned())?;
            let low = hex_value(bytes[index + 2])
                .ok_or_else(|| "owner contains an invalid percent escape".to_owned())?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).map_err(|_| "owner is not valid UTF-8".to_owned())
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn read_http_request(
    stream: &mut TcpStream,
) -> std::result::Result<(String, String, bool), String> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|error| error.to_string())?;
    let mut parts = line.split_whitespace();
    let method = parts.next().ok_or("missing HTTP method")?.to_owned();
    let path = parts.next().ok_or("missing HTTP path")?.to_owned();
    let mut bridge_header = false;
    loop {
        line.clear();
        reader
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
        if line == "\r\n" || line == "\n" || line.is_empty() {
            break;
        }
        if line.trim().eq_ignore_ascii_case("X-Grok-Bridge-WebUI: 1") {
            bridge_header = true;
        }
    }
    Ok((method, path, bridge_header))
}

fn write_http(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &str,
) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\n\r\n{body}",
        body.len()
    )
}

const WEB_UI: &str = r#"<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<meta name="color-scheme" content="dark">
<title>Grok Bridge 会话管理</title>
<style>
:root{font-family:"Microsoft YaHei UI","PingFang SC","Noto Sans CJK SC",system-ui,sans-serif;color:#edf3f0;background:#0c0f0e;line-height:1.5;font-synthesis:none}
*{box-sizing:border-box}
body{margin:0;min-width:320px;background:#0c0f0e;color:#edf3f0}
button{font:inherit}
button:focus-visible,summary:focus-visible,pre:focus-visible{outline:2px solid #5eead4;outline-offset:2px}
.shell{width:min(1440px,calc(100% - 32px));margin:0 auto;padding:32px 0 48px}
.hero{padding:8px 0 20px;border-bottom:1px solid #343a38}
.brand-row,.hero-actions,.stats,.group-summary,.session-heading,.session-meta,.refresh-row{display:flex;align-items:center}
.brand-row,.session-heading,.refresh-row{justify-content:space-between}
.eyebrow{margin:0;color:#5eead4;font-size:12px;font-weight:800;letter-spacing:0;text-transform:uppercase}
.runtime{display:inline-flex;align-items:center;gap:8px;color:#a7f3d0;font-size:13px}.runtime::before{content:"";width:8px;height:8px;border-radius:50%;background:#34d399;box-shadow:0 0 0 5px rgba(52,211,153,.12)}.runtime.error{color:#fecdd3}.runtime.error::before{background:#fb7185;box-shadow:0 0 0 5px rgba(251,113,133,.12)}
h1{margin:8px 0 4px;font-size:30px;line-height:1.2}h2,h3,p{margin-top:0}.hero-copy{max-width:820px;margin-bottom:16px;color:#a8b1ad}
.hero-actions{gap:10px;flex-wrap:wrap}
button{border:1px solid transparent;border-radius:6px;padding:7px 11px;cursor:pointer;transition:border-color .15s,background .15s}button:disabled{cursor:not-allowed;opacity:.55}
.secondary{border-color:#3a413f;background:#191d1c;color:#dce5e1}.secondary:hover{border-color:#5b6a65;background:#222827}
.danger{border-color:#7f3542;background:#3a1820;color:#fecdd3}.danger:hover{border-color:#fb7185;background:#521f2a}
.stats{display:grid;grid-template-columns:repeat(5,minmax(0,1fr));margin:18px 0;border:1px solid #343a38;border-width:1px 0;background:#111514}
.stat{padding:12px 16px;border-right:1px solid #343a38}.stat:last-child{border-right:0}.stat span{display:block;color:#89938f;font-size:12px}.stat strong{display:block;margin-top:1px;font-size:24px}
.refresh-row{gap:12px;margin:0 2px 10px;color:#77827d;font-size:12px}.notice{margin-bottom:10px;padding:9px 11px;border:1px solid #2d655a;border-radius:6px;background:#102b26;color:#a7f3d0}.notice[data-tone="error"]{border-color:#7f3542;background:#381a22;color:#fecdd3}
.groups{display:grid;gap:8px}
.group{overflow:hidden;border:1px solid #343a38;border-width:1px 0;background:#101312}
.group[open]{border-color:#45675f}.group-summary{gap:14px;min-height:64px;padding:12px 10px;cursor:pointer;list-style:none}.group-summary::-webkit-details-marker{display:none}.group-summary::before{content:"›";flex:0 0 auto;color:#5eead4;font-size:24px;line-height:1;transform:rotate(0);transition:transform .15s}.group[open]>.group-summary::before{transform:rotate(90deg)}
.group-title{min-width:0;flex:1}.group-title h2{overflow:hidden;margin:0;color:#f3f7f5;font-size:16px;text-overflow:ellipsis;white-space:nowrap}.group-title p{margin:2px 0 0;color:#7f8a85;font-size:12px}.group-count{flex:0 0 auto;padding:4px 8px;border:1px solid #3d645b;border-radius:999px;background:#142621;color:#a7f3d0;font-size:12px;font-weight:700}
.group-body{display:grid;gap:10px;padding:10px;border-top:1px solid #2b302e;background:#0e1110}.group-toolbar{display:flex;justify-content:flex-end}
.session{padding:14px;border:1px solid #343a38;border-radius:6px;background:#171b19}.session-heading{gap:14px;align-items:flex-start}.session-title{min-width:0}.session-title h3{overflow:hidden;margin:6px 0 0;color:#edf3f0;font-size:14px;text-overflow:ellipsis;white-space:nowrap}.badge{display:inline-flex;align-items:center;padding:3px 8px;border-radius:999px;font-size:11px;font-weight:800;letter-spacing:0;text-transform:uppercase}.badge-done{background:#15372f;color:#86efac}.badge-working{background:#14353a;color:#a5f3fc}.badge-waiting{background:#443515;color:#fde68a}.badge-stopped{background:#421d28;color:#fda4af}.badge-unknown{background:#302b3b;color:#ddd6fe}
.session-meta{align-items:flex-start;gap:8px 18px;flex-wrap:wrap;margin:12px 0}.meta-item{min-width:140px;color:#9ca8a3;font-size:12px}.meta-item.path{min-width:min(100%,420px);flex:1}.meta-item span{display:block;margin-bottom:2px;color:#717b77;font-size:10px;font-weight:800;letter-spacing:0;text-transform:uppercase}.meta-item code{display:block;overflow:hidden;color:#a5f3fc;font:12px Consolas,"SFMono-Regular",Menlo,monospace;text-overflow:ellipsis;white-space:nowrap}
.waiting-note{margin:0 0 12px;padding:9px 11px;border-left:3px solid #fbbf24;border-radius:0 6px 6px 0;background:#2d2513;color:#fde68a;font-size:12px}
pre{min-height:140px;max-height:460px;margin:0;overflow:auto;padding:12px;border:1px solid #252a28;border-radius:6px;background:#050706;color:#d1fae5;white-space:pre;tab-size:4;font:13px/1.55 Consolas,"SFMono-Regular",Menlo,monospace}
.empty{padding:48px 20px;border:1px dashed #3b423f;border-radius:8px;color:#89938f;text-align:center}.empty strong{display:block;margin-bottom:5px;color:#dce5e1;font-size:16px}
@media(max-width:760px){.shell{width:min(100% - 20px,1440px);padding-top:14px}.hero{padding-top:4px}.brand-row{align-items:flex-start;gap:10px;flex-direction:column}h1{font-size:24px}.stats{grid-template-columns:repeat(2,minmax(0,1fr))}.stat:nth-child(2),.stat:nth-child(4){border-right:0}.stat:nth-child(-n+4){border-bottom:1px solid #343a38}.stat:last-child{grid-column:1/-1}.group-summary{align-items:flex-start;flex-wrap:wrap}.group-title{flex-basis:calc(100% - 42px)}.group-count{margin-left:38px}.group-toolbar button{width:100%}.session-heading{align-items:stretch;flex-direction:column}.session-heading button{align-self:flex-start}.meta-item,.meta-item.path{min-width:100%}}
</style>
</head>
<body>
<div class="shell">
  <header class="hero">
    <div class="brand-row"><p class="eyebrow">Grok Bridge Runtime</p><span id="runtime-state" class="runtime">本机服务已连接</span></div>
    <h1>Grok 会话管理</h1>
    <p class="hero-copy">按 Codex 对话标题管理所属 Grok。分组可以折叠，终端画面每 2 秒自动刷新；关闭前请先核对标题、目录和终端内容。</p>
    <div class="hero-actions">
      <button id="refresh" class="secondary" type="button">立即刷新</button>
      <button id="expand-all" class="secondary" type="button">全部展开</button>
      <button id="collapse-all" class="secondary" type="button">全部折叠</button>
    </div>
  </header>
  <section class="stats" aria-label="会话统计">
    <article class="stat"><span>Codex 分组</span><strong id="owner-count">0</strong></article>
    <article class="stat"><span>Grok 会话</span><strong id="session-count">0</strong></article>
    <article class="stat"><span>工作中</span><strong id="working-count">0</strong></article>
    <article class="stat"><span>等待输入</span><strong id="waiting-count">0</strong></article>
    <article class="stat"><span>完成 / 空闲</span><strong id="done-count">0</strong></article>
  </section>
  <div class="refresh-row"><span id="last-updated">正在读取 Runtime 状态…</span><span>自动刷新：2 秒</span></div>
  <div id="notice" class="notice" role="status" aria-live="polite" hidden></div>
  <main id="groups" class="groups"></main>
</div>
<script>
const esc=s=>String(s??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const groupsElement=document.querySelector('#groups');
const ownerCountElement=document.querySelector('#owner-count');
const sessionCountElement=document.querySelector('#session-count');
const workingCountElement=document.querySelector('#working-count');
const waitingCountElement=document.querySelector('#waiting-count');
const doneCountElement=document.querySelector('#done-count');
const lastUpdatedElement=document.querySelector('#last-updated');
const noticeElement=document.querySelector('#notice');
const runtimeStateElement=document.querySelector('#runtime-state');
const refreshButton=document.querySelector('#refresh');
const collapsedOwners=new Set();
let loading=false;
let lastSignature=null;
const ownerKey=owner=>owner===null?'missing-owner':'owner:'+owner;
const activityOf=session=>{
  if(['exited','failed','stopped'].includes(session.phase))return 'stopped';
  if(session.activity&&session.activity!=='unknown')return session.activity;
  if(session.phase==='idle')return 'done';
  if(['starting','running'].includes(session.phase))return 'working';
  return 'unknown';
};
const activityLabel=activity=>({working:'工作中',waiting:'等待输入',done:'已完成',stopped:'已退出',unknown:'状态未知'}[activity]||activity);
const ageLabel=updatedAt=>{
  const seconds=Math.max(0,Math.floor((Date.now()-updatedAt)/1000));
  if(seconds<60)return seconds+' 秒前';
  if(seconds<3600)return Math.floor(seconds/60)+' 分钟前';
  return Math.floor(seconds/3600)+' 小时前';
};
function showNotice(message,tone='info'){
  noticeElement.hidden=!message;
  noticeElement.textContent=message;
  noticeElement.dataset.tone=tone;
}
function updateAges(){
  document.querySelectorAll('[data-updated-at]').forEach(element=>{
    element.textContent=ageLabel(Number(element.dataset.updatedAt));
  });
}
function render(sessions){
  const signature=JSON.stringify(sessions.map(session=>[
    session.session,
    session.owner,
    session.phase,
    session.title,
    session.cwd,
    session.process_id,
    session.updated_at_ms,
    session.activity,
    session.hook_event,
    session.hook_at_ms,
    session.tool_name,
    session.waiting_reason,
    session.screen
  ]));
  if(signature===lastSignature){
    updateAges();
    return;
  }
  lastSignature=signature;
  const terminalScroll=new Map([...document.querySelectorAll('[data-terminal]')].map(terminal=>[
    terminal.dataset.terminal,
    {
      top:terminal.scrollTop,
      left:terminal.scrollLeft,
      stickToBottom:terminal.scrollHeight-terminal.scrollTop-terminal.clientHeight<8
    }
  ]));
  const grouped=new Map();
  for(const session of sessions){
    const owner=session.owner??null;
    if(!grouped.has(owner))grouped.set(owner,[]);
    grouped.get(owner).push(session);
  }
  const activities=sessions.map(activityOf);
  ownerCountElement.textContent=grouped.size;
  sessionCountElement.textContent=sessions.length;
  workingCountElement.textContent=activities.filter(activity=>activity==='working').length;
  waitingCountElement.textContent=activities.filter(activity=>activity==='waiting').length;
  doneCountElement.textContent=activities.filter(activity=>activity==='done').length;
  const entries=[...grouped].sort(([left],[right])=>String(left??'').localeCompare(String(right??''),'zh-CN'));
  groupsElement.innerHTML=entries.map(([owner,list])=>{
    const key=ownerKey(owner);
    const working=list.filter(session=>activityOf(session)==='working').length;
    const waiting=list.filter(session=>activityOf(session)==='waiting').length;
    const done=list.filter(session=>activityOf(session)==='done').length;
    const statusSummary=[working&&working+' 个工作中',waiting&&waiting+' 个等待输入',done&&done+' 个完成/空闲'].filter(Boolean).join(' · ')||'无可用状态';
    return `
    <details class="group" data-owner-key="${esc(key)}"${collapsedOwners.has(key)?'':' open'}>
      <summary class="group-summary">
        <div class="group-title">
          <h2 title="${esc(owner??'未标记的 Codex 对话')}">${esc(owner??'未标记的 Codex 对话')}</h2>
          <p>${statusSummary}</p>
        </div>
        <span class="group-count">${list.length} 个 Grok</span>
      </summary>
      <div class="group-body">
        ${owner===null?'':`<div class="group-toolbar"><button class="danger group-close" type="button" data-close-owner="${esc(owner)}" data-count="${list.length}">关闭该 Codex 全部 Grok</button></div>`}
        ${list.map(session=>{
          const activity=activityOf(session);
          return `
        <article class="session">
          <div class="session-heading">
            <div class="session-title">
              <span class="badge badge-${activity}" title="PTY 阶段：${esc(session.phase)}">${esc(activityLabel(activity))}</span>
              <h3 title="${esc(session.title||session.session)}">${esc(session.title||session.session)}</h3>
            </div>
            <button class="danger" type="button" data-close="${esc(session.session)}">关闭 Grok</button>
          </div>
          <div class="session-meta">
            <div class="meta-item"><span>会话 ID</span><code title="${esc(session.session)}">${esc(session.session)}</code></div>
            <div class="meta-item"><span>进程</span><code>PID ${esc(session.process_id)}</code></div>
            <div class="meta-item"><span>最近更新</span><code data-updated-at="${esc(session.updated_at_ms)}">${ageLabel(session.updated_at_ms)}</code></div>
            ${session.hook_event?`<div class="meta-item"><span>最近 Hook</span><code>${esc(session.hook_event)}</code></div>`:''}
            ${session.tool_name?`<div class="meta-item"><span>当前工具</span><code title="${esc(session.tool_name)}">${esc(session.tool_name)}</code></div>`:''}
            <div class="meta-item path"><span>工作目录</span><code title="${esc(session.cwd)}">${esc(session.cwd)}</code></div>
          </div>
          ${session.waiting_reason?`<p class="waiting-note">等待 Codex：${esc(session.waiting_reason)}</p>`:''}
          <pre data-terminal="${esc(session.session)}" tabindex="0" aria-label="${esc(session.session)} 的终端画面">${esc(session.screen||'(终端尚无输出)')}</pre>
        </article>`}).join('')}
      </div>
    </details>`;
  }).join('')||'<div class="empty"><strong>暂无 Grok 会话</strong>新的 Codex 调用会自动显示在这里。</div>';
  document.querySelectorAll('details.group').forEach(group=>{
    group.ontoggle=()=>group.open?collapsedOwners.delete(group.dataset.ownerKey):collapsedOwners.add(group.dataset.ownerKey);
  });
  document.querySelectorAll('[data-terminal]').forEach(terminal=>{
    const saved=terminalScroll.get(terminal.dataset.terminal);
    if(!saved)return;
    terminal.scrollLeft=saved.left;
    terminal.scrollTop=saved.stickToBottom?terminal.scrollHeight:saved.top;
  });
  document.querySelectorAll('[data-close]').forEach(button=>{
    button.onclick=()=>closeSession(button.dataset.close,button);
  });
  document.querySelectorAll('[data-close-owner]').forEach(button=>{
    button.onclick=event=>{
      event.preventDefault();
      event.stopPropagation();
      closeOwner(button.dataset.closeOwner,Number(button.dataset.count),button);
    };
  });
}
async function load(){
  if(loading)return;
  loading=true;
  refreshButton.disabled=true;
  try{
    const response=await fetch('/api/sessions',{cache:'no-store'});
    if(!response.ok)throw new Error(await response.text());
    render(await response.json());
    runtimeStateElement.classList.remove('error');
    runtimeStateElement.textContent='本机服务已连接';
    lastUpdatedElement.textContent='最后刷新：'+new Date().toLocaleTimeString('zh-CN',{hour12:false});
  }catch(error){
    runtimeStateElement.classList.add('error');
    runtimeStateElement.textContent='Runtime 连接异常';
    lastUpdatedElement.textContent='刷新失败';
    showNotice('读取 Runtime 状态失败：'+error,'error');
  }finally{
    loading=false;
    refreshButton.disabled=false;
  }
}
async function closeSession(id,button){
  if(!confirm('确认关闭 '+id+' 及其 Grok 进程？'))return;
  button.disabled=true;
  try{
    const response=await fetch('/api/sessions/'+encodeURIComponent(id)+'/close',{
      method:'POST',
      headers:{'X-Grok-Bridge-WebUI':'1'}
    });
    if(!response.ok)throw new Error(await response.text());
    showNotice('已关闭 Grok 会话 '+id+'。');
  }catch(error){
    showNotice('关闭失败：'+error,'error');
  }finally{
    button.disabled=false;
    await load();
  }
}
async function closeOwner(owner,count,button){
  if(!confirm('确认关闭 Codex“'+owner+'”下的全部 '+count+' 个 Grok 会话？'))return;
  button.disabled=true;
  try{
    const response=await fetch('/api/owners/'+encodeURIComponent(owner)+'/close',{
      method:'POST',
      headers:{'X-Grok-Bridge-WebUI':'1'}
    });
    if(!response.ok)throw new Error(await response.text());
    const result=await response.json();
    if(result.matched===0){
      showNotice('该 Codex 分组已没有活跃 Grok 会话。');
    }else if(result.failures.length||result.closed!==result.matched){
      showNotice('已关闭 '+result.closed+'/'+result.matched+' 个会话；失败：'+result.failures.join('、'),'error');
    }else{
      showNotice('已关闭 Codex“'+owner+'”下的全部 '+result.closed+' 个 Grok 会话。');
    }
  }catch(error){
    showNotice('关闭失败：'+error,'error');
  }finally{
    button.disabled=false;
    await load();
  }
}
function setAllGroups(open){
  document.querySelectorAll('details.group').forEach(group=>{
    if(open)collapsedOwners.delete(group.dataset.ownerKey);
    else collapsedOwners.add(group.dataset.ownerKey);
    group.open=open;
  });
}
refreshButton.onclick=load;
document.querySelector('#expand-all').onclick=()=>setAllGroups(true);
document.querySelector('#collapse-all').onclick=()=>setAllGroups(false);
load();
setInterval(load,2000);
</script>
</body>
</html>"#;

fn wake_listener() {
    if let Ok(name) = runtime_name() {
        let _ = Stream::connect(name);
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_utf8_owner_path_segments_without_form_url_rules() {
        assert_eq!(
            percent_decode_path_segment("Codex-%E5%AF%B9%E8%AF%9D%2F100%25+ready").unwrap(),
            "Codex-对话/100%+ready"
        );
        assert_eq!(percent_decode_path_segment("A%2fb").unwrap(), "A/b");
    }

    #[test]
    fn rejects_malformed_owner_path_segments() {
        for value in ["owner%", "owner%2", "owner%GG", "%FF"] {
            assert!(percent_decode_path_segment(value).is_err(), "{value}");
        }
    }

    #[test]
    fn extracts_close_routes_without_overlapping_prefix_and_suffix() {
        assert_eq!(
            close_path_segment("/api/owners/Codex%20A/close", "/api/owners/"),
            Some("Codex%20A")
        );
        assert_eq!(
            close_path_segment("/api/owners//close", "/api/owners/"),
            Some("")
        );
        assert_eq!(
            close_path_segment("/api/owners/close", "/api/owners/"),
            None
        );
        assert_eq!(
            close_path_segment("/api/owners/a/b/close", "/api/owners/"),
            None
        );
        assert_eq!(
            close_path_segment("/api/sessions/close", "/api/sessions/"),
            None
        );
        assert_eq!(
            close_path_segment("/api/sessions/session-1/close", "/api/sessions/"),
            Some("session-1")
        );
    }

    #[test]
    fn webui_keeps_missing_owner_out_of_batch_close_groups() {
        assert!(WEB_UI.contains("const owner=session.owner??null;"));
        assert!(WEB_UI.contains("owner===null?'':"));
    }

    #[test]
    fn webui_renders_owner_groups_as_persistent_collapsible_panels() {
        assert!(WEB_UI.contains("<details class=\"group\""));
        assert!(WEB_UI.contains("<summary class=\"group-summary\">"));
        assert!(WEB_UI.contains("const collapsedOwners=new Set();"));
        assert!(WEB_UI.contains("collapsedOwners.has(key)?'':' open'"));
        assert!(WEB_UI.contains("group.ontoggle=()=>"));
        assert!(WEB_UI.contains("if(signature===lastSignature)"));
        assert!(WEB_UI.contains("const terminalScroll=new Map("));
        assert!(WEB_UI.contains("white-space:pre;tab-size:4"));
        assert!(!WEB_UI.contains("white-space:pre-wrap"));
        assert!(WEB_UI.contains("全部展开"));
        assert!(WEB_UI.contains("全部折叠"));
    }

    #[test]
    fn webui_keeps_session_context_and_close_controls_in_each_group() {
        for marker in [
            "Codex 分组",
            "Grok 会话",
            "工作中",
            "等待输入",
            "完成 / 空闲",
            "session.title||session.session",
            "session.process_id",
            "session.cwd",
            "session.screen||'(终端尚无输出)'",
            "session.activity",
            "session.hook_event",
            "session.tool_name",
            "session.waiting_reason",
            "等待 Codex：",
            "data-close-owner",
            "data-close=",
            "['starting','running']",
            "['exited','failed','stopped']",
        ] {
            assert!(WEB_UI.contains(marker), "missing WebUI marker: {marker}");
        }
        assert!(
            WEB_UI.find("['exited','failed','stopped']").unwrap()
                < WEB_UI
                    .find("session.activity&&session.activity!=='unknown'")
                    .unwrap()
        );
    }
}
