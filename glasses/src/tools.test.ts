/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 * All rights reserved.
 *
 * This source code is licensed under the BSD-style license found in the
 * LICENSE file in the root directory of this source tree.
 */

import { describe, expect, test } from 'vitest';
import { Relay, IDLE_STATE, type RelayState } from './relay.ts';
import { Walkaway } from './state.ts';
import { registerTools } from './tools.ts';
import { clearTools, executeTool } from './webmcp.ts';

// Network-level test double only: a scripted relay transport. The shipped
// UI has no scripted data; every pixel comes from companion/walkaway_relay.py.

function scriptedApp(responses: RelayState[]) {
  let i = 0;
  const fetchFn = async () =>
    new Response(JSON.stringify(responses[Math.min(i++, responses.length - 1)]), {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
    });
  return new Walkaway(new Relay('/api', fetchFn));
}

const PREDICTED: RelayState = {
  ...IDLE_STATE,
  status: 'predicted',
  goal: 'Zurich to London',
  page: { url: 'https://example.test', title: 'Flights', screenshot: null },
  decision: { operation: 'CLICK', target: '3', choice: 'a3', confidence: 0.9 },
  awaiting_approval: true,
  say: 'Agent proposes CLICK: Where to?. Approve or reject.',
};

describe('walkaway tools', () => {
  test('approve is refused before any proposal exists', async () => {
    clearTools();
    registerTools(scriptedApp([IDLE_STATE]));
    const result = (await executeTool('walkaway_approve_step')) as { isError?: boolean };
    expect(result.isError).toBe(true);
  });

  test('predicted result carries role and stop-and-wait next_action', async () => {
    clearTools();
    registerTools(scriptedApp([PREDICTED]));
    const result = (await executeTool('walkaway_propose_step')) as Record<string, unknown>;
    expect(result.awaiting_approval).toBe(true);
    expect(String(result.your_role)).toContain('never invent');
    expect(String(result.next_action)).toContain('stop calling tools');
  });

  test('terminal result omits next_action', async () => {
    clearTools();
    registerTools(scriptedApp([{ ...PREDICTED, status: 'done', decision: null, awaiting_approval: false }]));
    const result = (await executeTool('walkaway_read_status')) as Record<string, unknown>;
    expect(result.next_action).toBeUndefined();
  });
});
