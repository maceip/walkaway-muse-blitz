/**
 * The production server for redteam.model.tools.
 *
 * Dependency-free on purpose: it is a static file server for a Vite build,
 * so it needs nothing but the Node standard library. It binds to loopback
 * only and Caddy terminates TLS in front of it.
 *
 *   PORT   the port to listen on (default 8091)
 *   HOST   the address to bind (default 127.0.0.1)
 *   DIST   the directory to serve (default ../dist relative to this file)
 */
import { createServer } from 'node:http';
import { createReadStream } from 'node:fs';
import { stat } from 'node:fs/promises';
import { dirname, extname, isAbsolute, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const DIST = resolve(process.env.DIST || resolve(HERE, '../dist'));
const PORT = Number(process.env.PORT || 8091);
const HOST = process.env.HOST || '127.0.0.1';

const MIME_TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.webp': 'image/webp',
  '.ico': 'image/x-icon',
  '.woff': 'font/woff',
  '.woff2': 'font/woff2',
  '.mp4': 'video/mp4',
  '.txt': 'text/plain; charset=utf-8',
};

/** True when `candidate` stays inside `base`. Blocks path traversal. */
function inside(base, candidate) {
  const path = relative(base, candidate);
  return path !== '..' && !path.startsWith(`..${sep}`) && !isAbsolute(path);
}

function send(res, status, body, headers = {}) {
  res.writeHead(status, {
    'Content-Type': 'text/plain; charset=utf-8',
    'Content-Length': Buffer.byteLength(body),
    'X-Content-Type-Options': 'nosniff',
    ...headers,
  });
  res.end(body);
}

/** Content is immutable when the filename carries a content hash. */
function isImmutable(pathname) {
  return /\/assets\//.test(pathname) && /-[A-Za-z0-9_]{8,}\.[a-z0-9]+$/.test(pathname);
}

/**
 * Serve a file, honouring a single HTTP Range request.
 *
 * The backdrop video is several megabytes, so the browser asks for it in
 * parts. Without range support it would have to download the whole file
 * before it could play, which is exactly what the range header avoids.
 */
async function serveFile(req, res, filePath, status = 200) {
  const info = await stat(filePath);
  const type = MIME_TYPES[extname(filePath).toLowerCase()] || 'application/octet-stream';
  const headers = {
    'Content-Type': type,
    'Content-Length': info.size,
    'Last-Modified': info.mtime.toUTCString(),
    'X-Content-Type-Options': 'nosniff',
    'Accept-Ranges': 'bytes',
  };
  if (isImmutable(filePath)) {
    headers['Cache-Control'] = 'public, max-age=31536000, immutable';
  } else if (filePath.endsWith('.html')) {
    headers['Cache-Control'] = 'no-cache';
  } else {
    headers['Cache-Control'] = 'public, max-age=3600';
  }

  const range = req.headers.range;
  if (range && req.method === 'GET') {
    const match = /^bytes=(\d*)-(\d*)$/.exec(range.trim());
    if (match) {
      const size = info.size;
      let start = match[1] === '' ? null : Number(match[1]);
      let end = match[2] === '' ? null : Number(match[2]);

      if (start === null && end !== null) {
        // A suffix range: the last N bytes.
        start = Math.max(0, size - end);
        end = size - 1;
      } else if (start !== null) {
        if (end === null || end >= size) end = size - 1;
      }

      if (start === null || Number.isNaN(start) || Number.isNaN(end) || start > end || start >= size) {
        res.writeHead(416, { ...headers, 'Content-Range': `bytes */${size}` });
        return res.end();
      }

      res.writeHead(206, {
        ...headers,
        'Content-Range': `bytes ${start}-${end}/${size}`,
        'Content-Length': end - start + 1,
      });
      return createReadStream(filePath, { start, end }).pipe(res);
    }
  }

  res.writeHead(status, headers);
  if (req.method === 'HEAD') return res.end();
  createReadStream(filePath).pipe(res);
}

const server = createServer(async (req, res) => {
  try {
    const url = new URL(req.url || '/', `http://${req.headers.host || 'localhost'}`);
    const pathname = decodeURIComponent(url.pathname);

    if (req.method !== 'GET' && req.method !== 'HEAD') {
      return send(res, 405, 'Method not allowed.', { Allow: 'GET, HEAD' });
    }

    // The health endpoint is what the deploy check and the systemd unit
    // use to know the server is actually up and serving the build.
    if (pathname === '/healthz') {
      return send(res, 200, 'ok', { 'Content-Type': 'text/plain; charset=utf-8' });
    }

    const rel = pathname === '/' ? '/index.html' : pathname;
    const target = resolve(DIST, `.${rel}`);
    if (!inside(DIST, target)) {
      return send(res, 403, 'Forbidden.');
    }

    let filePath = target;
    try {
      const info = await stat(filePath);
      if (info.isDirectory()) filePath = resolve(filePath, 'index.html');
    } catch {
      // Unknown path. A path that names a file — anything with an extension
      // in it — is a broken asset, and it has to say so: falling back to the
      // SPA entry point for those would answer a missing picture with the
      // page's HTML and a 200, which hides the break from the browser, from
      // the logs and from the check that looks for it.
      if (/\.[A-Za-z0-9]+$/.test(pathname)) {
        return send(res, 404, 'Not found.', { 'Content-Type': 'text/plain; charset=utf-8' });
      }
      // Anything else is a route, and the page handles its own routes.
      filePath = resolve(DIST, 'index.html');
    }

    await serveFile(req, res, filePath);
  } catch (err) {
    send(res, 500, 'Internal error.');
    console.error('[redteam-web]', err);
  }
});

server.listen(PORT, HOST, () => {
  console.log(`[redteam-web] serving ${DIST} on http://${HOST}:${PORT}`);
});

for (const signal of ['SIGTERM', 'SIGINT']) {
  process.on(signal, () => {
    server.close(() => process.exit(0));
  });
}
