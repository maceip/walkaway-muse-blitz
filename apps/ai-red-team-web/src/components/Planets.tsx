import type { CSSProperties } from 'react';

/**
 * Two planets, both of them far away: small, soft and pushed out to the
 * edges of the frame so they never crowd the globe.
 */
interface Planet {
  x: number;
  y: number;
  size: number;
  lit: string;
  mid: string;
  dark: string;
  blur: number;
  dur: number;
  dx: number;
  dy: number;
  ring?: string;
}

const PLANETS: Planet[] = [
  { x: 13, y: 17, size: 54, lit: '#7d8cab', mid: '#414c66', dark: '#12172a', blur: 2.8, dur: 240, dx: -2.6, dy: 1.5 },
  { x: 82, y: 80, size: 48, lit: '#63818f', mid: '#334a56', dark: '#0e171c', blur: 2.4, dur: 310, dx: -1.6, dy: -1.9, ring: 'rgba(190, 220, 235, 0.3)' },
];

/** The per-planet custom properties the stylesheet reads. */
type PlanetVars = CSSProperties & {
  '--lit': string;
  '--mid': string;
  '--dark': string;
  '--dx': string;
  '--dy': string;
  '--ring'?: string;
};

export function Planets() {
  return (
    <>
      {PLANETS.map((p, i) => {
        const vars: PlanetVars = {
          '--lit': p.lit,
          '--mid': p.mid,
          '--dark': p.dark,
          '--dx': `${p.dx}%`,
          '--dy': `${p.dy}%`,
          ...(p.ring ? { '--ring': p.ring } : {}),
        };
        return (
          <div
            key={i}
            className="planet"
            style={{
              ...vars,
              left: `${p.x}%`,
              top: `${p.y}%`,
              width: `${p.size}px`,
              height: `${p.size}px`,
              filter: `blur(${p.blur}px)`,
              opacity: 0.22 + (3.4 - p.blur) * 0.1,
              animation: `planet-drift ${p.dur}s ease-in-out ${-i * 37}s infinite alternate`,
            }}
          >
            <span className="planet__body" />
            {p.ring ? <span className="planet__ring" /> : null}
          </div>
        );
      })}
    </>
  );
}
