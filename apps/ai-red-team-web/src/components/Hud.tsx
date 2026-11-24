import type { Phase } from '../lib/schedule';
import { CITY_STATE } from '../lib/schedule';

/**
 * The corner readouts. They are the page's only text besides the wordmark,
 * so they stay terse and monospaced.
 */
export function Hud({ phase, countdown }: { phase: Phase; countdown: string }) {
  const alert = phase === 'warning' || phase === 'exercise';
  const calm = phase === 'settle';

  return (
    <>
      <header className="hud hud--tl">
        <span className="hud__mark" aria-hidden="true" />
        <span className="hud__item">MODEL.TOOLS</span>
        <span className="hud__sep" aria-hidden="true">
          /
        </span>
        <span className="hud__item">AI RED TEAM</span>
      </header>

      <div className="hud hud--tr">
        <span className="hud__label">Next exercise</span>
        <span
          className={`hud__value${alert ? ' is-alert' : ''}${calm ? ' is-calm' : ''}`}
        >
          {countdown}
        </span>
      </div>

      <div className="hud hud--bl">
        <span className="hud__label">City</span>
        <span
          className={`hud__value hud__value--dim${alert ? ' is-alert' : ''}${calm ? ' is-calm' : ''}`}
        >
          {CITY_STATE[phase]}
        </span>
      </div>

      <div className="hud hud--br">
        <span className="hud__label">Grid</span>
        <span className="hud__value hud__value--dim">37.7749 N / 122.4194 W</span>
      </div>
    </>
  );
}
