const $ = (id) => document.getElementById(id);

const LS_KEY = 'gazetent.settings.v1';
const PACK_CATALOG = {
  'examples/packs/authority_spoofing.yaml': {
    title: 'Authority Spoofing',
    blurb: 'Executive, legal, and compliance pressure prompts that impersonate privileged users.',
    suites: ['adversarial'],
    scenarios: 2,
  },
  'examples/packs/basic_injection.yaml': {
    title: 'Core Injection',
    blurb: 'Direct override attempts against system and developer instructions.',
    suites: ['core', 'adversarial'],
    scenarios: 2,
  },
  'examples/packs/canary_leak.yaml': {
    title: 'Canary Leakage',
    blurb: 'Synthetic secret exfiltration tests with deterministic canary checks.',
    suites: ['core', 'leakage'],
    scenarios: 2,
  },
  'examples/packs/delegation_hijack.yaml': {
    title: 'Delegation Hijack',
    blurb: 'Sub-agent and helper-agent prompts that try to route around policy.',
    suites: ['adversarial'],
    scenarios: 2,
  },
  'examples/packs/encoding_evasion.yaml': {
    title: 'Encoding Evasion',
    blurb: 'Base64 and transformation prompts that try to bypass guardrails indirectly.',
    suites: ['adversarial'],
    scenarios: 2,
  },
  'examples/packs/jailbreak_suffix.yaml': {
    title: 'Jailbreak Suffix',
    blurb: 'Suffix and override strings meant to flip behavior late in the prompt.',
    suites: ['adversarial'],
    scenarios: 2,
  },
  'examples/packs/long_context_override.yaml': {
    title: 'Long Context Override',
    blurb: 'Late-context and long-input instructions designed to outrank earlier policy.',
    suites: ['core', 'adversarial'],
    scenarios: 2,
  },
  'examples/packs/markdown_injection.yaml': {
    title: 'Markdown Injection',
    blurb: 'Markdown, HTML, and code-block formatting prompts that try to expose hidden values.',
    suites: ['adversarial'],
    scenarios: 2,
  },
  'examples/packs/memory_poisoning.yaml': {
    title: 'Memory Poisoning',
    blurb: 'Prompts that try to persist unsafe instructions or hidden values across turns.',
    suites: ['core', 'adversarial'],
    scenarios: 2,
  },
  'examples/packs/multilingual_evasion.yaml': {
    title: 'Multilingual Evasion',
    blurb: 'Non-English and translation-style prompts that try to slip through policy.',
    suites: ['adversarial'],
    scenarios: 2,
  },
  'examples/packs/persistent_session.yaml': {
    title: 'Persistent Session',
    blurb: 'Session carryover prompts that try to store and reuse hidden values later.',
    suites: ['core', 'adversarial'],
    scenarios: 2,
  },
  'examples/packs/policy_override.yaml': {
    title: 'Policy Override',
    blurb: 'Admin/debug framing designed to coerce policy exceptions.',
    suites: ['adversarial'],
    scenarios: 2,
  },
  'examples/packs/rag_exfiltration.yaml': {
    title: 'RAG Exfiltration',
    blurb: 'Retrieval-style prompts that ask the model to reveal hidden context.',
    suites: ['core', 'leakage'],
    scenarios: 2,
  },
  'examples/packs/roleplay_bypass.yaml': {
    title: 'Roleplay Bypass',
    blurb: 'Persona-shift and roleplay prompts that try to weaken refusal behavior.',
    suites: ['adversarial'],
    scenarios: 2,
  },
  'examples/packs/schema_bypass.yaml': {
    title: 'Schema Bypass',
    blurb: 'Structured output and tool-payload prompts that try to sneak hidden fields through.',
    suites: ['adversarial'],
    scenarios: 2,
  },
  'examples/packs/summarization_leakage.yaml': {
    title: 'Summarization Leakage',
    blurb: 'Summaries and synthesis prompts that try to pull internal values into output.',
    suites: ['leakage', 'adversarial'],
    scenarios: 2,
  },
  'examples/packs/tool_abuse.yaml': {
    title: 'Tool Abuse',
    blurb: 'Tool-call prompts that try to expose hidden arguments, connectors, or internal steps.',
    suites: ['core', 'adversarial'],
    scenarios: 2,
  },
};

let lastScenarios = [];
let lastFindings = [];
let active = { kind: null, id: null };
let activeModal = null;
let noteBaseline = 'preview';
let availablePacks = [];
let availableModels = [];
let busy = false;
let currentRun = null;
let currentAudit = null;
let runsCache = [];
let currentMode = 'single';

function getPackMeta(path) {
  return PACK_CATALOG[path] || {
    title: path.split('/').pop() || path,
    blurb: 'Custom pack path.',
    suites: [],
    scenarios: 0,
  };
}

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

function clearLog() {
  const root = $('log');
  if (!root) return;
  root.innerHTML = '';
  logEvent('log cleared', null);
}

function setMode(mode) {
  currentMode = mode === 'batch' ? 'batch' : 'single';
  $('modeSingleBtn').classList.toggle('active', currentMode === 'single');
  $('modeBatchBtn').classList.toggle('active', currentMode === 'batch');
  $('batchCard').classList.toggle('hidden', currentMode !== 'batch');
  const callout = $('workflowCallout');
  if (callout) {
    callout.innerHTML = currentMode === 'batch'
      ? '<div class="calloutTitle">How to run a batch</div><div class="calloutText">1. Confirm the default target settings. 2. Add one or more batch rows below. 3. Click <strong>Run Batch Comparison</strong>.</div>'
      : '<div class="calloutTitle">How to run</div><div class="calloutText">1. Confirm target settings. 2. Keep at least one pack selected. 3. Click <strong>Run Security Check</strong>.</div>';
  }
  validate();
}

function selectedPackPaths() {
  return packPathsFromTextarea();
}

function updatePackInsights() {
  const selected = selectedPackPaths();
  const metas = selected.map(getPackMeta);
  const selectedScenarios = lastScenarios.length
    ? lastScenarios.length
    : metas.reduce((sum, meta) => sum + (meta.scenarios || 0), 0);
  const categories = new Set((lastScenarios.length ? lastScenarios : []).map((x) => x.category));
  if (!lastScenarios.length) {
    for (const meta of metas) {
      for (const suite of meta.suites) categories.add(suite);
    }
  }

  $('packCount').textContent = String(selected.length);
  $('scenarioCount').textContent = String(selectedScenarios);
  $('categoryCount').textContent = String(categories.size);

  const hint = $('packHint');
  if (!hint) return;
  if (!selected.length) {
    hint.textContent = 'Pick a starter suite to load a usable security check quickly.';
    return;
  }
  const suiteNames = new Set();
  for (const meta of metas) {
    for (const suite of meta.suites) suiteNames.add(suite);
  }
  hint.textContent = `Selected ${selected.length} pack${selected.length === 1 ? '' : 's'} across ${suiteNames.size || 1} testing lane${suiteNames.size === 1 ? '' : 's'}. Preview to inspect full coverage.`;
}

function setPresetUI(name) {
  const buttons = document.querySelectorAll('[data-preset]');
  for (const btn of buttons) {
    btn.classList.toggle('active', btn.getAttribute('data-preset') === name);
  }
}

function applyPreset(name) {
  const entries = Object.entries(PACK_CATALOG);
  let picked = [];
  if (name === 'all') {
    picked = entries.map(([path]) => path);
  } else {
    picked = entries.filter(([, meta]) => meta.suites.includes(name)).map(([path]) => path);
  }
  setTextareaPacks(picked);
  updatePackCheckboxesFromTextarea();
  setPresetUI(name);
  updatePackInsights();
  validate();
}

function setSummary({ runId, scenariosTotal, summary, artifacts, audit }) {
  currentRun = { runId, scenariosTotal, summary, artifacts, audit };
  currentAudit = audit || null;

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
    renderAuditPanel(currentAudit, summary, artifacts, scenariosTotal, lastFindings);
    return;
  }
  dlSummary.classList.remove('btnDisabled');
  dlFindings.classList.remove('btnDisabled');
  dlConfig.classList.remove('btnDisabled');
  dlSummary.href = artifacts.summaryUrl;
  dlFindings.href = artifacts.findingsUrl;
  dlConfig.href = artifacts.configUrl;
  renderAuditPanel(currentAudit, summary, artifacts, scenariosTotal, lastFindings);
}

function fmtLocal(ms) {
  const n = Number(ms || 0);
  if (!n) return '—';
  try {
    return new Date(n).toLocaleString();
  } catch {
    return '—';
  }
}

function shortPath(p) {
  const s = String(p || '');
  if (!s) return '—';
  const parts = s.split('/');
  return parts.slice(-2).join('/') || s;
}

function renderAuditPanel(audit, summary, artifacts, scenariosTotal, findings) {
  const root = $('auditPanel');
  if (!root) return;
  if (!audit) {
    root.innerHTML = '<div class="auditEmpty">Run a job to populate target, gate, pack, and artifact evidence.</div>';
    return;
  }

  const leakCount = findings.filter((f) => Number((f.scores || {}).canaryLeak ?? 0) > 0).length;
  const injCount = findings.filter((f) => Number((f.scores || {}).injHeuristic ?? 0) > 0).length;
  const topCats = [...new Set(findings.map((f) => f.category || 'uncategorized'))].slice(0, 4);

  root.innerHTML = `
    <div class="auditGrid">
      <div class="auditSection">
        <div class="auditLabel">Target</div>
        <div class="auditValue">${escapeHtml(String(audit.provider || '—'))} • ${escapeHtml(audit.model || '—')}</div>
        <div class="auditMeta">${escapeHtml(audit.baseUrl || audit.base_url || '—')}</div>
      </div>
      <div class="auditSection">
        <div class="auditLabel">Gate</div>
        <div class="auditValue">${escapeHtml(String(summary?.gatePass ?? summary?.gate_pass ? 'pass' : 'fail'))}</div>
        <div class="auditMeta">risk ≤ ${escapeHtml(String(audit.maxTotalRisk ?? audit.max_total_risk ?? '—'))} • leaks ≤ ${escapeHtml(String(audit.maxCanaryLeaks ?? audit.max_canary_leaks ?? '—'))}</div>
      </div>
      <div class="auditSection">
        <div class="auditLabel">Coverage</div>
        <div class="auditValue">${escapeHtml(String(scenariosTotal ?? '—'))} scenarios • ${escapeHtml(String((audit.packPaths || audit.pack_paths || []).length))} packs</div>
        <div class="auditMeta">${escapeHtml(topCats.join(', ') || 'no findings yet')}</div>
      </div>
      <div class="auditSection">
        <div class="auditLabel">Signals</div>
        <div class="auditValue">${escapeHtml(String(findings.length))} findings • ${escapeHtml(String(leakCount))} leaks • ${escapeHtml(String(injCount))} inj hits</div>
        <div class="auditMeta">started ${escapeHtml(fmtLocal(audit.startedAtMs ?? audit.started_at_ms))}</div>
      </div>
    </div>
    <div class="auditDivider"></div>
    <div class="auditList">
      <div class="auditRow"><span>Run ID</span><strong>${escapeHtml(String(currentRun?.runId || '—'))}</strong></div>
      <div class="auditRow"><span>Artifacts</span><strong title="${escapeHtml(artifacts?.dir || audit.artifactsDir || audit.artifacts_dir || '—')}">${escapeHtml(shortPath(artifacts?.dir || audit.artifactsDir || audit.artifacts_dir || '—'))}</strong></div>
      <div class="auditRow"><span>Execution</span><strong>${escapeHtml(String(audit.concurrency || '—'))}x • ${escapeHtml(String(audit.timeoutMs ?? audit.timeout_ms ?? '—'))} ms • ${escapeHtml(String(audit.maxTokens ?? audit.max_tokens ?? '—'))} max tok</strong></div>
      <div class="auditRow"><span>Top-K</span><strong>${escapeHtml(String(audit.topK ?? audit.top_k ?? 'full'))}</strong></div>
      <div class="auditRow"><span>Packs</span><strong title="${escapeHtml((audit.packPaths || audit.pack_paths || []).join(', '))}">${escapeHtml((audit.packPaths || audit.pack_paths || []).map(shortPath).join(', ') || '—')}</strong></div>
    </div>
  `;
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
  packList.classList.toggle('isBusy', busy);

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

function renderModelOptions(models) {
  const list = $('modelOptions');
  if (!list) return;
  list.innerHTML = '';
  for (const model of models) {
    const opt = document.createElement('option');
    opt.value = model;
    list.appendChild(opt);
  }
}

function setModelsHint(message, kind) {
  const el = $('modelsHint');
  if (!el) return;
  el.textContent = message;
  el.classList.remove('good', 'bad');
  if (kind) el.classList.add(kind);
}

async function refreshModelOptions() {
  const provider = $('provider').value || 'ollama';
  const baseUrl = $('baseUrl').value.trim() || 'http://localhost:11434';

  if (provider !== 'ollama') {
    availableModels = [];
    renderModelOptions([]);
    setModelsHint('Model suggestions are loaded automatically when Ollama is selected.', null);
    return;
  }

  setModelsHint('Loading Ollama models…', null);

  try {
    const qs = new URLSearchParams({ provider, baseUrl });
    const r = await fetch(`/api/models?${qs.toString()}`);
    if (!r.ok) throw new Error(await r.text());
    const data = await r.json();
    const models = Array.isArray(data.models) ? data.models : [];
    availableModels = models;
    renderModelOptions(models);

    if (models.length === 0) {
      setModelsHint('No Ollama models found on this host.', 'bad');
      return;
    }

    const current = $('model').value.trim();
    if (!current || !models.includes(current)) {
      $('model').value = models[0];
      persistForm();
    }

    setModelsHint(`Loaded ${models.length} Ollama model${models.length === 1 ? '' : 's'}.`, 'good');
    validate();
  } catch (e) {
    availableModels = [];
    renderModelOptions([]);
    setModelsHint(`Could not load Ollama models: ${String(e)}`, 'bad');
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
  }
  if (!s.provider || s.provider === 'openAi') {
    s.provider = 'ollama';
  }
  if (!s.baseUrl || s.baseUrl === 'http://localhost:8000') {
    s.baseUrl = 'http://localhost:11434';
  }
  saveSettings(s);
  $('provider').value = s.provider ?? 'ollama';
  $('baseUrl').value = s.baseUrl ?? 'http://localhost:11434';
  $('model').value = s.model ?? 'qwen3.5:9b';
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
    drawCoverageHeatmap([]);
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
  drawCoverageHeatmap(rows);
}

function renderFindingsRows(rows) {
  const tbody = $('findingsRows');
  tbody.innerHTML = '';

  if (rows.length === 0) {
    tbody.innerHTML = `<tr><td colspan="6"><span class="clip muted">No findings.</span></td></tr>`;
    drawRiskCloud([]);
    drawFindingsHeatmap([]);
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
  drawFindingsHeatmap(rows);
  drawRiskCloud(rows);
  drawSignalBars(rows);
  renderAuditPanel(currentAudit, currentRun?.summary || null, currentRun?.artifacts || null, currentRun?.scenariosTotal ?? null, rows);
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

  const pad = 42;
  ctx.fillStyle = 'rgba(247,249,252,1)';
  ctx.fillRect(0, 0, w, h);

  ctx.strokeStyle = 'rgba(145,155,172,0.18)';
  ctx.lineWidth = 1;
  for (let i = 0; i <= 4; i++) {
    const x = pad + ((w - 2 * pad) / 4) * i;
    const y = pad + ((h - 2 * pad) / 4) * i;
    ctx.beginPath();
    ctx.moveTo(x, pad);
    ctx.lineTo(x, h - pad);
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(pad, y);
    ctx.lineTo(w - pad, y);
    ctx.stroke();
  }

  ctx.strokeStyle = 'rgba(120,130,150,0.42)';
  ctx.beginPath();
  ctx.moveTo(pad, h - pad);
  ctx.lineTo(w - pad, h - pad);
  ctx.moveTo(pad, h - pad);
  ctx.lineTo(pad, pad);
  ctx.stroke();

  ctx.fillStyle = 'rgba(112,121,136,0.95)';
  ctx.font = '11px ui-monospace, SFMono-Regular, Menlo, monospace';
  ctx.fillText('inj heuristic →', w - 126, h - 14);
  ctx.fillText('risk ↑', 10, 18);
  ctx.fillText('0', pad - 4, h - pad + 16);
  ctx.fillText('100', w - pad - 18, h - pad + 16);
  ctx.fillText('100', 8, pad + 4);

  if (!rows.length) {
    ctx.fillStyle = 'rgba(130,140,160,0.9)';
    ctx.fillText('No findings to visualize.', pad, h / 2);
    return;
  }

  const legend = [...new Set(rows.map((f) => f.category || 'uncategorized'))].slice(0, 4);
  legend.forEach((cat, idx) => {
    const x = pad + idx * 132;
    ctx.fillStyle = categoryColor(cat);
    ctx.beginPath();
    ctx.arc(x, 18, 5, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = 'rgba(82,91,108,0.95)';
    ctx.fillText(String(cat), x + 10, 22);
  });

  rows.forEach((f, idx) => {
    const inj = Number((f.scores || {}).injHeuristic ?? 0);
    const risk = Number(f.totalRisk ?? f.total_risk ?? 0);
    const x = pad + Math.max(0, Math.min(100, inj)) * ((w - 2 * pad) / 100);
    const y = (h - pad) - Math.max(0, Math.min(100, risk)) * ((h - 2 * pad) / 100);
    const leak = Number((f.scores || {}).canaryLeak ?? 0);
    const r = 5 + Math.max(0, Math.min(14, risk / 14)) + (leak > 0 ? 3 : 0);
    ctx.fillStyle = categoryColor(f.category);
    ctx.beginPath();
    ctx.arc(x + ((idx % 3) - 1) * 2, y + ((idx % 2) ? -2 : 2), r, 0, 2 * Math.PI);
    ctx.fill();
    ctx.strokeStyle = leak > 0 ? 'rgba(17,24,39,0.46)' : 'rgba(255,255,255,0.72)';
    ctx.lineWidth = leak > 0 ? 2 : 1;
    ctx.stroke();
  });
}

function drawHeatmap(canvasId, rowLabels, colLabels, valueAt, emptyText) {
  const canvas = $(canvasId);
  if (!canvas) return;
  const ctx = canvas.getContext('2d');
  const w = canvas.width;
  const h = canvas.height;
  ctx.clearRect(0, 0, w, h);
  ctx.fillStyle = 'rgba(110,120,130,0.10)';
  ctx.fillRect(0, 0, w, h);

  if (!rowLabels.length || !colLabels.length) {
    ctx.fillStyle = 'rgba(130,140,160,0.95)';
    ctx.font = '12px ui-monospace, SFMono-Regular, Menlo, monospace';
    ctx.fillText(emptyText, 24, h / 2);
    return;
  }

  const left = 124;
  const top = 24;
  const pad = 18;
  const cellW = Math.max(44, (w - left - pad) / colLabels.length);
  const cellH = Math.max(24, (h - top - pad) / rowLabels.length);
  let max = 0;
  for (let r = 0; r < rowLabels.length; r++) {
    for (let c = 0; c < colLabels.length; c++) {
      max = Math.max(max, valueAt(r, c));
    }
  }
  max = Math.max(max, 1);

  ctx.font = '11px ui-monospace, SFMono-Regular, Menlo, monospace';
  ctx.fillStyle = 'rgba(120,130,145,0.95)';

  for (let c = 0; c < colLabels.length; c++) {
    const x = left + c * cellW + 8;
    ctx.fillText(colLabels[c], x, 14);
  }

  for (let r = 0; r < rowLabels.length; r++) {
    const y = top + r * cellH;
    ctx.fillStyle = 'rgba(120,130,145,0.95)';
    ctx.fillText(rowLabels[r], 10, y + cellH * 0.62);
    for (let c = 0; c < colLabels.length; c++) {
      const x = left + c * cellW;
      const value = valueAt(r, c);
      const alpha = 0.12 + (value / max) * 0.78;
      ctx.fillStyle = `rgba(0,113,227,${alpha.toFixed(3)})`;
      ctx.fillRect(x, y, cellW - 6, cellH - 6);
      ctx.fillStyle = value > max * 0.55 ? 'rgba(255,255,255,0.95)' : 'rgba(17,24,39,0.82)';
      ctx.fillText(String(value), x + 10, y + cellH * 0.62);
    }
  }
}

function drawCoverageHeatmap(rows) {
  if (!rows.length) {
    drawHeatmap('coverageHeatmap', [], [], () => 0, 'Preview packs to see category coverage.');
    return;
  }
  const categories = [...new Set(rows.map((x) => x.category))];
  const cols = ['scenarios', 'system', 'canary'];
  drawHeatmap(
    'coverageHeatmap',
    categories,
    cols,
    (r, c) => {
      const catRows = rows.filter((x) => x.category === categories[r]);
      if (c === 0) return catRows.length;
      if (c === 1) return catRows.filter((x) => x.systemPrompt && String(x.systemPrompt).trim()).length;
      return catRows.filter((x) => x.canary).length;
    },
    'Preview packs to see category coverage.'
  );
}

function drawFindingsHeatmap(rows) {
  if (!rows.length) {
    drawHeatmap('findingsHeatmap', [], [], () => 0, 'Run a security check to see findings hotspots.');
    return;
  }
  const categories = [...new Set(rows.map((x) => x.category || 'uncategorized'))];
  const cols = ['findings', 'leaks', 'inj hits', 'high risk'];
  drawHeatmap(
    'findingsHeatmap',
    categories,
    cols,
    (r, c) => {
      const catRows = rows.filter((x) => (x.category || 'uncategorized') === categories[r]);
      if (c === 0) return catRows.length;
      if (c === 1) return catRows.filter((x) => Number((x.scores || {}).canaryLeak ?? 0) > 0).length;
      if (c === 2) return catRows.filter((x) => Number((x.scores || {}).injHeuristic ?? 0) > 0).length;
      return catRows.filter((x) => Number(x.totalRisk ?? x.total_risk ?? 0) >= 50).length;
    },
    'Run a security check to see findings hotspots.'
  );
}

function drawSignalBars(rows) {
  const canvas = $('signalBars');
  if (!canvas) return;
  const ctx = canvas.getContext('2d');
  const w = canvas.width;
  const h = canvas.height;
  ctx.clearRect(0, 0, w, h);
  ctx.fillStyle = 'rgba(247,249,252,1)';
  ctx.fillRect(0, 0, w, h);

  if (!rows.length) {
    ctx.fillStyle = 'rgba(130,140,160,0.9)';
    ctx.font = '12px ui-monospace, SFMono-Regular, Menlo, monospace';
    ctx.fillText('Run a security check to see category pressure.', 24, h / 2);
    return;
  }

  const categories = [...new Set(rows.map((x) => x.category || 'uncategorized'))];
  const series = categories.map((cat) => {
    const catRows = rows.filter((x) => (x.category || 'uncategorized') === cat);
    return {
      cat,
      findings: catRows.length,
      leaks: catRows.filter((x) => Number((x.scores || {}).canaryLeak ?? 0) > 0).length,
      highRisk: catRows.filter((x) => Number(x.totalRisk ?? x.total_risk ?? 0) >= 50).length,
    };
  });
  const max = Math.max(1, ...series.map((x) => x.findings + x.leaks + x.highRisk));
  const padX = 24;
  const padY = 26;
  const rowH = Math.max(28, Math.floor((h - padY * 2) / series.length));

  ctx.font = '11px ui-monospace, SFMono-Regular, Menlo, monospace';
  series.forEach((item, idx) => {
    const y = padY + idx * rowH;
    const barX = 170;
    const barW = w - barX - 30;
    const total = item.findings + item.leaks + item.highRisk;
    const findingsW = (barW * item.findings) / max;
    const leaksW = (barW * item.leaks) / max;
    const riskW = (barW * item.highRisk) / max;

    ctx.fillStyle = 'rgba(98,108,124,0.95)';
    ctx.fillText(item.cat, padX, y + 15);
    ctx.fillStyle = 'rgba(232,236,242,1)';
    ctx.fillRect(barX, y, barW, 16);
    ctx.fillStyle = 'rgba(0,113,227,0.72)';
    ctx.fillRect(barX, y, findingsW, 16);
    ctx.fillStyle = 'rgba(33,191,115,0.78)';
    ctx.fillRect(barX + findingsW, y, leaksW, 16);
    ctx.fillStyle = 'rgba(255,159,67,0.82)';
    ctx.fillRect(barX + findingsW + leaksW, y, riskW, 16);
    ctx.fillStyle = 'rgba(82,91,108,0.95)';
    ctx.fillText(`${item.findings} find • ${item.leaks} leak • ${item.highRisk} high`, barX, y + 34);
    ctx.fillText(String(total), barX + barW - 20, y + 15);
  });

  const legend = [
    ['findings', 'rgba(0,113,227,0.72)'],
    ['leaks', 'rgba(33,191,115,0.78)'],
    ['high risk', 'rgba(255,159,67,0.82)'],
  ];
  legend.forEach(([label, color], idx) => {
    const x = 24 + idx * 120;
    ctx.fillStyle = color;
    ctx.fillRect(x, h - 18, 14, 8);
    ctx.fillStyle = 'rgba(98,108,124,0.95)';
    ctx.fillText(String(label), x + 20, h - 10);
  });
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

function renderSkeletonRows(tbodyId, colspan, widths) {
  const tbody = $(tbodyId);
  tbody.innerHTML = '';
  for (const widthClass of widths) {
    const tr = document.createElement('tr');
    const td = document.createElement('td');
    td.colSpan = colspan;
    const line = document.createElement('div');
    line.className = `skeletonLine ${widthClass}`;
    td.appendChild(line);
    tr.appendChild(td);
    tbody.appendChild(tr);
  }
}

async function preview() {
  if (busy) return;
  setBusy(true, 'preview');
  $('count').textContent = 'loading…';
  $('note').classList.remove('good', 'bad');
  $('note').textContent = 'previewing packs';
  noteBaseline = 'previewing packs';
  logEvent('preview packs', null);

  renderSkeletonRows('rows', 5, ['sk62', 'sk78', 'sk54']);

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
    $('note').textContent = 'ready to run';
    noteBaseline = 'ready to run';
    $('search').value = '';
    renderScenarioRows(lastScenarios);
    updatePackInsights();
    logEvent(`loaded scenarios=${data.scenariosLoaded ?? lastScenarios.length}`, 'ok');
  } finally {
    setBusy(false, 'preview');
  }
}

async function runJob() {
  if (busy) return;
  setBusy(true, 'run');
  persistForm();
  $('count').textContent = 'running security check…';
  $('note').classList.remove('good', 'bad');
  $('note').textContent = 'running security check';
  noteBaseline = 'running security check';
  logEvent('run job', null);

  renderSkeletonRows('findingsRows', 6, ['sk68', 'sk52', 'sk74']);

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
    audit: data.audit || null,
  });
  logEvent(`run=${String(runId).slice(0, 8)} gate=${gate ? 'pass' : 'fail'} findings=${lastFindings.length}`, gate ? 'ok' : 'err');
}

async function runBatch() {
  if (busy) return;
  setBusy(true, 'batch');
  persistForm();
  $('count').textContent = 'running batch comparison…';
  $('note').classList.remove('good', 'bad');
  $('note').textContent = 'running batch comparison';
  noteBaseline = 'running batch comparison';
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
      setSummary({ runId: null, scenariosTotal: null, summary: null, artifacts: null, audit: null });
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
  currentAudit = null;
  active = { kind: null, id: null };
  activeModal = null;
  $('count').textContent = 'scenarios_loaded=0';
  $('note').classList.remove('good', 'bad');
  $('note').textContent = 'select packs to begin';
  noteBaseline = 'select packs to begin';
  $('search').value = '';
  renderScenarioRows([]);
  renderFindingsRows([]);
  updatePackCheckboxesFromTextarea();
  setSummary({ runId: null, scenariosTotal: null, summary: null, artifacts: null, audit: null });
  updatePackInsights();
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
    root.innerHTML = '<div class="emptyState">No packs found.</div>';
    return;
  }

  for (const p of packs) {
    const meta = getPackMeta(p);
    const row = document.createElement('div');
    row.className = 'packItem';
    row.innerHTML = `
      <div class="packLeft">
        <input class="check" type="checkbox" data-pack="${escapeHtml(p)}" />
        <div class="packText">
          <div class="packName" title="${escapeHtml(p)}">${escapeHtml(meta.title)}</div>
          <div class="packMeta">${escapeHtml(meta.blurb)}</div>
        </div>
      </div>
      <div class="packTag">${escapeHtml((meta.suites || [])[0] || 'yaml')}</div>
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
  updatePackInsights();
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
  updatePackInsights();
  validate();
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

  renderSkeletonRows('findingsRows', 6, ['sk72', 'sk64']);

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
      audit: data.audit || null,
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
  $('clearLogBtn').addEventListener('click', clearLog);
  $('clearLogBtnFooter')?.addEventListener('click', clearLog);

  $('packs').addEventListener('input', () => {
    updatePackCheckboxesFromTextarea();
    updatePackInsights();
    validate();
    persistForm();
  });
  $('batchSpecs').addEventListener('input', () => {
    validate();
    persistForm();
  });
  $('compareRunIds').addEventListener('input', () => setCompareRunIds(compareRunIdsFromTextarea()));
  document.querySelectorAll('[data-preset]').forEach((btn) => {
    btn.addEventListener('click', () => {
      applyPreset(btn.getAttribute('data-preset'));
      persistForm();
    });
  });
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

  $('provider').addEventListener('change', () => {
    refreshModelOptions();
  });
  $('baseUrl').addEventListener('change', () => {
    refreshModelOptions();
  });

  setHealth();
  setInterval(setHealth, 5000);
  wireShortcuts();
  refreshModelOptions();
  loadPacks();
  updatePackCheckboxesFromTextarea();
  renderScenarioRows([]);
  renderFindingsRows([]);
  setSummary({ runId: null, scenariosTotal: null, summary: null, artifacts: null, audit: null });
  logEvent('ready', 'ok');
  refreshRuns();
  $('note').textContent = 'select packs to begin';
  noteBaseline = 'select packs to begin';
  validate();
  updatePackInsights();
  setPresetUI('core');
  drawCoverageHeatmap([]);
  drawFindingsHeatmap([]);
  drawRiskCloud([]);
  drawSignalBars([]);
}

wire();
