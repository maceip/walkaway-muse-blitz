# Walkaway Privacy Policy

Effective: 2026-11-03. Walkaway is self-hosted software. The developer operates no
Walkaway servers and receives no user data.

## What runs where

- The companion relay and browser agent run on the user's own machine.
- The Muse Connector endpoint run by the user accepts task goals and returns
  task status as text: goal, page title, page URL, proposed action, step counts,
  elapsed time, and model names.
- Browser screenshots are sent only from the user's companion relay to the
  user's own glasses Web App session. Screenshots are never returned through
  the Muse Connector and are never stored by Walkaway.

## Data shared with model providers

To choose steps, the companion sends page text, the visible element list, the
goal, and recent actions to the model provider the user configured (Meta Model
API when `MODEL_API_KEY` is set; otherwise the fallback provider named in the
user's local configuration). That provider's privacy policy governs data sent
to the provider. Walkaway does not send screenshots to model providers.

## Authentication

The connector requires a bearer token chosen by the user
(`WALKAWAY_CONNECTOR_TOKEN`). Tokens are stored only in the user's environment.

## Retention

Walkaway stores no task history on disk. State lives in memory in the relay
process and ends when the process stops or the user stops the task.

## Contact

Open an issue in the Walkaway GitHub repository.
