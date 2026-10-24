import { updatePlayback, reconcilePlayback } from './playback-model.js';

const titles = [
  { name: 'Dune: Part Two', year: '2024', genre: 'Science fiction', score: '8.5', imdb: 'tt15239678', duration: 166, description: 'Paul Atreides joins the Fremen and seeks revenge against the conspirators who destroyed his family.' },
  { name: 'Perfect Days', year: '2023', genre: 'Drama', score: '7.9', imdb: 'tt27503384', duration: 124 },
  { name: 'Past Lives', year: '2023', genre: 'Romance', score: '7.9', imdb: 'tt13238346', duration: 106 },
  { name: 'The Grand Budapest Hotel', year: '2014', genre: 'Comedy', score: '8.1', imdb: 'tt2278388', duration: 99 },
  { name: 'Interstellar', year: '2014', genre: 'Science fiction', score: '8.7', imdb: 'tt0816692', duration: 169 },
  { name: 'The Holdovers', year: '2023', genre: 'Comedy', score: '7.9', imdb: 'tt14849194', duration: 133 },
  { name: 'Severance', year: '2022–', genre: 'Thriller', score: '8.7', imdb: 'tt11280740', duration: 52, series: true, description: 'Office workers have their memories divided between their work and personal lives.' },
  { name: 'The Bear', year: '2022–', genre: 'Drama', score: '8.6', imdb: 'tt14452776', duration: 32, series: true },
  { name: 'Arrival', year: '2016', genre: 'Science fiction', score: '7.9', imdb: 'tt2543164', duration: 116 },
  { name: 'Aftersun', year: '2022', genre: 'Drama', score: '7.7', imdb: 'tt19770238', duration: 102 },
];
const paths = [['watching','Currently watching'],['movies','Movies'],['shows','TV series'],['library','To watch'],['watched','Watched'],['calendar','Calendar'],['addons','Supported add-ons'],['installed','Installed add-ons'],['search','Search'],['detail','Movie details'],['series','Series & episodes'],['sources','Sources'],['player','Player']];
const glyphs = { watching:'◷', movies:'▤', shows:'▣', library:'☷', calendar:'▦', addons:'⊞', play:'▶', plus:'+', check:'✓', back:'←', refresh:'↻', more:'⋯' };
const esc = value => String(value).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'})[c]);
const poster = t => `assets/${t.imdb}.jpg`;
const saved = new Set([1,2,3,4,6,7,9]);
const watched = new Set([8]);
const watchedEpisodes = new Set(['6:1:1','6:1:2','7:1:1']);
const playback = new Map([
  [0, { videoId:'0',position:42,duration:166,watchedTime:42,active:true,watched:false }],
  [6, { videoId:'6:1:3',position:18,duration:52,watchedTime:18,active:true,watched:false }],
  [7, { videoId:'7:1:2',position:0,duration:32,watchedTime:0,active:true,watched:false }],
]);
let route = location.hash.slice(1) || 'watching';
let selected = 0, season = 1, episode = 1, query = '', genre = 'All genres', libraryType = 'movie', viewState = 'ready', paused = false, weekOffset = 0;
let addonNames = ['Cinemeta'];
let status = 'Sample data · No provider connections';
const app = document.querySelector('#app');
const dialog = document.querySelector('#dialog');
const contextMenu = document.createElement('div');
contextMenu.className = 'context-menu';
contextMenu.setAttribute('role','menu');
contextMenu.hidden = true;
document.body.append(contextMenu);
const button = (text, action, cls = '') => `<button class="button ${cls}" data-action="${action}">${text}</button>`;
const titleName = () => esc(titles[selected].name);
function heading(title, actions = '', subtitle = '') { return `<div class="page-heading"><div><h1>${title}</h1>${subtitle ? `<p class="muted">${subtitle}</p>` : ''}</div><div class="button-row">${actions}</div></div>`; }
function go(next) { closeMenu(); if (next === route) render(); else location.hash = next; }
window.addEventListener('hashchange', () => { route = location.hash.slice(1) || 'watching'; viewState = 'ready'; render(); window.scrollTo(0,0); });
function shell(content) {
  const nav = (id, text) => `<a href="#${id}" class="nav-link ${route === id || (id === 'library' && route === 'watched') || (id === 'addons' && route === 'installed') ? 'active' : ''}"><span class="nav-icon">${glyphs[id] || '☷'}</span><span>${text}</span></a>`;
  return `<aside class="sidebar"><a href="#watching" class="brand"><svg viewBox="0 0 96 96" aria-hidden="true"><rect x="2" y="2" width="92" height="92" rx="10" fill="#202622"/><path d="M30 18H78L66 30H36L30 36l18 8-12 12-22-10V34l16-16Z" fill="#D8ED96"/><path d="m60 40 22 10v12L66 78H18l12-12h30l6-6-18-8 12-12Z" fill="#D8ED96"/></svg><span>SKRY</span></a><nav>${nav('watching','Currently watching')}${nav('movies','Movies')}${nav('shows','TV series')}<div class="divider"></div>${nav('library','Library')}${nav('calendar','Calendar')}${nav('addons','Add-ons')}</nav><div class="preview-controls"><span class="preview-label">HTML PREVIEW</span><label>Screen<select id="screen-select" aria-label="Preview screen">${paths.map(([id,label]) => `<option value="${id}" ${route === id ? 'selected' : ''}>${label}</option>`).join('')}</select></label><label>State<select id="state-select" aria-label="Preview state">${['ready','loading','empty','error'].map(s => `<option ${viewState === s ? 'selected' : ''}>${s}</option>`).join('')}</select></label><small><span class="live-dot"></span> Live reload</small></div></aside><div class="workspace"><header class="topbar"><button class="icon-button" data-action="back" aria-label="Back">←</button><button class="icon-button" data-action="forward" aria-label="Forward">→</button><form id="search-form"><input type="search" id="search-input" aria-label="Search movies and series" placeholder="Search movies and series…" value="${esc(query)}">${button('Search','submit-search')}</form><button class="icon-button" data-action="refresh" aria-label="Refresh">↻</button></header><main id="main">${content}</main><footer><span role="status">${esc(status)}</span><span>Right-click for actions</span></footer></div>`;
}
function card(id, progress = false) {
  const t = titles[id], p = playback.get(id);
  const video = p?.videoId?.split(':');
  return `<article class="card" data-title="${id}"><button class="poster" data-open="${id}" aria-label="Open ${esc(t.name)}"><img src="${poster(t)}" alt="" loading="lazy"><span class="poster-badge">${watched.has(id) ? '✓ Watched' : '★ '+t.score}</span>${progress && p ? `<span class="poster-progress" style="--progress:${p.position/p.duration*100}%"></span>` : ''}</button><div class="card-caption"><button class="title-link" data-open="${id}">${esc(t.name)}</button><button class="icon-button more-button" data-menu-title="${id}" aria-label="Actions for ${esc(t.name)}">⋯</button></div><p class="muted tiny">${t.year} · ${t.genre}</p>${progress ? `<div class="progress-meta"><span>${t.series ? `S${video[1]} E${video[2]}` : `${Math.round(p.position)} / ${p.duration} min`}</span><button class="text-button" data-resume="${id}">${p.position ? 'Resume' : 'Play next'} ▶</button></div>` : ''}</article>`;
}
const grid = ids => `<div class="poster-grid">${ids.map(id => card(id)).join('')}</div>`;
function blank(message) { return `<div class="empty"><h2>${message}</h2></div>`; }
function catalog() {
  const series = route === 'shows';
  const ids = titles.map((_,i) => i).filter(i => Boolean(titles[i].series) === series && (genre === 'All genres' || titles[i].genre === genre));
  return `${heading(series ? 'TV series' : 'Movies', button('Refresh','refresh'))}<div class="filter-bar"><label>Catalog <select aria-label="Catalog"><option>Cinemeta · Popular</option></select></label><label>Genre <select id="genre" aria-label="Genre">${['All genres','Science fiction','Drama','Comedy','Romance','Thriller'].map(g => `<option ${genre === g ? 'selected' : ''}>${g}</option>`).join('')}</select></label><span class="muted tiny">${ids.length} titles</span></div>${ids.length ? grid(ids) : blank('No titles match these filters.')}`;
}
function currentlyWatching() {
  const ids = [...playback].filter(([,p]) => p.active).map(([id]) => id);
  return `${heading('Currently watching', button('Refresh','refresh'), 'Resume a movie or continue with the next episode.')}<div class="watching-grid">${ids.length ? ids.map(id => card(id,true)).join('') : blank('Nothing in progress.')}</div><div class="info-line">Finished movies leave this list. Series move to the next released episode.</div>`;
}
function library() {
  const isWatched = route === 'watched';
  const ids = [...(isWatched ? watched : saved)].filter(id => libraryType === 'all' || Boolean(titles[id].series) === (libraryType === 'series'));
  return `${heading('Library', button('Refresh','refresh'))}<div class="tabs"><a href="#library" class="tab ${!isWatched ? 'active' : ''}">To watch <span>${saved.size}</span></a><a href="#watched" class="tab ${isWatched ? 'active' : ''}">Watched <span>${watched.size}</span></a></div><div class="filter-bar"><label>Type <select id="library-type" aria-label="Library type"><option value="movie" ${libraryType === 'movie' ? 'selected' : ''}>Movies</option><option value="series" ${libraryType === 'series' ? 'selected' : ''}>TV series</option><option value="all" ${libraryType === 'all' ? 'selected' : ''}>All</option></select></label><span class="muted tiny">${ids.length} titles · Saved on this device</span></div>${ids.length ? grid(ids) : blank(isWatched ? 'No watched titles in this filter.' : 'No titles saved in this filter.')}`;
}
function episodesFor(id) {
  if (!titles[id].series) return [];
  return Array.from({length:8},(_,i) => ({id:`${id}:${Math.floor(i/4)+1}:${i%4+1}`, released: i < 7 ? '2026-01-01' : '2099-01-01'}));
}
function detail() {
  const t = titles[selected];
  const names = ['Good News About Hell','Half Loop','In Perpetuity','The You You Are'];
  return `${button('← Back','back','subtle')}<section class="detail" data-title="${selected}"><img class="detail-poster" src="${poster(t)}" alt="${titleName()} poster"><div class="detail-copy"><h1>${titleName()}</h1><p class="muted">${t.year} · ${t.genre} · ★ ${t.score} · ${t.series ? '2 seasons' : t.duration+' min'}</p><p>${t.description || 'Sample title metadata. Descriptions, cast, and release dates come from the installed metadata provider.'}</p><div class="button-row">${button('Choose source','sources','primary')}${button(saved.has(selected) ? 'Remove from to watch' : 'Watch later','save-detail')}${button('⋯','detail-menu','more-button')}</div><p class="muted tiny">${watched.has(selected) ? 'Watched' : 'Not watched'}${t.series ? ' · Episode progress is tracked separately' : ''}</p></div></section>${t.series ? `<div class="section-heading"><h2>Episodes</h2><select id="season" aria-label="Season"><option value="1" ${season === 1 ? 'selected' : ''}>Season 1</option><option value="2" ${season === 2 ? 'selected' : ''}>Season 2</option></select></div><div class="episode-list">${names.map((name,i) => { const key = `${selected}:${season}:${i+1}`, checked = watchedEpisodes.has(key), upcoming = season === 2 && i === 3; return `<div class="episode-row ${episode === i+1 ? 'selected' : ''}" data-title="${selected}" data-episode-key="${key}"><button class="episode-main" data-episode="${i+1}" ${upcoming ? 'disabled' : ''}><span class="episode-number">${i+1}</span><span><strong>${t.name === 'Severance' ? name : 'Episode '+(i+1)}</strong><small>${upcoming ? 'Not yet released' : t.duration+' min'}</small></span><span class="episode-status">${checked ? '✓ Watched' : upcoming ? 'Upcoming' : episode === i+1 ? 'Selected' : 'Unwatched'}</span></button><button class="icon-button more-button" data-menu-episode="${key}" aria-label="Actions for episode ${i+1}">⋯</button></div>`; }).join('')}</div>` : ''}`;
}
function sources() {
  return `${button('← Back to title','title-back','subtle')}${heading('Sources', '', `${titleName()}${titles[selected].series ? ` · S${season} E${episode}` : ''}`)}<div class="source-list">${[['4K · HEVC · HDR10','18.4 GB','Direct HTTPS'],['1080p · H.264 · 5.1','4.2 GB','Direct HTTPS'],['1080p · H.264','2.8 GB','Torrent · 124 peers']].map((s,i) => `<button class="source-row" data-source="${i}"><span class="quality">${i ? 'HD' : '4K'}</span><span><strong>${s[0]}</strong><small>${s[1]} · ${s[2]} · Sample provider</small></span><span class="source-play">▶</span></button>`).join('')}</div><p class="notice">Only play media you have permission to access. Direct torrents expose your IP to peers, can upload data, and use disk space.</p>`;
}
function addonPage() {
  const installed = route === 'installed';
  const providers = installed ? addonNames : ['Cinemeta','Torrentio'];
  return `${heading('Add-ons', button('+ Add by URL','add-addon','primary'))}<div class="tabs"><a href="#addons" class="tab ${!installed ? 'active' : ''}">Supported</a><a href="#installed" class="tab ${installed ? 'active' : ''}">Installed <span>${addonNames.length}</span></a></div><div class="addon-list">${providers.map(name => `<article class="addon-card"><div class="addon-icon">${name === 'Cinemeta' ? '▤' : '▶'}</div><div class="addon-description"><h2>${esc(name)}</h2><p class="muted">${name === 'Cinemeta' ? 'Movie and series catalogs, metadata, and episode release dates.' : name === 'Torrentio' ? 'Stream provider. Supports direct torrents and configured debrid sources.' : 'Custom Stremio-compatible provider.'}</p><span class="tiny muted">${addonNames.includes(name) ? '✓ Installed' : 'Not installed'}</span></div>${installed ? `<button class="button" data-remove-addon="${esc(name)}">Remove</button>` : `<button class="button ${addonNames.includes(name) ? '' : 'primary'}" data-install-addon="${name}" ${addonNames.includes(name) ? 'disabled' : ''}>${addonNames.includes(name) ? 'Installed' : 'Install'}</button>`}</article>`).join('')}</div>${!providers.length ? blank('No add-ons installed.') : ''}<p class="notice">Manifest URLs may contain credentials. They are stored locally as plaintext and are never shown in this list.</p><section class="direct-section"><h2>Direct play</h2><form id="direct-form" class="inline-form"><input type="url" name="url" required placeholder="HTTP / HTTPS video URL" aria-label="Direct video URL">${button('Play URL','submit-direct')}</form></section>`;
}
const startOfWeek = () => { const d = new Date(); d.setHours(12,0,0,0); d.setDate(d.getDate() - (d.getDay()+6)%7 + weekOffset*7); return d; };
function calendar() {
  const start = startOfWeek();
  const days = Array.from({length:7},(_,i) => { const date = new Date(start); date.setDate(date.getDate()+i); return date; });
  const short = d => d.toLocaleDateString('en-GB',{day:'numeric',month:'short'});
  return `${heading('Calendar', `${button('←','previous-week')}${button('Today','today')}${button('→','next-week')}${button('Refresh','refresh')}`)}<div class="calendar-meta"><h2>${short(days[0])} – ${short(days[6])} ${days[6].getFullYear()}</h2><span class="muted tiny">Library series · Local dates · Sample schedule</span></div><div class="calendar-scroll"><div class="calendar-grid">${days.map((date,i) => { const today = date.toDateString() === new Date().toDateString(); const entries = weekOffset === 0 ? ({1:[6,7],3:[7],4:[6]})[i] || [] : []; return `<section class="calendar-day ${today ? 'today' : ''}"><header><span>${date.toLocaleDateString('en-GB',{weekday:'short'})}</span><strong>${date.getDate()}</strong>${today ? '<small>Today</small>' : ''}</header><div class="calendar-entries">${entries.length ? entries.map((id,j) => `<button class="calendar-entry" data-calendar-title="${id}" data-calendar-episode="${i+j+1}" data-title="${id}"><img src="${poster(titles[id])}" alt=""><strong>${esc(titles[id].name)}</strong><small>S1 E${i+j+1} · ${date < new Date() ? 'Released' : 'Upcoming'}</small><small>Sample episode</small></button>`).join('') : '<span class="no-releases">No releases</span>'}</div></section>`; }).join('')}</div></div><p class="notice">Release dates come from episode metadata, not stream availability. Titles without a release date are omitted.</p>`;
}
function search() {
  const ids = titles.map((_,i) => i).filter(i => titles[i].name.toLowerCase().includes(query.toLowerCase()));
  return `${heading('Search', '', `${ids.length} results${query ? ' for “'+esc(query)+'”' : ''}`)}${ids.length ? grid(ids) : blank('No matching titles.')}`;
}
const formatTime = minutes => `${Math.floor(minutes/60)}:${String(Math.floor(minutes%60)).padStart(2,'0')}:${String(Math.floor(minutes*60%60)).padStart(2,'0')}`;
function player() {
  const p = playback.get(selected);
  return `${heading(titleName(), button('Stop playback','stop'), titles[selected].series ? `Season ${season} · Episode ${episode}` : '')}<section class="player-screen"><span class="player-watermark">HTML player preview · No video loaded</span><div class="player-center">${paused ? 'Paused' : 'Simulated playback'}</div><div class="player-controls"><input id="timeline" type="range" aria-label="Seek position" min="0" max="${p.duration}" step="0.1" value="${p.position}"><div class="time-row"><span id="position-label">${formatTime(p.position)}</span><span>${formatTime(p.duration)}</span></div><div class="player-actions">${button(paused ? '▶ Play' : 'Ⅱ Pause','pause')}<button class="icon-button" data-action="seek-back" aria-label="Seek back 10 seconds">↶</button><button class="icon-button" data-action="seek-forward" aria-label="Seek forward 10 seconds">↷</button>${button('Mute','mute')}<input class="volume" type="range" min="0" max="100" value="75" aria-label="Volume"><span class="spacer"></span>${button('CC','subtitles')}${button('Audio','audio')}${button('⛶','fullscreen')}</div></div></section><div class="simulation-controls"><span class="muted tiny">Preview controls</span>${button('Simulate 71% watched','simulate-watched')}${button('Simulate finish & stop','simulate-finish')}<span class="tiny" id="watch-status">${p.watched ? '✓ Marked watched' : 'Not marked watched'}</span></div><p class="notice">Seeking does not count as time watched. More than 70% watched marks the video watched. Stopping beyond 90% advances to the next released episode, or removes a completed movie from Currently watching.</p>`;
}
function stateContent() {
  const name = paths.find(([id]) => id === route)?.[1] || 'Currently watching';
  if (viewState === 'loading') return `${heading(name)}<div class="poster-grid">${Array.from({length:6},() => '<div class="skeleton"><div></div><p></p></div>').join('')}</div><p class="notice" role="status">Loading…</p>`;
  if (viewState === 'error') return `${heading(name)}<div class="empty"><h2>Could not load data</h2><p class="muted">The provider did not respond. Your local library is unchanged.</p>${button('Retry','retry','primary')}</div>`;
  return `${heading(name)}<div class="empty"><h2>${route === 'watching' ? 'Nothing in progress' : route === 'calendar' ? 'No releases this week' : route === 'library' ? 'No saved titles' : 'No results'}</h2><p class="muted">${route === 'watching' ? 'Start a movie or episode to track it here.' : 'Try a different filter or refresh the page.'}</p>${button('Refresh','retry')}</div>`;
}
function render() {
  closeMenu();
  if (!paths.some(([id]) => id === route)) route = 'watching';
  if (route === 'series') selected = titles[selected].series ? selected : 6;
  if (route === 'detail' && titles[selected].series) selected = 0;
  if (route === 'player' && !playback.has(selected)) initPlayback();
  const screens = {watching:currentlyWatching,movies:catalog,shows:catalog,library,watched:library,calendar,addons:addonPage,installed:addonPage,search,detail,series:detail,sources,player};
  app.innerHTML = shell(viewState === 'ready' ? screens[route]() : stateContent());
  document.title = `Skry — ${paths.find(([id]) => id === route)[1]} · Preview`;
}
function toast(text) { const el=document.querySelector('#toast'); el.textContent=text; el.classList.add('visible'); clearTimeout(window.toastTimer); window.toastTimer=setTimeout(()=>el.classList.remove('visible'),2500); }
function modal(content) { closeMenu(); dialog.innerHTML=`<button class="icon-button dialog-close" data-action="close-dialog" aria-label="Close dialog">×</button>${content}`; dialog.showModal(); }
function initPlayback() {
  const id = titles[selected].series ? `${selected}:${season}:${episode}` : String(selected);
  const existing = playback.get(selected);
  if (!existing || existing.videoId !== id) playback.set(selected,{videoId:id,position:0,duration:titles[selected].duration,watchedTime:0,watched:watched.has(selected) || watchedEpisodes.has(id),active:true});
  else existing.active = true;
}
function startPlayer() { initPlayback(); paused=false; go('player'); }
function syncWatched(id) {
  const p = playback.get(id);
  if (p?.watched) { if (titles[id].series) watchedEpisodes.add(p.videoId); else watched.add(id); }
}
function stopPlayer() {
  syncWatched(selected);
  playback.set(selected,reconcilePlayback(playback.get(selected),episodesFor(selected)));
  go('watching');
}
function openTitle(id) { selected=Number(id); const p=playback.get(selected); if (titles[selected].series && p) { [,season,episode]=p.videoId.split(':').map(Number); } go(titles[selected].series ? 'series' : 'detail'); }
function closeMenu() { contextMenu.hidden=true; }
function titleActions(id, key = '') {
  return `<div class="menu-heading">${esc(titles[id].name)}${key ? ' · S'+key.split(':')[1]+' E'+key.split(':')[2] : ''}</div><button role="menuitem" data-quick="${saved.has(id) ? 'remove' : 'save'}" data-id="${id}">${saved.has(id) ? 'Remove from watch later' : 'Watch later'}</button><button role="menuitem" data-quick="watched" data-id="${id}" data-key="${key}">Mark as watched${titles[id].series && !key ? ' · Whole series' : ''}</button><button role="menuitem" data-quick="unwatched" data-id="${id}" data-key="${key}">Mark as unwatched${titles[id].series && !key ? ' · Whole series' : ''}</button><div class="menu-divider"></div><button role="menuitem" data-action="refresh">↻ Refresh</button>`;
}
function showMenu(x,y,id,key='') {
  contextMenu.innerHTML = id === undefined ? '<button role="menuitem" data-action="refresh">↻ Refresh</button>' : titleActions(Number(id),key);
  contextMenu.hidden=false;
  const rect=contextMenu.getBoundingClientRect();
  contextMenu.style.left=Math.max(8,Math.min(x,innerWidth-rect.width-8))+'px';
  contextMenu.style.top=Math.max(8,Math.min(y,innerHeight-rect.height-8))+'px';
  contextMenu.querySelector('button').focus();
}
document.addEventListener('contextmenu',event=>{
  if (event.target.closest('input,textarea,select') || dialog.open) return;
  event.preventDefault();
  const title=event.target.closest('[data-title]');
  showMenu(event.clientX,event.clientY,title?.dataset.title,event.target.closest('[data-episode-key]')?.dataset.episodeKey || '');
});
document.addEventListener('pointerdown',event=>{ if (!contextMenu.contains(event.target)) closeMenu(); });
window.addEventListener('scroll',closeMenu,true);
window.addEventListener('resize',closeMenu);
function quickAction(target) {
  const id=Number(target.dataset.id), action=target.dataset.quick, key=target.dataset.key;
  if (action === 'save') saved.add(id);
  if (action === 'remove') saved.delete(id);
  if (action === 'watched' || action === 'unwatched') {
    const value=action === 'watched';
    if (titles[id].series) {
      (key ? [{id:key}] : episodesFor(id).filter(e=>new Date(e.released)<=new Date())).forEach(e=> value ? watchedEpisodes.add(e.id) : watchedEpisodes.delete(e.id));
      if (!key) { if (value) watched.add(id); else watched.delete(id); }
    } else if (value) watched.add(id); else watched.delete(id);
    const p=playback.get(id);
    if (p && (!key || key === p.videoId)) {
      p.watched=value;
      p.position=0;
      p.watchedTime=0;
      if (value) {
        const next=titles[id].series && key ? episodesFor(id).find(e=>!watchedEpisodes.has(e.id) && new Date(e.released)<=new Date()) : undefined;
        p.active=Boolean(next);
        if (next) p.videoId=next.id;
      } else p.active=false;
    }
  }
  render();
  toast(action === 'save' ? 'Added to To watch' : action === 'remove' ? 'Removed from To watch' : action === 'watched' ? 'Marked as watched' : 'Marked as unwatched');
}
function installDialog(name='') {
  modal(`<h2>${name ? 'Install '+esc(name) : 'Add add-on'}</h2>${name === 'Torrentio' ? '<p class="muted">Use a configured manifest for debrid sources. The default manifest uses direct torrents.</p>' : ''}<form id="install-form"><label for="addon-name">Name</label><input id="addon-name" name="name" required value="${esc(name)}" placeholder="Provider name"><label for="manifest">HTTPS manifest URL</label><input type="url" id="manifest" name="manifest" required placeholder="https://…/manifest.json" value="${name === 'Cinemeta' ? 'https://v3-cinemeta.strem.io/manifest.json' : name === 'Torrentio' ? 'https://torrentio.strem.fun/manifest.json' : ''}"><p class="notice">This preview does not fetch or save this URL. Do not enter real credentials.</p>${button('Install','submit-install','primary')}</form>`);
}
document.addEventListener('click',event=>{
  const target=event.target.closest('button');
  if (!target) return;
  if (target.dataset.quick) { quickAction(target); return; }
  if (target.dataset.open !== undefined) { openTitle(target.dataset.open); return; }
  if (target.dataset.resume !== undefined) {
    selected=Number(target.dataset.resume); const p=playback.get(selected);
    if (titles[selected].series) [,season,episode]=p.videoId.split(':').map(Number);
    startPlayer(); return;
  }
  if (target.dataset.calendarTitle !== undefined) { selected=Number(target.dataset.calendarTitle);season=1;episode=Number(target.dataset.calendarEpisode);go('series');return; }
  if (target.dataset.menuTitle !== undefined || target.dataset.menuEpisode) { const rect=target.getBoundingClientRect(); const key=target.dataset.menuEpisode || ''; showMenu(rect.left,rect.bottom+5,target.dataset.menuTitle ?? key.split(':')[0],key);return; }
  if (target.dataset.episode) { episode=Number(target.dataset.episode);render();return; }
  if (target.dataset.installAddon) { installDialog(target.dataset.installAddon);return; }
  if (target.dataset.removeAddon) { modal(`<h2>Remove ${esc(target.dataset.removeAddon)}?</h2><p class="muted">Only the preview list will change.</p><button class="button primary" data-confirm-remove="${esc(target.dataset.removeAddon)}">Remove</button>`);return; }
  if (target.dataset.confirmRemove) { addonNames=addonNames.filter(name=>name!==target.dataset.confirmRemove);dialog.close();render();return; }
  if (target.dataset.source !== undefined) {
    if (target.dataset.source === '2') modal(`<h2>Direct torrent playback</h2><p class="muted">Your IP address is visible to peers. Torrents may upload data and use disk space. Only play authorised media.</p><p class="notice">No torrent is started in this preview.</p>${button('Continue','confirm-torrent','primary')}`);
    else startPlayer();
    return;
  }
  const action=target.dataset.action;
  if (action === 'back') history.back();
  if (action === 'forward') history.forward();
  if (action === 'refresh') { viewState='ready';status=`Refreshed ${new Date().toLocaleTimeString()} · Sample data`;render();toast('Preview refreshed · No live provider request'); }
  if (action === 'retry') { viewState='ready';render(); }
  if (action === 'sources') go('sources');
  if (action === 'title-back') go(titles[selected].series ? 'series' : 'detail');
  if (action === 'save-detail') { if (saved.has(selected)) saved.delete(selected); else saved.add(selected);render(); }
  if (action === 'detail-menu') { const r=target.getBoundingClientRect();showMenu(r.left,r.bottom,selected); }
  if (action === 'add-addon') installDialog();
  if (action === 'close-dialog') dialog.close();
  if (action === 'confirm-torrent') { dialog.close();startPlayer(); }
  if (action === 'previous-week' || action === 'next-week' || action === 'today') { weekOffset=action === 'today' ? 0 : weekOffset+(action === 'previous-week' ? -1 : 1);render(); }
  if (action === 'pause') { paused=!paused;render(); }
  if (action === 'stop') stopPlayer();
  if (action === 'simulate-watched' || action === 'simulate-finish') {
    const p=playback.get(selected), position=p.duration*(action === 'simulate-watched' ? .71 : .95);
    playback.set(selected,updatePlayback(p,{position,duration:p.duration,elapsed:Math.max(0,position-p.watchedTime)}));
    syncWatched(selected);
    if (action === 'simulate-finish') { stopPlayer();toast(titles[selected].series ? 'Episode finished · Continue with the next released episode' : 'Movie finished · Removed from Currently watching'); }
    else { render();toast('Marked watched · Playback can continue'); }
  }
  if (action === 'mute') { const range=document.querySelector('.volume'); range.value=range.value === '0' ? 75 : 0;target.textContent=range.value === '0' ? 'Unmute' : 'Mute'; }
  if (action === 'seek-back' || action === 'seek-forward') { const range=document.querySelector('#timeline');range.value=Number(range.value)+(action === 'seek-back' ? -1/6 : 1/6);range.dispatchEvent(new Event('input',{bubbles:true})); }
  if (action === 'fullscreen') { const el=document.querySelector('.player-screen'); if (document.fullscreenElement) document.exitFullscreen();else el.requestFullscreen().catch(()=>toast('Full screen unavailable')); }
  if (action === 'subtitles' || action === 'audio') modal(`<h2>${action === 'subtitles' ? 'Subtitles' : 'Audio'}</h2><div class="choice-list">${['English','French',action === 'subtitles' ? 'Off' : 'English · Audio description'].map(label=>`<button class="button" data-track="${action}">${label}</button>`).join('')}</div>`);
  if (target.dataset.track) { document.querySelector(`[data-action="${target.dataset.track}"]`).textContent=(target.dataset.track==='audio'?'Audio: ':'CC: ')+target.textContent;dialog.close(); }
});
document.addEventListener('change',event=>{
  if (event.target.id === 'screen-select') { const next=event.target.value; if (next === 'series') selected=6;if (next === 'detail') selected=0;go(next); }
  if (event.target.id === 'state-select') { viewState=event.target.value;render(); }
  if (event.target.id === 'genre') { genre=event.target.value;render(); }
  if (event.target.id === 'library-type') { libraryType=event.target.value;render(); }
  if (event.target.id === 'season') { season=Number(event.target.value);episode=1;render(); }
});
document.addEventListener('input',event=>{
  if (event.target.id === 'timeline') {
    const p=playback.get(selected);
    playback.set(selected,updatePlayback(p,{position:Number(event.target.value),duration:p.duration,seek:true}));
    document.querySelector('#position-label').textContent=formatTime(Number(event.target.value));
  }
});
document.addEventListener('submit',event=>{
  event.preventDefault();
  const data=new FormData(event.target);
  if (event.target.id === 'search-form') { query=document.querySelector('#search-input').value.trim();go('search'); }
  if (event.target.id === 'install-form') {
    if (new URL(data.get('manifest')).protocol !== 'https:') { toast('Use an HTTPS manifest URL');return; }
    const name=data.get('name').trim();if (!name) return;
    if (!addonNames.includes(name)) addonNames.push(name);
    dialog.close();go('installed');toast('Installed in preview only');
  }
  if (event.target.id === 'direct-form') { if (!['http:','https:'].includes(new URL(data.get('url')).protocol)) { toast('Use an HTTP or HTTPS URL');return; }startPlayer();toast('Simulated player · URL not fetched'); }
});
document.addEventListener('keydown',event=>{
  if (event.key === 'Escape') closeMenu();
  if (!contextMenu.hidden && ['ArrowDown','ArrowUp'].includes(event.key)) { event.preventDefault();const buttons=[...contextMenu.querySelectorAll('button')];const i=buttons.indexOf(document.activeElement);buttons[(i+(event.key==='ArrowDown'?1:buttons.length-1))%buttons.length].focus();return; }
  if (event.target.matches('input,select,textarea') || dialog.open || !contextMenu.hidden) return;
  if (event.key === '/') { event.preventDefault();document.querySelector('#search-input').focus(); }
  if (route === 'player') { const action={' ':'pause',k:'pause',m:'mute',f:'fullscreen',ArrowLeft:'seek-back',ArrowRight:'seek-forward'}[event.key];if (action) { event.preventDefault();document.querySelector(`[data-action="${action}"]`)?.click(); } }
});
render();
