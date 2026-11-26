import { useCallback, useEffect, useState } from 'react';
import { DEFAULT_SCENE_ID, SCENE_BY_ID, SCENES, type Scene } from '../media/scenes';
import { TRANSITION_MS } from './orbCompositor';
import { nextScene } from './sequence';

/**
 * How long the scene being crossed away from stays on the page.
 *
 * The globe's renderer crosses in TRANSITION_MS, and keeps drawing the
 * outgoing film from its element for as long as the element is here, so the
 * element outlives the crossing by a moment. The stylesheet's fallback
 * crossfade (used without WebGL) runs for the same time.
 */
export const FADE_MS = TRANSITION_MS + 200;

export interface SceneState {
  /** The scene the page is on now. */
  current: Scene;
  /** The scene it is crossing away from, while the crossing lasts. */
  previous: Scene | null;
  /** True when the scene was opened directly, and the page will not move on. */
  pinned: boolean;
  /** Move on to another scene now, rather than waiting for the timer. */
  advance: () => void;
}

function prefersReducedMotion(): boolean {
  return (
    typeof window !== 'undefined' &&
    typeof window.matchMedia === 'function' &&
    window.matchMedia('(prefers-reduced-motion: reduce)').matches
  );
}

/**
 * The scenes the page moves through.
 *
 * The camera moves are films of the camera going into the city, which is a
 * great deal of movement; somebody who has asked for less motion gets the
 * colour changes and none of the flying about.
 */
function cycleScenes(): Scene[] {
  if (prefersReducedMotion()) {
    const stills = SCENES.filter((scene) => scene.kind === 'image' || scene.kind === 'alert');
    if (stills.length) return stills;
  }
  return SCENES;
}

/**
 * Which scene the page opens on.
 *
 * A scene can be opened directly with `?scene=<id>`, which pins it: the page
 * shows that scene and stays there, which is how the alternatives are checked
 * without changing what the page ships.
 */
function sceneFromLocation(): { scene: Scene; pinned: boolean } {
  if (typeof window === 'undefined') {
    return { scene: SCENE_BY_ID[DEFAULT_SCENE_ID], pinned: false };
  }
  let requested: string | null = null;
  try {
    requested = new URLSearchParams(window.location.search).get('scene');
  } catch {
    requested = null;
  }
  if (requested && SCENE_BY_ID[requested]) {
    return { scene: SCENE_BY_ID[requested], pinned: true };
  }
  const list = cycleScenes();
  const defaultScene = SCENE_BY_ID[DEFAULT_SCENE_ID];
  const start = list.some((scene) => scene.id === defaultScene.id)
    ? defaultScene
    : list[0];
  return { scene: start, pinned: false };
}

/**
 * Which scene the page shows, and when it moves on.
 *
 * The scenes are the ones the media pipeline built: the city-globe in any of
 * the delivered looks, the camera moves that take the camera into the city,
 * the storm rolling over it, and the red alert state. Which one is the
 * default is a build-time decision, written into the manifest by
 * `scripts/build-redteam-media.py`.
 *
 * Every scene is built at the same size with the globe in the same place, so
 * moving from one to the next is a change of colour and camera and nothing
 * else — the globe does not jump.
 */
export function useScene(): SceneState {
  const [state, setState] = useState<Omit<SceneState, 'advance'>>(() => {
    const { scene, pinned } = sceneFromLocation();
    return { current: scene, previous: null, pinned };
  });

  // move on to another scene
  const advance = useCallback(() => {
    setState((s) => {
      if (s.pinned) return s;
      return {
        current: nextScene(s.current, cycleScenes()),
        previous: s.current,
        pinned: false,
      };
    });
  }, []);

  // move on when this scene has had its time. A film that does not loop says
  // when it has finished and the page moves on then; the timer is only the
  // backstop for a film that never gets to say. A picture says nothing, so
  // its timer is its time: eight seconds, not eight plus the film's backstop.
  const waitsForEnd = state.current.kind === 'video' && !state.current.loop;
  useEffect(() => {
    if (state.pinned) return;
    const wait = waitsForEnd ? state.current.durationMs + 5000 : state.current.durationMs;
    const timer = window.setTimeout(advance, wait);
    return () => window.clearTimeout(timer);
  }, [state.current.id, waitsForEnd, state.current.durationMs, state.pinned, advance]);

  // and let the scene that was crossed away from go once the crossing is over
  useEffect(() => {
    if (!state.previous) return;
    const timer = window.setTimeout(() => {
      setState((s) => (s.previous ? { ...s, previous: null } : s));
    }, FADE_MS);
    return () => window.clearTimeout(timer);
  }, [state.previous?.id]);

  // the page can be told which scene to show, and follows the browser's back
  useEffect(() => {
    const onChange = () => {
      const { scene, pinned } = sceneFromLocation();
      setState({ current: scene, previous: null, pinned });
    };
    window.addEventListener('popstate', onChange);
    return () => window.removeEventListener('popstate', onChange);
  }, []);

  return { ...state, advance };
}
