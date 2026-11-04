/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 * All rights reserved.
 *
 * This source code is licensed under the BSD-style license found in the
 * LICENSE file in the root directory of this source tree.
 */

import './style.css';
import { Walkaway } from './state.ts';
import { registerTools } from './tools.ts';
import type { RelayState } from './relay.ts';

const app = new Walkaway();
registerTools(app);

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const statusEl = $<HTMLSpanElement>('status');
const frameEl = $<HTMLImageElement>('frame');
const noFrameEl = $<HTMLParagraphElement>('no-frame');
const sayEl = $<HTMLParagraphElement>('say');
const metaEl = $<HTMLParagraphElement>('meta');
const proposeBtn = $<HTMLButtonElement>('propose');
const approveBtn = $<HTMLButtonElement>('approve');
const rejectBtn = $<HTMLButtonElement>('reject');
const stopBtn = $<HTMLButtonElement>('stop');

function render(state: RelayState): void {
  statusEl.textContent = state.status;
  statusEl.dataset.status = state.status;
  sayEl.textContent = state.say;
  const shot = state.page?.screenshot;
  if (shot) {
    frameEl.src = `data:image/jpeg;base64,${shot}`;
    frameEl.hidden = false;
    noFrameEl.hidden = true;
  } else {
    frameEl.hidden = true;
    noFrameEl.hidden = false;
  }
  const lines: string[] = [];
  if (state.goal) lines.push(`goal: ${state.goal}`);
  if (state.page) lines.push(`${state.page.title} — ${state.page.url}`);
  if (state.decision) {
    lines.push(
      `proposed: ${state.decision.operation} ${state.decision.target ?? ''} ` +
        `(confidence ${state.decision.confidence.toFixed(2)})`,
    );
  }
  lines.push(`steps acted: ${state.history_count} · elapsed ${state.elapsed_ms}ms`);
  if (state.decision_model) lines.push(`decision model: ${state.decision_model}`);
  if (state.text_model) lines.push(`text model: ${state.text_model}`);
  metaEl.textContent = lines.join('\n');

  const running = state.status === 'ready' || state.status === 'predicted';
  proposeBtn.disabled = !running || state.decision !== null;
  approveBtn.disabled = state.decision === null;
  rejectBtn.disabled = state.decision === null;
  stopBtn.disabled = state.status === 'idle';
}

app.subscribe(render);

async function run(fn: () => Promise<unknown>): Promise<void> {
  try {
    await fn();
  } catch (error) {
    sayEl.textContent = error instanceof Error ? error.message : String(error);
  }
}

$<HTMLFormElement>('start').addEventListener('submit', (e) => {
  e.preventDefault();
  void run(() => app.startTask($<HTMLInputElement>('goal').value, $<HTMLSelectElement>('scenario').value));
});
proposeBtn.addEventListener('click', () => void run(() => app.proposeStep()));
approveBtn.addEventListener('click', () => void run(() => app.approveStep()));
rejectBtn.addEventListener('click', () => void run(() => app.rejectStep()));
stopBtn.addEventListener('click', () => void run(() => app.stopTask()));

// Poll so frames the companion produces on its own (page loads, waits)
// reach the lens without a command. Polling never approves anything.
setInterval(() => void app.refresh().catch(() => undefined), 1500);
void app.refresh().catch(() => undefined);

// navigator.install exists only in the glasses runtime. The button stays
// hidden in every other browser, and rejection is an expected outcome.
const installBtn = $<HTMLButtonElement>('install');
const nav = navigator as Navigator & { install?: (url?: string, options?: { name?: string }) => Promise<void> };
if (typeof nav.install === 'function') {
  installBtn.hidden = false;
  installBtn.addEventListener('click', () => {
    void nav.install?.(undefined, { name: 'Walkaway' }).catch(() => {
      sayEl.textContent = 'Installation is unavailable on this device. Use the QR install link instead.';
    });
  });
}

declare global {
  interface Window {
    Walkaway: { app: Walkaway };
  }
}
window.Walkaway = { app };
