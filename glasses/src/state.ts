/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 * All rights reserved.
 *
 * This source code is licensed under the BSD-style license found in the
 * LICENSE file in the root directory of this source tree.
 */

import { IDLE_STATE, Relay, type RelayState } from './relay.ts';

// One state API for the DOM and the walkaway_* tools. The page is the
// referee: the relay may predict a step, but an `act` command is only sent
// after approveStep() runs in this class. Rejecting drops the decision
// relay-side, so a rejected step can never execute late.

type Listener = (state: RelayState) => void;

export class Walkaway {
  private current: RelayState = IDLE_STATE;
  private readonly listeners = new Set<Listener>();
  readonly relay: Relay;

  constructor(relay: Relay = new Relay()) {
    this.relay = relay;
  }

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    listener(this.current);
    return () => this.listeners.delete(listener);
  }

  snapshot(): RelayState {
    return this.current;
  }

  private publish(state: RelayState): RelayState {
    this.current = state;
    for (const listener of this.listeners) listener(state);
    return state;
  }

  async refresh(): Promise<RelayState> {
    return this.publish(await this.relay.state());
  }

  async startTask(goal: string, scenario = 'flights'): Promise<RelayState> {
    if (!goal.trim()) throw new Error('A goal is required to start a task.');
    return this.publish(await this.relay.command('start', { goal: goal.trim(), scenario }));
  }

  async proposeStep(): Promise<RelayState> {
    return this.publish(await this.relay.command('predict'));
  }

  async approveStep(): Promise<RelayState> {
    if (!this.current.decision) throw new Error('No proposed step to approve. Propose a step first.');
    return this.publish(await this.relay.command('approve'));
  }

  async rejectStep(): Promise<RelayState> {
    if (!this.current.decision) throw new Error('No proposed step to reject.');
    return this.publish(await this.relay.command('reject'));
  }

  async stopTask(): Promise<RelayState> {
    return this.publish(await this.relay.command('stop'));
  }
}
