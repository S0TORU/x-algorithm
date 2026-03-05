const $ = (id) => document.getElementById(id);

const LS_KEY = 'gazetent.settings.v1';

let lastScenarios = [];
let lastFindings = [];
let active = { kind: null, id: null };
let activeModal = null;
let noteBaseline = 'preview';
let availablePacks = [];
let busy = false;
let currentRun = null;
let runsCache = [];
let currentMode = 'single';

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

function validate() {
  const baseOk = !!$('baseUrl').value.trim();
  const modelOk = !!$('model').value.trim();
  const packsOk = packPathsFromTextarea().length > 0;
  const batchOk = parseBatchSpecs().length > 0;

  $('baseUrl').classList.toggle('invalid', !baseOk);
  $('model').classList.toggle('invalid', !modelOk);
  $('packs').classList.toggle('invalid', !packsOk);
  $('batchSpecs').classList.toggle('invalid', currentMode === 'batch' && !batchOk);

  const previewBtn = $('previewBtn');
  const runBtn = $('runBtn');
  const runBatchBtn = $('runBatchBtn');
  previewBtn.disabled = busy || !packsOk;
  runBtn.disabled = busy || currentMode !== 'single' || !(baseOk && modelOk && packsOk);
  runBatchBtn.disabled = busy || currentMode !== 'batch' || !(baseOk && packsOk && batchOk);

  return { baseOk, modelOk, packsOk, batchOk };
}

function ts() {
  const d = new Date();
  const hh = String(d.getHours()).padStart(2, '0');
  const mm = String(d.getMinutes()).padStart(2, '0');
  const ss = String(d.getSeconds()).padStart(2, '0');
  return `${hh}:${mm}:${ss}`;
}

function logEvent(msg, kind) {
  const root = $('log');
  if (!root) return;
  const row = document.createElement('div');
  row.className = `logRow${kind ? ` ${kind}` : ''}`;
  row.innerHTML = `<div class="logTs">${escapeHtml(ts())}</div><div class="logMsg">${escapeHtml(msg)}</div>`;
  root.appendChild(row);
  while (root.children.length > 80) root.removeChild(root.firstChild);
  root.scrollTop = root.scrollHeight;
}

function setMode(mode) {
  currentMode = mode === 'batch' ? 'batch' : 'single';
  $('modeSingleBtn').classList.toggle('active', currentMode === 'single');
  $('modeBatchBtn').classList.toggle('active', currentMode === 'batch');
  $('batchCard').classList.toggle('hidden', currentMode !== 'batch');
  $('runBtn').classList.toggle('hidden', currentMode !== 'single');
  validate();
}

function setSummary({ runId, scenariosTotal, summary, artifacts }) {
  currentRun = { runId, scenariosTotal, summary, artifacts };

  $('sumRunId').textContent = runId ? String(runId).slice(0, 8) : '—';

  const gatePass = summary ? (summary.gatePass ?? summary.gate_pass) : null;
  const gateEl = $('sumGate');
  gateEl.textContent = gatePass == null ? '—' : (gatePass ? 'pass' : 'fail');
  gateEl.classList.toggle('good', !!gatePass);
  gateEl.classList.toggle('bad', gatePass === false);

  const totalRisk = summary ? Number(summary.totalRisk ?? summary.total_risk ?? 0) : null;
  $('sumRisk').textContent = totalRisk == null ? '—' : totalRisk.toFixed(1);

  const leaks = summary ? Number(summary.canaryLeaks ?? summary.canary_leaks ?? 0) : null;
  $('sumLeaks').textContent = leaks == null ? '—' : String(leaks);

  $('sumScenarios').textContent = scenariosTotal == null ? '—' : String(scenariosTotal);
  $('sumFindings').textContent = summary ? String(summary.findingsTotal ?? summary.findings_total ?? '—') : '—';

  const dlSummary = $('dlSummary');
  const dlFindings = $('dlFindings');
  const dlConfig = $('dlConfig');
  if (!artifacts || !artifacts.summaryUrl) {
    dlSummary.setAttribute('href', '#');
    dlFindings.setAttribute('href', '#');
    dlConfig.setAttribute('href', '#');
    dlSummary.classList.add('btnDisabled');
    dlFindings.classList.add('btnDisabled');
    dlConfig.classList.add('btnDisabled');
    return;
  }
  dlSummary.classList.remove('btnDisabled');
  dlFindings.classList.remove('btnDisabled');
  dlConfig.classList.remove('btnDisabled');
  dlSummary.href = artifacts.summaryUrl;
  dlFindings.href = artifacts.findingsUrl;
  dlConfig.href = artifacts.configUrl;
}

function setBusy(on, kind) {
  busy = !!on;
  const runBtn = $('runBtn');
  const previewBtn = $('previewBtn');
  const runBatchBtn = $('runBatchBtn');
  const clearBtn = $('clearBtn');
  const refreshRunsBtn = $('refreshRunsBtn');
  const compareBtn = $('compareBtn');
  const clearCompareBtn = $('clearCompareBtn');

  runBtn.disabled = busy;
  previewBtn.disabled = busy;
  runBatchBtn.disabled = busy;
  clearBtn.disabled = busy;
  if (refreshRunsBtn) refreshRunsBtn.disabled = busy;
  if (compareBtn) compareBtn.disabled = busy;
  if (clearCompareBtn) clearCompareBtn.disabled = busy;

  runBtn.setAttribute('aria-busy', busy && kind === 'run' ? 'true' : 'false');
  runBatchBtn.setAttribute('aria-busy', busy && kind === 'batch' ? 'true' : 'false');
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
  $('batchSpecs').disabled = busy;
  $('compareRunIds').disabled = busy;
  $('runsSearch').disabled = busy;
  $('clearLogBtn').disabled = busy;
  $('modeSingleBtn').disabled = busy;
  $('modeBatchBtn').disabled = busy;

  const packList = $('packList');
  packList.style.pointerEvents = busy ? 'none' : 'auto';
  packList.style.opacity = busy ? '0.7' : '1';

  if (!busy) validate();
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

function parseBatchSpecs() {
  const lines = String($('batchSpecs').value || '')
    .split('\n')
    .map((x) => x.trim())
    .filter(Boolean);

  const out = [];
  for (const line of lines) {
    const parts = line.split('|').map((x) => x.trim());
    out.push({
      label: parts[0] || '',
      model: parts[1] || '',
      baseUrl: parts[2] || '',
      provider: parts[3] || '',
    });
  }
  return out;
}

function applyFormDefaults() {
  const s = loadSettings() || {};
  if (Object.prototype.hasOwnProperty.call(s, 'apiKey')) {
    delete s.apiKey;
    saveSettings(s);
  }
  $('provider').value = s.provider ?? 'openAi';
  $('baseUrl').value = s.baseUrl ?? 'http://localhost:8000';
  $('model').value = s.model ?? 'meta-llama/Meta-Llama-3.1-8B-Instruct';
  $('apiKey').value = '';
  $('concurrency').value = String(s.concurrency ?? 16);
  $('timeoutMs').value = String(s.timeoutMs ?? 60000);
  $('maxTokens').value = String(s.maxTokens ?? 256);
  $('topK').value = s.topK == null ? '50' : String(s.topK);
  $('maxCanaryLeaks').value = String(s.maxCanaryLeaks ?? 0);
  $('maxTotalRisk').value = String(s.maxTotalRisk ?? 50);
  $('batchSpecs').value = s.batchSpecs ?? $('batchSpecs').value;
  setMode(s.mode ?? 'single');
}

function persistForm() {
  const s = readForm();
  saveSettings({
    mode: currentMode,
    provider: s.provider,
    baseUrl: s.baseUrl,
    model: s.model,
    concurrency: s.concurrency,
    timeoutMs: s.timeoutMs,
    maxTokens: s.maxTokens,
    topK: s.topK,
    maxCanaryLeaks: s.maxCanaryLeaks,
    maxTotalRisk: s.maxTotalRisk,
    batchSpecs: $('batchSpecs').value,
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
    drawRiskCloud([]);
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
  drawRiskCloud(rows);
}

function categoryColor(cat) {
  const s = String(cat || 'x');
  let h = 0;
  for (let i = 0; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) % 360;
  return `hsla(${h}, 75%, 58%, 0.85)`;
}

function drawRiskCloud(rows) {
  const canvas = $('riskCloud');
  if (!canvas) return;
  const ctx = canvas.getContext('2d');
  const w = canvas.width;
  const h = canvas.height;
  ctx.clearRect(0, 0, w, h);

  ctx.fillStyle = 'rgba(110,120,130,0.15)';
  ctx.fillRect(0, 0, w, h);

  const pad = 24;
  ctx.strokeStyle = 'rgba(140,150,170,0.35)';
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(pad, h - pad);
  ctx.lineTo(w - pad, h - pad);
  ctx.moveTo(pad, h - pad);
  ctx.lineTo(pad, pad);
  ctx.stroke();

  ctx.fillStyle = 'rgba(130,140,160,0.9)';
  ctx.font = '11px ui-monospace, SFMono-Regular, Menlo, monospace';
  ctx.fillText('inj heuristic →', w - 120, h - 8);
  ctx.fillText('risk ↑', 8, 16);

  if (!rows.length) {
    ctx.fillStyle = 'rgba(130,140,160,0.9)';
    ctx.fillText('No findings to visualize.', pad, h / 2);
    return;
  }

  for (const f of rows) {
    const inj = Number((f.scores || {}).injHeuristic ?? 0);
    const risk = Number(f.totalRisk ?? f.total_risk ?? 0);
    const x = pad + Math.max(0, Math.min(100, inj)) * ((w - 2 * pad) / 100);
    const y = (h - pad) - Math.max(0, Math.min(100, risk)) * ((h - 2 * pad) / 100);
    const r = 3 + Math.max(0, Math.min(16, risk / 10));
    ctx.fillStyle = categoryColor(f.category);
    ctx.beginPath();
    ctx.arc(x, y, r, 0, 2 * Math.PI);
    ctx.fill();
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
  logEvent('preview packs', null);

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
      logEvent(`preview error: ${msg}`, 'err');
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
    logEvent(`loaded scenarios=${data.scenariosLoaded ?? lastScenarios.length}`, 'ok');
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
  logEvent('run job', null);

  $('findingsRows').innerHTML = `
    <tr><td colspan="6"><div class="skeletonLine" style="width: 68%"></div></td></tr>
    <tr><td colspan="6"><div class="skeletonLine" style="width: 52%"></div></td></tr>
    <tr><td colspan="6"><div class="skeletonLine" style="width: 74%"></div></td></tr>
  `;

  const body = buildRunRequestFromForm();

  try {
    const data = await postJson('/api/run', body);
    applyRunResult(data);
    logEvent(`run=${String(data.runId ?? data.run_id ?? '').slice(0, 8)} complete`, 'ok');
    refreshRuns();
  } catch (e) {
    const msg = String(e);
    $('count').textContent = 'error';
    $('note').classList.remove('good');
    $('note').classList.add('bad');
    $('note').textContent = msg;
    noteBaseline = msg;
    logEvent(`run error: ${msg}`, 'err');
    lastFindings = [];
    renderFindingsRows([]);
  } finally {
    setBusy(false, 'run');
  }
}

function buildRunRequestFromForm(overrides) {
  const s = readForm();
  const packPaths = packPathsFromTextarea();
  return {
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
    ...(overrides || {}),
  };
}

function applyRunResult(data) {
  const summary = data.summary || {};
  const artifacts = data.artifacts || null;
  const runId = data.runId ?? data.run_id ?? '';
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
  setSummary({
    runId,
    scenariosTotal: data.scenariosTotal ?? data.scenarios_total ?? null,
    summary,
    artifacts,
  });
  logEvent(`run=${String(runId).slice(0, 8)} gate=${gate ? 'pass' : 'fail'} findings=${lastFindings.length}`, gate ? 'ok' : 'err');
}

async function runBatch() {
  if (busy) return;
  setBusy(true, 'batch');
  persistForm();
  $('count').textContent = 'running_batch…';
  $('note').classList.remove('good', 'bad');
  $('note').textContent = 'batch';
  noteBaseline = 'batch';
  logEvent('run batch', null);

  try {
    const defaultForm = readForm();
    const specs = parseBatchSpecs();
    const runs = specs.map((spec, idx) => ({
      label: spec.label || `run-${String(idx + 1).padStart(2, '0')}`,
      ...buildRunRequestFromForm({
        provider: spec.provider || defaultForm.provider,
        model: spec.model || defaultForm.model,
        baseUrl: spec.baseUrl || defaultForm.baseUrl,
      }),
    }));
    const data = await postJson('/api/run/batch', { runs });

    const items = data.items || [];
    const ok = Number(data.passedRuns ?? data.passed_runs ?? 0);
    const fail = Number(data.failedRuns ?? data.failed_runs ?? 0);
    $('count').textContent = `batch=${items.length}`;
    $('note').classList.remove('good', 'bad');
    $('note').classList.add(fail === 0 ? 'good' : 'bad');
    $('note').textContent = `pass=${ok} fail=${fail}`;
    noteBaseline = `pass=${ok} fail=${fail}`;
    logEvent(`batch=${items.length} pass=${ok} fail=${fail}`, fail === 0 ? 'ok' : 'err');

    const firstSuccess = items.find((x) => x.runId || x.run_id);
    if (firstSuccess && (firstSuccess.runId || firstSuccess.run_id)) {
      await openRun(String(firstSuccess.runId || firstSuccess.run_id));
    } else {
      renderFindingsRows([]);
      setSummary({ runId: null, scenariosTotal: null, summary: null, artifacts: null });
    }
    refreshRuns();
  } catch (e) {
    const msg = String(e);
    $('count').textContent = 'error';
    $('note').classList.remove('good');
    $('note').classList.add('bad');
    $('note').textContent = msg;
    noteBaseline = msg;
    logEvent(`batch error: ${msg}`, 'err');
  } finally {
    setBusy(false, 'batch');
  }
}

async function postJson(url, body) {
  const r = await fetch(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!r.ok) throw new Error(await r.text());
  return await r.json();
}

function compareRunIdsFromTextarea() {
  return String($('compareRunIds').value || '')
    .split(/\n+/)
    .map((x) => x.trim())
    .filter(Boolean);
}

function setCompareRunIds(ids) {
  const uniq = [];
  for (const id of ids) {
    if (!uniq.includes(id)) uniq.push(id);
  }
  $('compareRunIds').value = uniq.slice(0, 5).join('\n');
}

function addCompareRunId(id) {
  if (!id || id === '—') return;
  const ids = compareRunIdsFromTextarea();
  if (!ids.includes(id)) ids.push(id);
  setCompareRunIds(ids);
}

async function runCompare() {
  const ids = compareRunIdsFromTextarea();
  if (ids.length < 2) {
    $('compareOut').textContent = 'Select at least 2 run IDs.';
    return;
  }
  if (ids.length > 5) {
    $('compareOut').textContent = 'Use at most 5 run IDs.';
    return;
  }

  try {
    const data = await postJson('/api/runs/compare', { runIds: ids });
    const base = data.base || {};
    const items = data.items || [];
    if (!items.length) {
      $('compareOut').textContent = 'No comparison data.';
      return;
    }
    const lines = [
      `base=${String(base.runId || '').slice(0, 8)} risk=${Number(base.summary?.totalRisk ?? base.summary?.total_risk ?? 0).toFixed(1)}`
    ];
    for (const item of items) {
      const rid = String(item.runId || item.run_id || '').slice(0, 8);
      const dr = Number(item.deltaTotalRisk ?? item.delta_total_risk ?? 0).toFixed(1);
      const dl = Number(item.deltaCanaryLeaks ?? item.delta_canary_leaks ?? 0);
      const df = Number(item.deltaFindingsTotal ?? item.delta_findings_total ?? 0);
      const gc = item.gateChanged ?? item.gate_changed;
      lines.push(`${rid}: Δrisk=${dr} Δleaks=${dl} Δfind=${df} gateChanged=${gc ? 'yes' : 'no'}`);
    }
    $('compareOut').textContent = lines.join(' | ');
    logEvent(`compare done (${ids.length} runs)`, 'ok');
  } catch (e) {
    $('compareOut').textContent = `Compare error: ${String(e)}`;
    logEvent(`compare error: ${String(e)}`, 'err');
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
  setSummary({ runId: null, scenariosTotal: null, summary: null, artifacts: null });
  logEvent('cleared packs', null);
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
      if (currentMode === 'batch') {
        runBatch();
      } else {
        runJob();
      }
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

function renderRunsRows(rows) {
  const tbody = $('runsRows');
  tbody.innerHTML = '';
  if (!rows.length) {
    tbody.innerHTML = `<tr><td colspan="7"><span class="clip muted">No runs yet.</span></td></tr>`;
    return;
  }

  for (const r of rows) {
    const tr = document.createElement('tr');
    tr.className = 'rowBtn';
    const rid = r.runId ?? r.run_id ?? '—';
    const ms = Number(r.modifiedMs ?? r.modified_ms ?? 0);
    const time = ms ? new Date(ms).toLocaleString() : '—';
    const s = r.summary || null;
    const gate = s ? (s.gatePass ?? s.gate_pass) : null;
    const risk = s ? Number(s.totalRisk ?? s.total_risk ?? 0).toFixed(1) : '—';
    const leaks = s ? String(s.canaryLeaks ?? s.canary_leaks ?? 0) : '—';
    const scn = r.scenariosTotal ?? r.scenarios_total ?? '—';
    const find = s ? String(s.findingsTotal ?? s.findings_total ?? '—') : '—';

    tr.innerHTML = `
      <td><span class="clip" title="${escapeHtml(rid)}">${escapeHtml(String(rid).slice(0, 12))}</span></td>
      <td><span class="clip" title="${escapeHtml(time)}">${escapeHtml(time)}</span></td>
      <td>${gate == null ? '—' : (gate ? 'pass' : 'fail')}</td>
      <td>${escapeHtml(risk)}</td>
      <td>${escapeHtml(leaks)}</td>
      <td>${escapeHtml(String(scn))}</td>
      <td>${escapeHtml(String(find))}</td>
    `;

    tr.addEventListener('click', () => {
      addCompareRunId(String(rid));
      openRun(String(rid));
    });
    tbody.appendChild(tr);
  }
}

function applyRunsSearch() {
  const q = $('runsSearch').value.trim().toLowerCase();
  if (!q) {
    renderRunsRows(runsCache);
    return;
  }
  renderRunsRows(runsCache.filter(r => String(r.runId ?? r.run_id ?? '').toLowerCase().includes(q)));
}

async function refreshRuns() {
  if (busy) return;
  const btn = $('refreshRunsBtn');
  btn?.setAttribute('aria-busy', 'true');
  try {
    const r = await fetch('/api/runs/list');
    if (!r.ok) throw new Error(await r.text());
    const data = await r.json();
    runsCache = data.runs || [];
    applyRunsSearch();
    logEvent(`runs refreshed (${runsCache.length})`, 'ok');
  } catch (e) {
    logEvent(`runs refresh error: ${String(e)}`, 'err');
  } finally {
    btn?.setAttribute('aria-busy', 'false');
  }
}

async function openRun(runId) {
  if (!runId || runId === '—') return;
  if (busy) return;
  logEvent(`open run=${String(runId).slice(0, 8)}`, null);

  $('findingsRows').innerHTML = `
    <tr><td colspan="6"><div class="skeletonLine" style="width: 72%"></div></td></tr>
    <tr><td colspan="6"><div class="skeletonLine" style="width: 64%"></div></td></tr>
  `;

  try {
    const r = await fetch(`/api/runs/${encodeURIComponent(runId)}`);
    if (!r.ok) throw new Error(await r.text());
    const data = await r.json();
    const summary = data.summary || {};
    const artifacts = data.artifacts || null;
    const loadedFindings = data.findings || [];

    lastFindings = loadedFindings;
    renderFindingsRows(lastFindings);

    const gate = summary.gatePass ?? summary.gate_pass;
    $('count').textContent = `findings=${summary.findingsTotal ?? summary.findings_total ?? lastFindings.length}`;
    $('note').classList.remove('good', 'bad');
    $('note').classList.add(gate ? 'good' : 'bad');
    $('note').textContent = `gate_pass=${gate ? 'true' : 'false'}`;
    noteBaseline = `gate_pass=${gate ? 'true' : 'false'}`;

    setSummary({
      runId: data.runId ?? data.run_id ?? runId,
      scenariosTotal: data.scenariosTotal ?? data.scenarios_total ?? null,
      summary,
      artifacts,
    });
    logEvent(`loaded run findings=${lastFindings.length}`, 'ok');
  } catch (e) {
    logEvent(`open run error: ${String(e)}`, 'err');
    renderFindingsRows([]);
  }
}

function wire() {
  applyFormDefaults();

  $('modeSingleBtn').addEventListener('click', () => {
    setMode('single');
    persistForm();
  });
  $('modeBatchBtn').addEventListener('click', () => {
    setMode('batch');
    persistForm();
  });

  $('previewBtn').addEventListener('click', preview);
  $('runBtn').addEventListener('click', runJob);
  $('runBatchBtn').addEventListener('click', runBatch);
  $('clearBtn').addEventListener('click', clearPacks);
  $('refreshRunsBtn').addEventListener('click', refreshRuns);
  $('runsSearch').addEventListener('input', applyRunsSearch);
  $('compareBtn').addEventListener('click', runCompare);
  $('clearCompareBtn').addEventListener('click', () => {
    $('compareRunIds').value = '';
    $('compareOut').textContent = 'No comparison yet.';
  });
  $('clearLogBtn').addEventListener('click', () => {
    $('log').innerHTML = '';
    logEvent('log cleared', null);
  });

  $('packs').addEventListener('input', () => {
    updatePackCheckboxesFromTextarea();
    validate();
    persistForm();
  });
  $('batchSpecs').addEventListener('input', () => {
    validate();
    persistForm();
  });
  $('compareRunIds').addEventListener('input', () => setCompareRunIds(compareRunIdsFromTextarea()));
  $('search').addEventListener('input', applySearch);

  $('copyBtn')?.addEventListener('click', copyModalPrompt);
  $('closeBtn').addEventListener('click', closeModal);
  $('modal').addEventListener('click', (e) => {
    if (e.target.id === 'modal') closeModal();
  });

  for (const id of ['provider','baseUrl','model','apiKey','concurrency','timeoutMs','maxTokens','topK','maxCanaryLeaks','maxTotalRisk']) {
    $(id).addEventListener('change', () => {
      persistForm();
      validate();
    });
  }

  setHealth();
  setInterval(setHealth, 5000);
  wireShortcuts();
  loadPacks();
  updatePackCheckboxesFromTextarea();
  renderFindingsRows([]);
  setSummary({ runId: null, scenariosTotal: null, summary: null, artifacts: null });
  logEvent('ready', 'ok');
  refreshRuns();
  validate();
  drawRiskCloud([]);
}

wire();
