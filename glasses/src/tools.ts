/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 * All rights reserved.
 *
 * This source code is licensed under the BSD-style license found in the
 * LICENSE file in the root directory of this source tree.
 */

import { registerTool } from './webmcp.ts';
import type { RelayState } from './relay.ts';
import type { Walkaway } from './state.ts';

// Tool contract, following the lessons in examples/20-questions and
// examples/password: repeat the role on every result, end non-terminal
// results with a stop-and-wait next_action, omit next_action when terminal,
// and keep the approval decision on the page side of the boundary.

const ROLE =
  'You relay the wearer\u2019s voice commands to Walkaway. A companion browser agent proposes steps; ' +
  'the wearer approves or rejects each proposal. You never invent a step, a result, or an approval.';

const TERMINAL = new Set(['done', 'blocked', 'idle']);

function view(state: RelayState) {
  const terminal = TERMINAL.has(state.status);
  return {
    say: state.say,
    your_role: ROLE,
    ...(terminal
      ? {}
      : {
          next_action:
            'Report `say` to the wearer, then stop calling tools and wait for the wearer\u2019s next instruction.',
        }),
    status: state.status,
    goal: state.goal,
    scenario: state.scenario,
    page: state.page ? { url: state.page.url, title: state.page.title } : null,
    decision: state.decision,
    awaiting_approval: state.awaiting_approval,
    history_count: state.history_count,
    elapsed_ms: state.elapsed_ms,
    decision_model: state.decision_model,
    text_model: state.text_model,
  };
}

async function guard(fn: () => Promise<RelayState>) {
  try {
    return view(await fn());
  } catch (error) {
    return {
      content: [{ type: 'text' as const, text: error instanceof Error ? error.message : String(error) }],
      isError: true,
    };
  }
}

export function registerTools(app: Walkaway): void {
  registerTool({
    name: 'walkaway_start_task',
    description:
      'Start a companion browser task with a spoken goal. Scenario is flights (Google Flights) or a local travel/research fixture.',
    inputSchema: {
      type: 'object',
      properties: {
        goal: { type: 'string', description: 'The full natural-language goal, verbatim from the wearer.' },
        scenario: { type: 'string', enum: ['flights', 'travel', 'research'] },
      },
      required: ['goal'],
    },
    execute: (args) => guard(() => app.startTask(String(args.goal ?? ''), String(args.scenario ?? 'flights'))),
  });
  registerTool({
    name: 'walkaway_read_status',
    description: 'Read the current task status, page, and any proposed step without changing anything.',
    inputSchema: { type: 'object', properties: {} },
    annotations: { readOnlyHint: true },
    execute: () => guard(() => app.refresh()),
  });
  registerTool({
    name: 'walkaway_propose_step',
    description: 'Ask the companion agent to propose its next browser step. Proposing never executes the step.',
    inputSchema: { type: 'object', properties: {} },
    execute: () => guard(() => app.proposeStep()),
  });
  registerTool({
    name: 'walkaway_approve_step',
    description:
      'Execute the currently proposed step. Only call after the wearer explicitly approves the exact step shown on the glasses.',
    inputSchema: { type: 'object', properties: {} },
    annotations: { destructiveHint: true },
    execute: () => guard(() => app.approveStep()),
  });
  registerTool({
    name: 'walkaway_reject_step',
    description: 'Discard the currently proposed step so the step can never execute. The agent may propose a new step after.',
    inputSchema: { type: 'object', properties: {} },
    execute: () => guard(() => app.rejectStep()),
  });
  registerTool({
    name: 'walkaway_stop_task',
    description: 'Stop the task and close the companion browser.',
    inputSchema: { type: 'object', properties: {} },
    annotations: { destructiveHint: true },
    execute: () => guard(() => app.stopTask()),
  });
}
