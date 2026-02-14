const $ = (id) => document.getElementById(id);

const LS_KEY = 'gazetent.settings.v1';

let lastScenarios = [];
let lastFindings = [];
let active = { kind: null, id: null };
let activeModal = null;
let noteBaseline = 'preview';
let availablePacks = [];
let busy = false;

async function setHealth() {
  const el = $('health');
  const dot = $('healthDot');
  try {
    const r = await fetch('/api/health');
    if (!r.ok) throw new Error('bad');
    el.textContent = 'server=ok';
    dot.classList.remove('err');
    dot.classList.add('ok');
  } catch {
    el.textContent = 'server=down';
    dot.classList.remove('ok');
    dot.classList.add('err');
  }
}

function setBusy(on, kind) {
  busy = !!on;
  const runBtn = $('runBtn');
  const previewBtn = $('previewBtn');
  const clearBtn = $('clearBtn');

  runBtn.disabled = busy;
  previewBtn.disabled = busy;
  clearBtn.disabled = busy;

  runBtn.setAttribute('aria-busy', busy && kind === 'run' ? 'true' : 'false');
  previewBtn.setAttribute('aria-busy', busy && kind === 'preview' ? 'true' : 'false');

  // Keep the rest of the UI responsive but prevent accidental edits mid-run.
  $('packs').disabled = busy;
  $('provider').disabled = busy;
  $('baseUrl').disabled = busy;
  $('model').disabled = busy;
  $('apiKey').disabled = busy;
  $('concurrency').disabled = busy;
  $('timeoutMs').disabled = busy;
  $('maxTokens').disabled = busy;
  $('topK').disabled = busy;
  $('maxCanaryLeaks').disabled = busy;
  $('maxTotalRisk').disabled = busy;

  const packList = $('packList');
  packList.style.pointerEvents = busy ? 'none' : 'auto';
  packList.style.opacity = busy ? '0.7' : '1';
}

function escapeHtml(s) {
  return String(s)
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#039;');
}

function packPathsFromTextarea() {
  const packText = $('packs').value;
  return packText.split(/\n+/).map(s => s.trim()).filter(Boolean);
}

function setTextareaPacks(lines) {
  $('packs').value = lines.join('\n') + (lines.length ? '\n' : '');
}

function parseNum(s, fallback) {
  const n = Number(String(s ?? '').trim());
  return Number.isFinite(n) ? n : fallback;
}

function loadSettings() {
  try {
    const raw = localStorage.getItem(LS_KEY);
    if (!raw) return null;
    return JSON.parse(raw);
  } catch {
    return null;
  }
}

function saveSettings(obj) {
  try {
    localStorage.setItem(LS_KEY, JSON.stringify(obj));
  } catch {
    // ignore
  }
}

function readForm() {
  return {
    provider: $('provider').value || 'openAi',
    baseUrl: $('baseUrl').value.trim(),
    model: $('model').value.trim(),
    apiKey: $('apiKey').value,
    concurrency: parseNum($('concurrency').value, 16),
    timeoutMs: parseNum($('timeoutMs').value, 60000),
    maxTokens: parseNum($('maxTokens').value, 256),
    topK: ($('topK').value || '').trim() ? parseNum($('topK').value, 50) : null,
    maxCanaryLeaks: parseNum($('maxCanaryLeaks').value, 0),
    maxTotalRisk: parseNum($('maxTotalRisk').value, 50),
  };
}

function applyFormDefaults() {
  const s = loadSettings() || {};
  $('provider').value = s.provider ?? 'openAi';
  $('baseUrl').value = s.baseUrl ?? 'http://localhost:8000';
  $('model').value = s.model ?? 'meta-llama/Meta-Llama-3.1-8B-Instruct';
  $('apiKey').value = s.apiKey ?? '';
  $('concurrency').value = String(s.concurrency ?? 16);
  $('timeoutMs').value = String(s.timeoutMs ?? 60000);
  $('maxTokens').value = String(s.maxTokens ?? 256);
  $('topK').value = s.topK == null ? '50' : String(s.topK);
  $('maxCanaryLeaks').value = String(s.maxCanaryLeaks ?? 0);
  $('maxTotalRisk').value = String(s.maxTotalRisk ?? 50);
}

function persistForm() {
  const s = readForm();
  saveSettings({
    provider: s.provider,
    baseUrl: s.baseUrl,
    model: s.model,
    apiKey: s.apiKey,
    concurrency: s.concurrency,
    timeoutMs: s.timeoutMs,
    maxTokens: s.maxTokens,
    topK: s.topK,
    maxCanaryLeaks: s.maxCanaryLeaks,
    maxTotalRisk: s.maxTotalRisk,
  });
}

function renderScenarioRows(rows) {
  const tbody = $('rows');
  tbody.innerHTML = '';

  if (rows.length === 0) {
    tbody.innerHTML = `<tr><td colspan="5"><span class="clip muted">No scenarios loaded.</span></td></tr>`;
    return;
  }

  for (const s of rows) {
    const tr = document.createElement('tr');
    const isActive = active.kind === 'scenario' && active.id === s.id;
    tr.className = `rowBtn${isActive ? ' active' : ''}`;
    const hasSystem = !!(s.systemPrompt && String(s.systemPrompt).trim());
    const promptFirst = String(s.prompt || '').split(/\n/)[0];
    tr.innerHTML = `
      <td>${escapeHtml(s.id)}</td>
      <td><span class="badge">${escapeHtml(s.category)}</span></td>
      <td><span class="clip" title="${escapeHtml(s.prompt || '')}">${escapeHtml(promptFirst)}</span></td>
      <td>${hasSystem ? 'yes' : 'no'}</td>
      <td>${s.canary ? 'yes' : 'no'}</td>
    `;
    tr.addEventListener('click', () => openModal({
      kind: 'scenario',
      id: s.id,
      category: s.category,
      systemPrompt: s.systemPrompt || '',
      prompt: s.prompt || '',
      responseText: '',
      meta: `preview • system=${hasSystem ? 'yes' : 'no'} • canary=${s.canary ? 'yes' : 'no'}`,
    }));
    tbody.appendChild(tr);
  }
}

function renderFindingsRows(rows) {
  const tbody = $('findingsRows');
  tbody.innerHTML = '';

  if (rows.length === 0) {
    tbody.innerHTML = `<tr><td colspan="6"><span class="clip muted">No findings.</span></td></tr>`;
    return;
  }

  for (const f of rows) {
    const tr = document.createElement('tr');
    const id = f.scenarioId || f.scenario_id || '—';
    const cat = f.category || '—';
    const risk = Number(f.totalRisk ?? f.total_risk ?? 0).toFixed(1);
    const leak = Number((f.scores || {}).canaryLeak ?? 0).toFixed(0);
    const inj = Number((f.scores || {}).injHeuristic ?? 0).toFixed(0);
    const resp = String(f.responseText ?? f.response_text ?? '');
    const respFirst = resp.split(/\n/)[0];
    const isActive = active.kind === 'finding' && active.id === id;
    tr.className = `rowBtn${isActive ? ' active' : ''}`;
    tr.innerHTML = `
      <td>${escapeHtml(id)}</td>
      <td><span class="badge">${escapeHtml(cat)}</span></td>
      <td>${escapeHtml(risk)}</td>
      <td>${escapeHtml(leak)}</td>
      <td>${escapeHtml(inj)}</td>
      <td><span class="clip" title="${escapeHtml(resp)}">${escapeHtml(respFirst)}</span></td>
    `;
    tr.addEventListener('click', () => openModal({
      kind: 'finding',
      id,
      category: cat,
      systemPrompt: String(f.systemPrompt ?? f.system_prompt ?? ''),
      prompt: String(f.prompt ?? ''),
      responseText: resp,
      meta: `risk=${risk} • leak=${leak} • inj=${inj}`,
    }));
    tbody.appendChild(tr);
  }
}

function applySearch() {
  const q = $('search').value.trim().toLowerCase();
  if (!q) {
    renderScenarioRows(lastScenarios);
    return;
  }
  const filtered = lastScenarios.filter(s =>
    String(s.id).toLowerCase().includes(q) ||
    String(s.category).toLowerCase().includes(q) ||
    String(s.prompt).toLowerCase().includes(q)
  );
  renderScenarioRows(filtered);
}

async function preview() {
  if (busy) return;
  setBusy(true, 'preview');
  $('count').textContent = 'loading…';
  $('note').classList.remove('good', 'bad');
  $('note').textContent = 'preview';
  noteBaseline = 'preview';

  $('rows').innerHTML = `
    <tr><td colspan="5"><div class="skeletonLine" style="width: 62%"></div></td></tr>
    <tr><td colspan="5"><div class="skeletonLine" style="width: 78%"></div></td></tr>
    <tr><td colspan="5"><div class="skeletonLine" style="width: 54%"></div></td></tr>
  `;

  try {
    const packPaths = packPathsFromTextarea();
    const r = await fetch('/api/packs/preview', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ packPaths })
    });

    if (!r.ok) {
      const msg = await r.text();
      $('count').textContent = 'error';
      $('note').classList.remove('good');
      $('note').classList.add('bad');
      $('note').textContent = msg;
      noteBaseline = msg;
      lastScenarios = [];
      renderScenarioRows([]);
      return;
    }

    const data = await r.json();
    lastScenarios = data.scenarios || [];
    active = { kind: null, id: null };
    activeModal = null;
    $('count').textContent = `scenarios_loaded=${data.scenariosLoaded ?? lastScenarios.length}`;
    $('note').classList.remove('good', 'bad');
    $('note').textContent = `packs=${packPaths.length}`;
    noteBaseline = `packs=${packPaths.length}`;
    $('search').value = '';
    renderScenarioRows(lastScenarios);
  } finally {
    setBusy(false, 'preview');
  }
}

async function runJob() {
  if (busy) return;
  setBusy(true, 'run');
  persistForm();
  $('count').textContent = 'running…';
  $('note').classList.remove('good', 'bad');
  $('note').textContent = 'run';
  noteBaseline = 'run';

  $('findingsRows').innerHTML = `
    <tr><td colspan="6"><div class="skeletonLine" style="width: 68%"></div></td></tr>
    <tr><td colspan="6"><div class="skeletonLine" style="width: 52%"></div></td></tr>
    <tr><td colspan="6"><div class="skeletonLine" style="width: 74%"></div></td></tr>
  `;

  const s = readForm();
  const packPaths = packPathsFromTextarea();
  const body = {
    provider: s.provider,
    baseUrl: s.baseUrl,
    model: s.model,
    apiKey: s.apiKey || null,
    packPaths,
    concurrency: Math.max(1, Math.min(256, Math.floor(s.concurrency))),
    timeoutMs: Math.max(1000, Math.floor(s.timeoutMs)),
    maxTokens: Math.max(1, Math.min(8192, Math.floor(s.maxTokens))),
    maxCanaryLeaks: Math.max(0, Math.floor(s.maxCanaryLeaks)),
    maxTotalRisk: Number(s.maxTotalRisk),
    topK: s.topK == null ? null : Math.max(1, Math.floor(s.topK)),
  };

  try {
    const r = await fetch('/api/run', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });

    if (!r.ok) {
      const msg = await r.text();
      $('count').textContent = 'error';
      $('note').classList.remove('good');
      $('note').classList.add('bad');
      $('note').textContent = msg;
      noteBaseline = msg;
      lastFindings = [];
      renderFindingsRows([]);
      return;
    }

    const data = await r.json();
    const summary = data.summary || {};
    lastFindings = data.findings || [];
    active = { kind: null, id: null };
    activeModal = null;

    const gate = summary.gatePass ?? summary.gate_pass;
    $('count').textContent = `findings=${summary.findingsTotal ?? summary.findings_total ?? lastFindings.length}`;
    $('note').classList.remove('good', 'bad');
    $('note').classList.add(gate ? 'good' : 'bad');
    $('note').textContent = `gate_pass=${gate ? 'true' : 'false'}`;
    noteBaseline = `gate_pass=${gate ? 'true' : 'false'}`;

    renderFindingsRows(lastFindings);
  } finally {
    setBusy(false, 'run');
  }
}

function clearPacks() {
  if (busy) return;
  $('packs').value = '';
  lastScenarios = [];
  lastFindings = [];
  active = { kind: null, id: null };
  activeModal = null;
  $('count').textContent = 'scenarios_loaded=0';
  $('note').classList.remove('good', 'bad');
  $('note').textContent = 'preview';
  noteBaseline = 'preview';
  $('search').value = '';
  renderScenarioRows([]);
  renderFindingsRows([]);
  updatePackCheckboxesFromTextarea();
}

function openModal(obj) {
  active = { kind: obj.kind, id: obj.id };
  activeModal = obj;

  $('modalId').textContent = obj.id;
  $('modalMeta').textContent = `${obj.category} • ${obj.meta}`;
  $('modalSystem').textContent = obj.systemPrompt && obj.systemPrompt.trim() ? obj.systemPrompt : '—';
  $('modalPrompt').textContent = obj.prompt || '—';
  $('modalResponse').textContent = obj.responseText && obj.responseText.trim() ? obj.responseText : '—';
  $('modal').classList.add('show');

  // refresh highlights
  renderScenarioRows($('search').value.trim() ? lastScenarios.filter(x => {
    const q = $('search').value.trim().toLowerCase();
    return String(x.id).toLowerCase().includes(q) ||
      String(x.category).toLowerCase().includes(q) ||
      String(x.prompt).toLowerCase().includes(q);
  }) : lastScenarios);
  renderFindingsRows(lastFindings);
}

function closeModal() {
  $('modal').classList.remove('show');
}

async function copyModalPrompt() {
  if (!activeModal) return;
  const text = activeModal.prompt || '';
  const prev = $('note').textContent;
  try {
    await navigator.clipboard.writeText(text);
    $('note').textContent = 'copied';
    setTimeout(() => { $('note').textContent = prev || noteBaseline; }, 900);
  } catch {
    const sel = window.getSelection();
    if (!sel) return;
    sel.removeAllRanges();
    const range = document.createRange();
    range.selectNodeContents($('modalPrompt'));
    sel.addRange(range);
    $('note').textContent = 'select-to-copy';
    setTimeout(() => { $('note').textContent = prev || noteBaseline; }, 900);
  }
}

function wireShortcuts() {
  document.addEventListener('keydown', (e) => {
    const meta = e.metaKey || e.ctrlKey;
    if (e.key === 'Escape') {
      closeModal();
      return;
    }
    if (busy) return;
    if (meta && (e.key === 'k' || e.key === 'K')) {
      e.preventDefault();
      $('search').focus();
      $('search').select();
      return;
    }
    if (meta && e.key === 'Enter') {
      e.preventDefault();
      preview();
      return;
    }
  });
}

function renderPackList(packs) {
  const root = $('packList');
  root.innerHTML = '';
  if (!packs.length) {
    root.innerHTML = `<div style="color: var(--muted); font-size: 12px">No packs found.</div>`;
    return;
  }

  for (const p of packs) {
    const row = document.createElement('div');
    row.className = 'packItem';
    row.innerHTML = `
      <div class="packLeft">
        <input class="check" type="checkbox" data-pack="${escapeHtml(p)}" />
        <div class="packName" title="${escapeHtml(p)}">${escapeHtml(p)}</div>
      </div>
      <div class="packTag">yaml</div>
    `;
    const check = row.querySelector('input');
    check.addEventListener('change', () => syncTextareaWithChecks());
    row.addEventListener('click', (e) => {
      if (e.target && (e.target.tagName === 'INPUT' || e.target.closest('input'))) return;
      check.checked = !check.checked;
      syncTextareaWithChecks();
    });
    root.appendChild(row);
  }

  updatePackCheckboxesFromTextarea();
}

function selectedPacksFromChecks() {
  const checks = $('packList').querySelectorAll('input[type="checkbox"][data-pack]');
  const out = [];
  for (const c of checks) {
    if (c.checked) out.push(c.getAttribute('data-pack'));
  }
  return out;
}

function updatePackCheckboxesFromTextarea() {
  const current = new Set(packPathsFromTextarea());
  const checks = $('packList').querySelectorAll('input[type="checkbox"][data-pack]');
  for (const c of checks) {
    const p = c.getAttribute('data-pack');
    c.checked = current.has(p);
  }
}

function syncTextareaWithChecks() {
  const checked = selectedPacksFromChecks();
  const current = packPathsFromTextarea();
  const avail = new Set(availablePacks);
  const custom = current.filter(p => !avail.has(p));
  const merged = [...checked, ...custom];
  setTextareaPacks(merged);
}

async function loadPacks() {
  try {
    const r = await fetch('/api/packs/list');
    if (!r.ok) throw new Error('bad');
    const data = await r.json();
    availablePacks = data.packs || [];
    renderPackList(availablePacks);
  } catch {
    availablePacks = [];
    renderPackList([]);
  }
}

function wire() {
  applyFormDefaults();

  $('previewBtn').addEventListener('click', preview);
  $('runBtn').addEventListener('click', runJob);
  $('clearBtn').addEventListener('click', clearPacks);

  $('packs').addEventListener('input', () => updatePackCheckboxesFromTextarea());
  $('search').addEventListener('input', applySearch);

  $('copyBtn')?.addEventListener('click', copyModalPrompt);
  $('closeBtn').addEventListener('click', closeModal);
  $('modal').addEventListener('click', (e) => {
    if (e.target.id === 'modal') closeModal();
  });

  for (const id of ['provider','baseUrl','model','apiKey','concurrency','timeoutMs','maxTokens','topK','maxCanaryLeaks','maxTotalRisk']) {
    $(id).addEventListener('change', persistForm);
  }

  setHealth();
  setInterval(setHealth, 5000);
  wireShortcuts();
  loadPacks();
  updatePackCheckboxesFromTextarea();
  renderFindingsRows([]);
}

wire();
