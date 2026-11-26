import type { Scene } from '../media/scenes';

/**
 * Which scene comes after this one.
 *
 * The page is not a showreel with a running order. From an ordinary scene it
 * drifts to any other ordinary scene, chosen at random, and never shows the
 * same scene twice in a row, which would read as a hitch rather than a drift.
 *
 * Two things break the drift, both set in videos/scenes.json:
 *
 *  - **A transition** (`after`) is a time-lapse that opens on another scene's
 *    picture — the daylight city, the grey city — and carries that same city
 *    into a different light. The page goes into it from the scene it opens
 *    on, always, so the picture does not change at all as it starts; and
 *    from nowhere else, so it is never seen opening on a picture that is not
 *    there.
 *  - **Where a transition ends** (`then`): the page goes on to that scene,
 *    the one the time-lapse has arrived at.
 *
 * Only scenes in `list` are chosen, so the reduced-motion cycle, which is
 * stills only, never reaches for a film.
 */
export function nextScene(current: Scene, list: Scene[], random: () => number = Math.random): Scene {
  const pick = (pool: Scene[]) => pool[Math.min(pool.length - 1, Math.floor(random() * pool.length))];
  const then = current.then ? list.find((scene) => scene.id === current.then) : undefined;
  if (then) return then;
  const transitions = list.filter((scene) => scene.after.includes(current.id));
  if (transitions.length) return pick(transitions);
  return pick(driftPool(current, list));
}

/** The scenes the page may drift to from this one, in manifest order. */
export function driftPool(current: Scene, list: Scene[]): Scene[] {
  const others = list.filter((scene) => scene.id !== current.id && scene.after.length === 0);
  return others.length ? others : list;
}
