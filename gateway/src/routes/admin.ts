import type { Env, InviteRecord, DeviceRecord } from '../types';
import { generateSecureToken, sha256Hex } from '../crypto';

const SEVEN_DAYS_MS = 7 * 24 * 60 * 60 * 1000;

// Confirms the request carries the admin secret. Returns null when it is valid,
// or a 401 response when it is missing or wrong. The comparison hashes both
// sides first, so it runs in constant time and does not leak the secret through
// timing. When ADMIN_SECRET is unset, the admin API stays disabled.
export async function requireAdmin(request: Request, env: Env): Promise<Response | null> {
  const expected = env.ADMIN_SECRET;
  if (!expected) {
    return Response.json({ error: 'Admin API is not configured' }, { status: 503 });
  }

  const header = request.headers.get('Authorization') || '';
  const provided = header.startsWith('Bearer ') ? header.substring(7).trim() : '';
  if (!provided) {
    return Response.json({ error: 'Missing admin secret' }, { status: 401 });
  }

  const a = await sha256Hex(provided);
  const b = await sha256Hex(expected);
  let diff = 0;
  for (let i = 0; i < a.length; i += 1) {
    diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  }
  if (diff !== 0) {
    return Response.json({ error: 'Invalid admin secret' }, { status: 401 });
  }

  return null;
}

// Creates one invite. The raw code is returned once and never stored; only its
// hash goes to the database.
export async function handleAdminCreateInvite(request: Request, env: Env): Promise<Response> {
  let body: any = {};
  try {
    body = await request.json();
  } catch {
    body = {};
  }
  const maxDevices =
    typeof body.max_devices === 'number' &&
    Number.isInteger(body.max_devices) &&
    body.max_devices >= 1 &&
    body.max_devices <= 100
      ? body.max_devices
      : 1;

  const code = generateSecureToken(32);
  const codeHash = await sha256Hex(code);
  const now = Date.now();

  const result = await env.DB.prepare(
    'INSERT INTO invites (code_hash, created_at, max_devices, redeemed_count, is_revoked) VALUES (?, ?, ?, 0, 0)'
  )
    .bind(codeHash, now, maxDevices)
    .run();

  if (!result.success) {
    return Response.json({ error: 'Failed to create invite' }, { status: 500 });
  }

  return Response.json({ code, max_devices: maxDevices, created_at: now });
}

// Lists invites. Never returns a code, only the hash prefix and counters.
export async function handleAdminListInvites(env: Env): Promise<Response> {
  const rows = await env.DB.prepare(
    'SELECT code_hash, created_at, max_devices, redeemed_count, is_revoked FROM invites ORDER BY created_at DESC LIMIT 200'
  ).all<InviteRecord>();

  const now = Date.now();
  const invites = (rows.results || []).map((r) => ({
    hash: r.code_hash.substring(0, 12),
    created_at: r.created_at,
    max_devices: r.max_devices,
    redeemed_count: r.redeemed_count,
    is_revoked: r.is_revoked === 1,
    is_expired: now - r.created_at > SEVEN_DAYS_MS,
  }));
  return Response.json({ invites });
}

// Revokes every invite whose hash starts with the given prefix.
export async function handleAdminRevokeInvite(request: Request, env: Env): Promise<Response> {
  const prefix = await readHashPrefix(request);
  if (!prefix) {
    return Response.json({ error: 'A hash prefix of at least 8 hex characters is required' }, { status: 400 });
  }
  const result = await env.DB.prepare(
    "UPDATE invites SET is_revoked = 1 WHERE code_hash LIKE ? || '%'"
  )
    .bind(prefix)
    .run();
  return Response.json({ revoked: result.meta.changes });
}

// Lists devices. Never returns a token, only the hash prefix and usage.
export async function handleAdminListDevices(env: Env): Promise<Response> {
  const rows = await env.DB.prepare(
    'SELECT token_hash, invite_code_hash, created_at, last_used_at, is_revoked, request_count FROM devices ORDER BY last_used_at DESC LIMIT 200'
  ).all<DeviceRecord>();

  const devices = (rows.results || []).map((r) => ({
    hash: r.token_hash.substring(0, 12),
    via_invite: r.invite_code_hash.substring(0, 12),
    created_at: r.created_at,
    last_used_at: r.last_used_at,
    request_count: r.request_count,
    is_revoked: r.is_revoked === 1,
  }));
  return Response.json({ devices });
}

// Revokes every device whose token hash starts with the given prefix.
export async function handleAdminRevokeDevice(request: Request, env: Env): Promise<Response> {
  const prefix = await readHashPrefix(request);
  if (!prefix) {
    return Response.json({ error: 'A hash prefix of at least 8 hex characters is required' }, { status: 400 });
  }
  const result = await env.DB.prepare(
    "UPDATE devices SET is_revoked = 1 WHERE token_hash LIKE ? || '%'"
  )
    .bind(prefix)
    .run();
  return Response.json({ revoked: result.meta.changes });
}

async function readHashPrefix(request: Request): Promise<string | null> {
  let body: any = {};
  try {
    body = await request.json();
  } catch {
    return null;
  }
  const raw = typeof body.hash_prefix === 'string' ? body.hash_prefix.trim().toLowerCase() : '';
  if (raw.length < 8 || !/^[0-9a-f]+$/.test(raw)) {
    return null;
  }
  return raw;
}

// The admin page. It carries no secret; the operator pastes the admin secret
// into the page, which the browser keeps in session storage and sends as a
// bearer token on each API call.
export function adminPageHtml(): string {
  return ADMIN_HTML;
}

const ADMIN_HTML = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<meta name="robots" content="noindex" />
<title>Flix Gateway Admin</title>
<style>
  :root { color-scheme: dark; }
  * { box-sizing: border-box; }
  body { margin: 0; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    background: #0f1013; color: #f0f1f4; line-height: 1.5; }
  header { padding: 18px 24px; border-bottom: 1px solid #2b2f3a; display: flex; align-items: center; justify-content: space-between; }
  h1 { font-size: 1.05rem; margin: 0; color: #e50914; letter-spacing: 0.04em; }
  main { max-width: 960px; margin: 0 auto; padding: 24px; }
  section { background: #1d2027; border: 1px solid #2b2f3a; border-radius: 8px; padding: 18px; margin-bottom: 20px; }
  h2 { font-size: 0.95rem; margin: 0 0 14px; }
  label { font-size: 0.8rem; color: #9ea3ae; display: block; margin-bottom: 4px; }
  input { background: #16181d; border: 1px solid #2b2f3a; color: #f0f1f4; border-radius: 6px; padding: 8px 10px; font: inherit; width: 100%; }
  button { background: #e50914; color: #fff; border: none; border-radius: 6px; padding: 8px 14px; font: inherit; font-weight: 600; cursor: pointer; }
  button.secondary { background: #262a33; }
  button:disabled { opacity: 0.5; cursor: default; }
  .row { display: flex; gap: 10px; align-items: flex-end; flex-wrap: wrap; }
  .row > div { flex: 1; min-width: 120px; }
  table { width: 100%; border-collapse: collapse; font-size: 0.85rem; }
  th, td { text-align: left; padding: 7px 8px; border-bottom: 1px solid #2b2f3a; }
  th { color: #9ea3ae; font-weight: 600; }
  code { font-family: ui-monospace, Menlo, monospace; }
  .code-box { background: #16181d; border: 1px solid #e50914; border-radius: 6px; padding: 12px; margin-top: 12px; word-break: break-all; font-family: ui-monospace, Menlo, monospace; }
  .muted { color: #6b7280; }
  .pill { font-size: 0.72rem; padding: 1px 7px; border-radius: 10px; }
  .pill.ok { background: #14351f; color: #22c55e; }
  .pill.bad { background: #3a1a1a; color: #ef4444; }
  .err { color: #ef4444; font-size: 0.85rem; margin-top: 8px; }
  .toolbar { display: flex; gap: 8px; margin-bottom: 12px; }
</style>
</head>
<body>
<header>
  <h1>FLIX GATEWAY ADMIN</h1>
  <button id="lock" class="secondary" onclick="lock()">Lock</button>
</header>
<main>
  <section id="auth">
    <h2>Admin secret</h2>
    <div class="row">
      <div><label for="secret">Paste the admin secret</label><input id="secret" type="password" autocomplete="off" /></div>
      <button onclick="unlock()">Unlock</button>
    </div>
    <div id="auth-err" class="err"></div>
  </section>

  <div id="panel" style="display:none">
    <section>
      <h2>Create invite</h2>
      <div class="row">
        <div><label for="max">Max devices</label><input id="max" type="number" value="1" min="1" max="100" /></div>
        <button onclick="createInvite()">Create</button>
      </div>
      <div id="new-code" class="code-box" style="display:none"></div>
    </section>

    <section>
      <h2>Invites</h2>
      <div class="toolbar"><button class="secondary" onclick="loadInvites()">Refresh</button></div>
      <table><thead><tr><th>Hash</th><th>Devices</th><th>Used</th><th>Created</th><th>Status</th><th></th></tr></thead>
      <tbody id="invites"></tbody></table>
    </section>

    <section>
      <h2>Devices</h2>
      <div class="toolbar"><button class="secondary" onclick="loadDevices()">Refresh</button></div>
      <table><thead><tr><th>Token</th><th>Via invite</th><th>Requests</th><th>Last used</th><th>Status</th><th></th></tr></thead>
      <tbody id="devices"></tbody></table>
    </section>
  </div>
</main>
<script>
  const base = '/v1/admin';
  function secret() { return sessionStorage.getItem('flix_admin'); }
  function fmt(ms) { return ms ? new Date(ms).toISOString().slice(0,16).replace('T',' ') : '-'; }
  function esc(s){ return String(s).replace(/[&<>]/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;'}[c])); }

  async function api(path, method, body) {
    const res = await fetch(base + path, {
      method: method || 'GET',
      headers: { 'Authorization': 'Bearer ' + secret(), 'Content-Type': 'application/json' },
      body: body ? JSON.stringify(body) : undefined,
    });
    if (res.status === 401) { throw new Error('unauthorized'); }
    const data = await res.json().catch(() => ({}));
    if (!res.ok) { throw new Error(data.error || ('HTTP ' + res.status)); }
    return data;
  }

  async function unlock() {
    const s = document.getElementById('secret').value.trim();
    if (!s) return;
    sessionStorage.setItem('flix_admin', s);
    try {
      await api('/invites');
      document.getElementById('auth').style.display = 'none';
      document.getElementById('panel').style.display = 'block';
      document.getElementById('auth-err').textContent = '';
      loadInvites(); loadDevices();
    } catch (e) {
      sessionStorage.removeItem('flix_admin');
      document.getElementById('auth-err').textContent = e.message === 'unauthorized' ? 'Wrong secret.' : e.message;
    }
  }
  function lock() { sessionStorage.removeItem('flix_admin'); location.reload(); }

  async function createInvite() {
    const max = parseInt(document.getElementById('max').value, 10) || 1;
    try {
      const r = await api('/invites', 'POST', { max_devices: max });
      const box = document.getElementById('new-code');
      box.style.display = 'block';
      box.innerHTML = 'Invite code (shown once):<br><br><strong>' + esc(r.code) + '</strong>';
      loadInvites();
    } catch (e) { alert(e.message); }
  }

  async function loadInvites() {
    try {
      const r = await api('/invites');
      document.getElementById('invites').innerHTML = r.invites.map(i =>
        '<tr><td><code>' + esc(i.hash) + '</code></td><td>' + i.max_devices + '</td><td>' + i.redeemed_count +
        '</td><td>' + fmt(i.created_at) + '</td><td>' +
        (i.is_revoked ? '<span class="pill bad">revoked</span>' : i.is_expired ? '<span class="pill bad">expired</span>' : '<span class="pill ok">active</span>') +
        '</td><td>' + (i.is_revoked ? '' : '<button class="secondary" onclick="revokeInvite(\\'' + esc(i.hash) + '\\')">Revoke</button>') + '</td></tr>'
      ).join('') || '<tr><td colspan="6" class="muted">No invites.</td></tr>';
    } catch (e) { if (e.message === 'unauthorized') lock(); }
  }
  async function revokeInvite(hash) {
    if (!confirm('Revoke invite ' + hash + '?')) return;
    try { await api('/invites/revoke', 'POST', { hash_prefix: hash }); loadInvites(); } catch (e) { alert(e.message); }
  }

  async function loadDevices() {
    try {
      const r = await api('/devices');
      document.getElementById('devices').innerHTML = r.devices.map(d =>
        '<tr><td><code>' + esc(d.hash) + '</code></td><td><code>' + esc(d.via_invite) + '</code></td><td>' + d.request_count +
        '</td><td>' + fmt(d.last_used_at) + '</td><td>' +
        (d.is_revoked ? '<span class="pill bad">revoked</span>' : '<span class="pill ok">active</span>') +
        '</td><td>' + (d.is_revoked ? '' : '<button class="secondary" onclick="revokeDevice(\\'' + esc(d.hash) + '\\')">Revoke</button>') + '</td></tr>'
      ).join('') || '<tr><td colspan="6" class="muted">No devices.</td></tr>';
    } catch (e) { if (e.message === 'unauthorized') lock(); }
  }
  async function revokeDevice(hash) {
    if (!confirm('Revoke device ' + hash + '?')) return;
    try { await api('/devices/revoke', 'POST', { hash_prefix: hash }); loadDevices(); } catch (e) { alert(e.message); }
  }

  if (secret()) { unlock(); }
</script>
</body>
</html>`;
