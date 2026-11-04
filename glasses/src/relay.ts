/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 * All rights reserved.
 *
 * This source code is licensed under the BSD-style license found in the
 * LICENSE file in the root directory of this source tree.
 */

// The companion relay contract. The relay (companion/walkaway_relay.py)
// owns the real browser agent; this page only displays frames and records
// the wearer's approval. Every field below comes from the relay verbatim.

export interface Decision {
  operation: string;
  target: string | null;
  choice: string;
  confidence: number;
  model?: string;
  latency_ms?: number;
}

export interface PageFrame {
  url: string;
  title: string;
  screenshot: string | null;
}

export interface RelayState {
  status: 'idle' | 'ready' | 'predicted' | 'done' | 'blocked' | 'error';
  goal: string | null;
  scenario: string | null;
  page: PageFrame | null;
  decision: Decision | null;
  awaiting_approval: boolean;
  history_count: number;
  elapsed_ms: number;
  decision_model: string | null;
  text_model: string | null;
  say: string;
}

export const IDLE_STATE: RelayState = {
  status: 'idle',
  goal: null,
  scenario: null,
  page: null,
  decision: null,
  awaiting_approval: false,
  history_count: 0,
  elapsed_ms: 0,
  decision_model: null,
  text_model: null,
  say: 'No task running.',
};

export type FetchFn = (url: string, init?: RequestInit) => Promise<Response>;

/** Relay token: `?k=` on first open, then localStorage. Empty in local dev. */
function relayToken(): string {
  try {
    const fromUrl = new URLSearchParams(location.search).get('k');
    if (fromUrl) localStorage.setItem('walkaway-token', fromUrl);
    return fromUrl ?? localStorage.getItem('walkaway-token') ?? '';
  } catch {
    return '';
  }
}

export class Relay {
  private readonly base: string;
  private readonly doFetch: FetchFn;

  constructor(base = '/api', doFetch: FetchFn = (u, i) => fetch(u, i)) {
    this.base = base;
    this.doFetch = doFetch;
  }

  private authHeaders(): Record<string, string> {
    const token = relayToken();
    return token ? { 'X-Walkaway-Token': token } : {};
  }

  async state(): Promise<RelayState> {
    const res = await this.doFetch(`${this.base}/state`, { headers: this.authHeaders() });
    if (!res.ok) throw new Error(`Relay state failed: HTTP ${res.status}`);
    return (await res.json()) as RelayState;
  }

  async command(name: string, body: Record<string, unknown> = {}): Promise<RelayState> {
    const res = await this.doFetch(`${this.base}/${name}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', ...this.authHeaders() },
      body: JSON.stringify(body),
    });
    const payload = (await res.json()) as RelayState & { error?: string };
    if (!res.ok) throw new Error(payload.error ?? `Relay command ${name} failed: HTTP ${res.status}`);
    return payload;
  }
}
