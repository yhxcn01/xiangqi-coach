'use strict';
// 象棋思考教练 — 页面逻辑：触屏走棋、本地续局、GLM 讲解、复盘分析。
// 所有引擎计算都发给 worker.js（WASM 宿主），本文件只管 UI 与网络。

const $ = (id) => document.getElementById(id);
const canvas = $('board'), ctx = canvas.getContext('2d');
const CELL = 60, PAD = 40, BW = 560, BH = 620;
const CHARS = {1:'帅',2:'仕',3:'相',4:'马',5:'车',6:'炮',7:'兵','-1':'将','-2':'士','-3':'象','-4':'马','-5':'车','-6':'砲','-7':'卒'};
const LS_MOVES = 'xq_moves_v1', LS_SETTINGS = 'xq_settings_v1';

let moves = [];            // 权威棋谱：[{from,to},...]，刷新后从 localStorage 恢复
let state = null;          // 最近一次 WASM 返回的状态
let selected = -1, targets = [];
let busy = false, reviewBusy = false, typeTimer = null;
let settings = { base_url: 'https://open.bigmodel.cn/api/paas/v4', key: '', model: 'glm-4.7-flash' };

// ---------- Worker RPC ----------
const worker = new Worker('worker.js?v=4');
let rpcId = 0;
const pending = new Map();
worker.onmessage = (e) => {
  const { id, ok, data, error } = e.data;
  const p = pending.get(id);
  if (!p) return;
  pending.delete(id);
  ok ? p.resolve(data) : p.reject(new Error(error || '引擎调用失败'));
};
function rpc(cmd, extra = {}) {
  return new Promise((resolve, reject) => {
    const id = ++rpcId;
    pending.set(id, { resolve, reject });
    worker.postMessage({ id, cmd, moves, ...extra });
  });
}

// ---------- 本地存储 ----------
function loadLocal() {
  try {
    const m = JSON.parse(localStorage.getItem(LS_MOVES) || '[]');
    if (Array.isArray(m)) moves = m.filter(x => x && Number.isInteger(x.from) && Number.isInteger(x.to));
  } catch (_) { moves = []; }
  try {
    const s = JSON.parse(localStorage.getItem(LS_SETTINGS) || '{}');
    if (typeof s.base_url === 'string' && s.base_url) settings.base_url = s.base_url;
    if (typeof s.key === 'string') settings.key = s.key;
    if (typeof s.model === 'string' && s.model) settings.model = s.model;
  } catch (_) {}
}
function persistMoves() { localStorage.setItem(LS_MOVES, JSON.stringify(moves)); }
function persistSettings() { localStorage.setItem(LS_SETTINGS, JSON.stringify(settings)); }

// ---------- 棋盘绘制 ----------
const X = (c) => PAD + c * CELL, Y = (r) => PAD + r * CELL;

function resizeBoard() {
  const col = document.querySelector('.board-col');
  const cssW = Math.max(280, Math.min(col.clientWidth - 16, 520));
  const scale = cssW / BW;
  const dpr = Math.min(window.devicePixelRatio || 1, 3);
  canvas.style.width = cssW + 'px';
  canvas.style.height = Math.round(BH * scale) + 'px';
  canvas.width = Math.round(cssW * dpr);
  canvas.height = Math.round(BH * scale * dpr);
  draw();
}
window.addEventListener('resize', resizeBoard);

function line(x1, y1, x2, y2) { ctx.beginPath(); ctx.moveTo(x1, y1); ctx.lineTo(x2, y2); ctx.stroke(); }

function draw() {
  const dpr = canvas.width / BW;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, BW, BH);
  ctx.strokeStyle = '#5d3c14'; ctx.lineWidth = 1.5;
  for (let r = 0; r < 10; r++) line(X(0), Y(r), X(8), Y(r));
  for (let c = 0; c < 9; c++) {
    if (c === 0 || c === 8) line(X(c), Y(0), X(c), Y(9));
    else { line(X(c), Y(0), X(c), Y(4)); line(X(c), Y(5), X(c), Y(9)); }
  }
  ctx.lineWidth = 3; ctx.strokeRect(X(0)-6, Y(0)-6, CELL*8+12, CELL*9+12);
  ctx.lineWidth = 1.5;
  line(X(3), Y(0), X(5), Y(2)); line(X(5), Y(0), X(3), Y(2));
  line(X(3), Y(7), X(5), Y(9)); line(X(5), Y(7), X(3), Y(9));
  [[1,2],[7,2],[0,3],[2,3],[4,3],[6,3],[8,3],[1,7],[7,7],[0,6],[2,6],[4,6],[6,6],[8,6]].forEach(([c,r]) => star(c, r));
  ctx.fillStyle = '#5d3c14'; ctx.font = '26px KaiTi, STKaiti, serif';
  ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
  ctx.fillText('楚    河', PAD + CELL * 2, Y(4.5));
  ctx.fillText('汉    界', PAD + CELL * 6, Y(4.5));

  if (state && moves.length) {
    const last = moves[moves.length - 1];
    mark(last.from); mark(last.to);
  }
  if (state) {
    state.cells.forEach((p, i) => {
      if (p === 0) return;
      const r = Math.floor(i / 9), c = i % 9;
      const x = X(c), y = Y(r);
      ctx.beginPath(); ctx.arc(x, y, 25, 0, Math.PI * 2);
      ctx.fillStyle = i === selected ? '#ffe9b0' : '#f8ecc9';
      ctx.fill();
      ctx.lineWidth = 2.5; ctx.strokeStyle = p > 0 ? '#c0392b' : '#2c3e50'; ctx.stroke();
      ctx.beginPath(); ctx.arc(x, y, 21, 0, Math.PI * 2); ctx.lineWidth = 1; ctx.stroke();
      ctx.fillStyle = p > 0 ? '#c0392b' : '#2c3e50';
      ctx.font = '26px KaiTi, STKaiti, "SimSun", serif';
      ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
      ctx.fillText(CHARS[p], x, y + 1);
    });
    targets.forEach((t) => {
      const r = Math.floor(t / 9), c = t % 9;
      ctx.beginPath(); ctx.arc(X(c), Y(r), state.cells[t] !== 0 ? 27 : 7, 0, Math.PI * 2);
      ctx.strokeStyle = '#1e7d4f'; ctx.fillStyle = 'rgba(30,125,79,.5)'; ctx.lineWidth = 2.5;
      state.cells[t] !== 0 ? ctx.stroke() : ctx.fill();
    });
  }
}
function star(c, r) {
  const d = 5, l = 10;
  for (const [sx, sy] of [[1,1],[1,-1],[-1,1],[-1,-1]]) {
    if ((c === 0 && sx < 0) || (c === 8 && sx > 0)) continue;
    const x = X(c) + sx * d, y = Y(r) + sy * d;
    line(x, y, x + sx * l, y); line(x, y, x, y + sy * l);
  }
}
function mark(sq) {
  const r = Math.floor(sq / 9), c = sq % 9;
  ctx.strokeStyle = 'rgba(192,57,43,.8)'; ctx.lineWidth = 2;
  ctx.strokeRect(X(c) - 24, Y(r) - 24, 48, 48);
}

// ---------- 渲染 ----------
function esc(s) { return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;'); }

function render() {
  draw(); renderStatus(); renderHistory();
  $('btnUndo').disabled = busy || reviewBusy || !moves.length;
  $('btnExplainPiece').disabled = busy || reviewBusy || explaining || selected < 0;
  $('btnExplainPos').disabled = busy || reviewBusy || explaining;
  $('btnReview').disabled = busy || reviewBusy || !moves.length;
  $('btnNew').disabled = busy || reviewBusy;
}

function renderStatus() {
  const el = $('status');
  if (!state) { el.textContent = '载入中…'; return; }
  if (state.game_over) { el.innerHTML = '🏁 ' + esc(state.game_over); showOver(state.game_over); return; }
  hideOver();
  const turn = state.red_turn ? '🔴 红方（你）走棋' : '⚫ 黑方（引擎）思考中…';
  el.innerHTML = turn + (state.in_check ? ' <span class="check">将军！</span>' : '');
}

function renderHistory() {
  const el = $('history');
  if (!state) { el.innerHTML = '<span>尚未走子</span>'; return; }
  let html = '';
  state.history.forEach((n, i) => {
    if (i % 2 === 0) html += `<b>${Math.floor(i/2)+1}. </b>`;
    html += `<span>${esc(n)}</span> `;
  });
  el.innerHTML = html || '<span>尚未走子</span>';
  el.scrollTop = el.scrollHeight;
}

function showOver(t) { const b = $('overBanner'); b.textContent = t; b.style.display = 'block'; }
function hideOver() { $('overBanner').style.display = 'none'; }
function toast(t) {
  const el = $('toast'); el.textContent = t; el.style.display = 'block';
  clearTimeout(toast._t); toast._t = setTimeout(() => { el.style.display = 'none'; }, 2600);
}

function typewriter(text, source) {
  const el = $('explain');
  if (typeTimer) clearInterval(typeTimer);
  const badge = source === 'ai'
    ? '<span class="src src-ai">AI 棋理讲解 · GLM</span><br>'
    : '<span class="src src-fallback">引擎基础提示（未配置 AI）</span><br>';
  let i = 0;
  typeTimer = setInterval(() => {
    i += 2;
    el.innerHTML = badge + esc(text.slice(0, i)).replace(/\n/g, '<br>');
    if (i >= text.length) clearInterval(typeTimer);
  }, 22);
}

// ---------- 走棋 ----------
canvas.addEventListener('pointerdown', async (e) => {
  if (busy || reviewBusy || !state || state.game_over || !state.red_turn) return;
  e.preventDefault();
  const rect = canvas.getBoundingClientRect();
  const scale = rect.width / BW;
  const x = (e.clientX - rect.left) / scale, y = (e.clientY - rect.top) / scale;
  const c = Math.round((x - PAD) / CELL), r = Math.round((y - PAD) / CELL);
  if (r < 0 || r > 9 || c < 0 || c > 8) return;
  const i = r * 9 + c;
  if (selected >= 0 && targets.includes(i)) { await playerMove(selected, i); return; }
  const p = state.cells[i];
  if (p !== 0) {
    selected = i;
    if (p > 0) {
      // 己方棋子：显示可走点
      try {
        const d = await rpc('legal', { sq: i });
        targets = d.targets || [];
      } catch (_) { targets = []; }
    } else {
      // 对方棋子：不可走，但可「讲解这枚棋」（讲它的威胁与应对）
      targets = [];
    }
  } else {
    selected = -1; targets = [];
  }
  render();
}, { passive: false });

async function playerMove(from, to) {
  busy = true; render();
  try {
    const st = await rpc('move', { from, to });
    moves.push({ from, to });
    state = st; selected = -1; targets = [];
    persistMoves(); render();
    if (!state.game_over && !state.red_turn) await engineGo();
  } catch (e) { toast(e.message); }
  busy = false; render();
}

async function engineGo() {
  try {
    const st = await rpc('engine', { depth: 4 });
    if (st.engine_move) moves.push(st.engine_move);
    state = st;
    persistMoves(); render();
    if (navigator.vibrate) navigator.vibrate(15);
  } catch (_) { /* 对局结束等情况忽略 */ }
}

// ---------- 讲解 ----------
async function callLLMOnce(system, prompt) {
  const base = (settings.base_url || '').replace(/\/+$/, '');
  const modelName = settings.model || '';
  const body = {
    model: modelName,
    messages: [{ role: 'system', content: system }, { role: 'user', content: prompt }],
    temperature: 0.3, max_tokens: 2000,
  };
  // 4.7-flash 是混合思考模型：讲棋不需要深度思考，关掉可避免几十秒延迟与思考吃光 token 预算
  if (modelName === 'glm-4.7-flash') body.thinking = { type: 'disabled' };
  // 5.3 系思考不可关，但 reasoning_effort=low 可大幅提速（实测 30s+/空回复 -> 9s/322 字正常）
  if (modelName.startsWith('glm-5.3')) {
    body.reasoning_effort = 'low';
    body.thinking = { type: 'enabled' };
  }
  const timeoutMs = modelName.startsWith('glm-5.3') ? 60000 : 30000;
  const resp = await fetch(base + '/chat/completions', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', 'Authorization': 'Bearer ' + settings.key.trim() },
    body: JSON.stringify(body),
    signal: AbortSignal.timeout(timeoutMs),
  });
  if (!resp.ok) {
    // 透出服务商返回的具体原因（限流/Key 无效/未实名等）
    let detail = '';
    try {
      const v = await resp.json();
      detail = (v && v.error && (v.error.message || v.error.code)) || String(v).slice(0, 120);
    } catch (_) {
      detail = await resp.text().catch(() => '');
    }
    const err = new Error('AI 服务返回 ' + resp.status + (detail ? '：' + String(detail).slice(0, 150) : ''));
    err.status = resp.status;
    throw err;
  }
  const v = await resp.json();
  const m = v && v.choices && v.choices[0] && v.choices[0].message;
  // 思考型模型偶发正文为空但思考有内容：拿思考文本兜底，好过直接报错
  const t = (m && m.content && m.content.trim()) || (m && m.reasoning_content && String(m.reasoning_content).trim());
  if (!t) throw new Error('AI 返回内容为空');
  return t.trim();
}

// 免费模型并发/频率限流（429）常见，按 2s/4s 退避自动重试两次
async function callLLM(system, prompt) {
  let lastErr;
  for (let attempt = 0; attempt < 3; attempt++) {
    try {
      return await callLLMOnce(system, prompt);
    } catch (e) {
      lastErr = e;
      if (e.status !== 429 || attempt === 2) break;
      await new Promise((r) => setTimeout(r, 2000 * (attempt + 1)));
    }
  }
  throw lastErr;
}

let explaining = false;
async function explain(kind) {
  if (busy || reviewBusy || explaining) return;
  const el = $('explain');
  explaining = true; render();
  el.innerHTML = '<span class="thinking">教练思考中…</span>';
  try {
    const d = kind === 'position'
      ? await rpc('explain_position', { depth: 4 })
      : await rpc('explain_piece', { sq: selected, depth: 4 });
    let text = d.fallback, src = 'fallback';
    if (settings.key.trim()) {
      try {
        text = await callLLM(d.system, d.prompt);
        src = 'ai';
      } catch (e) {
        let hint = '';
        if (e.status === 429) {
          hint = /余额不足|资源包/.test(e.message)
            ? '（此 Key 无该模型的资源包：若用的是 GLM Coding Plan 订阅 Key，请把接口地址改为 https://open.bigmodel.cn/api/coding/paas/v4 并把模型改为 glm-5.3-flash）'
            : '（免费模型限流，自动重试后仍失败：稍等几秒再点一次）';
        }
        text = d.fallback + '\n（AI 讲解失败：' + e.message + hint + '，已用引擎基础提示）';
      }
    }
    typewriter(text, src);
  } catch (e) {
    el.innerHTML = '<span class="thinking">讲解失败：' + esc(e.message) + '</span>';
  }
  explaining = false; render();
}

// ---------- 复盘 ----------
async function runReview() {
  if (reviewBusy || busy) return;
  if (!moves.length) { toast('还没有棋谱可复盘'); return; }
  reviewBusy = true; render();
  $('reviewCard').style.display = 'block';
  const rowsEl = $('reviewRows'); rowsEl.innerHTML = '';
  const total = Math.ceil(moves.length / 2);
  let blunders = 0, bigBlunders = 0;
  for (let t = 0; t < total; t++) {
    $('reviewProgress').textContent = `分析中 ${t + 1}/${total}…`;
    try {
      const r = await rpc('review', { turn: t, depth: 3 });
      const cls = r.loss >= 150 ? ' bad' : r.loss >= 80 ? ' warn' : '';
      if (r.loss >= 80) blunders++;
      if (r.loss >= 150) bigBlunders++;
      const row = document.createElement('div');
      row.className = 'review-row' + cls;
      row.innerHTML = `<b>第${t + 1}手</b> ${esc(r.notation)}`
        + (r.loss > 0
          ? ` → 最佳 <b>${esc(r.best)}</b>，<span>亏 ${r.loss} 分</span>`
          : ' ✓ 与最佳一致');
      rowsEl.appendChild(row);
      rowsEl.scrollTop = rowsEl.scrollHeight;
    } catch (_) { /* 跳过无法分析的手数 */ }
  }
  $('reviewProgress').textContent =
    `复盘完成：共 ${total} 手，${blunders} 手亏损着法` + (bigBlunders ? `（其中 ${bigBlunders} 手大亏，红标提示）` : '');
  reviewBusy = false; render();
}

// ---------- 悔棋 / 新对局 ----------
async function undo() {
  if (busy || reviewBusy || !moves.length) return;
  const n = Math.min(2, moves.length);
  moves.splice(moves.length - n, n);
  selected = -1; targets = [];
  persistMoves();
  busy = true; render();
  try { state = await rpc('state'); } catch (_) {}
  busy = false; render();
}

async function newGame() {
  if (reviewBusy) return;
  if (moves.length && !confirm('要放弃当前对局，重新开始吗？')) return;
  moves = []; selected = -1; targets = []; state = null;
  persistMoves();
  busy = true; render();
  try { state = await rpc('state'); } catch (_) {}
  busy = false;
  $('explain').innerHTML = '<span class="thinking">新对局开始。点「AI 讲解当前局面」，教练告诉你这局面该怎么想。</span>';
  render();
}

// ---------- 设置 ----------
const mask = $('modalMask');
$('btnSettings').onclick = () => {
  $('inpBase').value = settings.base_url;
  $('inpModel').value = settings.model;
  $('inpKey').value = settings.key;
  $('inpKey').placeholder = settings.key ? '已保存（输入新 Key 可更换）' : '留空则使用引擎基础提示';
  mask.classList.add('show');
};
$('btnCancel').onclick = () => mask.classList.remove('show');
mask.addEventListener('click', (e) => { if (e.target === mask) mask.classList.remove('show'); });
$('btnSave').onclick = () => {
  const base = $('inpBase').value.trim();
  const model = $('inpModel').value.trim();
  settings.base_url = base || 'https://open.bigmodel.cn/api/paas/v4';
  settings.model = model || 'glm-4.7-flash';
  settings.key = $('inpKey').value.trim();
  persistSettings();
  mask.classList.remove('show');
  toast(settings.key ? '已保存，AI 讲解已就绪' : '已保存（未配 Key，用引擎基础提示）');
};

// ---------- 事件绑定与启动 ----------
$('btnExplainPos').onclick = () => explain('position');
$('btnExplainPiece').onclick = () => { if (selected >= 0) explain('piece'); };
$('btnUndo').onclick = undo;
$('btnNew').onclick = newGame;
$('btnReview').onclick = runReview;

async function init() {
  loadLocal();
  try {
    state = await rpc('state');
  } catch (_) {
    moves = [];
    persistMoves();
    try { state = await rpc('state'); } catch (e2) { toast('引擎加载失败：' + e2.message); return; }
  }
  resizeBoard();
  render();
  // 刷新发生在引擎思考时：恢复后轮到黑方则自动续走
  if (!state.game_over && !state.red_turn) { busy = true; render(); await engineGo(); busy = false; render(); }
}
init();
