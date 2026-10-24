import http from 'node:http';
import { readFile, stat } from 'node:fs/promises';
import { watch } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.dirname(fileURLToPath(import.meta.url));
const clients = new Set();
const types = { '.html': 'text/html', '.css': 'text/css', '.js': 'text/javascript', '.svg': 'image/svg+xml' };
const server = http.createServer(async (req, res) => {
  if (req.url === '/__reload') {
    res.writeHead(200, { 'Content-Type': 'text/event-stream', 'Cache-Control': 'no-cache', Connection: 'keep-alive' });
    res.write(': connected\n\n');
    clients.add(res);
    req.on('close', () => clients.delete(res));
    return;
  }
  try {
    const url = new URL(req.url, 'http://localhost');
    const file = path.resolve(root, '.' + decodeURIComponent(url.pathname === '/' ? '/index.html' : url.pathname));
    if (!file.startsWith(root + path.sep) || !(await stat(file)).isFile()) throw new Error('Not found');
    res.writeHead(200, { 'Content-Type': types[path.extname(file)] || 'application/octet-stream', 'Cache-Control': 'no-store' });
    res.end(await readFile(file));
  } catch {
    res.writeHead(404);
    res.end('Not found');
  }
});
let debounce;
watch(root, { recursive: true }, (_, filename) => {
  if (!filename || !/\.(html|css|js)$/.test(filename)) return;
  clearTimeout(debounce);
  debounce = setTimeout(() => { for (const client of clients) client.write('data: reload\n\n'); }, 100);
});
const heartbeat = setInterval(() => { for (const client of clients) client.write(': heartbeat\n\n'); }, 20000);
server.on('close', () => clearInterval(heartbeat));
server.listen(Number(process.env.PORT || 4173), '127.0.0.1', () => console.log('Skry design preview: http://127.0.0.1:' + server.address().port));
