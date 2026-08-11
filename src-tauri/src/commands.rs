/// Tauri command handlers.
/// These replace the Flask API endpoints and Electron IPC proxy.
use crate::analysis;
use crate::db_scan;
use crate::export_chat;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{Manager, State};
#[cfg(windows)]
use windows::Win32::Foundation::HANDLE;

// ── Shared application state ──

pub struct AppState {
    pub export_log: Arc<Mutex<Vec<LogMessage>>>,
    pub export_done: Arc<Mutex<bool>>,
    pub export_ok: Arc<Mutex<bool>>,
    pub export_summary: Arc<Mutex<Option<ExportSummary>>>,
    pub key_log: Arc<Mutex<Vec<LogMessage>>>,
    pub key_done: Arc<Mutex<bool>>,
    pub key_ok: Arc<Mutex<bool>>,
    pub key_result: Arc<Mutex<Option<String>>>,
    pub analysis_log: Arc<Mutex<Vec<LogMessage>>>,
    pub analysis_done: Arc<Mutex<bool>>,
    pub csv_progress: Arc<Mutex<Vec<LogMessage>>>,
    pub csv_done: Arc<Mutex<bool>>,
    pub msg_store: Arc<Mutex<std::collections::HashMap<String, Arc<export_chat::MessageStore>>>>,
    pub cancel_flag: Arc<AtomicBool>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            export_log: Arc::new(Mutex::new(Vec::new())),
            export_done: Arc::new(Mutex::new(false)),
            export_ok: Arc::new(Mutex::new(false)),
            export_summary: Arc::new(Mutex::new(None)),
            key_log: Arc::new(Mutex::new(Vec::new())),
            key_done: Arc::new(Mutex::new(false)),
            key_ok: Arc::new(Mutex::new(false)),
            key_result: Arc::new(Mutex::new(None)),
            analysis_log: Arc::new(Mutex::new(Vec::new())),
            analysis_done: Arc::new(Mutex::new(false)),
            csv_progress: Arc::new(Mutex::new(Vec::new())),
            csv_done: Arc::new(Mutex::new(false)),
            msg_store: Arc::new(Mutex::new(std::collections::HashMap::new())),
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogMessage {
    pub level: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportSummary {
    pub total: usize,
    pub groups: usize,
    #[serde(rename = "private")]
    pub private_count: usize,
    pub dir: String,
}

#[derive(Debug, Serialize)]
pub struct PingResponse {
    pub ok: bool,
}

#[derive(Debug, Serialize)]
pub struct ScanResponse {
    pub ok: bool,
    pub databases: Vec<db_scan::DatabaseInfo>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct KeyStatusResponse {
    pub done: bool,
    pub ok: bool,
    pub key: Option<String>,
    pub messages: Vec<LogMessage>,
}

#[derive(Debug, Serialize)]
pub struct ExportStatusResponse {
    pub done: bool,
    pub ok: bool,
    pub messages: Vec<LogMessage>,
    pub summary: Option<ExportSummary>,
}

#[derive(Debug, Serialize)]
pub struct SimpleResponse {
    pub ok: bool,
    pub error: Option<String>,
}

// ── Commands ──

#[tauri::command]
pub fn ping() -> PingResponse {
    PingResponse { ok: true }
}

#[tauri::command]
pub fn scan_databases() -> ScanResponse {
    let mut dbs = db_scan::find_qq_databases();

    // macOS stores accounts under hashed directory names. Match every saved
    // account key against each database so the UI can display the QQ number.
    #[cfg(target_os = "macos")]
    {
        let saved_keys = load_keys_map();
        for db in &mut dbs {
            for (qq, encoded) in &saved_keys {
                let Some(key) = deobfuscate_key(encoded) else {
                    continue;
                };
                if export_chat::open_db_for_analysis(&db.path, &key).is_ok() {
                    db.qq = qq.clone();
                    break;
                }
            }
        }
    }

    ScanResponse {
        ok: true,
        databases: dbs,
        error: None,
    }
}

#[tauri::command]
pub fn extract_key(state: State<'_, AppState>, qq: Option<String>) -> SimpleResponse {
    // Reset state
    {
        let mut log = state.key_log.lock().unwrap();
        log.clear();
    }
    {
        *state.key_done.lock().unwrap() = false;
    }
    {
        *state.key_ok.lock().unwrap() = false;
    }
    {
        *state.key_result.lock().unwrap() = None;
    }

    let log = state.key_log.clone();
    let done = state.key_done.clone();
    let ok = state.key_ok.clone();
    let result = state.key_result.clone();

    // Add initial log message
    {
        let mut l = log.lock().unwrap();
        l.push(LogMessage {
            level: "info".to_string(),
            text: "正在启动密钥提取...".to_string(),
        });
        #[cfg(windows)]
        l.push(LogMessage {
            level: "warn".to_string(),
            text: "请在弹出的 QQ 窗口中登录账号".to_string(),
        });
        #[cfg(target_os = "macos")]
        l.push(LogMessage {
            level: "warn".to_string(),
            text: "提取期间请保持 QQ 窗口可操作；稍后可能需要退出账号后重新登录".to_string(),
        });
    }

    // Spawn key extraction in background thread
    std::thread::spawn(move || {
        match extract_key_impl(qq.as_deref(), &log) {
            Ok(key) => {
                let mut l = log.lock().unwrap();
                l.push(LogMessage {
                    level: "success".to_string(),
                    text: "密钥获取并验证成功".to_string(),
                });
                *ok.lock().unwrap() = true;
                *result.lock().unwrap() = Some(key);
            }
            Err(error) => {
                let mut l = log.lock().unwrap();
                l.push(LogMessage {
                    level: "error".to_string(),
                    text: error,
                });
            }
        }
        *done.lock().unwrap() = true;
    });

    SimpleResponse {
        ok: true,
        error: None,
    }
}

fn push_key_log(log: &Arc<Mutex<Vec<LogMessage>>>, level: &str, text: impl Into<String>) {
    if let Ok(mut messages) = log.lock() {
        messages.push(LogMessage {
            level: level.to_string(),
            text: text.into(),
        });
    }
}

#[cfg(windows)]
fn extract_key_impl(
    _qq: Option<&str>,
    _log: &Arc<Mutex<Vec<LogMessage>>>,
) -> Result<String, String> {
    let qq_info = find_qq_installation().ok_or("未找到 QQ NT 安装目录")?;
    let function_rva = analyze_pe(&qq_info.wrapper_node_path).ok_or("无法定位 QQ 密钥函数")?;
    debug_extract_key(&qq_info.qq_exe_path, function_rva).ok_or("密钥提取失败".to_string())
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn extract_key_impl(
    _qq: Option<&str>,
    _log: &Arc<Mutex<Vec<LogMessage>>>,
) -> Result<String, String> {
    Err("当前平台暂不支持自动提取密钥".to_string())
}

#[cfg(target_os = "macos")]
fn extract_key_impl(qq: Option<&str>, log: &Arc<Mutex<Vec<LogMessage>>>) -> Result<String, String> {
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn find_qq_app() -> Option<PathBuf> {
        let mut candidates = vec![PathBuf::from("/Applications/QQ.app")];
        if let Ok(home) = std::env::var("HOME") {
            candidates.push(PathBuf::from(home).join("Applications/QQ.app"));
        }
        candidates.into_iter().find(|path| path.is_dir())
    }

    fn find_qq_pid(app: &Path) -> Option<u32> {
        let pattern = format!("{}/Contents/MacOS/QQ$", app.to_string_lossy());
        let output = Command::new("/usr/bin/pgrep")
            .args(["-f", &pattern])
            .output()
            .ok()?;
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()?
            .trim()
            .parse()
            .ok()
    }

    fn wrapper_is_loaded(pid: u32) -> bool {
        Command::new("/usr/sbin/lsof")
            .arg(format!("-p{}", pid))
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).contains("wrapper.node"))
            .unwrap_or(false)
    }

    fn find_target_db() -> Option<PathBuf> {
        let home = PathBuf::from(std::env::var("HOME").ok()?);
        let bases = [
            home.join("Library/Containers/com.tencent.qq/Data/Library/Application Support/QQ"),
            home.join("Library/Application Support/QQ"),
        ];
        let mut found = Vec::new();
        for base in bases {
            let Ok(entries) = std::fs::read_dir(base) else {
                continue;
            };
            for entry in entries.flatten() {
                if !entry.file_name().to_string_lossy().starts_with("nt_qq_") {
                    continue;
                }
                let path = entry.path().join("nt_db/nt_msg.db");
                if path.is_file() {
                    let modified = std::fs::metadata(&path)
                        .and_then(|m| m.modified())
                        .unwrap_or(UNIX_EPOCH);
                    found.push((modified, path));
                }
            }
        }
        found.sort_by(|a, b| b.0.cmp(&a.0));
        found.into_iter().next().map(|(_, path)| path)
    }

    fn send_lldb(stdin: &mut std::process::ChildStdin, command: &str) -> Result<(), String> {
        writeln!(stdin, "{}", command).map_err(|e| format!("无法控制 LLDB: {}", e))?;
        stdin
            .flush()
            .map_err(|e| format!("无法刷新 LLDB 命令: {}", e))
    }

    fn stop_lldb(child: &mut std::process::Child, stdin: &mut std::process::ChildStdin) {
        let _ = send_lldb(stdin, "process interrupt");
        std::thread::sleep(Duration::from_millis(200));
        let _ = send_lldb(stdin, "process detach");
        let _ = send_lldb(stdin, "quit");
        for _ in 0..10 {
            if child.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = child.kill();
        let _ = child.wait();
    }

    let qq_app = find_qq_app().ok_or("未找到 QQ.app，请先安装 macOS 版 QQ")?;
    let wrapper = qq_app.join("Contents/Resources/app/wrapper.node");
    if !wrapper.is_file() {
        return Err("QQ 安装中缺少 wrapper.node，当前版本可能不兼容".to_string());
    }
    if !Command::new("/usr/bin/xcrun")
        .args(["--find", "lldb"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return Err("未找到 LLDB，请先安装 Xcode Command Line Tools".to_string());
    }

    let sip_output = Command::new("/usr/bin/csrutil").arg("status").output().ok();
    let sip_enabled = sip_output
        .as_ref()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .to_lowercase()
                .contains("enabled")
        })
        .unwrap_or(false);
    if sip_enabled {
        return Err("macOS SIP 已开启，原版 QQ 不允许调试附加。请先关闭 SIP，或明确授权后将 QQ 改为 ad-hoc 本地签名。".to_string());
    }

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let temp_dir =
        std::env::temp_dir().join(format!("qqflow_key_{}_{}", std::process::id(), nonce));
    std::fs::create_dir_all(&temp_dir).map_err(|e| format!("无法创建临时目录: {}", e))?;
    let script_path = temp_dir.join("qq_key_extractor_macos.py");
    let key_path = temp_dir.join("key.txt");
    let breakpoint_path = temp_dir.join("breakpoint.txt");
    std::fs::write(&script_path, include_str!("qq_key_extractor_macos.py"))
        .map_err(|e| format!("无法准备 macOS 提取脚本: {}", e))?;

    push_key_log(log, "info", "已确认 LLDB 权限，正在连接 QQ...");
    let mut child = Command::new("/usr/bin/xcrun")
        .arg("lldb")
        .arg("--one-line")
        .arg(format!(
            "command script import {}",
            script_path.to_string_lossy()
        ))
        .env("QQFLOW_WRAPPER_PATH", &wrapper)
        .env("QQFLOW_KEY_RESULT_PATH", &key_path)
        .env("QQFLOW_BP_RESULT_PATH", &breakpoint_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("无法启动 LLDB: {}", e))?;
    let mut stdin = child.stdin.take().ok_or("无法连接 LLDB 标准输入")?;

    let capture_result = (|| -> Result<String, String> {
        let pid = match find_qq_pid(&qq_app) {
            Some(pid) => pid,
            None => {
                Command::new("/usr/bin/open")
                    .arg(&qq_app)
                    .status()
                    .map_err(|e| format!("无法启动 QQ: {}", e))?;
                let mut pid = None;
                for _ in 0..40 {
                    std::thread::sleep(Duration::from_millis(500));
                    pid = find_qq_pid(&qq_app);
                    if pid.is_some() {
                        break;
                    }
                }
                pid.ok_or("启动后未找到 QQ 主进程")?
            }
        };
        push_key_log(
            log,
            "info",
            format!("已找到 QQ 进程 PID={}，正在附加...", pid),
        );
        send_lldb(&mut stdin, &format!("process attach --pid {}", pid))?;
        std::thread::sleep(Duration::from_millis(1500));

        if !wrapper_is_loaded(pid) {
            send_lldb(&mut stdin, "process continue")?;
            let mut loaded = false;
            for _ in 0..120 {
                std::thread::sleep(Duration::from_millis(500));
                if wrapper_is_loaded(pid) {
                    loaded = true;
                    break;
                }
            }
            if !loaded {
                return Err("QQ 启动超时：wrapper.node 未加载".to_string());
            }
            send_lldb(&mut stdin, "process interrupt")?;
            std::thread::sleep(Duration::from_millis(1200));
        }

        push_key_log(log, "info", "wrapper.node 已加载，正在设置密钥断点...");
        send_lldb(&mut stdin, "qqflow-set-key-breakpoint")?;
        let mut breakpoint_ready = false;
        for _ in 0..40 {
            std::thread::sleep(Duration::from_millis(250));
            if breakpoint_path.is_file() {
                breakpoint_ready = true;
                break;
            }
        }
        if !breakpoint_ready {
            return Err("无法设置密钥断点；请确认 QQ 6.9.x 与 LLDB 调试权限可用".to_string());
        }
        send_lldb(&mut stdin, "process continue")?;
        push_key_log(
            log,
            "warn",
            "断点已就绪：若 QQ 已登录，请在 QQ 内退出账号后重新登录；不要关闭 QQ 进程",
        );

        let target_db = find_target_db();
        if let Some(path) = &target_db {
            push_key_log(
                log,
                "info",
                format!("捕获后将使用数据库验证密钥: {}", path.display()),
            );
        } else {
            push_key_log(log, "warn", "未找到 nt_msg.db，捕获后将无法自动验证密钥");
        }

        let mut last_candidate = String::new();
        for tick in 0..720 {
            std::thread::sleep(Duration::from_millis(250));
            if tick > 0 && tick % 120 == 0 {
                push_key_log(
                    log,
                    "info",
                    "仍在等待 QQ 触发数据库打开，请完成一次退出账号并重新登录...",
                );
            }
            let Ok(raw) = std::fs::read_to_string(&key_path) else {
                continue;
            };
            let candidate = raw.trim().to_string();
            if candidate.is_empty() || candidate == last_candidate {
                continue;
            }
            last_candidate = candidate.clone();
            push_key_log(
                log,
                "info",
                format!("捕获到 {} 字节候选，正在验证...", candidate.len()),
            );
            if let Some(db_path) = &target_db {
                match export_chat::open_db_for_analysis(&db_path.to_string_lossy(), &candidate) {
                    Ok(_) => return Ok(candidate),
                    Err(_) => {
                        push_key_log(log, "warn", "该候选无法打开所选数据库，继续等待下一个调用");
                        continue;
                    }
                }
            }
            if (8..=128).contains(&candidate.len()) {
                return Ok(candidate);
            }
        }
        let account = qq.filter(|value| !value.is_empty()).unwrap_or("所选账号");
        Err(format!(
            "180 秒内未捕获到 {} 的有效密钥；请重新提取，并在断点就绪后退出账号再登录",
            account
        ))
    })();

    stop_lldb(&mut child, &mut stdin);
    let _ = std::fs::remove_file(&key_path);
    let _ = std::fs::remove_file(&breakpoint_path);
    let _ = std::fs::remove_file(&script_path);
    let _ = std::fs::remove_dir(&temp_dir);
    capture_result
}

#[cfg(windows)]
struct QqInfo {
    qq_exe_path: String,
    wrapper_node_path: String,
}

#[cfg(windows)]
fn find_qq_installation() -> Option<QqInfo> {
    use std::path::PathBuf;

    let program_files = std::env::var("ProgramFiles").unwrap_or_default();
    let program_files_x86 = std::env::var("ProgramFiles(x86)").unwrap_or_default();

    let possible_paths = [
        format!("{}\\Tencent\\QQNT", program_files),
        format!("{}\\Tencent\\QQNT", program_files_x86),
        "C:\\Program Files\\Tencent\\QQNT".to_string(),
        "C:\\Program Files (x86)\\Tencent\\QQNT".to_string(),
    ];

    for base in &possible_paths {
        let base_path = PathBuf::from(base);
        if !base_path.is_dir() {
            continue;
        }

        let qq_exe = base_path.join("QQ.exe");
        if !qq_exe.is_file() {
            continue;
        }

        let versions_dir = base_path.join("versions");
        if !versions_dir.is_dir() {
            continue;
        }

        if let Ok(entries) = std::fs::read_dir(&versions_dir) {
            for entry in entries.flatten() {
                let wrapper = entry
                    .path()
                    .join("resources")
                    .join("app")
                    .join("wrapper.node");
                if wrapper.is_file() {
                    return Some(QqInfo {
                        qq_exe_path: qq_exe.to_string_lossy().to_string(),
                        wrapper_node_path: wrapper.to_string_lossy().to_string(),
                    });
                }
            }
        }
    }

    None
}

/// Analyze PE64 file to find the RVA of nt_sqlite3_key_v2 function.
#[cfg(windows)]
fn analyze_pe(wrapper_node_path: &str) -> Option<u64> {
    let data = std::fs::read(wrapper_node_path).ok()?;

    if data.len() < 0x40 || data[0] != b'M' || data[1] != b'Z' {
        return None;
    }

    let e_lfanew = read_u32(&data, 0x3C)? as usize;

    if read_u32(&data, e_lfanew)? != 0x00004550 {
        return None;
    }

    let coff_offset = e_lfanew + 4;
    let num_sections = read_u16(&data, coff_offset + 2)? as usize;
    let size_optional = read_u16(&data, coff_offset + 16)? as usize;
    let optional_offset = coff_offset + 20;
    let magic = read_u16(&data, optional_offset)?;

    if magic != 0x20B {
        return None;
    }

    let data_dir_offset = optional_offset + 112;
    let exception_dir_rva = read_u32(&data, data_dir_offset + 3 * 8)?;
    let exception_dir_size = read_u32(&data, data_dir_offset + 3 * 8 + 4)?;

    let sections_offset = optional_offset + size_optional;
    let mut sections: Vec<(String, u32, u32, u32, u32)> = Vec::new();
    for i in 0..num_sections {
        let off = sections_offset + i * 40;
        let name = String::from_utf8_lossy(&data.get(off..off + 8).unwrap_or(b""))
            .trim_end_matches('\0')
            .to_string();
        let vsize = read_u32(&data, off + 8).unwrap_or(0);
        let va = read_u32(&data, off + 12).unwrap_or(0);
        let raw_ptr = read_u32(&data, off + 20).unwrap_or(0);
        let raw_size = read_u32(&data, off + 16).unwrap_or(0);
        sections.push((name, va, vsize, raw_ptr, raw_size));
    }

    let target = b"nt_sqlite3_key_v2: db=%p zDb=%s";
    let rdata = sections.iter().find(|s| s.0 == ".rdata")?;
    let rdata_start = rdata.3 as usize;
    let rdata_end = rdata_start + rdata.4 as usize;

    let pattern_pos = find_pattern(&data, target, rdata_start, rdata_end)?;
    let string_rva = rdata.1 as u64 + (pattern_pos as u64 - rdata.3 as u64);

    let text = sections.iter().find(|s| s.0 == ".text")?;
    let text_start = text.3 as usize;
    let text_size = text.4 as usize;
    let text_rva = text.1 as u64;

    let mut lea_rva: Option<u64> = None;
    for i in 1..(text_size.saturating_sub(6)) {
        let file_offset = text_start + i;
        if file_offset >= data.len() {
            break;
        }
        if data[file_offset] != 0x8D {
            continue;
        }
        if file_offset == 0 {
            continue;
        }
        let rex = data[file_offset - 1];
        if (rex & 0xF8) != 0x48 {
            continue;
        }
        if file_offset + 3 >= data.len() {
            continue;
        }
        let modrm = data[file_offset + 1];
        if (modrm & 0xC7) != 0x05 {
            continue;
        }
        let disp = read_i32(&data, file_offset + 2)? as i64;
        let instr_rva = text_rva + (i as u64 - 1);
        let instr_len = 7u64;
        let target_rva = (instr_rva as i64 + instr_len as i64 + disp) as u64;
        if target_rva == string_rva {
            lea_rva = Some(instr_rva);
            break;
        }
    }

    let lea_instruction_rva = lea_rva?;

    let exception_section = sections
        .iter()
        .find(|s| exception_dir_rva >= s.1 && exception_dir_rva < s.1 + s.2)?;

    let ex_file_offset =
        exception_section.3 as usize + (exception_dir_rva as usize - exception_section.1 as usize);
    let entry_size = 12;
    let num_entries = exception_dir_size as usize / entry_size;

    let target_u32 = lea_instruction_rva as u32;
    let mut left = 0usize;
    let mut right = num_entries;

    while left < right {
        let mid = (left + right) / 2;
        let entry_off = ex_file_offset + mid * entry_size;
        if entry_off + 8 > data.len() {
            break;
        }
        let begin = read_u32(&data, entry_off)?;
        let end = read_u32(&data, entry_off + 4)?;

        if target_u32 < begin {
            right = mid;
        } else if target_u32 >= end {
            left = mid + 1;
        } else {
            return Some(begin as u64);
        }
    }

    None
}

/// Debug QQ process to extract encryption key.
#[cfg(windows)]
fn debug_extract_key(qq_exe_path: &str, function_rva: u64) -> Option<String> {
    use std::ffi::CString;
    use windows::Win32::Foundation::{
        CloseHandle, BOOL, DBG_CONTINUE, DBG_EXCEPTION_NOT_HANDLED, EXCEPTION_BREAKPOINT,
    };
    use windows::Win32::System::Diagnostics::Debug::{
        ContinueDebugEvent, WaitForDebugEvent, DEBUG_EVENT, EXCEPTION_DEBUG_EVENT,
        EXIT_PROCESS_DEBUG_EVENT, LOAD_DLL_DEBUG_EVENT,
    };
    use windows::Win32::System::Threading::{
        CreateProcessA, TerminateProcess, DEBUG_ONLY_THIS_PROCESS, PROCESS_INFORMATION,
        STARTUPINFOA,
    };

    unsafe {
        let exe_path = CString::new(qq_exe_path).ok()?;
        let mut si: STARTUPINFOA = std::mem::zeroed();
        si.cb = std::mem::size_of::<STARTUPINFOA>() as u32;
        let mut pi: PROCESS_INFORMATION = std::mem::zeroed();

        let ok = CreateProcessA(
            None,
            windows::core::PSTR(exe_path.as_ptr() as *mut u8),
            None,
            None,
            BOOL(0),
            DEBUG_ONLY_THIS_PROCESS,
            None,
            None,
            &mut si,
            &mut pi,
        );

        if ok.is_err() {
            return None;
        }

        let h_process = pi.hProcess;
        let h_thread = pi.hThread;
        let pid = pi.dwProcessId;

        let mut wrapper_base: u64 = 0;
        let mut breakpoint_set = false;
        let mut breakpoint_addr: u64 = 0;
        let mut original_byte: u8 = 0;
        let max_wait = std::time::Duration::from_secs(120);
        let start_time = std::time::Instant::now();
        let mut extracted_key: Option<String> = None;

        'debug_loop: loop {
            // Timeout check
            if start_time.elapsed() > max_wait {
                break;
            }

            let mut evt: DEBUG_EVENT = std::mem::zeroed();
            if WaitForDebugEvent(&mut evt, 5000).is_err() {
                continue;
            }

            let mut continue_status = DBG_CONTINUE;

            match evt.dwDebugEventCode {
                LOAD_DLL_DEBUG_EVENT => {
                    let info = evt.u.LoadDll;
                    if !info.hFile.is_invalid() {
                        let _ = CloseHandle(info.hFile);
                    }

                    if wrapper_base == 0 {
                        if let Some(base) = get_module_base(pid, "wrapper.node") {
                            wrapper_base = base;
                            let target_addr = wrapper_base + function_rva;

                            // Always set breakpoint immediately when wrapper.node loads.
                            // The INT3 will fire when nt_sqlite3_key_v2 is called (after login).
                            if set_breakpoint(h_process, target_addr, &mut original_byte) {
                                breakpoint_set = true;
                                breakpoint_addr = target_addr;
                            }
                        }
                    }
                }

                EXCEPTION_DEBUG_EVENT => {
                    let info = evt.u.Exception;
                    let exception_record = info.ExceptionRecord;
                    let code = exception_record.ExceptionCode.0;
                    let addr = exception_record.ExceptionAddress as u64;

                    if code == EXCEPTION_BREAKPOINT.0 && breakpoint_set && addr == breakpoint_addr {
                        restore_byte(h_process, breakpoint_addr, original_byte);

                        if let Some(key) = read_key_from_r8(pid, evt.dwThreadId, h_process) {
                            extracted_key = Some(key);
                            let _ = TerminateProcess(h_process, 0);
                            loop {
                                let mut exit_evt: DEBUG_EVENT = std::mem::zeroed();
                                if WaitForDebugEvent(&mut exit_evt, 5000).is_err() {
                                    break;
                                }
                                if exit_evt.dwDebugEventCode == EXIT_PROCESS_DEBUG_EVENT {
                                    break 'debug_loop;
                                }
                                let _ = ContinueDebugEvent(
                                    exit_evt.dwProcessId,
                                    exit_evt.dwThreadId,
                                    DBG_CONTINUE,
                                );
                            }
                        } else {
                            // Key read failed (R8 didn't contain valid key yet).
                            // Re-set breakpoint for the next call.
                            if set_breakpoint(h_process, breakpoint_addr, &mut original_byte) {
                                breakpoint_set = true;
                            } else {
                                breakpoint_set = false;
                            }
                        }
                    } else if code != EXCEPTION_BREAKPOINT.0 {
                        continue_status = DBG_EXCEPTION_NOT_HANDLED;
                    }
                }

                EXIT_PROCESS_DEBUG_EVENT => {
                    break;
                }

                _ => {}
            }

            let _ = ContinueDebugEvent(evt.dwProcessId, evt.dwThreadId, continue_status);
        }

        let _ = CloseHandle(h_process);
        let _ = CloseHandle(h_thread);
        extracted_key
    }
}

#[cfg(windows)]
unsafe fn get_module_base(pid: u32, module_name: &str) -> Option<u64> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Module32FirstW, Module32NextW, MODULEENTRY32W, TH32CS_SNAPMODULE,
        TH32CS_SNAPMODULE32,
    };

    let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid).ok()?;
    if snapshot.is_invalid() {
        return None;
    }

    let mut entry: MODULEENTRY32W = std::mem::zeroed();
    entry.dwSize = std::mem::size_of::<MODULEENTRY32W>() as u32;

    if Module32FirstW(snapshot, &mut entry).is_ok() {
        loop {
            let name = String::from_utf16_lossy(
                &entry.szModule[..entry.szModule.iter().position(|&c| c == 0).unwrap_or(256)],
            );
            if name.eq_ignore_ascii_case(module_name) {
                let _ = CloseHandle(snapshot);
                return Some(entry.modBaseAddr as u64);
            }
            if Module32NextW(snapshot, &mut entry).is_err() {
                break;
            }
        }
    }

    let _ = CloseHandle(snapshot);
    None
}

#[cfg(windows)]
unsafe fn set_breakpoint(h_process: HANDLE, addr: u64, original_byte: &mut u8) -> bool {
    use windows::Win32::System::Diagnostics::Debug::{
        FlushInstructionCache, ReadProcessMemory, WriteProcessMemory,
    };

    let mut buf = [0u8; 1];
    let mut bytes_read = 0usize;

    if ReadProcessMemory(
        h_process,
        addr as *const _,
        buf.as_mut_ptr() as *mut _,
        1,
        Some(&mut bytes_read),
    )
    .is_err()
        || bytes_read != 1
    {
        return false;
    }

    *original_byte = buf[0];

    let int3 = [0xCCu8];
    let mut bytes_written = 0usize;

    if WriteProcessMemory(
        h_process,
        addr as *const _,
        int3.as_ptr() as *const _,
        1,
        Some(&mut bytes_written),
    )
    .is_err()
        || bytes_written != 1
    {
        return false;
    }

    let _ = FlushInstructionCache(h_process, Some(addr as *const _), 1);
    true
}

#[cfg(windows)]
unsafe fn restore_byte(h_process: HANDLE, addr: u64, byte: u8) {
    use windows::Win32::System::Diagnostics::Debug::{FlushInstructionCache, WriteProcessMemory};

    let buf = [byte];
    let mut written = 0usize;
    let _ = WriteProcessMemory(
        h_process,
        addr as *const _,
        buf.as_ptr() as *const _,
        1,
        Some(&mut written),
    );
    let _ = FlushInstructionCache(h_process, Some(addr as *const _), 1);
}

#[cfg(windows)]
unsafe fn read_key_from_r8(_pid: u32, thread_id: u32, h_process: HANDLE) -> Option<String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::Debug::{
        GetThreadContext, ReadProcessMemory, CONTEXT, CONTEXT_ALL_AMD64,
    };
    use windows::Win32::System::Threading::{OpenThread, THREAD_ALL_ACCESS};

    let h_thread = OpenThread(THREAD_ALL_ACCESS, false, thread_id).ok()?;
    if h_thread.is_invalid() {
        return None;
    }

    let mut ctx: CONTEXT = std::mem::zeroed();
    ctx.ContextFlags = CONTEXT_ALL_AMD64;

    if GetThreadContext(h_thread, &mut ctx).is_err() {
        let _ = CloseHandle(h_thread);
        return None;
    }

    ctx.Rip -= 1;

    let r8_value = ctx.R8;
    let mut buf = [0u8; 256];
    let mut bytes_read = 0usize;

    if ReadProcessMemory(
        h_process,
        r8_value as *const _,
        buf.as_mut_ptr() as *mut _,
        256,
        Some(&mut bytes_read),
    )
    .is_err()
        || bytes_read == 0
    {
        let _ = CloseHandle(h_thread);
        return None;
    }

    let null_pos = buf.iter().position(|&b| b == 0).unwrap_or(bytes_read);
    let key = String::from_utf8_lossy(&buf[..null_pos]).to_string();

    if key.len() == 16 && key.chars().all(|c| (32..=126).contains(&(c as u32))) {
        let _ = CloseHandle(h_thread);
        return Some(key);
    }

    let _ = CloseHandle(h_thread);
    None
}

// ── Key status polling ──

#[tauri::command]
pub fn get_key_status(state: State<'_, AppState>) -> KeyStatusResponse {
    let messages: Vec<LogMessage> = {
        let mut log = state.key_log.lock().unwrap();
        log.drain(..).collect()
    };
    let done = *state.key_done.lock().unwrap();
    let ok = *state.key_ok.lock().unwrap();
    let key = state.key_result.lock().unwrap().clone();

    KeyStatusResponse {
        done,
        ok,
        key,
        messages,
    }
}

// ── Export ──

fn default_export_dir() -> String {
    #[cfg(windows)]
    {
        // Preserve the original Windows default: next to the executable.
        let mut path = std::env::current_exe().unwrap_or_default();
        path.pop();
        path.push("output");
        return path.to_string_lossy().to_string();
    }
    #[cfg(target_os = "macos")]
    {
        let base = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir());
        return base
            .join("Documents")
            .join("QQFlow")
            .join("exports")
            .to_string_lossy()
            .to_string();
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        let base = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir());
        base.join("Documents")
            .join("QQFlow")
            .join("exports")
            .to_string_lossy()
            .to_string()
    }
}

#[derive(Debug, Deserialize)]
pub struct ExportParams {
    pub db_path: String,
    pub key: String,
    pub output_dir: Option<String>,
    pub group_ids: Option<Vec<String>>,
    pub peer_ids: Option<Vec<String>>,
}

#[tauri::command]
pub fn clear_msg_store(state: State<'_, AppState>) -> SimpleResponse {
    let mut guard = state.msg_store.lock().unwrap();
    let count = guard.len();
    guard.clear();
    eprintln!("[clear_msg_store] 已清除 {} 个缓存", count);
    SimpleResponse {
        ok: true,
        error: None,
    }
}

#[tauri::command]
pub fn cancel_export(state: State<'_, AppState>) -> SimpleResponse {
    state.cancel_flag.store(true, Ordering::SeqCst);
    eprintln!("[cancel_export] 取消标志已设置");
    SimpleResponse {
        ok: true,
        error: None,
    }
}

#[tauri::command]
pub fn start_export(state: State<'_, AppState>, params: ExportParams) -> SimpleResponse {
    {
        state.export_log.lock().unwrap().clear();
        *state.export_done.lock().unwrap() = false;
        *state.export_ok.lock().unwrap() = false;
        *state.export_summary.lock().unwrap() = None;
        state.cancel_flag.store(false, Ordering::SeqCst);
    }

    let log = state.export_log.clone();
    let done = state.export_done.clone();
    let ok = state.export_ok.clone();
    let summary = state.export_summary.clone();
    let store_mutex = state.msg_store.clone();
    let cancel_flag = state.cancel_flag.clone();

    let output_dir = params.output_dir.unwrap_or_else(default_export_dir);

    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if cancel_flag.load(Ordering::SeqCst) {
                log.lock().unwrap().push(LogMessage {
                    level: "warn".to_string(),
                    text: "导出已被取消".to_string(),
                });
                return;
            }
            log.lock().unwrap().push(LogMessage {
                level: "info".to_string(),
                text: "正在解密并导出聊天记录...".to_string(),
            });

            // Use cached store if available, otherwise load from DB with progress
            let cached_store = {
                let guard = store_mutex.lock().unwrap();
                guard.get(&params.db_path).cloned()
            };
            let store = match cached_store {
                Some(s) => {
                    log.lock().unwrap().push(LogMessage {
                        level: "info".to_string(),
                        text: "使用已缓存的消息存储".to_string(),
                    });
                    s
                }
                None => {
                    log.lock().unwrap().push(LogMessage {
                        level: "info".to_string(),
                        text: "正在加载数据库（首次约需30-60秒，请耐心等待）...".to_string(),
                    });
                    eprintln!("[start_export] 开始加载 MessageStore: {}", &params.db_path);
                    match export_chat::MessageStore::load_with_progress(
                        &params.db_path,
                        &params.key,
                        Some(&log),
                    ) {
                        Ok(s) => {
                            let s = Arc::new(s);
                            log.lock().unwrap().push(LogMessage {
                                level: "info".to_string(),
                                text: format!(
                                    "加载完成: {} 个群, {} 个私聊",
                                    s.group_msgs.len(),
                                    s.c2c_msgs.len()
                                ),
                            });
                            let mut guard = store_mutex.lock().unwrap();
                            if let Some(existing) = guard.get(&params.db_path) {
                                existing.clone()
                            } else {
                                guard.insert(params.db_path.clone(), s.clone());
                                s
                            }
                        }
                        Err(e) => {
                            log.lock().unwrap().push(LogMessage {
                                level: "error".to_string(),
                                text: e,
                            });
                            return;
                        }
                    }
                }
            };
            let store_ref = Some(store.as_ref());

            match export_chat::decrypt_and_export(
                &params.db_path,
                &params.key,
                &output_dir,
                params.group_ids.as_deref(),
                params.peer_ids.as_deref(),
                store_ref,
                Some(&log),
                Some(&cancel_flag),
            ) {
                Ok(result) => {
                    log.lock().unwrap().push(LogMessage {
                        level: "success".to_string(),
                        text: format!(
                            "导出完成: {} 条消息, {} 个群聊, {} 个私聊",
                            result.total, result.groups, result.private
                        ),
                    });
                    *summary.lock().unwrap() = Some(ExportSummary {
                        total: result.total,
                        groups: result.groups,
                        private_count: result.private,
                        dir: output_dir,
                    });
                    *ok.lock().unwrap() = true;
                }
                Err(e) => {
                    log.lock().unwrap().push(LogMessage {
                        level: "error".to_string(),
                        text: e,
                    });
                }
            }
        }));

        if let Err(_panic) = result {
            log.lock().unwrap().push(LogMessage {
                level: "error".to_string(),
                text: "导出过程发生内部错误".to_string(),
            });
        }
        *done.lock().unwrap() = true;
    });

    SimpleResponse {
        ok: true,
        error: None,
    }
}

#[tauri::command]
pub fn get_export_status(state: State<'_, AppState>) -> ExportStatusResponse {
    let messages: Vec<LogMessage> = {
        let mut log = state.export_log.lock().unwrap();
        log.drain(..).collect()
    };
    let done = *state.export_done.lock().unwrap();
    let ok = *state.export_ok.lock().unwrap();
    let summary = state.export_summary.lock().unwrap().clone();

    ExportStatusResponse {
        done,
        ok,
        messages,
        summary,
    }
}

// ── Analysis progress ──

#[derive(Debug, Serialize)]
pub struct ProgressResponse {
    pub done: bool,
    pub messages: Vec<LogMessage>,
}

#[tauri::command]
pub fn get_analysis_progress(state: State<'_, AppState>) -> ProgressResponse {
    let messages: Vec<LogMessage> = {
        let mut log = state.analysis_log.lock().unwrap();
        log.drain(..).collect()
    };
    let done = *state.analysis_done.lock().unwrap();
    ProgressResponse { done, messages }
}

#[tauri::command]
pub fn get_csv_progress(state: State<'_, AppState>) -> ProgressResponse {
    let messages: Vec<LogMessage> = {
        let mut log = state.csv_progress.lock().unwrap();
        log.drain(..).collect()
    };
    let done = *state.csv_done.lock().unwrap();
    ProgressResponse { done, messages }
}

// ── CSV Export ──

#[tauri::command]
pub async fn export_csv(app: tauri::AppHandle, params: ExportParams) -> serde_json::Value {
    let output_dir = params.output_dir.unwrap_or_else(default_export_dir);
    let dir = output_dir.clone();
    let store_mutex = app.state::<AppState>().msg_store.clone();

    let result = tokio::task::spawn_blocking(move || {
        let store = match load_store_if_needed(&store_mutex, &params.db_path, &params.key) {
            Ok(s) => Some(s),
            Err(e) => return Err(e),
        };
        analysis::export_csv(
            &params.db_path,
            &params.key,
            &output_dir,
            params.group_ids.as_deref(),
            params.peer_ids.as_deref(),
            store.as_deref(),
            None,
            None,
        )
    })
    .await;

    match result {
        Ok(Ok(data)) => serde_json::json!({
            "ok": true, "groups": data.groups, "private": data.private, "total": data.total, "dir": dir,
        }),
        Ok(Err(e)) => serde_json::json!({ "ok": false, "error": e }),
        Err(e) => serde_json::json!({ "ok": false, "error": format!("{}", e) }),
    }
}

// ── Analysis ──

#[derive(Debug, Deserialize)]
pub struct GroupAnalysisParams {
    pub db_path: String,
    pub key: String,
    pub group_id: Option<String>,
}

#[tauri::command]
pub async fn analyze_group(
    app: tauri::AppHandle,
    params: GroupAnalysisParams,
) -> serde_json::Value {
    let store_mutex = app.state::<AppState>().msg_store.clone();

    let result = tokio::task::spawn_blocking(move || {
        match params.group_id {
            // Listing: use SQL GROUP BY (no BLOBs, fast even for 190MB DB)
            None => analysis::list_groups(&params.db_path, &params.key, None)
                .map(|data| serde_json::json!({ "ok": true, "data": data }))
                .unwrap_or_else(|e| serde_json::json!({ "ok": false, "error": e })),
            // Detail: load store if not cached, then analyze from memory
            Some(ref gid) => {
                let store = {
                    let guard = store_mutex.lock().unwrap();
                    match guard.get(&params.db_path) {
                        Some(s) => {
                            eprintln!("[analyze_group] 缓存命中");
                            s.clone()
                        }
                        None => {
                            drop(guard);
                            eprintln!("[analyze_group] 缓存未命中, 加载 MessageStore...");
                            match export_chat::MessageStore::load_with_progress(
                                &params.db_path,
                                &params.key,
                                None,
                            ) {
                                Ok(s) => {
                                    let s = Arc::new(s);
                                    let mut g = store_mutex.lock().unwrap();
                                    if let Some(existing) = g.get(&params.db_path) {
                                        existing.clone()
                                    } else {
                                        g.insert(params.db_path.clone(), s.clone());
                                        s
                                    }
                                }
                                Err(e) => return serde_json::json!({ "ok": false, "error": e }),
                            }
                        }
                    }
                };
                analysis::analyze_group_detail(
                    &params.db_path,
                    &params.key,
                    gid,
                    Some(store.as_ref()),
                    None,
                )
                .map(|data| serde_json::json!({ "ok": true, "data": data }))
                .unwrap_or_else(|e| serde_json::json!({ "ok": false, "error": e }))
            }
        }
    })
    .await;

    match result {
        Ok(val) => val,
        Err(e) => serde_json::json!({ "ok": false, "error": format!("任务执行失败: {}", e) }),
    }
}

#[derive(Debug, Deserialize)]
pub struct PrivateAnalysisParams {
    pub db_path: String,
    pub key: String,
    pub peer_id: Option<String>,
}

#[tauri::command]
pub async fn analyze_private(
    app: tauri::AppHandle,
    params: PrivateAnalysisParams,
) -> serde_json::Value {
    let store_mutex = app.state::<AppState>().msg_store.clone();

    let result = tokio::task::spawn_blocking(move || {
        match params.peer_id {
            // Listing: use SQL GROUP BY (no BLOBs, fast)
            None => analysis::list_contacts(&params.db_path, &params.key, None)
                .map(|data| serde_json::json!({ "ok": true, "data": data }))
                .unwrap_or_else(|e| serde_json::json!({ "ok": false, "error": e })),
            // Detail: load store if not cached, then analyze from memory
            Some(ref pid) => {
                let store = {
                    let guard = store_mutex.lock().unwrap();
                    match guard.get(&params.db_path) {
                        Some(s) => {
                            eprintln!("[analyze_private] 缓存命中");
                            s.clone()
                        }
                        None => {
                            drop(guard);
                            eprintln!("[analyze_private] 缓存未命中, 加载 MessageStore...");
                            match export_chat::MessageStore::load_with_progress(
                                &params.db_path,
                                &params.key,
                                None,
                            ) {
                                Ok(s) => {
                                    let s = Arc::new(s);
                                    let mut g = store_mutex.lock().unwrap();
                                    if let Some(existing) = g.get(&params.db_path) {
                                        existing.clone()
                                    } else {
                                        g.insert(params.db_path.clone(), s.clone());
                                        s
                                    }
                                }
                                Err(e) => return serde_json::json!({ "ok": false, "error": e }),
                            }
                        }
                    }
                };
                analysis::analyze_private_detail(
                    &params.db_path,
                    &params.key,
                    pid,
                    Some(store.as_ref()),
                    None,
                )
                .map(|data| serde_json::json!({ "ok": true, "data": data }))
                .unwrap_or_else(|e| serde_json::json!({ "ok": false, "error": e }))
            }
        }
    })
    .await;

    match result {
        Ok(val) => val,
        Err(e) => serde_json::json!({ "ok": false, "error": format!("任务执行失败: {}", e) }),
    }
}

fn load_store_if_needed(
    mtx: &Arc<Mutex<std::collections::HashMap<String, Arc<export_chat::MessageStore>>>>,
    db_path: &str,
    key: &str,
) -> Result<Arc<export_chat::MessageStore>, String> {
    // Check cache first — keyed by db_path so different QQ accounts get different stores
    {
        let guard = mtx.lock().unwrap();
        if let Some(s) = guard.get(db_path) {
            eprintln!(
                "[load_store_if_needed] 缓存命中 (db_path={})，直接返回",
                db_path
            );
            return Ok(s.clone());
        }
    } // Lock released here before expensive operations

    // Load from database (potentially slow)
    eprintln!(
        "[load_store_if_needed] db_path={} 缓存未命中，开始加载数据库...",
        db_path
    );
    let conn = export_chat::open_db_for_analysis(db_path, key)
        .map_err(|e| format!("打开数据库失败: {}", e))?;
    eprintln!("[load_store_if_needed] 数据库已打开，开始加载消息...");
    let mut loaded =
        export_chat::MessageStore::load(&conn).map_err(|e| format!("加载消息失败: {}", e))?;
    loaded.profile_names = export_chat::load_profile_names(db_path, key);
    let store = Arc::new(loaded);
    eprintln!(
        "[load_store_if_needed] 消息加载完成: {} 个群, {} 个私聊",
        store.group_msgs.len(),
        store.c2c_msgs.len()
    );

    // Store in cache keyed by db_path
    let mut guard = mtx.lock().unwrap();
    // Double-check: another thread may have loaded while we were loading
    if let Some(s) = guard.get(db_path) {
        eprintln!("[load_store_if_needed] 其他线程已加载，使用已有缓存");
        return Ok(s.clone());
    }
    guard.insert(db_path.to_string(), store.clone());
    Ok(store)
}

#[tauri::command]
pub async fn debug_db_schema(params: PrivateAnalysisParams) -> serde_json::Value {
    let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let conn = export_chat::open_db_for_analysis(&params.db_path, &params.key)?;

        // List all tables with columns
        let tables = export_chat::list_tables(&conn);

        // Row counts for key tables
        let mut table_counts = serde_json::Map::new();
        for (name, _) in &tables {
            let count: i64 = conn
                .query_row(
                    &format!("SELECT count(*) FROM \"{}\"", name.replace('"', "\"\"")),
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            table_counts.insert(name.clone(), serde_json::json!(count));
        }

        // Sample from UID mapping table
        let uid_map = export_chat::load_uid_map(&conn);
        let uid_sample: Vec<(String, String)> = uid_map
            .iter()
            .take(5)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        // Sample from c2c_msg_table
        let c2c_sample: Vec<serde_json::Value> = {
            let mut stmt = conn
                .prepare("SELECT \"40001\", \"40020\", \"40093\" FROM c2c_msg_table LIMIT 3")
                .map_err(|e| e.to_string())?;
            let rows: Vec<serde_json::Value> = stmt
                .query_map([], |row| {
                    Ok(serde_json::json!({
                        "40001": row.get::<_, i64>(0).unwrap_or(0),
                        "40020": row.get::<_, String>(1).unwrap_or_default(),
                        "40093": row.get::<_, String>(2).unwrap_or_default(),
                    }))
                })
                .map_err(|e| e.to_string())?
                .flatten()
                .collect();
            rows
        };

        Ok(serde_json::json!({
            "tables": tables,
            "row_counts": table_counts,
            "uid_map_sample": uid_sample,
            "uid_map_size": uid_map.len(),
            "c2c_sample": c2c_sample,
        }))
    })
    .await;

    match result {
        Ok(Ok(data)) => serde_json::json!({ "ok": true, "data": data }),
        Ok(Err(e)) => serde_json::json!({ "ok": false, "error": e }),
        Err(e) => serde_json::json!({ "ok": false, "error": format!("{}", e) }),
    }
}

// ── Key persistence ──

const XOR_KEY: &[u8] = b"QQFlow2024!@#$%^";

fn keys_file_path() -> std::path::PathBuf {
    #[cfg(windows)]
    {
        let app_data = std::env::var("APPDATA").unwrap_or_default();
        return std::path::PathBuf::from(app_data)
            .join("qqflow")
            .join("qqflow_keys.json");
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").unwrap_or_default();
        return std::path::PathBuf::from(home)
            .join("Library/Application Support/QQFlow/qqflow_keys.json");
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        let base = std::env::var("XDG_CONFIG_HOME")
            .ok()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config")
            });
        base.join("qqflow/qqflow_keys.json")
    }
}

fn load_keys_map() -> std::collections::HashMap<String, String> {
    let path = keys_file_path();
    let data = match std::fs::read_to_string(&path) {
        Ok(d) => d,
        Err(_) => return std::collections::HashMap::new(),
    };
    serde_json::from_str(&data).unwrap_or_default()
}

fn save_keys_map(map: &std::collections::HashMap<String, String>) -> Result<(), String> {
    let path = keys_file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(map).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

fn obfuscate_key(key: &str) -> String {
    let encoded: Vec<u8> = key
        .bytes()
        .enumerate()
        .map(|(i, b)| b ^ XOR_KEY[i % XOR_KEY.len()])
        .collect();
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &encoded)
}

fn deobfuscate_key(encoded_b64: &str) -> Option<String> {
    let obfuscated =
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded_b64).ok()?;
    let decoded: Vec<u8> = obfuscated
        .iter()
        .enumerate()
        .map(|(i, &b)| b ^ XOR_KEY[i % XOR_KEY.len()])
        .collect();
    let key = String::from_utf8_lossy(&decoded).to_string();
    let valid = if cfg!(windows) {
        key.len() == 16
    } else {
        (8..=128).contains(&key.len())
    };
    if valid {
        Some(key)
    } else {
        None
    }
}

#[tauri::command]
pub fn save_key(key: String, qq_number: String) -> SimpleResponse {
    let mut map = load_keys_map();
    map.insert(qq_number, obfuscate_key(&key));
    match save_keys_map(&map) {
        Ok(_) => SimpleResponse {
            ok: true,
            error: None,
        },
        Err(e) => SimpleResponse {
            ok: false,
            error: Some(e),
        },
    }
}

#[tauri::command]
pub fn load_keys() -> serde_json::Value {
    let map = load_keys_map();
    let mut result = serde_json::Map::new();
    for (qq, encoded) in &map {
        if let Some(key) = deobfuscate_key(encoded) {
            result.insert(qq.clone(), serde_json::json!(key));
        }
    }
    serde_json::json!({ "ok": true, "keys": result })
}

#[tauri::command]
pub fn clear_key(qq_number: String) -> SimpleResponse {
    let mut map = load_keys_map();
    map.remove(&qq_number);
    match save_keys_map(&map) {
        Ok(_) => SimpleResponse {
            ok: true,
            error: None,
        },
        Err(e) => SimpleResponse {
            ok: false,
            error: Some(e),
        },
    }
}

// ── Helper functions ──

fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    let b = data.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([b[0], b[1]]))
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    let b = data.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn read_i32(data: &[u8], offset: usize) -> Option<i32> {
    let b = data.get(offset..offset + 4)?;
    Some(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn find_pattern(data: &[u8], pattern: &[u8], start: usize, end: usize) -> Option<usize> {
    let end = end.min(data.len());
    if start + pattern.len() > end {
        return None;
    }
    for i in start..=(end - pattern.len()) {
        if &data[i..i + pattern.len()] == pattern {
            return Some(i);
        }
    }
    None
}
