#!/usr/bin/env node
// MCP server voor JM-Orgue test API.
//
// Spawnt (of hergebruikt) een JM-Orgue instance met --test-api en biedt
// tools waarmee Claude de software kan aansturen voor self-testing.

import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from '@modelcontextprotocol/sdk/types.js';
import { spawn } from 'child_process';
import { existsSync } from 'fs';
import { dirname, resolve } from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const API_HOST = process.env.JM_ORGUE_API_HOST || '127.0.0.1';
const API_PORT = parseInt(process.env.JM_ORGUE_API_PORT || '8765', 10);
const API_BASE = `http://${API_HOST}:${API_PORT}`;
const APP_EXE = process.env.JM_ORGUE_EXE ||
  resolve(__dirname, '..', '..', 'VirtualPipeOrgan', 'target', 'release', 'vpo-app.exe');

let appProcess = null;

// ============ HTTP helpers ============

async function apiCall(method, path, body) {
  const url = `${API_BASE}${path}`;
  const opts = { method, headers: {} };
  if (body !== undefined) {
    opts.headers['Content-Type'] = 'application/json';
    opts.body = typeof body === 'string' ? body : JSON.stringify(body);
  }
  const r = await fetch(url, opts);
  const text = await r.text();
  try {
    return { status: r.status, body: JSON.parse(text) };
  } catch {
    return { status: r.status, body: text };
  }
}

async function isApiAlive(timeoutMs = 1000) {
  try {
    const ctrl = new AbortController();
    const t = setTimeout(() => ctrl.abort(), timeoutMs);
    const r = await fetch(`${API_BASE}/version`, { signal: ctrl.signal });
    clearTimeout(t);
    return r.ok;
  } catch {
    return false;
  }
}

async function waitForApi(maxMs = 15000) {
  const start = Date.now();
  while (Date.now() - start < maxMs) {
    if (await isApiAlive(500)) return true;
    await new Promise(r => setTimeout(r, 250));
  }
  return false;
}

async function ensureAppRunning() {
  if (await isApiAlive()) {
    return { started: false, note: 'reeds actief' };
  }
  if (!existsSync(APP_EXE)) {
    throw new Error(
      `vpo-app.exe niet gevonden op ${APP_EXE}. Bouw eerst met: cargo build --release -p vpo-app`
    );
  }
  appProcess = spawn(APP_EXE, ['--test-api', String(API_PORT)], {
    detached: false,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  appProcess.on('exit', () => { appProcess = null; });

  const ok = await waitForApi(15000);
  if (!ok) {
    if (appProcess) appProcess.kill();
    appProcess = null;
    throw new Error('JM-Orgue startte, maar test-API werd niet bereikbaar binnen 15s');
  }
  return { started: true, exe: APP_EXE, port: API_PORT };
}

function killApp() {
  if (appProcess) {
    try { appProcess.kill(); } catch {}
    appProcess = null;
  }
  // Probeer ook eventuele andere vpo-app instances
  try {
    spawn('taskkill', ['/F', '/IM', 'vpo-app.exe'], { stdio: 'ignore' }).on('exit', () => {});
  } catch {}
}

// ============ MCP Tool definities ============

const TOOLS = [
  {
    name: 'start_app',
    description: 'Start de JM-Orgue applicatie met de test-API ingeschakeld. No-op als hij al draait. Doe dit als eerste vóór andere tools.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
  },
  {
    name: 'stop_app',
    description: 'Stop alle draaiende JM-Orgue instances.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
  },
  {
    name: 'get_status',
    description: 'Lees status van de app: audio_running, sample_rate, voice_count, peak L/R, organ_loaded, MIDI status.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
  },
  {
    name: 'get_organ_info',
    description: 'Lees info van het geladen orgel (naam, divisies, registers met IDs en MIDI ranges).',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
  },
  {
    name: 'get_drawn_stops',
    description: 'Lijst van actieve (getrokken) registers.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
  },
  {
    name: 'toggle_stop',
    description: 'Toggle een register aan/uit op zijn stop ID (zie get_organ_info voor IDs).',
    inputSchema: {
      type: 'object',
      properties: {
        stop_id: { type: 'string', description: 'De stop ID zoals "1_Prestant_8" of soortgelijk' },
      },
      required: ['stop_id'],
      additionalProperties: false,
    },
  },
  {
    name: 'play_note',
    description: 'Stuur een MIDI NoteOn naar alle getrokken registers die de noot accepteren.',
    inputSchema: {
      type: 'object',
      properties: {
        midi_note: { type: 'integer', minimum: 0, maximum: 127, description: 'MIDI noot 0-127 (60 = middle C)' },
        velocity: { type: 'integer', minimum: 1, maximum: 127, description: 'Velocity, default 100' },
      },
      required: ['midi_note'],
      additionalProperties: false,
    },
  },
  {
    name: 'stop_note',
    description: 'Stuur een MIDI NoteOff voor de gegeven noot.',
    inputSchema: {
      type: 'object',
      properties: {
        midi_note: { type: 'integer', minimum: 0, maximum: 127 },
      },
      required: ['midi_note'],
      additionalProperties: false,
    },
  },
  {
    name: 'panic',
    description: 'All notes off — stop alle klinkende voices direct.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
  },
  {
    name: 'get_logs',
    description: 'Lees de laatste N regels uit de JM-Orgue logfile.',
    inputSchema: {
      type: 'object',
      properties: {
        lines: { type: 'integer', minimum: 1, maximum: 1000, description: 'Aantal regels, default 50' },
      },
      additionalProperties: false,
    },
  },
  {
    name: 'get_division_volumes',
    description: 'Lees de huidige zwel-gain per divisie (0.0-1.0).',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
  },
  {
    name: 'play_chord',
    description: 'Speel meerdere noten tegelijk. Houdt ze 500ms vast tenzij andere duration_ms gegeven.',
    inputSchema: {
      type: 'object',
      properties: {
        notes: {
          type: 'array',
          items: { type: 'integer', minimum: 0, maximum: 127 },
          description: 'Lijst van MIDI noten',
        },
        velocity: { type: 'integer', minimum: 1, maximum: 127 },
        duration_ms: { type: 'integer', minimum: 50, maximum: 10000, description: 'Hoe lang vasthouden (ms), default 500' },
      },
      required: ['notes'],
      additionalProperties: false,
    },
  },
  {
    name: 'verify_audio',
    description: 'Smoke test: start app, lees status, speel akkoord, controleer voice_count > 0 en peak > 0. Geeft pass/fail.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
  },
  {
    name: 'list_library',
    description: 'Geef de orgelbibliotheek terug — alle eerder geladen orgels met hun source_path. Gebruik dit om bekende paden te vinden voordat je load_organ aanroept.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
  },
  {
    name: 'load_organ',
    description: 'Laad een .organ ODF bestand. Pad moet een absoluut pad zijn naar het .organ file. Synchroon — wacht tot alle samples geladen zijn (kan even duren bij grote sets).',
    inputSchema: {
      type: 'object',
      properties: {
        path: { type: 'string', description: 'Absoluut pad naar .organ bestand' },
      },
      required: ['path'],
      additionalProperties: false,
    },
  },
  {
    name: 'load_directory',
    description: 'Scan een sample map (zonder .organ bestand) en laad alle samples. Pad moet een absoluut pad zijn naar een map met sample subfolders per register.',
    inputSchema: {
      type: 'object',
      properties: {
        path: { type: 'string', description: 'Absoluut pad naar sample directory' },
      },
      required: ['path'],
      additionalProperties: false,
    },
  },
];

// ============ Tool handlers ============

async function handleCall(name, args) {
  if (name === 'start_app') {
    const r = await ensureAppRunning();
    return r;
  }
  if (name === 'stop_app') {
    killApp();
    return { ok: true };
  }

  // Voor alle andere tools: zorg dat de app draait
  await ensureAppRunning();

  switch (name) {
    case 'get_status': {
      const r = await apiCall('GET', '/status');
      return r.body;
    }
    case 'get_organ_info': {
      const r = await apiCall('GET', '/organ');
      return r.body;
    }
    case 'get_drawn_stops': {
      const r = await apiCall('GET', '/stops/drawn');
      return r.body;
    }
    case 'toggle_stop': {
      const r = await apiCall('POST', `/stops/${encodeURIComponent(args.stop_id)}/toggle`);
      return r.body;
    }
    case 'play_note': {
      const v = args.velocity ?? 100;
      const r = await apiCall('POST', `/notes/${args.midi_note}/on?velocity=${v}`);
      return r.body;
    }
    case 'stop_note': {
      const r = await apiCall('POST', `/notes/${args.midi_note}/off`);
      return r.body;
    }
    case 'panic': {
      const r = await apiCall('POST', '/panic');
      return r.body;
    }
    case 'get_logs': {
      const n = args.lines ?? 50;
      const r = await apiCall('GET', `/logs?lines=${n}`);
      return r.body;
    }
    case 'get_division_volumes': {
      const r = await apiCall('GET', '/division_volumes');
      return r.body;
    }
    case 'play_chord': {
      const v = args.velocity ?? 100;
      const dur = args.duration_ms ?? 500;
      const results = [];
      for (const n of args.notes) {
        const r = await apiCall('POST', `/notes/${n}/on?velocity=${v}`);
        results.push({ note: n, on: r.body });
      }
      await new Promise(r => setTimeout(r, dur));
      for (const n of args.notes) {
        await apiCall('POST', `/notes/${n}/off`);
      }
      return { played: args.notes, duration_ms: dur, results };
    }
    case 'list_library': {
      const r = await apiCall('GET', '/library');
      return r.body;
    }
    case 'load_organ': {
      // Sample-loading kan minuten duren bij grote sets — geef geen timeout-fout
      const r = await apiCall('POST', '/load_organ', { path: args.path });
      if (r.status >= 400) {
        throw new Error(`load_organ failed (${r.status}): ${typeof r.body === 'object' ? r.body.error || JSON.stringify(r.body) : r.body}`);
      }
      return r.body;
    }
    case 'load_directory': {
      const r = await apiCall('POST', '/load_directory', { path: args.path });
      if (r.status >= 400) {
        throw new Error(`load_directory failed (${r.status}): ${typeof r.body === 'object' ? r.body.error || JSON.stringify(r.body) : r.body}`);
      }
      return r.body;
    }
    case 'verify_audio': {
      const checks = [];
      // 1. Status check
      const status = (await apiCall('GET', '/status')).body;
      checks.push({ check: 'audio_running', pass: status.audio_running === true, value: status.audio_running });
      checks.push({ check: 'sample_rate > 0', pass: status.sample_rate > 0, value: status.sample_rate });

      // 2. Probeer een testtoon (C-majeur akkoord) — werkt alleen als orgel is geladen
      if (status.organ_loaded) {
        for (const n of [60, 64, 67]) {
          await apiCall('POST', `/notes/${n}/on?velocity=100`);
        }
        await new Promise(r => setTimeout(r, 400));
        const status2 = (await apiCall('GET', '/status')).body;
        checks.push({ check: 'voice_count > 0 na akkoord', pass: status2.voice_count > 0, value: status2.voice_count });
        const peak = Math.max(status2.peak_left, status2.peak_right);
        checks.push({ check: 'peak meter activiteit', pass: peak > 0.001, value: peak });
        for (const n of [60, 64, 67]) {
          await apiCall('POST', `/notes/${n}/off`);
        }
      } else {
        checks.push({ check: 'orgel geladen', pass: false, value: false, note: 'Laad eerst een orgel via de UI' });
      }
      const overall = checks.every(c => c.pass);
      return { overall_pass: overall, checks };
    }
  }
  throw new Error(`Onbekende tool: ${name}`);
}

// ============ MCP server setup ============

const server = new Server(
  { name: 'jm-orgue', version: '0.1.0' },
  { capabilities: { tools: {} } },
);

server.setRequestHandler(ListToolsRequestSchema, async () => ({ tools: TOOLS }));

server.setRequestHandler(CallToolRequestSchema, async req => {
  const name = req.params.name;
  const args = req.params.arguments ?? {};
  try {
    const result = await handleCall(name, args);
    return {
      content: [{ type: 'text', text: JSON.stringify(result, null, 2) }],
    };
  } catch (e) {
    return {
      content: [{ type: 'text', text: `Error: ${e.message}` }],
      isError: true,
    };
  }
});

process.on('SIGINT', () => { killApp(); process.exit(0); });
process.on('SIGTERM', () => { killApp(); process.exit(0); });

const transport = new StdioServerTransport();
await server.connect(transport);
