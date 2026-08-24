use std::io::{Cursor, Write as _};

use anyhow::Result;
use axum::{
    Router,
    extract::Multipart,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use http::{StatusCode, header};
use tokio::net::TcpListener;

use crate::formats;

pub async fn serve() -> Result<()> {
    let app = Router::new()
        .route("/", get(index))
        .route("/strip", post(strip));

    let listener = TcpListener::bind("127.0.0.1:3301").await?;
    eprintln!("unmetadata web UI → http://localhost:3301");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn index() -> Html<&'static str> {
    Html(UI_HTML)
}

async fn strip(mut multipart: Multipart) -> impl IntoResponse {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut is_folder = false;
    let mut folder_name = String::from("folder");

    while let Ok(Some(field)) = multipart.next_field().await {
        match field.name().unwrap_or("") {
            "is_folder" => {
                if let Ok(v) = field.text().await {
                    is_folder = v == "true";
                }
            }
            "folder_name" => {
                if let Ok(v) = field.text().await {
                    folder_name = v;
                }
            }
            _ => {
                let filename = field
                    .file_name()
                    .unwrap_or("file")
                    .to_string();
                if let Ok(bytes) = field.bytes().await {
                    files.push((filename, bytes.to_vec()));
                }
            }
        }
    }

    if files.is_empty() {
        return error_response(StatusCode::BAD_REQUEST, "no files received");
    }

    // Strip metadata from every file
    let processed: Vec<(String, Vec<u8>)> = files
        .into_iter()
        .map(|(name, data)| {
            let cleaned = formats::detect(&data)
                .and_then(|fmt| formats::strip(fmt, &data, &name).ok().map(|(b, _)| b))
                .unwrap_or(data);
            (name, cleaned)
        })
        .collect();

    if processed.len() == 1 && !is_folder {
        let (filename, data) = processed.into_iter().next().unwrap();
        let cd = format!(
            "attachment; filename=\"{}\"",
            sanitize(&filename)
        );
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .header(header::CONTENT_DISPOSITION, cd)
            .body(axum::body::Body::from(data))
            .unwrap()
    } else {
        let zip_name = if is_folder {
            format!("{}-cleaned.zip", sanitize(&folder_name))
        } else {
            "files-cleaned.zip".to_string()
        };
        match build_zip(&processed) {
            Ok(zip_bytes) => {
                let cd = format!("attachment; filename=\"{}\"", zip_name);
                Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "application/zip")
                    .header(header::CONTENT_DISPOSITION, cd)
                    .body(axum::body::Body::from(zip_bytes))
                    .unwrap()
            }
            Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        }
    }
}

fn build_zip(files: &[(String, Vec<u8>)]) -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    {
        let mut w = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in files {
            w.start_file(name, opts)?;
            w.write_all(data)?;
        }
        w.finish()?;
    }
    Ok(buf.into_inner())
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || "._- ()[]{}".contains(c) { c } else { '_' })
        .collect()
}

fn error_response(status: StatusCode, msg: &str) -> Response<axum::body::Body> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain")
        .body(axum::body::Body::from(msg.to_string()))
        .unwrap()
}

// ── Embedded Web UI ────────────────────────────────────────────────────────

const UI_HTML: &str = r###"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>unmetadata</title>
<style>
*, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }

:root {
  --bg:       #0d0c14;
  --surface:  #17152a;
  --raised:   #201d35;
  --border:   #302c50;
  --accent:   #7c5cbf;
  --accent-hi:#a07ee0;
  --ok:       #34d399;
  --err:      #f87171;
  --warn:     #fbbf24;
  --text:     #ddd8f0;
  --sub:      #8880aa;
  --radius:   10px;
  --font:     'Segoe UI', system-ui, sans-serif;
}

body {
  background: var(--bg);
  color: var(--text);
  font-family: var(--font);
  font-size: 15px;
  min-height: 100vh;
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 32px 16px 64px;
}

header {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 36px;
  user-select: none;
}
header svg { flex-shrink: 0; }
header h1 { font-size: 1.55rem; font-weight: 700; letter-spacing: -.5px; }
header span {
  font-size: .75rem;
  color: var(--sub);
  background: var(--raised);
  border: 1px solid var(--border);
  border-radius: 99px;
  padding: 2px 10px;
  margin-left: 4px;
}

main {
  width: 100%;
  max-width: 640px;
  display: flex;
  flex-direction: column;
  gap: 20px;
}

#drop-zone {
  background: var(--surface);
  border: 2px dashed var(--border);
  border-radius: var(--radius);
  padding: 52px 32px;
  text-align: center;
  cursor: pointer;
  transition: border-color .18s, background .18s, transform .12s;
  position: relative;
}
#drop-zone:hover, #drop-zone.drag-over {
  border-color: var(--accent-hi);
  background: var(--raised);
  transform: scale(1.005);
}
#drop-zone.drag-over { border-style: solid; }

#drop-zone .icon {
  font-size: 2.6rem;
  margin-bottom: 14px;
  display: block;
  pointer-events: none;
}
#drop-zone .primary {
  font-size: 1.05rem;
  font-weight: 600;
  margin-bottom: 6px;
  pointer-events: none;
}
#drop-zone .secondary {
  font-size: .85rem;
  color: var(--sub);
  pointer-events: none;
}
#drop-zone .secondary kbd {
  background: var(--raised);
  border: 1px solid var(--border);
  border-radius: 4px;
  padding: 1px 7px;
  font-size: .8rem;
  font-family: monospace;
}

#file-input { display: none; }

#file-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.file-item {
  display: flex;
  align-items: center;
  gap: 12px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 11px 14px;
  animation: slide-in .18s ease;
}
@keyframes slide-in {
  from { opacity: 0; transform: translateY(-6px); }
  to   { opacity: 1; transform: translateY(0); }
}

.file-item .name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: .9rem;
}
.file-item .size {
  font-size: .78rem;
  color: var(--sub);
  flex-shrink: 0;
}
.file-item .badge {
  flex-shrink: 0;
  font-size: .72rem;
  font-weight: 600;
  padding: 3px 9px;
  border-radius: 99px;
  letter-spacing: .4px;
}
.badge-processing { background: #2d2550; color: var(--accent-hi); }
.badge-done       { background: #0d2e22; color: var(--ok); }
.badge-error      { background: #2e1212; color: var(--err); }

#status-bar {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 14px 18px;
  display: none;
  align-items: center;
  gap: 12px;
}
#status-bar.visible { display: flex; }
#status-bar .msg { flex: 1; font-size: .9rem; }
#status-bar .dot {
  width: 9px; height: 9px;
  border-radius: 50%;
  flex-shrink: 0;
}
.dot-busy { background: var(--accent-hi); animation: pulse .9s infinite; }
.dot-ok   { background: var(--ok); }
.dot-err  { background: var(--err); }
@keyframes pulse {
  0%,100% { opacity: 1; }
  50%      { opacity: .35; }
}

.hint {
  font-size: .8rem;
  color: var(--sub);
  text-align: center;
  padding: 0 8px;
}
.hint a { color: var(--accent-hi); text-decoration: none; }
.hint a:hover { text-decoration: underline; }
</style>
</head>
<body>

<header>
  <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="var(--accent-hi)" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
    <rect x="3" y="11" width="18" height="11" rx="2" ry="2"/>
    <path d="M7 11V7a5 5 0 0 1 10 0v4"/>
    <line x1="12" y1="15" x2="12" y2="17"/>
  </svg>
  <h1>unmetadata</h1>
  <span>local</span>
</header>

<main>
  <div id="drop-zone">
    <span class="icon">&#x1F5C2;</span>
    <p class="primary">Drop files or a folder here</p>
    <p class="secondary">or press <kbd>Space</kbd> / click to browse &nbsp;&middot;&nbsp; any supported format</p>
  </div>
  <input id="file-input" type="file" multiple>

  <div id="status-bar">
    <div class="dot" id="status-dot"></div>
    <span class="msg" id="status-msg">Idle</span>
  </div>

  <div id="file-list"></div>

  <p class="hint">
    Strips EXIF &bull; IPTC &bull; XMP &bull; GPS &bull; comments &bull; thumbnails &bull; author fields.
    Files are processed locally &mdash; nothing is stored.
  </p>
</main>

<script>
const dropZone   = document.getElementById('drop-zone');
const fileInput  = document.getElementById('file-input');
const fileList   = document.getElementById('file-list');
const statusBar  = document.getElementById('status-bar');
const statusMsg  = document.getElementById('status-msg');
const statusDot  = document.getElementById('status-dot');

// ── Drag-and-drop ────────────────────────────────────────────────

dropZone.addEventListener('click', () => fileInput.click());
dropZone.addEventListener('keydown', e => {
  if (e.key === ' ' || e.key === 'Enter') { e.preventDefault(); fileInput.click(); }
});
dropZone.setAttribute('tabindex', '0');
dropZone.setAttribute('role', 'button');
dropZone.setAttribute('aria-label', 'Drop files here or click to browse');

let dragCounter = 0;
dropZone.addEventListener('dragenter', e => { e.preventDefault(); dragCounter++; dropZone.classList.add('drag-over'); });
dropZone.addEventListener('dragleave', () => { dragCounter--; if (dragCounter <= 0) { dragCounter = 0; dropZone.classList.remove('drag-over'); } });
dropZone.addEventListener('dragover', e => e.preventDefault());

dropZone.addEventListener('drop', async e => {
  e.preventDefault();
  dragCounter = 0;
  dropZone.classList.remove('drag-over');
  await handleTransfer(e.dataTransfer);
});

fileInput.addEventListener('change', async () => {
  if (!fileInput.files.length) return;
  const entries = Array.from(fileInput.files).map(f => ({ file: f, path: f.name }));
  await process(entries, false, '');
  fileInput.value = '';
});

// ── DataTransfer handling ────────────────────────────────────────

async function handleTransfer(dt) {
  const items = Array.from(dt.items || []);
  const hasFolder = items.some(i => {
    const e = i.webkitGetAsEntry && i.webkitGetAsEntry();
    return e && e.isDirectory;
  });

  if (hasFolder && items[0].webkitGetAsEntry) {
    const allEntries = [];
    let folderName = 'folder';
    for (const item of items) {
      const entry = item.webkitGetAsEntry();
      if (!entry) continue;
      if (entry.isDirectory) folderName = entry.name;
      const collected = await collectEntry(entry, '');
      allEntries.push(...collected);
    }
    await process(allEntries, true, folderName);
  } else {
    const files = Array.from(dt.files);
    const entries = files.map(f => ({ file: f, path: f.name }));
    await process(entries, false, '');
  }
}

async function collectEntry(entry, prefix) {
  if (entry.isFile) {
    return new Promise(res => entry.file(f => res([{ file: f, path: prefix + f.name }])));
  }
  if (entry.isDirectory) {
    const reader = entry.createReader();
    const children = await readAll(reader);
    const nested = await Promise.all(children.map(c => collectEntry(c, prefix + entry.name + '/')));
    return nested.flat();
  }
  return [];
}

async function readAll(reader) {
  const all = [];
  while (true) {
    const batch = await new Promise(res => reader.readEntries(res));
    if (!batch.length) break;
    all.push(...batch);
  }
  return all;
}

// ── Processing & upload ──────────────────────────────────────────

async function process(fileEntries, isFolder, folderName) {
  if (!fileEntries.length) return;

  fileList.innerHTML = '';
  fileEntries.forEach(({ file, path }) => addFileItem(path || file.name, file.size, 'processing'));
  setStatus('busy', `Stripping metadata from ${fileEntries.length} file${fileEntries.length !== 1 ? 's' : ''}...`);

  const form = new FormData();
  form.append('is_folder', isFolder ? 'true' : 'false');
  form.append('folder_name', folderName || 'folder');
  for (const { file, path } of fileEntries) {
    form.append('file', file, path || file.name);
  }

  try {
    const res = await fetch('/strip', { method: 'POST', body: form });
    if (!res.ok) {
      const msg = await res.text().catch(() => res.statusText);
      throw new Error(msg || 'server error');
    }

    const blob = await res.blob();
    const cd = res.headers.get('content-disposition') || '';
    const m = cd.match(/filename="([^"]+)"/);
    const filename = m ? m[1] : (isFolder ? folderName + '-cleaned.zip' : 'cleaned-file');

    // Trigger download
    const url = URL.createObjectURL(blob);
    const a = Object.assign(document.createElement('a'), { href: url, download: filename });
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    setTimeout(() => URL.revokeObjectURL(url), 2000);

    markAll('done');
    setStatus('ok', `Done — ${filename} downloaded.`);
  } catch (err) {
    markAll('error');
    setStatus('err', `Error: ${err.message}`);
  }
}

// ── UI helpers ───────────────────────────────────────────────────

function addFileItem(path, size, state) {
  const name = path.split('/').pop() || path;
  const item = document.createElement('div');
  item.className = 'file-item';
  item.dataset.path = path;
  item.innerHTML = `
    <span class="name" title="${esc(path)}">${esc(name)}</span>
    <span class="size">${fmtSize(size)}</span>
    <span class="badge badge-${state}">${statLabel(state)}</span>`;
  fileList.appendChild(item);
}

function markAll(state) {
  fileList.querySelectorAll('.badge').forEach(b => {
    b.className = `badge badge-${state}`;
    b.textContent = statLabel(state);
  });
}

function setStatus(type, msg) {
  statusBar.className = 'visible';
  statusMsg.textContent = msg;
  statusDot.className = `dot dot-${type === 'busy' ? 'busy' : type === 'ok' ? 'ok' : 'err'}`;
}

function statLabel(s) {
  return s === 'processing' ? 'stripping...' : s === 'done' ? '✓ clean' : '✗ error';
}

function fmtSize(n) {
  if (!n) return '';
  if (n < 1024) return n + ' B';
  if (n < 1048576) return (n / 1024).toFixed(1) + ' KB';
  return (n / 1048576).toFixed(1) + ' MB';
}

function esc(s) {
  return s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/"/g,'&quot;');
}
</script>
</body>
</html>
"###;
