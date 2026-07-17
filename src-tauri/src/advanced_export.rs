/// Advanced export & report module for QQFlow
/// ============================================
/// Ported & adapted from WeFlow concepts, designed for QQ NT database.
///
/// Features:
///   1. HTML export — full WeFlow-style chat bubbles with CSS, media labels
///   2. JSON export — ChatLab-compatible structured data
///   3. Duo report — deep two-person conversation analysis
///   4. Annual report — yearly statistics with heatmaps & streaks

use crate::analysis::normalize_ts;
use crate::export_chat::{MessageStore, sanitize_filename};
use crate::message_parser::extract_text;
use chrono::{Datelike, NaiveDateTime, Timelike, Weekday};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::Path;

// ══════════════════════════════════════════════════════════════════════
// SHARED HELPERS
// ══════════════════════════════════════════════════════════════════════

fn ts_to_datetime(ts: i64) -> Option<NaiveDateTime> {
    let secs = normalize_ts(ts);
    if secs == 0 { return None; }
    NaiveDateTime::from_timestamp_opt(secs, 0)
}

fn ts_to_str(ts: i64) -> String {
    ts_to_datetime(ts)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default()
}

fn ts_to_date_cn(ts: i64) -> String {
    ts_to_datetime(ts)
        .map(|dt| {
            let wd = ["周日","周一","周二","周三","周四","周五","周六"];
            format!("{} {}", dt.format("%Y-%m-%d"), wd[dt.weekday().num_days_from_sunday() as usize])
        })
        .unwrap_or_default()
}

fn html_esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('\n', "<br>")
}

fn avatar_of(name: &str) -> String {
    name.chars().next().map(|c| c.to_string()).unwrap_or_else(|| "?".to_string())
}

/// Count messages in store for a given year
fn count_year_messages(store: &MessageStore, year: i32) -> usize {
    let mut count = 0usize;
    for msgs in store.group_msgs.values() {
        for m in msgs {
            if let Some(dt) = ts_to_datetime(m.msg_id) {
                if dt.year() == year { count += 1; }
            }
        }
    }
    for msgs in store.c2c_msgs.values() {
        for m in msgs {
            if let Some(dt) = ts_to_datetime(m.msg_id) {
                if dt.year() == year { count += 1; }
            }
        }
    }
    count
}

// ══════════════════════════════════════════════════════════════════════
// 1. HTML EXPORT
// ══════════════════════════════════════════════════════════════════════

const HTML_FULL_CSS: &str = r#"<style>
:root{--bg:#f6f7fb;--card:#fff;--text:#1f2a37;--muted:#6b7280;--accent:#4f46e5;--sent:#dbeafe;--received:#fff;--border:#e5e7eb;--radius:16px;color-scheme:light}
*{box-sizing:border-box;margin:0;padding:0}
body{font-family:"PingFang SC","Microsoft YaHei",system-ui,-apple-system,sans-serif;background:var(--bg);color:var(--text)}
.page{max-width:1080px;margin:0 auto;padding:8px 20px;display:flex;flex-direction:column}
.header{background:var(--card);border-radius:12px;box-shadow:0 2px 8px rgba(15,23,42,0.06);padding:12px 20px;flex-shrink:0;margin-bottom:12px}
.header h1{font-size:18px;font-weight:600;margin:0 0 4px}
.header .meta{font-size:13px;color:var(--muted)}
.header .meta span{margin-right:12px}
.scroll-container{flex:1;min-height:0;overflow-y:auto;border:1px solid var(--border);border-radius:var(--radius);background:var(--bg);padding:12px;-webkit-overflow-scrolling:touch}
.scroll-container::-webkit-scrollbar{width:6px}
.scroll-container::-webkit-scrollbar-thumb{background:#c1c1c1;border-radius:3px}
.msg{display:flex;flex-direction:column;gap:6px;margin-bottom:12px}
.msg.hidden{display:none}
.msg-row{display:flex;gap:12px;align-items:flex-end}
.msg.sent .msg-row{flex-direction:row-reverse}
.avatar{width:40px;height:40px;border-radius:12px;background:#eef2ff;display:flex;align-items:center;justify-content:center;flex-shrink:0;color:#475569;font-weight:600;font-size:14px}
.msg.sent .avatar{background:#07c160;color:#fff}
.bubble{max-width:min(70%,720px);background:var(--received);border-radius:18px;padding:12px 14px;border:1px solid var(--border);box-shadow:0 8px 20px rgba(15,23,42,0.06)}
.msg.sent .bubble{background:var(--sent);border-color:transparent}
.sender-name{font-size:12px;color:var(--muted);margin-bottom:6px}
.msg.sent .sender-name{text-align:right}
.msg-content{font-size:14px;line-height:1.6;word-break:break-word}
.msg-time{font-size:11px;color:var(--muted);text-align:right;margin-top:4px}
.date-sep{text-align:center;margin:18px 0}
.date-sep span{background:var(--border);color:var(--muted);font-size:12px;padding:4px 14px;border-radius:10px}
.system-msg{text-align:center;color:var(--muted);font-size:12px;margin:14px 0}
.media-label{display:inline-block;background:#f1f5f9;border-radius:8px;padding:6px 12px;font-size:13px;color:#64748b}
.footer{text-align:center;font-size:12px;color:var(--muted);margin-top:30px;padding:20px;border-top:1px solid var(--border)}
.stat-box{display:inline-block;background:#eef2ff;border-radius:6px;padding:1px 8px;font-size:11px;margin-right:4px;font-weight:500}
</style>"#;

fn html_message_label(msg_type: &str, content: &str) -> String {
    match msg_type {
        "image" => format!(r#"<div class="media-label">🖼 图片</div>"#),
        "voice" => format!(r#"<div class="media-label">🎤 语音</div>"#),
        "video" => format!(r#"<div class="media-label">🎬 视频</div>"#),
        "miniapp" => format!(r#"<div class="media-label">📦 {}</div>"#, html_esc(content)),
        "recall" => r#"<div class="media-label">↩ 撤回了一条消息</div>"#.to_string(),
        "system" => format!(r#"<div class="system-msg">{}</div>"#, html_esc(content)),
        _ => format!(r#"<div class="msg-content">{}</div>"#, html_esc(content)),
    }
}

fn build_html_messages_c2c(store: &MessageStore, peer_uid: &str, my_name: &str, peer_name: &str) -> String {
    let msgs = match store.c2c_msgs.get(peer_uid) { Some(m) => m, None => return String::new() };
    let mut sorted: Vec<_> = msgs.iter().collect();
    sorted.sort_by_key(|m| m.msg_id);

    let mut html = String::from(r#"<div class="scroll-container"><div class="message-list">"#);
    let mut last_date = String::new();

    for m in &sorted {
        let date = ts_to_date_cn(m.msg_id);
        if date != last_date {
            last_date = date.clone();
            html.push_str(&format!(r#"<div class="date-sep"><span>{}</span></div>"#, date));
        }

        let parsed = extract_text(&m.blob);
        if parsed.msg_type == "system" {
            html.push_str(&format!(r#"<div class="system-msg">{}</div>"#, html_esc(&parsed.content)));
            continue;
        }

        let is_self = m.is_self;
        let css = if is_self { "msg sent" } else { "msg" };
        let sender = if is_self { my_name } else { peer_name };
        let avatar = avatar_of(sender);

        html.push_str(&format!(
            r#"<div class="{}"><div class="msg-row"><div class="avatar">{}</div><div class="bubble"><div class="sender-name">{}</div>{}</div></div><div class="msg-time">{}</div></div>"#,
            css, avatar, html_esc(sender), html_message_label(&parsed.msg_type, &parsed.content), ts_to_str(m.msg_id)
        ));
    }
    html.push_str("</div></div>");
    html
}

fn build_html_messages_group(store: &MessageStore, group_id: &str) -> String {
    let msgs = match store.group_msgs.get(group_id) { Some(m) => m, None => return String::new() };
    let mut sorted: Vec<_> = msgs.iter().collect();
    sorted.sort_by_key(|m| m.msg_id);

    let self_uid = &store.self_uid;
    let mut html = String::from(r#"<div class="scroll-container"><div class="message-list">"#);
    let mut last_date = String::new();

    for m in &sorted {
        let date = ts_to_date_cn(m.msg_id);
        if date != last_date {
            last_date = date.clone();
            html.push_str(&format!(r#"<div class="date-sep"><span>{}</span></div>"#, date));
        }

        let parsed = extract_text(&m.blob);
        if parsed.msg_type == "system" {
            html.push_str(&format!(r#"<div class="system-msg">{}</div>"#, html_esc(&parsed.content)));
            continue;
        }

        let is_self = &m.uid == self_uid;
        let css = if is_self { "msg sent" } else { "msg" };
        let sender = if !m.nick.is_empty() { &m.nick }
            else { store.uid_map.get(&m.uid).map(|s| s.as_str()).unwrap_or(&m.uid) };
        let avatar = avatar_of(sender);

        html.push_str(&format!(
            r#"<div class="{}"><div class="msg-row"><div class="avatar">{}</div><div class="bubble"><div class="sender-name">{}</div>{}</div></div><div class="msg-time">{}</div></div>"#,
            css, avatar, html_esc(sender), html_message_label(&parsed.msg_type, &parsed.content), ts_to_str(m.msg_id)
        ));
    }
    html.push_str("</div></div>");
    html
}

pub fn export_html_c2c(
    store: &MessageStore, peer_uid: &str, output_dir: &str,
) -> Result<String, String> {
    let msgs = store.c2c_msgs.get(peer_uid)
        .ok_or_else(|| format!("未找到联系人: {}", peer_uid))?;

    let qq = store.uid_map.get(peer_uid).cloned().unwrap_or_else(|| peer_uid.to_string());
    let nick = msgs.iter().find(|m| !m.nick.is_empty()).map(|m| m.nick.as_str()).unwrap_or(&qq);
    let label = format!("{}_{}", qq, sanitize_filename(nick));
    let title = format!("与 {} 的聊天记录", label);

    let mut sorted: Vec<_> = msgs.iter().collect();
    sorted.sort_by_key(|m| m.msg_id);
    let total = sorted.len();
    let first_str = sorted.first().map(|m| ts_to_str(m.msg_id)).unwrap_or_default();
    let last_str = sorted.last().map(|m| ts_to_str(m.msg_id)).unwrap_or_default();

    let my_name = &qq;
    let peer_name = nick;

    let body = build_html_messages_c2c(store, peer_uid, my_name, peer_name);
    let full = format!(
        r#"<!DOCTYPE html><html><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{title}</title>{HTML_FULL_CSS}</head><body><div class="page"><div class="header"><h1>{title}</h1><div class="meta"><span>📊 {total} 条消息</span><span>📅 {first_str} ~ {last_str}</span></div></div>{body}<div class="footer">由 QQFlow 生成 · {total} 条消息 · 导出时间: {now}</div></div></body></html>"#,
        title = html_esc(&title),
        total = total,
        first_str = first_str,
        last_str = last_str,
        body = body,
        now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
    );

    let html_path = Path::new(output_dir).join(format!("{}.html", sanitize_filename(&label)));
    fs::write(&html_path, &full).map_err(|e| format!("写入HTML失败: {}", e))?;
    Ok(html_path.to_string_lossy().to_string())
}

pub fn export_html_group(
    store: &MessageStore, group_id: &str, output_dir: &str,
) -> Result<String, String> {
    let msgs = store.group_msgs.get(group_id)
        .ok_or_else(|| format!("未找到群聊: {}", group_id))?;

    let group_name = store.group_names.get(group_id).cloned().unwrap_or_else(|| group_id.to_string());
    let label = format!("群_{}", sanitize_filename(&group_name));

    let mut sorted: Vec<_> = msgs.iter().collect();
    sorted.sort_by_key(|m| m.msg_id);
    let total = sorted.len();
    let first_str = sorted.first().map(|m| ts_to_str(m.msg_id)).unwrap_or_default();
    let last_str = sorted.last().map(|m| ts_to_str(m.msg_id)).unwrap_or_default();

    let body = build_html_messages_group(store, group_id);
    let full = format!(
        r#"<!DOCTYPE html><html><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{title}</title>{HTML_FULL_CSS}</head><body><div class="page"><div class="header"><h1>{title}</h1><div class="meta"><span>📊 {total} 条消息</span><span>📅 {first_str} ~ {last_str}</span></div></div>{body}<div class="footer">由 QQFlow 生成 · {total} 条消息 · 导出时间: {now}</div></div></body></html>"#,
        title = html_esc(&group_name),
        total = total,
        first_str = first_str,
        last_str = last_str,
        body = body,
        now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
    );

    let html_path = Path::new(output_dir).join(format!("{}.html", label));
    fs::write(&html_path, &full).map_err(|e| format!("写入HTML失败: {}", e))?;
    Ok(html_path.to_string_lossy().to_string())
}

// ══════════════════════════════════════════════════════════════════════
// 2. JSON EXPORT — ChatLab-compatible
// ══════════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonHeader { version: String, exported_at: i64, generator: String }

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonMeta { name: String, platform: String, r#type: String, group_id: Option<String> }

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonMember { platform_id: String, account_name: String, group_nickname: Option<String> }

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonMessage {
    sender: String, account_name: String, group_nickname: Option<String>,
    timestamp: i64, r#type: i32, content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] platform_message_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonDoc { header: JsonHeader, meta: JsonMeta, members: Vec<JsonMember>, messages: Vec<JsonMessage> }

fn json_msg_type(t: &str) -> i32 {
    match t { "text"=>0, "image"=>1, "voice"=>2, "video"=>3, "system"=>4, "miniapp"=>5, "recall"=>6, _=>99 }
}

fn build_json_c2c(store: &MessageStore, peer_uid: &str) -> Result<JsonDoc, String> {
    let msgs = store.c2c_msgs.get(peer_uid).ok_or("未找到联系人")?;
    let qq = store.uid_map.get(peer_uid).cloned().unwrap_or_else(|| peer_uid.to_string());
    let nick = msgs.iter().find(|m| !m.nick.is_empty()).map(|m| m.nick.as_str()).unwrap_or(&qq);

    let mut sorted: Vec<_> = msgs.iter().collect();
    sorted.sort_by_key(|m| m.msg_id);

    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
    let mut mset: HashMap<String, JsonMember> = HashMap::new();
    let mut jmsgs = Vec::with_capacity(sorted.len());

    let my_uid = &store.self_uid;
    let my_name = qq.clone();

    mset.insert(my_uid.clone(), JsonMember { platform_id: my_uid.clone(), account_name: my_name.clone(), group_nickname: None });

    for m in &sorted {
        let is_self = m.is_self;
        let sid = if is_self { my_uid.as_str() } else { m.peer.as_str() };
        let sname = if is_self { &my_name } else if !m.nick.is_empty() { &m.nick } else { &qq };
        if !mset.contains_key(sid) {
            mset.insert(sid.to_string(), JsonMember { platform_id: sid.to_string(), account_name: sname.to_string(), group_nickname: None });
        }
        let parsed = extract_text(&m.blob);
        jmsgs.push(JsonMessage {
            sender: sid.to_string(), account_name: sname.to_string(), group_nickname: None,
            timestamp: normalize_ts(m.msg_id), r#type: json_msg_type(&parsed.msg_type),
            content: Some(parsed.content), platform_message_id: Some(m.msg_id.to_string()),
        });
    }

    Ok(JsonDoc {
        header: JsonHeader { version: "1.0".into(), exported_at: now, generator: "QQFlow".into() },
        meta: JsonMeta { name: format!("与 {} 的私聊", nick), platform: "QQ".into(), r#type: "private".into(), group_id: None },
        members: mset.into_values().collect(), messages: jmsgs,
    })
}

fn build_json_group(store: &MessageStore, group_id: &str) -> Result<JsonDoc, String> {
    let msgs = store.group_msgs.get(group_id).ok_or("未找到群聊")?;
    let name = store.group_names.get(group_id).cloned().unwrap_or_else(|| group_id.to_string());

    let mut sorted: Vec<_> = msgs.iter().collect();
    sorted.sort_by_key(|m| m.msg_id);

    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
    let mut mset: HashMap<String, JsonMember> = HashMap::new();
    let mut jmsgs = Vec::with_capacity(sorted.len());

    for m in &sorted {
        let sname = if !m.nick.is_empty() { m.nick.clone() } else { store.uid_map.get(&m.uid).cloned().unwrap_or_else(|| m.uid.clone()) };
        if !mset.contains_key(&m.uid) {
            mset.insert(m.uid.clone(), JsonMember { platform_id: m.uid.clone(), account_name: sname.clone(), group_nickname: if !m.nick.is_empty() { Some(m.nick.clone()) } else { None } });
        }
        let parsed = extract_text(&m.blob);
        jmsgs.push(JsonMessage {
            sender: m.uid.clone(), account_name: sname, group_nickname: if !m.nick.is_empty() { Some(m.nick.clone()) } else { None },
            timestamp: normalize_ts(m.msg_id), r#type: json_msg_type(&parsed.msg_type),
            content: Some(parsed.content), platform_message_id: Some(m.msg_id.to_string()),
        });
    }

    Ok(JsonDoc {
        header: JsonHeader { version: "1.0".into(), exported_at: now, generator: "QQFlow".into() },
        meta: JsonMeta { name, platform: "QQ".into(), r#type: "group".into(), group_id: Some(group_id.to_string()) },
        members: mset.into_values().collect(), messages: jmsgs,
    })
}

pub fn export_json_c2c(store: &MessageStore, peer_uid: &str, output_dir: &str) -> Result<String, String> {
    let doc = build_json_c2c(store, peer_uid)?;
    let qq = store.uid_map.get(peer_uid).cloned().unwrap_or_else(|| peer_uid.to_string());
    let msgs = store.c2c_msgs.get(peer_uid).unwrap();
    let nick = msgs.iter().find(|m| !m.nick.is_empty()).map(|m| m.nick.as_str()).unwrap_or(&qq);
    let path = Path::new(output_dir).join(format!("{}_{}.json", qq, sanitize_filename(nick)));
    let j = serde_json::to_string_pretty(&doc).map_err(|e| format!("JSON序列化失败: {}", e))?;
    fs::write(&path, &j).map_err(|e| format!("写入JSON失败: {}", e))?;
    Ok(path.to_string_lossy().to_string())
}

pub fn export_json_group(store: &MessageStore, group_id: &str, output_dir: &str) -> Result<String, String> {
    let doc = build_json_group(store, group_id)?;
    let name = store.group_names.get(group_id).cloned().unwrap_or_else(|| group_id.to_string());
    let path = Path::new(output_dir).join(format!("群_{}.json", sanitize_filename(&name)));
    let j = serde_json::to_string_pretty(&doc).map_err(|e| format!("JSON序列化失败: {}", e))?;
    fs::write(&path, &j).map_err(|e| format!("写入JSON失败: {}", e))?;
    Ok(path.to_string_lossy().to_string())
}

// ══════════════════════════════════════════════════════════════════════
// 3. DUO REPORT
// ══════════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuoReport {
    // 基础信息
    pub peer_name: String, pub peer_qq: String, pub self_name: String,
    pub total_messages: usize, pub my_messages: usize, pub peer_messages: usize,
    pub first_message: String, pub last_message: String, pub chat_age_days: i64,
    // 活跃度
    pub active_days: usize, pub most_active_hour: i32, pub most_active_weekday: String,
    pub monthly_trend: Vec<(String, usize)>, pub hourly_heatmap: Vec<i32>,
    // 内容分析
    pub total_words: usize, pub avg_words_per_msg: f64,
    pub longest_message: String, pub top_phrases: Vec<(String, usize)>,
    pub my_exclusive_phrases: Vec<(String, usize)>, pub peer_exclusive_phrases: Vec<(String, usize)>,
    // 互动模式
    pub initiative: (usize, usize),       // (我发起, 对方发起)
    pub response_avg_hours: f64, pub response_fastest_hours: f64,
    pub streak_days: usize, pub streak_start: String, pub streak_end: String,
    // 媒体统计
    pub type_breakdown: HashMap<String, usize>,
    pub my_images: usize, pub peer_images: usize,
    pub my_voices: usize, pub peer_voices: usize,
}

pub fn generate_duo_report(store: &MessageStore, peer_uid: &str) -> Result<DuoReport, String> {
    let msgs = store.c2c_msgs.get(peer_uid).ok_or("未找到联系人")?;
    if msgs.is_empty() { return Err("无消息数据".to_string()); }

    let qq = store.uid_map.get(peer_uid).cloned().unwrap_or_else(|| peer_uid.to_string());
    let nick = msgs.iter().find(|m| !m.nick.is_empty()).map(|m| m.nick.as_str()).unwrap_or(&qq);

    let mut sorted: Vec<_> = msgs.iter().collect();
    sorted.sort_by_key(|m| m.msg_id);

    // --- Accumulators ---
    let (mut my_cnt, mut peer_cnt, mut my_words, mut peer_words) = (0usize, 0usize, 0usize, 0usize);
    let (mut my_img, mut peer_img) = (0usize, 0usize);
    let (mut my_vc, mut peer_vc) = (0usize, 0usize);
    let mut hourly = [0i32; 24];
    let mut wday = [0usize; 7];
    let mut monthly: HashMap<String, usize> = HashMap::new();
    let mut types: HashMap<String, usize> = HashMap::new();
    let mut phrase_all: HashMap<String, usize> = HashMap::new();
    let mut phrase_my: HashMap<String, usize> = HashMap::new();
    let mut phrase_peer: HashMap<String, usize> = HashMap::new();
    let mut daily: HashSet<String> = HashSet::new();
    let mut longest = String::new();
    let mut longest_len = 0usize;

    // Initiative: 谁先说话（每次对方发言后自己发言 = 自己回复, 反之对方回复）
    let (mut my_init, mut peer_init) = (0usize, 0usize);
    let mut last_speaker_was_me: Option<bool> = None;
    let mut reply_gaps: Vec<f64> = Vec::new();
    let mut last_msg_ts: Option<i64> = None;
    let mut last_msg_was_me = false;

    // Streak tracking
    let mut streak_max = 0usize;
    let mut streak_cur = 0usize;
    let mut streak_start = String::new();
    let mut streak_end = String::new();
    let mut streak_cur_start = String::new();
    let mut prev_date = String::new();

    for (i, m) in sorted.iter().enumerate() {
        let parsed = extract_text(&m.blob);
        let is_self = m.is_self;

        if is_self { my_cnt += 1; } else { peer_cnt += 1; }
        *types.entry(parsed.msg_type.clone()).or_insert(0) += 1;

        match parsed.msg_type.as_str() {
            "text" => {
                let wc = parsed.content.chars().count();
                if is_self { my_words += wc; } else { peer_words += wc; }
                if wc > longest_len { longest_len = wc; longest = parsed.content.clone(); }
                // Phrases (2-20 chars, non-URL)
                if (2..=20).contains(&wc) && !parsed.content.contains("http") {
                    *phrase_all.entry(parsed.content.clone()).or_insert(0) += 1;
                    if is_self { *phrase_my.entry(parsed.content.clone()).or_insert(0) += 1; }
                    else { *phrase_peer.entry(parsed.content.clone()).or_insert(0) += 1; }
                }
            }
            "image" => { if is_self { my_img += 1; } else { peer_img += 1; } }
            "voice" => { if is_self { my_vc += 1; } else { peer_vc += 1; } }
            _ => {}
        }

        if let Some(dt) = ts_to_datetime(m.msg_id) {
            hourly[dt.hour() as usize] += 1;
            wday[dt.weekday().num_days_from_sunday() as usize] += 1;
            *monthly.entry(dt.format("%Y-%m").to_string()).or_insert(0) += 1;
            let d = dt.format("%Y-%m-%d").to_string();
            daily.insert(d.clone());

            // Initiative: track speaker transitions
            match last_speaker_was_me {
                None => { if is_self { my_init += 1; } else { peer_init += 1; } }
                Some(was_me) => {
                    if was_me && !is_self { peer_init += 1; }
                    if !was_me && is_self { my_init += 1; }
                }
            }
            last_speaker_was_me = Some(is_self);

            // Reply gaps (within 7 days)
            if let Some(last_ts) = last_msg_ts {
                if last_msg_was_me != is_self {
                    let gap = (normalize_ts(m.msg_id) as f64 - normalize_ts(last_ts) as f64) / 3600.0;
                    if gap > 0.0 && gap < 168.0 { reply_gaps.push(gap); }
                }
            }
            last_msg_ts = Some(m.msg_id);
            last_msg_was_me = is_self;

            // Streak
            if d != prev_date {
                let is_consecutive = if prev_date.is_empty() { false } else {
                    if let (Some(prev_dt), Some(cur_dt)) = (
                        NaiveDateTime::parse_from_str(&format!("{} 00:00:00", prev_date), "%Y-%m-%d %H:%M:%S").ok(),
                        NaiveDateTime::parse_from_str(&format!("{} 00:00:00", d), "%Y-%m-%d %H:%M:%S").ok(),
                    ) {
                        (cur_dt - prev_dt).num_days() == 1
                    } else { false }
                };
                if is_consecutive {
                    streak_cur += 1;
                } else {
                    if streak_cur > streak_max {
                        streak_max = streak_cur;
                        streak_end = prev_date;
                    }
                    streak_cur = 1;
                    streak_cur_start = d.clone();
                }
                prev_date = d;
            }
        }
    }
    // Final streak check
    if streak_cur > streak_max { streak_max = streak_cur; streak_end = prev_date; streak_start = streak_cur_start; }

    let total = sorted.len();
    let first_str = sorted.first().map(|m| ts_to_str(m.msg_id)).unwrap_or_default();
    let last_str = sorted.last().map(|m| ts_to_str(m.msg_id)).unwrap_or_default();
    let first_sec = sorted.first().map(|m| normalize_ts(m.msg_id)).unwrap_or(0);
    let last_sec = sorted.last().map(|m| normalize_ts(m.msg_id)).unwrap_or(0);

    let most_active_hour = hourly.iter().enumerate().max_by_key(|(_, v)| *v).map(|(i, _)| i as i32).unwrap_or(0);
    let most_active_wday_idx = wday.iter().enumerate().max_by_key(|(_, v)| *v).map(|(i, _)| i).unwrap_or(0);
    let wdays = ["周日","周一","周二","周三","周四","周五","周六"];

    let avg_reply = if reply_gaps.is_empty() { 0.0 } else { reply_gaps.iter().sum::<f64>() / reply_gaps.len() as f64 };
    let fastest = reply_gaps.iter().cloned().fold(f64::INFINITY, f64::min);
    if fastest == f64::INFINITY { /* no gaps */ }

    let mut monthly_sorted: Vec<_> = monthly.into_iter().collect();
    monthly_sorted.sort_by_key(|(k, _)| k.clone());

    let sort_phrases = |h: HashMap<String, usize>| -> Vec<(String, usize)> {
        let mut v: Vec<_> = h.into_iter().filter(|(_, c)| *c >= 2).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1)); v.into_iter().take(20).collect()
    };

    Ok(DuoReport {
        peer_name: nick.to_string(), peer_qq: qq.clone(), self_name: qq,
        total_messages: total, my_messages: my_cnt, peer_messages: peer_cnt,
        first_message: first_str, last_message: last_str,
        chat_age_days: if first_sec > 0 { (last_sec - first_sec) / 86400 } else { 0 },
        active_days: daily.len(), most_active_hour,
        most_active_weekday: wdays[most_active_wday_idx].to_string(),
        monthly_trend: monthly_sorted,
        hourly_heatmap: hourly.to_vec(),
        total_words: my_words + peer_words,
        avg_words_per_msg: if total > 0 { (my_words + peer_words) as f64 / total as f64 } else { 0.0 },
        longest_message: longest, top_phrases: sort_phrases(phrase_all),
        my_exclusive_phrases: sort_phrases(phrase_my),
        peer_exclusive_phrases: sort_phrases(phrase_peer),
        initiative: (my_init, peer_init),
        response_avg_hours: avg_reply, response_fastest_hours: fastest,
        streak_days: streak_max, streak_start, streak_end,
        type_breakdown: types,
        my_images: my_img, peer_images: peer_img,
        my_voices: my_vc, peer_voices: peer_vc,
    })
}

// ══════════════════════════════════════════════════════════════════════
// 4. ANNUAL REPORT
// ══════════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnualReport {
    pub year: i32, pub total_messages: usize,
    // 全局统计
    pub total_groups: usize, pub total_contacts: usize,
    pub most_active_month: String, pub most_active_day: String,
    pub most_active_hour: i32,
    // 排行榜
    pub top_groups: Vec<(String, usize)>, pub top_contacts: Vec<(String, usize)>,
    pub top_phrases: Vec<(String, usize)>,
    // 趋势
    pub monthly_breakdown: Vec<(String, usize)>,
    pub hourly_heatmap: Vec<i32>,
    pub weekday_breakdown: Vec<(String, usize)>,
    // 内容统计
    pub total_text_length: usize, pub total_images: usize,
    pub total_voices: usize, pub total_videos: usize,
    pub type_breakdown: HashMap<String, usize>,
    // 里程碑
    pub longest_conversation_day: (String, usize),
    pub earliest_message: String, pub latest_message: String,
    // 对话分析
    pub active_contacts_count: usize, pub active_groups_count: usize,
    pub new_contacts_year: usize, // 年内新增联系人（首次发言在本年的）
}

pub fn generate_annual_report(store: &MessageStore, year: i32) -> Result<AnnualReport, String> {
    let mut total = 0usize;
    let mut groups: HashMap<String, usize> = HashMap::new();
    let mut contacts: HashMap<String, usize> = HashMap::new();
    let mut monthly: HashMap<String, usize> = HashMap::new();
    let mut hourly = vec![0i32; 24];
    let mut wday: HashMap<i32, usize> = HashMap::new();
    let mut types: HashMap<String, usize> = HashMap::new();
    let mut text_len = 0usize;
    let (mut imgs, mut vcs, mut vids) = (0usize, 0usize, 0usize);
    let mut daily: HashMap<String, usize> = HashMap::new();
    let mut all_phrases: HashMap<String, usize> = HashMap::new();
    let mut first_ts = i64::MAX;
    let mut last_ts = i64::MIN;
    let mut contact_first_year: HashMap<String, bool> = HashMap::new();
    let mut new_contacts_count = 0usize;

    let process = |msgs: &Vec<_>, label: &str, counter: &mut HashMap<String, usize>,
                    types: &mut HashMap<String, usize>, phrases: &mut HashMap<String, usize>,
                    first_ts: &mut i64, last_ts: &mut i64| {
        for m in msgs {
            let dt = match ts_to_datetime(m.msg_id) { Some(d) => d, None => continue };
            if dt.year() != year { continue; }
            if m.msg_id < *first_ts { *first_ts = m.msg_id; }
            if m.msg_id > *last_ts { *last_ts = m.msg_id; }
            total += 1;
            *counter.entry(label.to_string()).or_insert(0) += 1;
            *monthly.entry(dt.format("%Y-%m").to_string()).or_insert(0) += 1;
            hourly[dt.hour() as usize] += 1;
            *wday.entry(dt.weekday().num_days_from_sunday() as i32).or_insert(0) += 1;
            *daily.entry(dt.format("%Y-%m-%d").to_string()).or_insert(0) += 1;
            let parsed = extract_text(&m.blob);
            *types.entry(parsed.msg_type.clone()).or_insert(0) += 1;
            match parsed.msg_type.as_str() {
                "text" => {
                    text_len += parsed.content.chars().count();
                    let wc = parsed.content.chars().count();
                    if (2..=20).contains(&wc) && !parsed.content.contains("http") {
                        *phrases.entry(parsed.content.clone()).or_insert(0) += 1;
                    }
                }
                "image" => imgs += 1,
                "voice" => vcs += 1,
                "video" => vids += 1,
                _ => {}
            }
        }
    };

    // Process all group messages
    for (gid, msgs) in &store.group_msgs {
        let name = store.group_names.get(gid).cloned().unwrap_or_else(|| gid.clone());
        process(msgs, &name, &mut groups, &mut types, &mut all_phrases, &mut first_ts, &mut last_ts);
    }

    // Process all C2C messages
    for (peer, msgs) in &store.c2c_msgs {
        let qq = store.uid_map.get(peer).cloned().unwrap_or_else(|| peer.clone());
        // Track new contacts (first message in this year, no messages in prior years)
        let mut has_prior = false;
        let mut has_this_year = false;
        for m in msgs {
            if let Some(dt) = ts_to_datetime(m.msg_id) {
                if dt.year() < year { has_prior = true; }
                if dt.year() == year { has_this_year = true; }
            }
        }
        if has_this_year && !has_prior { new_contacts_count += 1; }

        process(msgs, &qq, &mut contacts, &mut types, &mut all_phrases, &mut first_ts, &mut last_ts);
    }

    if total == 0 { return Err(format!("{} 年无消息数据", year)); }

    let most_active_month = monthly.iter().max_by_key(|(_, v)| *v).map(|(k, _)| k.clone()).unwrap_or_default();
    let most_active_day = daily.iter().max_by_key(|(_, v)| *v).map(|(k, _)| k.clone()).unwrap_or_default();
    let most_active_hour = hourly.iter().enumerate().max_by_key(|(_, v)| *v).map(|(i, _)| i as i32).unwrap_or(0);
    let longest_day = daily.iter().max_by_key(|(_, v)| *v).map(|(k, v)| (k.clone(), *v)).unwrap_or_default();

    let sort_vec = |h: HashMap<String, usize>, n: usize| -> Vec<(String, usize)> {
        let mut v: Vec<_> = h.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1)); v.into_iter().take(n).collect()
    };

    let mut monthly_sorted: Vec<_> = monthly.into_iter().collect();
    monthly_sorted.sort_by_key(|(k, _)| k.clone());

    let wd_names = ["周日","周一","周二","周三","周四","周五","周六"];
    let wday_sorted: Vec<_> = (0i32..7).map(|i| (wd_names[i as usize].to_string(), *wday.get(&i).unwrap_or(&0))).collect();

    let mut phrase_vec: Vec<_> = all_phrases.into_iter().filter(|(_, c)| *c >= 3).collect();
    phrase_vec.sort_by(|a, b| b.1.cmp(&a.1));

    Ok(AnnualReport {
        year, total_messages: total,
        total_groups: groups.len(), total_contacts: contacts.len(),
        most_active_month, most_active_day, most_active_hour,
        top_groups: sort_vec(groups, 10), top_contacts: sort_vec(contacts, 10),
        top_phrases: phrase_vec.into_iter().take(20).collect(),
        monthly_breakdown: monthly_sorted, hourly_heatmap: hourly, weekday_breakdown: wday_sorted,
        total_text_length: text_len, total_images: imgs, total_voices: vcs, total_videos: vids,
        type_breakdown: types, longest_conversation_day: longest_day,
        earliest_message: ts_to_str(if first_ts != i64::MAX { first_ts } else { 0 }),
        latest_message: ts_to_str(if last_ts != i64::MIN { last_ts } else { 0 }),
        active_contacts_count: contacts.iter().filter(|(_, &c)| c > 0).count(),
        active_groups_count: groups.iter().filter(|(_, &c)| c > 0).count(),
        new_contacts_year: new_contacts_count,
    })
}
