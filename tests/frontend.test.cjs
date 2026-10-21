const test = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const path = require('node:path');

function keyEvent(code, extra = {}) {
    return { code, key:code, target:{closest:()=>null}, preventDefault(){ this.prevented=true; }, ...extra };
}

function fixture(language = 'ko', withLanguageControl = false) {
    const elements = new Map();
    class Element {
        constructor() {
            this.children = []; this.style = {}; this.dataset = {}; this.value = ''; this.listeners = {};
            this.className = ''; this.naturalWidth = 10; this.naturalHeight = 10;
            this.scrollTop = 0; this.clientHeight = 600; this.clientWidth = 380; this.parentNode = null; this.measuredHeight = 136;
            const classes = new Set();
            this.classList = {add: x => classes.add(x), remove: x => classes.delete(x), contains: x => classes.has(x), toggle: (x,on) => on ? classes.add(x) : classes.delete(x)};
        }
        set innerHTML(value) { this.children = []; this.html = value; }
        get innerHTML() { return this.html || ''; }
        appendChild(child) { return this.insertBefore(child, null); }
        insertBefore(child, next) {
            child.remove(); child.parentNode = this;
            const index = next ? this.children.indexOf(next) : this.children.length;
            this.children.splice(index, 0, child); return child;
        }
        remove() {
            if (this.parentNode) this.parentNode.children = this.parentNode.children.filter(child=>child !== this);
            this.parentNode = null;
        }
        contains(child) { return child === this || this.children.some(el=>el.contains(child)); }
        getBoundingClientRect() { return {height:this.measuredHeight}; }
        append(...children) { children.forEach(child=>this.appendChild(child)); }
        replaceChildren(...children) { this.children.forEach(child=>child.parentNode=null); this.children=[]; this.append(...children); }
        addEventListener(name, callback) { this.listeners[name] = callback; }
        removeEventListener(name, callback) { if (this.listeners[name] === callback) delete this.listeners[name]; }
        querySelector() { return null; }
        querySelectorAll() { return []; }
        removeAttribute(name) { this[name] = ''; }
        setAttribute(name, value) { this[name] = value; }
        closest() { return null; }
        focus() { this.focused = true; document.activeElement = this; }
        blur() { this.focused = false; document.activeElement = null; }
        select() {}
        click() { return this.listeners.click?.({stopPropagation(){}}); }
        showModal() { this.open = true; }
        close() { this.open = false; }
        pause() {} load() {} play() { return Promise.resolve(); } scrollIntoView() {} scrollTo() {}
        getContext() { return {drawImage() {}, fillRect() {}, scale() {}, fillText() {}, measureText(text){return {width:text.length*10};}}; }
        toDataURL() { return 'data:image/jpeg;base64,/9j/2Q=='; }
        decode() { return Promise.resolve(); }
        toBlob(callback) { this.blobCallback = callback; }
    }
    const get = id => { if (!elements.has(id)) elements.set(id, new Element()); return elements.get(id); };
    let boot; let now = 0;let reloads=0;
    const document = { body: new Element(), getElementById: get, createElement: () => new Element(), querySelector: get, querySelectorAll: selector => withLanguageControl&&selector==='[data-language-select]'?[get('language-control')]:[],
        addEventListener: (name, callback) => { if (name === 'DOMContentLoaded') boot = callback; } };
    const frames = new Map(); let frameId = 0;
    const timers = new Map(); let timerId = 0;
    const calls = []; const images = []; const confirmations = []; const alerts = []; const ipcMessages=[]; let confirmResult = true;
    let responder = async () => new Response('{}', {status:200});
    const context = vm.createContext({ document, window:{ipc:{postMessage:message=>ipcMessages.push(message)},__valtheumLanguage:language,__valtheumToken:'native-bootstrap-token',location:{origin:'http://127.0.0.1:8080',href:'http://127.0.0.1:8080/',reload:()=>reloads++}},
        crypto:require("node:crypto").webcrypto, URL, Headers, AbortController, Blob, FormData, TextEncoder, Date:class extends Date {static now(){return Date.now()+now;}}, performance:{now:()=>now}, console:{log(){}, error(){}}, confirm:message=>{ confirmations.push(message); return confirmResult; }, alert:message=>alerts.push(message),
        requestAnimationFrame: callback=>{ frames.set(++frameId, callback); return frameId; }, cancelAnimationFrame:id=>frames.delete(id),
        setTimeout:(fn,ms)=>{timers.set(++timerId,{fn,ms});return timerId;}, clearTimeout(id){timers.delete(id);}, Image:class extends Element {constructor(){ super(); images.push(this); }},
        fetch: async (...args) => { calls.push(args); return responder(...args); }
    });
    vm.runInContext(fs.readFileSync(path.join(__dirname,'../static/i18n-catalog.js'),'utf8'),context);
    vm.runInContext(fs.readFileSync(path.join(__dirname,'../static/i18n.js'),'utf8'),context);
    vm.runInContext(fs.readFileSync(path.join(__dirname, '../static/archive-job-monitor.js'), 'utf8'), context);
    vm.runInContext(fs.readFileSync(path.join(__dirname, '../static/archive-page-editor.js'), 'utf8'), context);
    vm.runInContext(fs.readFileSync(path.join(__dirname, '../static/media-name.js'), 'utf8'), context);
    vm.runInContext(fs.readFileSync(path.join(__dirname, '../static/virtual-list.js'), 'utf8'), context);
    vm.runInContext(fs.readFileSync(path.join(__dirname, '../static/series-editor.js'), 'utf8'), context);
    vm.runInContext(fs.readFileSync(path.join(__dirname, '../static/backup-restore.js'), 'utf8'), context);
    vm.runInContext(fs.readFileSync(path.join(__dirname, '../static/picture-frame.js'), 'utf8'), context);
    vm.runInContext(fs.readFileSync(path.join(__dirname, '../static/privacy-screen.js'), 'utf8'), context);
    vm.runInContext(fs.readFileSync(path.join(__dirname, '../static/security.js'), 'utf8'), context);
    vm.runInContext(fs.readFileSync(path.join(__dirname, '../static/archive-security.js'), 'utf8'), context);
    vm.runInContext(fs.readFileSync(path.join(__dirname, '../static/storage-recovery.js'), 'utf8'), context);
    vm.runInContext(fs.readFileSync(path.join(__dirname, '../static/integrity-check.js'), 'utf8'), context);
    vm.runInContext(fs.readFileSync(path.join(__dirname, '../static/bookmark-mutations.js'), 'utf8'), context);
    vm.runInContext(fs.readFileSync(path.join(__dirname, '../static/pdf-viewer.js'), 'utf8'), context);
    vm.runInContext(fs.readFileSync(path.join(__dirname, '../static/note-editor.js'), 'utf8'), context);
    let source = fs.readFileSync(path.join(__dirname, '../static/app.js'), 'utf8');
    source = source.slice(0, source.indexOf('    // Initial load:')) + `
        globalThis.hooks = { requestCount:()=>activeRequests.size, updateTrashCount, restoreFromTrash, permanentlyDelete, handleVote, handleArenaPass, emptyTrash, moveToTrash, loadTrashItems, noteEditor, pdfViewer, getFilteredFiles, storageRecovery, pageEditor, nameEditor, initializeVaultSession, navigateMedia, privacy, dbRestore, archiveSecurity, dbSecurity, adoptLibraryState, vaultFetch, scopedUrl, invalidateRequests, captureImageThumbnail, captureFrameAndUpload, selectMedia, renderScanStatus,
            getImageMeta, preloadArchiveImages, clearImageJobs, addTag, removeTag, setSort:key=>currentSortBy=key, selectMedia, loadMediaFiles, saveBookmark, loadBookmarks, resetBookmarks,
            loadArenaPair, handleArenaArchiveWheel, exitArena, setArenaMode:()=>{inArenaMode=true;},
            handleShortcut, shortcuts, loadTrashLogs, resetLibraryView, applySearchAndRender, mediaVirtualList, seriesEditor, updateFileRating, resetWatchCount, renderArenaItem, showArchiveImage, buildArchiveOrder,
            cacheImage: (path, entry, meta) => imageMetaCache.set(metaKey(path,entry), meta),
            setArchive: (entries, index) => { setArchiveEntries(entries); currentImageIdx = index; },
            setSession: (generation) => { vaultSession = {token:'test-token', generation}; },
            setFiles: (files) => { filesData = files; applySearchAndRender(); },
            setCurrent: file => currentFile = file,
            bumpSelection: () => { selectionVersion += 1; },
            stats: () => ({active:imageMetaActive, pending:imageMetaPending.size, cache:imageMetaCache.size})
        };
    });`;
    vm.runInContext(source, context); boot(); context.hooks.setSession(1);
    return {h:context.hooks, reloadCount:()=>reloads, get, calls, images, confirmations, alerts, timers, ipcMessages, advance:ms=>{now+=ms;}, confirm: value => confirmResult = value, flush: () => { const callbacks = [...frames.values()]; frames.clear(); callbacks.forEach(callback=>callback()); }, respond: fn => responder = fn};
}

function requestRecoveryFixture(kind){
 const f=fixture('en'),file={id:1,rel_path:'book.zip',name:'book.zip',file_type:'ARCHIVE',encrypted:true,size:10,state:kind==='encrypt'?'plain':'ready',original:true,tags:[]};
 const editor=kind==='name'?f.h.nameEditor:kind==='pages'?f.h.pageEditor:f.h.archiveSecurity;
 let changed=0;editor.onChanged=async()=>{changed++;};
 const open=()=>{if(kind==='name')editor.open(file);else if(kind==='pages')editor.open(file,['a.png','b.png'],0);else editor.open();};
 if(kind==='name'){open();editor.input.value='new.zip';}else if(kind==='pages')open();
 else {editor.files=[file];editor.selected=new Set([1]);editor.dialog.showModal();f.get('media-encryption-risk-ack').checked=true;}
 const start=()=>kind==='name'||kind==='pages'?editor.submit():kind==='encrypt'?editor.batch():kind.startsWith('single-')?editor.single(file,kind.slice(7)):editor.batchAction(kind);
 const tick=async ms=>{const entry=[...f.timers].find(([,t])=>t.ms===ms);assert.ok(entry,'timer '+ms);f.timers.delete(entry[0]);entry[1].fn();await new Promise(setImmediate);};
 const listReply=url=>new Response(JSON.stringify(url.includes('/api/files?')?[]:{files:[file]}));
 const mutations=()=>f.calls.filter(([url,options])=>['POST','DELETE'].includes(options.method)&&!url.includes('/cancel?'));
 return {f,editor,start,open,tick,listReply,mutations,changed:()=>changed};
}

for(const kind of ['name','pages','encrypt','export','plaintext','single-export','single-plaintext']){
 for(const mode of ['lost','stalled'])test(kind+' recovers a lost start acknowledgement without another mutation: '+mode,async()=>{
  const {f,editor,start,open,tick,listReply,mutations,changed}=requestRecoveryFixture(kind);let receipt,body;
  f.respond(async(url,options)=>{
   if(['POST','DELETE'].includes(options.method)){
    receipt=new Headers(options.headers).get('X-Vault-Request');assert.match(receipt,/^[a-f0-9]{32}$/);
    return {ok:true,json:()=>mode==='lost'?Promise.reject(new TypeError('Lost start body')):new Promise(r=>body=r),text:async()=>'',blob:async()=>new Blob()};
   }
   if(url.includes('request_id=')){assert.ok(url.includes(receipt));return new Response(JSON.stringify({job:{request_id:receipt,job_id:'own-job',running:false,completed:true}}));}
   if(url.includes('/progress?'))return new Response('{"job":{"job_id":"own-job","running":false,"completed":true}}');
   return listReply(url);
  });
  const work=start();await new Promise(setImmediate);if(mode==='stalled')await tick(15000);
  assert.equal(editor.pending,true);assert.equal(mutations().length,1);
  editor.dismiss();assert.equal(editor.isOpen(),false);await open();await start();assert.equal(mutations().length,1);
  await tick(1000);await work;
  assert.equal(editor.pending,false);assert.equal(changed(),kind.startsWith('single-')?0:1);assert.equal(mutations().length,1);
  if(body){body({status:'cancelled'});await new Promise(setImmediate);assert.equal(editor.pending,false);assert.equal(mutations().length,1);}
  assert.equal(f.timers.size,0);
 });
}

for(const kind of ['name','pages','encrypt'])test(kind+' retries an unknown start with the same identity and tolerates a busy replay response',async()=>{
 const {f,editor,start,tick,listReply,mutations}=requestRecoveryFixture(kind);let receipt,lookups=0;
 f.respond(async(url,options)=>{
  if(options.method==='POST'||options.method==='DELETE'){
   const id=new Headers(options.headers).get('X-Vault-Request');if(!receipt){receipt=id;throw new TypeError('Lost request');}
   assert.equal(id,receipt);return new Response('Busy',{status:409});
  }
  if(url.includes('request_id='))return new Response(JSON.stringify(++lookups===1?{status:'unknown'}:{job:{request_id:receipt,job_id:'job',running:false,completed:true}}));
  if(url.includes('/progress?'))return new Response('{"job":{"job_id":"job","running":false,"completed":true}}');
  return listReply(url);
 });
 const work=start();await new Promise(setImmediate);await tick(1000);
 assert.equal(editor.pending,true);assert.equal(mutations().length,2);assert.equal(mutations()[0][1].body,mutations()[1][1].body);
 await tick(2000);await work;assert.equal(editor.pending,false);assert.equal(mutations().length,2);
});

for(const kind of ['encrypt','export','plaintext','single-export','single-plaintext'])test(kind+' keeps monitoring and cancellation after a transient progress failure',async()=>{
 const {f,editor,start,tick,listReply,mutations}=requestRecoveryFixture(kind);let polls=0,cancelled=false;
 f.respond(async(url,options)=>{
  if(url.includes('/cancel?')){cancelled=true;return new Response('{"status":"cancelling"}');}
  if(options.method==='POST'||options.method==='DELETE')return new Response('{"status":"started","job_id":"job"}');
  if(url.includes('/progress?')){
   if(++polls===1)throw new TypeError('Lost progress');
   return new Response(JSON.stringify({job:{job_id:'job',running:false,error:'Cancelled; original preserved'}}));
  }
  return listReply(url);
 });
 const work=start();await new Promise(setImmediate);
 assert.equal(editor.pending,true);assert.equal(editor.cancel.hidden,false);await editor.cancel.click();assert.equal(cancelled,true);
 await tick(1000);await work;assert.equal(editor.pending,false);assert.equal(mutations().length,1);assert.equal(f.timers.size,0);
});

for(const stage of ['body','retry'])test('reset stops lost-start reconciliation during '+stage,async()=>{
 const {f,editor,start,open,mutations}=requestRecoveryFixture('pages');let body;
 f.respond(async()=>({ok:true,json:()=>stage==='retry'?Promise.reject(new TypeError('Lost')):new Promise(r=>body=r),text:async()=>'',blob:async()=>new Blob()}));
 const work=start();await new Promise(setImmediate);f.h.resetLibraryView();f.h.setSession(2);open();editor.message.textContent='New library';
 await work;if(body){body({status:'started',job_id:'old'});await new Promise(setImmediate);}
 assert.equal(editor.message.textContent,'New library');assert.equal(mutations().length,1);assert.equal(f.timers.size,0);
});

for(const nextLibrary of [false,true])test('late encryption cancellation cannot change a newer job (new library='+nextLibrary+')',async()=>{
 const f=fixture('en'),s=f.h.archiveSecurity;let cancelReply,progressReply,job=0;
 f.respond(async url=>{
  if(url.includes('/cancel?'))return new Promise(r=>cancelReply=r);
  if(url.includes('/progress?'))return new Promise(r=>progressReply=r);
  if(url.includes('/encrypt?'))return new Response(JSON.stringify({status:'started',job_id:'job-'+(++job)}));
  return new Response(JSON.stringify(url.includes('/api/files?')?[]:{files:[]}));
 });
 function prepare(id){s.files=[{id,name:id+'.zip',size:10,state:'plain'}];s.selected=new Set([id]);s.dialog.showModal();f.get('media-encryption-risk-ack').checked=true;}
 prepare(1);const first=s.batch();await new Promise(setImmediate);
 const cancelling=s.cancel.click();await new Promise(setImmediate);
 await s.cancel.click();assert.equal(f.calls.filter(([url])=>url.includes('/cancel?')).length,1);
 progressReply(new Response(JSON.stringify({job:{job_id:'job-1',running:false,completed:true}})));await first;
 s.dismiss();if(nextLibrary){f.h.resetLibraryView();f.h.setSession(2);}
 prepare(2);const second=s.batch();await new Promise(setImmediate);
 s.status.textContent='New job progress';f.get('scan-status').textContent='Current library';
 cancelReply(new Response('Old cancellation failed',{status:500}));await cancelling;
 assert.equal(s.status.textContent,'New job progress');assert.equal(s.cancel.disabled,false);
 assert.equal(f.get('scan-status').textContent,'Current library');assert.equal(s.pending,true);
 progressReply(new Response(JSON.stringify({job:{job_id:'job-2',running:false,completed:true}})));await second;
});

test('current encryption cancellation failure remains visible and retryable',async()=>{
 const f=fixture('en'),s=f.h.archiveSecurity;
 s.scope=s.captureScope();s.jobId='job';s.setPending(true);
 f.respond(async()=>new Response('Cancel temporarily failed',{status:500}));
 await s.cancel.click();assert.equal(s.status.textContent,'Cancel temporarily failed');assert.equal(s.cancel.disabled,false);
 await s.cancel.click();assert.equal(f.calls.length,2);
 s.reset();
});

for(const kind of ['name','pages']){
 function prepare(){
  const f=fixture('en'),editor=kind==='name'?f.h.nameEditor:f.h.pageEditor;
  const file={id:1,rel_path:'book.zip',file_type:'ARCHIVE',encrypted:true,tags:[]};
  const open=()=>kind==='name'?editor.open(file):editor.open(file,['a.png','b.png'],0);
  open();if(kind==='name')f.get('media-name-input').value='renamed.zip';
  let changed=0;editor.onChanged=async()=>{changed++;};
  const tick=async ms=>{const entry=[...f.timers].find(([,t])=>t.ms===ms);assert.ok(entry,'timer '+ms);f.timers.delete(entry[0]);entry[1].fn();await new Promise(setImmediate);};
  return {f,editor,open,tick,changed:()=>changed};
 }
 for(const result of ['complete','cancelled'])test(kind+' retains job, cancellation and selection after a lost progress response: '+result,async()=>{
  const {f,editor,open,tick,changed}=prepare();let polls=0;
  f.respond(async(url,opts)=>{
   if(url.includes('/cancel?'))return new Response('{"status":"cancelling"}');
   if(url.includes('/progress?')){
    polls++;if(polls===1)throw new TypeError('Temporary connection failure');
    return new Response(JSON.stringify({job:{job_id:'job',running:false,completed:result==='complete',error:result==='cancelled'?'Cancelled; original preserved':null}}));
   }
   return new Response('{"status":"started","job_id":"job"}');
  });
  const pending=editor.submit();await new Promise(setImmediate);
  assert.equal(editor.pending,true);assert.equal(editor.cancel.hidden,false);assert.equal(editor.cancel.disabled,false);assert.equal(editor.save.disabled,true);
  assert.match(editor.message.textContent,/Checking again/);assert.equal(editor.jobId,'job');
  if(kind==='name')assert.equal(editor.input.value,'renamed.zip');else assert.deepEqual([...editor.selected],['a.png']);
  editor.dismiss();assert.equal(editor.isOpen(),false);open();assert.equal(editor.isOpen(),true);
  await editor.submit();assert.equal(f.calls.filter(([url])=>url.includes('/1/name')||url.includes('/1/page')).length,1);
  await editor.cancel.click();assert.ok(f.calls.some(([url])=>url.includes('/cancel?job_id=job')));
  await tick(1000);await pending;
  assert.equal(editor.pending,false);assert.equal(changed(),result==='complete'?1:0);
  if(result==='cancelled'){assert.match(editor.message.textContent,/original preserved/);assert.equal(editor.save.disabled,false);}
  else assert.equal(editor.isOpen(),false);
  assert.equal(f.timers.size,0);
 });
 test(kind+' bounds a stalled progress body and ignores its late completion',async()=>{
  const {f,editor,tick,changed}=prepare();let body,polls=0;
  f.respond(async url=>{
   if(!url.includes('/progress?'))return new Response('{"status":"started","job_id":"job"}');
   if(++polls===1)return {ok:true,json:()=>new Promise(r=>body=r),text:async()=>'',blob:async()=>new Blob()};
   return new Response('{"job":{"job_id":"job","running":false,"completed":true}}');
  });
  const pending=editor.submit();await new Promise(setImmediate);await tick(15000);
  assert.equal(editor.pending,true);assert.match(editor.message.textContent,/Checking again/);assert.equal(editor.cancel.hidden,false);
  body({job:{job_id:'job',running:false,error:'Late obsolete error'}});await new Promise(setImmediate);
  assert.doesNotMatch(editor.message.textContent,/obsolete/);await tick(1000);await pending;
  assert.equal(changed(),1);assert.equal(f.timers.size,0);
 });
 for(const stage of ['retry','body'])test(kind+' stops recovery on reset during '+stage,async()=>{
  const {f,editor,open,changed}=prepare();let body;
  f.respond(async url=>{
   if(!url.includes('/progress?'))return new Response('{"status":"started","job_id":"job"}');
   if(stage==='retry')throw new TypeError('Temporary failure');
   return {ok:true,json:()=>new Promise(r=>body=r),text:async()=>'',blob:async()=>new Blob()};
  });
  const pending=editor.submit();await new Promise(setImmediate);
  f.h.resetLibraryView();f.h.setSession(2);open();editor.message.textContent='New library';
  await pending;if(body){body({job:{job_id:'job',running:false,completed:true}});await new Promise(setImmediate);}
  assert.equal(editor.message.textContent,'New library');assert.equal(editor.pending,false);assert.equal(changed(),0);assert.equal(f.timers.size,0);
 });
 test(kind+' uses bounded retry backoff and rejects unrelated completion',async()=>{
  const {f,editor,tick,changed}=prepare();let polls=0;
  f.respond(async url=>{
   if(!url.includes('/progress?'))return new Response('{"status":"started","job_id":"job"}');
   polls++;return new Response(JSON.stringify({job:{job_id:polls<6?'other':'job',running:false,completed:true}}));
  });
  const pending=editor.submit();await new Promise(setImmediate);
  for(const delay of [1000,2000,4000,5000,5000]){assert.equal(editor.pending,true);assert.equal(changed(),0);await tick(delay);}
  await pending;assert.equal(changed(),1);assert.equal(polls,6);assert.equal(f.timers.size,0);
 });
}

for(const stage of ['headers','body','error'])test('backup '+stage+' arriving after concealment cannot alert or alter a newer backup',async()=>{
    const f=fixture('en'),button=f.get('btn-backup-db');button.textContent='Backup';let release;
    f.respond(async()=>stage==='body'?{ok:true,json:()=>new Promise(r=>release=r),text:async()=>'',blob:async()=>new Blob()}:new Promise(r=>release=r));
    const old=button.click();await new Promise(setImmediate);assert.equal(button.disabled,true);
    await button.click();assert.equal(f.calls.length,1,'duplicate clicks do not open more backup pickers');
    f.h.resetLibraryView();f.h.privacy.setCovered(true);
    assert.equal(button.textContent,'Backup');assert.equal(button.style.background,'');
    const result={status:'success',key_included:true,backup_path:'private-backup.vtbackup'};
    release(stage==='body'?result:stage==='error'?new Response('old failure',{status:500}):new Response(JSON.stringify(result)));
    await old;assert.equal(f.alerts.length,0);assert.equal(button.textContent,'Backup');
    assert.equal([...f.timers.values()].filter(t=>t.ms===2000).length,0);
    f.h.privacy.setCovered(false);f.h.setSession(2);
    f.respond(async()=>new Response(JSON.stringify(result)));await button.click();
    assert.equal(f.alerts.length,1);assert.match(button.textContent,/complete/i);
});

test('backup errors remain visible in their own library and stale reset timers cannot touch a newer attempt',async()=>{
    const f=fixture('en'),button=f.get('btn-backup-db');button.textContent='Backup';
    f.respond(async()=>new Response('backup failed',{status:500}));await button.click();
    assert.equal(f.alerts.length,1);assert.match(f.alerts[0],/error.*backing up/i);
    const timer=[...f.timers.values()].find(t=>t.ms===2000);assert.ok(timer);
    f.h.resetLibraryView();f.h.setSession(2);let release;
    f.respond(async()=>new Promise(r=>release=r));const next=button.click();await new Promise(setImmediate);
    const label=button.textContent;timer.fn();assert.equal(button.textContent,label);assert.equal(button.disabled,true);
    release(new Response('{"status":"cancelled"}'));await next;
    assert.equal(button.textContent,'Backup');assert.equal(button.disabled,false);assert.equal(f.alerts.length,1);
});

for(const cancelled of [false,true])for(const listingFails of [false,true])test('partial encryption refreshes main list after '+(cancelled?'cancel':'failure')+' (security listing failure='+listingFails+')',async()=>{
    const f=fixture('en'),s=f.h.archiveSecurity;
    const media=[1,2].map(id=>({id,rel_path:id+'.zip',file_type:'ARCHIVE',encrypted:false,tags:[]}));
    f.h.setFiles(media);s.files=media.map(m=>({id:m.id,name:m.rel_path,file_type:m.file_type,size:10,state:'plain'}));
    s.selected=new Set([1,2]);s.dialog.showModal();f.get('media-encryption-risk-ack').checked=true;
    let committed=false,job=0;
    f.respond(async url=>{
        if(url.includes('/encrypt')){job++;return new Response(JSON.stringify({status:'started',job_id:'job-'+job}));}
        if(url.includes('/progress')){
            if(job===1){committed=true;return new Response(JSON.stringify({job:{job_id:'job-1',running:false,completed:true}}));}
            return new Response(JSON.stringify({job:{job_id:'job-2',running:false,error:cancelled?'작업이 취소되었습니다.':'Disk full'}}));
        }
        if(url.includes('/api/files?'))return new Response(JSON.stringify(media.map(m=>({...m,encrypted:m.id===1&&committed}))));
        if(url.includes('/trash/count'))return new Response('{"count":0}');
        return listingFails?new Response('Listing unavailable',{status:503}):new Response(JSON.stringify({files:s.files.map(m=>({...m,state:m.id===1&&committed?'ready':'plain'}))}));
    });
    await s.batch();assert.equal(f.h.getFilteredFiles()[0].encrypted,true);assert.equal(f.h.getFilteredFiles()[1].encrypted,false);
    assert.equal(s.encryptionBatch.completed,1);assert.deepEqual([...s.selected],[2]);assert.equal(s.pending,false);
    assert.match(s.status.textContent,cancelled?/cancel/i:/Disk full/);
    assert.equal(f.calls.filter(([u])=>u.includes('/api/files?')).length,1);
});

test('partial encryption refresh cannot restore old media or start arena reads after a library change',async()=>{
    const f=fixture('en'),s=f.h.archiveSecurity;let list,security;
    const scope=s.captureScope(),version=s.version;
    f.h.setArenaMode();
    f.respond(async url=>url.includes('/api/files?')?new Promise(r=>list=r):new Promise(r=>security=r));
    const refreshing=s.refreshViews(scope,version);await new Promise(setImmediate);
    f.h.resetLibraryView();f.h.setSession(2);f.h.setArenaMode();
    list(new Response('[{"id":1,"rel_path":"private.zip","file_type":"ARCHIVE","encrypted":true,"tags":[]}]'));
    security(new Response('{"files":[]}'));await refreshing;
    assert.equal(f.h.getFilteredFiles().length,0);assert.equal(s.files.length,0);
    assert.equal(f.calls.filter(([u])=>u.includes('/arena/pair')).length,0);
});

test('API carries session/library and rejects a body that completes after navigation', async () => {
    const f = fixture(); let resolveBody;
    f.respond(async () => ({ok:true, json:()=>new Promise(resolve=>resolveBody=resolve), text:async()=>'',blob:async()=>new Blob()}));
    const response = await f.h.vaultFetch('/api/files?sort_by=name');
    assert.match(f.calls[0][0], /library=1/);
    assert.equal(f.calls[0][1].headers.get('X-Vault-Token'), 'test-token');
    const pending = response.json(); f.h.invalidateRequests(); resolveBody([]);
    await assert.rejects(pending, error => error.name === 'AbortError');
});

test('delayed canvas capture is discarded before sending to another folder', async () => {
    const f = fixture(); f.h.captureImageThumbnail(1);
    f.h.invalidateRequests(); f.h.setSession(2);
    await f.get('thumbnail-canvas').blobCallback(new Blob(['fixture']));
    assert.equal(f.calls.length, 0);
});

test('virtual scrolling bounds the DOM and keyboard selection reveals a distant item', async () => {
    const f = fixture(); const files = Array.from({length:10000},(_,i)=>({id:i+1,rel_path:`${i}.png`,file_type:'IMAGE',file_size:1,tags:[]}));
    f.h.setFiles(files);
    const list = f.get('media-list');
    const cards = () => list.children[0].children;
    assert.ok(cards().length < 25);
    const firstCard = cards()[2];
    list.scrollTop = 141; list.listeners.scroll(); f.flush();
    assert.ok(cards().includes(firstCard), 'overlapping cards retain their DOM nodes');
    list.scrollTop = 5000 * 141; list.listeners.scroll(); f.flush();
    assert.ok(cards().length < 25);
    assert.ok(cards().some(card=>card.dataset.id === 5001));
    assert.ok(!cards().includes(firstCard));
    await f.h.selectMedia(files[9900]);
    assert.ok(cards().some(card=>card.dataset.id === 9901));
    assert.ok(cards().length < 25);
    list.listeners.wheel(); list.scrollTop = 0; list.listeners.scroll(); f.flush();
    assert.equal(cards()[0].dataset.id, 1);
});

test('virtual list measures variable heights and preserves its anchor on updates', () => {
    const f = fixture(); const files = Array.from({length:100},(_,i)=>({id:i+1,rel_path:`${i}.png`,file_type:'IMAGE',tags:[]}));
    f.h.setFiles(files);
    const virtual = f.h.mediaVirtualList; const list = f.get('media-list');
    virtual.nodes.get(0).measuredHeight = 250; virtual.render();
    assert.equal(virtual.index.offset(1), 255);
    list.scrollTop = virtual.index.offset(50) + 12; virtual.render();
    const before = virtual.anchor();
    f.h.setFiles([files[99], ...files.slice(0,99)]);
    assert.equal(virtual.anchor().id, before.id);
    assert.equal(virtual.anchor().inset, before.inset);
    list.clientWidth = 300; virtual.render();
    assert.equal(virtual.anchor().id, before.id);
    f.h.setFiles(files.slice(0,3));
    assert.ok(list.scrollTop < 500);
    assert.ok(virtual.nodes.size <= 3);
});

test('search resets scroll, refreshes retain filters, and folder reset clears the virtual list', () => {
    const f = fixture(); const files = Array.from({length:100},(_,i)=>({id:i+1,rel_path:`${i}.png`,file_type:'IMAGE',tags:[]}));
    f.h.setFiles(files); const list = f.get('media-list');
    list.scrollTop = 50 * 141; list.listeners.scroll(); f.flush();
    f.get('search-input').value = '99.png';
    f.h.applySearchAndRender({resetScroll:true});
    assert.equal(list.scrollTop, 0);
    assert.equal(f.h.mediaVirtualList.nodes.size, 1);
    f.h.applySearchAndRender();
    assert.equal(f.h.mediaVirtualList.items[0].id, 100);
    f.get('search-input').value = 'missing'; f.h.applySearchAndRender({resetScroll:true});
    assert.equal(f.h.mediaVirtualList.nodes.size, 0);
    list.listeners.scroll();
    f.h.resetLibraryView(); f.flush();
    assert.equal(f.h.mediaVirtualList.items.length, 0);
    assert.equal(f.h.mediaVirtualList.heights.size, 0);
});

test('focused tag input survives scrolling and refresh until focus leaves', () => {
    const f = fixture(); const files = Array.from({length:500},(_,i)=>({id:i+1,rel_path:`${i}.png`,file_type:'IMAGE',tags:[]}));
    f.h.setFiles(files); const virtual = f.h.mediaVirtualList;
    const card = virtual.nodes.get(0);
    const input = card.children[1].children[1].children[0].children[0];
    input.value = '작성 중'; input.focus();
    f.get('media-list').scrollTop = 300 * 141; virtual.render();
    assert.equal(virtual.nodes.get(0), card);
    assert.equal(input.value, '작성 중');
    f.h.applySearchAndRender();
    assert.equal(virtual.nodes.get(0), card);
    assert.ok(virtual.nodes.size < 25);
    input.blur(); virtual.render();
    assert.equal(virtual.nodes.has(0), false);
});

test('archive metadata decoding is bounded and navigation cancels queued jobs', async () => {
    const f = fixture(); f.h.setCurrent({id:1,rel_path:'pages.zip',file_type:'ARCHIVE'});
    const pending = Array.from({length:10},(_,i)=>f.h.getImageMeta('pages.zip',`${i}.png`));
    assert.equal(f.images.length,2); assert.equal(f.h.stats().active,2);
    f.images[0].onload(); assert.equal(f.images.length,3); assert.equal(f.h.stats().active,2);
    f.h.invalidateRequests(); f.h.clearImageJobs();
    await Promise.all(pending);
    assert.equal(f.h.stats().active,0); assert.equal(f.h.stats().pending,0); assert.equal(f.h.stats().cache,0);
});

test('incomplete scan exposes affected paths and preservation notice', () => {
    const f=fixture(); f.h.renderScanStatus({job_id:"test-job",running:false,report:{errors:['folder/blocked: access denied']}});
    assert.match(f.get('scan-status').textContent,/기존 기록을 보존/);
    assert.match(f.get('scan-status').textContent,/folder\/blocked/);
});

for (const completion of ['response', 'body', 'error', 'abort']) test('rescan ignores old-library ' + completion + ' after switching folders', async () => {
    const f = fixture('en');
    let finish;
    f.respond(async () => {
        if (completion === 'body') return {ok:true, json:() => new Promise(resolve => { finish = resolve; })};
        return new Promise((resolve, reject) => {
            finish = completion === 'error' ? () => reject(new TypeError('Old folder unavailable')) :
                completion === 'abort' ? () => reject(Object.assign(new Error('Cancelled'), {name:'AbortError'})) : resolve;
        });
    });
    const work = f.get('btn-rescan-folder').click();
    await new Promise(setImmediate);
    f.h.resetLibraryView(); f.h.setSession(2);
    f.h.renderScanStatus({running:true, scanned_count:42});
    const status = f.get('scan-status').textContent;
    finish(completion === 'body' ? {running:true, scanned_count:1} : new Response('{"running":true,"scanned_count":1}'));
    await work;
    assert.equal(f.get('scan-status').textContent, status);
    assert.equal(f.get('scan-status').title, status);
    assert.equal(f.get('btn-rescan-folder').disabled, true);
    assert.equal(f.get('rescan-icon').classList.contains('spin-animation'), true);
    assert.equal(f.calls.filter(([url]) => url.startsWith('/api/scan')).length, 0);
});

for (const failed of [false, true]) test('rescan still displays current-library ' + (failed ? 'errors' : 'completion'), async () => {
    const f = fixture('en');
    f.respond(async () => {
        if (failed) throw new TypeError('Current folder unavailable');
        return new Response('{"running":false,"report":{"scanned_count":42,"new_or_updated":3}}');
    });
    await f.get('btn-rescan-folder').click();
    assert.match(f.get('scan-status').textContent, failed ? /Current folder unavailable/ : /42.*3/);
    assert.equal(f.get('btn-rescan-folder').disabled, false);
    assert.equal(f.get('rescan-icon').classList.contains('spin-animation'), false);
});

test('shortcut help is generated from registry and blocks background playback shortcuts', () => {
    const f=fixture(); f.h.setCurrent({id:1,file_type:'VIDEO'});
    f.get('main-video').currentTime=20; f.get('main-video').duration=100;
    f.h.handleShortcut(keyEvent('F1'));
    assert.equal(f.get('shortcut-dialog').open,true);
    const displayed=f.get('shortcut-list').children.reduce((sum,group)=>sum+group.children.length-1,0);
    assert.equal(displayed,f.h.shortcuts.length);
    f.h.handleShortcut(keyEvent('ArrowRight'));
    assert.equal(f.get('main-video').currentTime,20);
    f.h.handleShortcut(keyEvent('Escape'));
    assert.equal(f.get('shortcut-dialog').open,false);
    f.h.handleShortcut(keyEvent('ArrowRight'));
    assert.equal(f.get('main-video').currentTime,25);
});

test('shortcuts ignore editing, IME, unintended modifiers and repeated save operations', () => {
    const f=fixture(); f.h.setCurrent({id:1,file_type:'VIDEO'});
    const video=f.get('main-video'); video.currentTime=20; video.duration=100;
    f.h.handleShortcut(keyEvent('ArrowRight',{target:{closest:()=>({})}}));
    f.h.handleShortcut(keyEvent('ArrowRight',{isComposing:true}));
    f.h.handleShortcut(keyEvent('ArrowRight',{ctrlKey:true}));
    assert.equal(video.currentTime,20);
    f.h.handleShortcut(keyEvent('KeyS',{repeat:true}));
    assert.equal(f.calls.length,0);
    f.h.handleShortcut(keyEvent('KeyF',{ctrlKey:true,target:{closest:()=>({})}}));
    assert.equal(f.get('search-input').focused,true);
});

test('video speed shortcuts preserve option values and stop at speed limits', () => {
    const f=fixture(); f.h.setCurrent({id:1,file_type:'VIDEO'});
    const speed=f.get('speed-select'); speed.options=[{value:'0.5'},{value:'1.0'},{value:'2.0'}]; speed.value='0.5';
    f.h.handleShortcut(keyEvent('BracketRight'));
    assert.equal(speed.value,'1.0'); assert.equal(f.get('main-video').playbackRate,1);
    f.h.handleShortcut(keyEvent('BracketRight')); f.h.handleShortcut(keyEvent('BracketRight'));
    assert.equal(speed.value,'2.0');
});

test('archive Home/End shortcuts and video-only keys respect the viewer context', () => {
    const f=fixture(); f.h.setCurrent({id:1,file_type:'ARCHIVE',rel_path:'pages.zip'});
    f.h.setArchive(['first.png','last.png'],0);
    f.h.handleShortcut(keyEvent('End'));
    assert.match(f.get('viewer-image').src,/entry=last.png/);
    f.h.handleShortcut(keyEvent('Home'));
    assert.match(f.get('viewer-image').src,/entry=first.png/);
    const mute=keyEvent('KeyM'); f.h.handleShortcut(mute); assert.equal(mute.prevented,undefined);
});

test('video bookmark saves timestamp and clicking thumbnail seeks to that time', async () => {
    const f = fixture(); f.h.setCurrent({id:7, file_type:'VIDEO', rel_path:'clip.mp4'});
    Object.assign(f.get('main-video'), {currentTime:42.5, duration:120, readyState:2, videoWidth:1920, videoHeight:1080});
    f.respond(async (_url, options) => new Response(options.method === 'POST' ? '{}' : JSON.stringify([{id:3,position:42.5,thumbnail_url:'/api/files/7/bookmarks/3/thumbnail'}])));
    await f.h.saveBookmark();
    const saved = JSON.parse(f.calls[0][1].body);
    assert.equal(saved.position,42.5); assert.ok(saved.thumbnail_data);
    const card = f.get('video-bookmarks').children[0];
    assert.match(card.children[0].children[0].src,/library=1/);
    f.get('main-video').currentTime=0; card.children[0].onclick();
    assert.equal(f.get('main-video').currentTime,42.5);
    await card.children[1].onclick();
    assert.equal(f.calls[2][1].method,'DELETE');
});

test('archive saves zero-based page while displaying one-based label', async () => {
    const f=fixture(); f.h.setCurrent({id:2,file_type:'ARCHIVE',rel_path:'pages.zip'});
    f.h.setArchive(['a.png','b.png','c.png'],2);
    f.respond(async (_url, options) => new Response(options.method === 'POST' ? '{}' : JSON.stringify([{id:1,position:2,thumbnail_url:'/thumb'}])));
    await f.h.saveBookmark();
    assert.equal(JSON.parse(f.calls[0][1].body).position,2);
    assert.equal(f.get('image-bookmarks').children[0].children[0].children[1].textContent,'3페이지');
    f.h.setArchive(['a.png','b.png','c.png'],0);
    f.get('image-bookmarks').children[0].children[0].onclick();
    assert.match(f.get('viewer-image').src,/entry=c.png/);
});

test('archive capture finishing after folder switch cannot save into new library', async () => {
    const f=fixture(); f.h.setCurrent({id:2,file_type:'ARCHIVE',rel_path:'pages.zip'});
    f.h.setArchive(['a.png'],0);
    let decoded; f.get('viewer-image').decode=()=>new Promise(resolve=>decoded=resolve);
    const pending=f.h.saveBookmark();
    f.h.invalidateRequests(); f.h.resetBookmarks(); f.h.setSession(2); decoded(); await pending;
    assert.equal(f.calls.length,0);
});

test('saved locations from a previously selected media cannot repaint the current media', async () => {
    const f=fixture(); f.h.setCurrent({id:1,file_type:'VIDEO',rel_path:'a.mp4'});
    let respond;
    f.respond(()=>new Promise(resolve=>respond=resolve));
    const pending=f.h.loadBookmarks();
    await new Promise(setImmediate);
    f.h.setCurrent({id:2,file_type:'VIDEO',rel_path:'b.mp4'}); f.h.resetBookmarks();
    respond(new Response(JSON.stringify([{id:1,position:10,thumbnail_url:'/old'}])));
    await pending;
    assert.equal(f.get('video-bookmarks').children.length,0);
});

test('thumbnail capture is discarded when the selected media changes in the same library', async () => {
    const f=fixture();
    f.h.setCurrent({id:1,file_type:'IMAGE',rel_path:'a.png'});
    Object.assign(f.get('viewer-image'), {naturalWidth:100,naturalHeight:100});
    f.h.captureImageThumbnail(1);
    f.h.bumpSelection(); f.h.setCurrent({id:2,file_type:'IMAGE',rel_path:'b.png'});
    await f.get('thumbnail-canvas').blobCallback(new Blob(['fixture']));
    assert.equal(f.calls.length,0);
});

test('video metadata duration is scoped to the selected media', async () => {
    const f=fixture();
    await f.h.selectMedia({id:1,file_type:'VIDEO',rel_path:'a.mp4'});
    Object.assign(f.get('main-video'), {videoWidth:1920,videoHeight:1080,duration:12});
    f.h.bumpSelection(); f.h.setCurrent({id:2,file_type:'VIDEO',rel_path:'b.mp4'});
    f.get('main-video').listeners.loadedmetadata?.();
    assert.equal(f.calls.some(call => String(call[0]).includes('/api/duration/')), false);
});

test('extreme aspect ratio thumbnail captures stay within canvas limits', () => {
    const f=fixture();
    f.h.setCurrent({id:1,file_type:'IMAGE',rel_path:'poster.png'});
    Object.assign(f.get('viewer-image'), {naturalWidth:1,naturalHeight:100000});
    f.h.captureImageThumbnail(1);
    const canvas = f.get('thumbnail-canvas');
    assert.ok(canvas.width <= 640);
    assert.ok(canvas.height <= 640);
    f.h.setCurrent({id:2,file_type:'VIDEO',rel_path:'tall.mp4'});
    Object.assign(f.get('main-video'), {videoWidth:1,videoHeight:100000,currentTime:0});
    f.h.captureFrameAndUpload(2);
    assert.ok(canvas.width <= 640);
    assert.ok(canvas.height <= 640);
});


test('tar.zst selection loads the shared archive viewer', async () => {
    const f = fixture();
    f.respond(async url => new Response(String(url).includes('/api/archive/list/')
        ? '["nested/page.png"]' : '[]', {status:200}));
    await f.h.selectMedia({id:1, rel_path:'pages.tar.zst', file_type:'ARCHIVE', file_size:100, tags:[]});
    assert.ok(f.calls.some(([url]) => String(url).includes('/api/archive/list/pages.tar.zst')));
    assert.ok(f.images.some(image => String(image.src).includes('/api/archive/file/pages.tar.zst')));
});


function historyRow(id, name = `deleted-${id}.png`) {
    return {id, original_rel_path:name, file_type:'IMAGE', file_size:100, score:1000, tags_info:'tag', deleted_time_str:'2026-09-26'};
}

test('history row and clear buttons delete only logs and refresh the count', async () => {
    const f = fixture(); let logs = [historyRow(1, '<img onerror=alert(1)>.png'), historyRow(2)];
    f.respond(async (url, options) => {
        const pathname = new URL(url, 'http://localhost').pathname;
        if (options.method === 'DELETE') {
            logs = pathname.endsWith('/1') ? logs.filter(log=>log.id !== 1) : [];
            return new Response('', {status:200});
        }
        return new Response(JSON.stringify(logs), {status:200});
    });
    await f.h.loadTrashLogs();
    const tbody = f.get('trash-logs-tbody');
    assert.equal(tbody.children[0].children[1].textContent, '<img onerror=alert(1)>.png');
    assert.equal(tbody.children[0].children[1].innerHTML, '');
    await tbody.children[0].children[7].children[0].onclick();
    assert.equal(f.get('logs-count').textContent, 1);
    assert.equal(tbody.children[0].children[0].textContent, 2);
    await f.get('btn-clear-trash-logs').click();
    assert.equal(f.get('logs-count').textContent, 0);
    assert.equal(f.get('btn-clear-trash-logs').disabled, true);
    assert.match(tbody.innerHTML, /colspan="8"/);
    const deletes = f.calls.filter(([, options])=>options.method === 'DELETE');
    assert.deepEqual(deletes.map(([url])=>new URL(url,'http://localhost').pathname), ['/api/trash/logs/1','/api/trash/logs']);
    assert.equal(f.confirmations.length, 2);
});

test('history deletion cancellation and server failure preserve displayed rows', async () => {
    const f = fixture();
    f.respond(async (url, options) => options.method === 'DELETE'
        ? new Response('database unavailable', {status:500})
        : new Response(JSON.stringify([historyRow(1)]), {status:200}));
    await f.h.loadTrashLogs();
    f.confirm(false);
    await f.get('btn-clear-trash-logs').click();
    assert.equal(f.calls.filter(([, options])=>options.method === 'DELETE').length, 0);
    f.confirm(true);
    await f.get('btn-clear-trash-logs').click();
    assert.equal(f.get('logs-count').textContent, 1);
    assert.equal(f.get('trash-logs-tbody').children.length, 1);
    assert.equal(f.get('btn-clear-trash-logs').disabled, false);
    assert.match(f.get('trash-logs-status').textContent, /database unavailable/);
});

test('pending history deletion cannot run twice or repaint a switched library', async () => {
    const f = fixture(); let finishDelete;
    f.respond(async (url, options) => options.method === 'DELETE'
        ? new Promise(resolve=>finishDelete=resolve)
        : new Response(JSON.stringify([historyRow(1)]), {status:200}));
    await f.h.loadTrashLogs();
    const oldButton = f.get('trash-logs-tbody').children[0].children[7].children[0];
    const pending = oldButton.onclick();
    await f.get('btn-clear-trash-logs').click();
    assert.equal(f.calls.filter(([, options])=>options.method === 'DELETE').length, 1);
    f.h.resetLibraryView(); f.h.setSession(2);
    f.respond(async () => new Response(JSON.stringify([historyRow(9)]), {status:200}));
    await f.h.loadTrashLogs();
    finishDelete(new Response('', {status:200})); await pending;
    await oldButton.onclick();
    assert.equal(f.calls.filter(([, options])=>options.method === 'DELETE').length, 1);
    assert.equal(f.get('trash-logs-tbody').children[0].children[0].textContent, 9);
    assert.equal(f.get('btn-clear-trash-logs').disabled, false);
    assert.equal(f.get('trash-logs-status').textContent, '');
});

function seriesResponse(url, archives, series = []) {
    const path = new URL(url, 'http://localhost').pathname;
    return new Response(JSON.stringify(path === '/api/series' ? series : path === '/api/files' ? archives : path === '/api/trash' ? [] : {}));
}

test('video cards open series assignment, preserve episode order and show representative video in arena', async () => {
    const f=fixture();
    const files=[
        {id:1,rel_path:'Season_2.mp4',file_type:'VIDEO',tags:[]},
        {id:2,rel_path:'Season_10.mkv',file_type:'VIDEO',tags:[]},
        {id:3,rel_path:'poster.png',file_type:'IMAGE',tags:[]},
        {id:4,rel_path:'Special.MOV',file_type:'VIDEO',tags:[]},
        {id:5,rel_path:'comic.zip',file_type:'ARCHIVE',tags:[]}
    ];
    f.respond(async url=>seriesResponse(url,files));f.h.setFiles(files);
    const card=f.h.mediaVirtualList.nodes.get(1);
    const buttons=card.children[0].children[1].children[0].children;
    const assign=buttons.find(el=>el.className==='card-series-btn');
    assert.ok(assign,'video card exposes series assignment');
    let clicked;const open=f.h.seriesEditor.open.bind(f.h.seriesEditor);
    f.h.seriesEditor.open=file=>{clicked=file;};assign.onclick({stopPropagation(){}});
    assert.equal(clicked.id,2);f.h.seriesEditor.open=open;
    await open(files[1]);const editor=f.h.seriesEditor;
    assert.equal(f.get('series-name').value,'Season');
    assert.deepEqual([...editor.orderedIds()],[2]);
    assert.ok(editor.files.some(file=>file.id===3), 'individual images can join a series');
    assert.ok(editor.files.some(file=>file.id===5),'archive support remains available');
    editor.selected.set(1,0);editor.representative=1;
    let payload;editor.api.onSaved=async()=>{};
    f.respond(async(url,options)=>{payload=JSON.parse(options.body);return new Response('{}');});
    await editor.save();assert.deepEqual(payload.file_ids,[1,2]);assert.equal(payload.representative_id,1);
    f.respond(async url=>seriesResponse(url,files));await open(files[3]);
    assert.equal(f.get('series-name').value,'Special');
    const els=['thumb','fallback','type','title','score','delta','page'].map(id=>f.get(id));
    f.h.renderArenaItem(...els,{...files[0],series_id:1,series_name:'Season',score:1200});
    assert.equal(f.get('type').textContent,'📚 시리즈 · 대표 영상');
    assert.equal(f.get('title').title,'대표 영상: Season_2.mp4');
});

test('series editor preserves order, representative, safe names and bounds archive picker DOM', async () => {
    const f=fixture();
    const archives=Array.from({length:5000},(_,i)=>({id:i+1,rel_path:`volume${i+1}.zip`,file_type:'ARCHIVE',tags:[]}));
    const saved=[{id:8,name:'<b>series</b>',file_ids:[3,1],representative_id:1}];
    archives[0].series_id=8;archives[2].series_id=8;
    f.respond(async url=>seriesResponse(url,archives,saved));
    await f.h.seriesEditor.open(archives[0]);
    const editor=f.h.seriesEditor;
    assert.equal(f.get('series-name').value,'<b>series</b>');
    assert.equal(f.get('series-select').children[1].textContent,'<b>series</b>');
    assert.deepEqual([...editor.orderedIds()],[3,1]);
    assert.ok(editor.virtual.nodes.size < 30);
    const row=editor.virtual.nodes.get(1);
    const checkbox=row.children[0].children[0];
    checkbox.checked=true;checkbox.listeners.change();
    assert.deepEqual([...editor.orderedIds()],[3,1,2]);
    const second=editor.virtual.nodes.get(1);
    second.children[1].value='1';second.children[1].listeners.input();
    // Input must survive a row refresh even before the deferred change event.
    editor.render();
    second.children[2].click();
    assert.equal(editor.representative,2);
    let payload;
    editor.api.onSaved=async()=>{};
    f.respond(async (url,options)=> {payload=JSON.parse(options.body);return new Response('{}');});
    await editor.save();
    assert.deepEqual(payload,{id:8,name:'<b>series</b>',file_ids:[2,3,1],representative_id:2});
    assert.equal(editor.isOpen(),false);
});

test('series editor blocks shortcuts, duplicate saves and stale folder callbacks', async () => {
    const f=fixture();const archives=[{id:1,rel_path:'1.zip',file_type:'ARCHIVE'}];
    f.respond(async url=>seriesResponse(url,archives));
    await f.h.seriesEditor.open(archives[0]);
    const editor=f.h.seriesEditor;f.get('series-name').value='S';
    const shortcut=keyEvent('KeyF',{ctrlKey:true});f.h.handleShortcut(shortcut);
    assert.equal(f.get('search-input').focused,undefined);
    let resolve;let reloaded=0;editor.api.onSaved=async()=>reloaded++;
    f.respond(()=>new Promise(r=>resolve=r));
    const pending=editor.save();await editor.save();
    assert.equal(f.calls.filter(([,o])=>o.method==='POST').length,1);
    f.h.resetLibraryView();f.h.setSession(2);
    resolve(new Response('{}'));await pending;
    assert.equal(reloaded,0);assert.equal(editor.isOpen(),false);
});

test('series save failure retains edits and delayed loading cannot reopen a closed editor', async () => {
    const f=fixture();const archives=[{id:1,rel_path:'1.zip',file_type:'ARCHIVE'}];
    f.respond(async url=>seriesResponse(url,archives));await f.h.seriesEditor.open(archives[0]);
    f.get('series-name').value='Keep';
    f.respond(async()=>new Response('conflict',{status:409}));await f.h.seriesEditor.save();
    assert.equal(f.get('series-name').value,'Keep');
    assert.equal(f.get('series-status').textContent,'conflict');assert.equal(f.h.seriesEditor.busy,false);
    const pendingResponses=[];
    f.respond(url=>new Promise(resolve=>pendingResponses.push(()=>resolve(seriesResponse(url,archives)))));
    const pending=f.h.seriesEditor.open();f.h.seriesEditor.close();
    pendingResponses.forEach(resolve=>resolve());await pending;
    assert.equal(f.h.seriesEditor.isOpen(),false);assert.equal(f.h.seriesEditor.virtual.items.length,0);
});

async function seriesMutationFixture(operation = 'edit') {
    const f = fixture('en');
    const files = [1,2].map(id => ({id,rel_path:id+'.zip',file_type:'ARCHIVE',tags:[],...(operation === 'create' ? {} : {series_id:8})}));
    const initial = operation === 'create' ? [] : [{id:8,name:'Original',file_ids:[1,2],representative_id:1}];
    f.respond(async url => seriesResponse(url,files,initial));
    const editor = f.h.seriesEditor;
    await editor.open(files[0]);
    let refreshed = 0;
    editor.api.onSaved = async () => { refreshed++; };
    f.get('series-name').value = 'Changed';
    editor.selected = new Map([[2,1],[1,2]]); editor.representative = 2;
    const start = () => operation === 'delete' ? editor.dissolve() : editor.save();
    const writes = () => f.calls.filter(([,o]) => ['POST','DELETE'].includes(o.method)).length;
    const tick = async () => {
        const entry = [...f.timers].find(([,t]) => t.ms > 0 && t.ms <= 15000);
        assert.ok(entry, 'mutation wait has a deadline');
        f.advance(15001); f.timers.delete(entry[0]); entry[1].fn();
        await new Promise(setImmediate);
    };
    return {f,editor,files,initial,start,writes,tick,refreshed:()=>refreshed};
}

for (const operation of ['create','edit','delete']) test('series ' + operation + ' remains exclusive across close and reopen', async () => {
    const {f,editor,files,initial,start,writes,refreshed} = await seriesMutationFixture(operation);
    let finish;
    f.respond(async (url,options) => options.method ? new Promise(resolve => {finish=resolve;}) : seriesResponse(url,files,initial));
    const work = start(); await new Promise(setImmediate);
    const oldVersion = editor.version, reads = f.calls.length;
    editor.close(); assert.equal(editor.isOpen(),false);
    await editor.open(files[1]);
    assert.equal(editor.version,oldVersion); assert.equal(f.calls.length,reads);
    assert.equal(editor.busy,true); assert.equal(f.get('series-fields').disabled,true);
    assert.equal(f.get('series-save').disabled,true); assert.equal(f.get('series-dissolve').disabled,true);
    await editor.save(); await editor.dissolve(); assert.equal(writes(),1);
    finish(new Response('{}')); await work;
    assert.equal(refreshed(),1); assert.equal(editor.mutation,null); assert.equal(editor.isOpen(),false);
    f.respond(async url => seriesResponse(url,files,initial)); await editor.open(files[0]);
    assert.equal(editor.busy,false); assert.equal(f.get('series-save').disabled,false);
});

for (const operation of ['create','edit','delete']) for (const failure of ['lost','stalled']) for (const committed of [false,true])
test(`series ${operation} reconciles ${failure} response without repeating writes, committed=${committed}`, async () => {
    const {f,editor,files,initial,start,writes,tick,refreshed} = await seriesMutationFixture(operation);
    let records=initial, late;
    f.respond(async (url,options) => {
        if (['POST','DELETE'].includes(options.method)) {
            assert.ok(Number(new Headers(options.headers).get('X-Vault-Write-Deadline')) > Date.now());
            if (committed) records=operation === 'delete' ? [] : [{...JSON.parse(options.body),id:8}];
            if (failure === 'lost') throw new TypeError('Lost acknowledgement');
            return new Promise(resolve => {late=resolve;});
        }
        return seriesResponse(url,files,records);
    });
    const work=start(); await new Promise(setImmediate);
    editor.close(); await editor.open(files[1]); await editor.save(); await editor.dissolve();
    assert.equal(writes(),1); assert.equal(editor.busy,true);
    await tick(); await work;
    assert.equal(writes(),1); assert.equal(editor.mutation,null);
    assert.equal(refreshed(),committed?1:0);
    if (!committed) {
        assert.match(f.get('series-status').textContent,/not applied/);
        assert.equal(f.get('series-fields').disabled,false);
        assert.equal(f.get('series-name').value,'Changed');
    }
    if (late) { late(new Response('{}')); await new Promise(setImmediate); assert.equal(refreshed(),committed?1:0); }
});

for (const stage of ['headers','body']) test('series unresolved write stays locked after failed reconciliation ' + stage, async () => {
    const {f,editor,files,start,writes,tick,refreshed}=await seriesMutationFixture();
    let payload, late;
    f.respond(async (url,options) => {
        if (options.method === 'POST') {payload=JSON.parse(options.body);throw new TypeError('Lost');}
        return stage === 'headers' ? new Promise(resolve=>{late=resolve;}) : {ok:true,json:()=>new Promise(resolve=>{late=resolve;})};
    });
    const work=start(); await new Promise(setImmediate); await tick(); await tick(); await work;
    assert.equal(editor.busy,true); assert.equal(f.get('series-retry').hidden,false);
    editor.close(); await editor.open(files[1]); await editor.save(); await editor.dissolve();
    assert.equal(writes(),1); assert.equal(f.get('series-save').disabled,true);
    f.respond(async()=>new Response(JSON.stringify([{...payload,id:8}])));
    await f.get('series-retry').click(); assert.equal(refreshed(),1); assert.equal(editor.mutation,null);
    late(stage === 'headers' ? new Response('[]') : []); await new Promise(setImmediate);
    assert.equal(refreshed(),1); assert.equal(writes(),1);
});

for (const stage of ['writing','waiting','reading']) test('library reset cancels series mutation during ' + stage, async () => {
    const {f,editor,start,tick,refreshed}=await seriesMutationFixture();
    let late;
    f.respond(async (url,options) => {
        if (options.method === 'POST' && stage !== 'writing') throw new TypeError('Lost');
        return new Promise(resolve=>{late=resolve;});
    });
    const work=start(); await new Promise(setImmediate);
    if (stage === 'reading') await tick();
    f.h.resetLibraryView(); f.h.setSession(2); await work;
    assert.equal(editor.mutation,null); assert.equal(editor.busy,false); assert.equal(editor.isOpen(),false);
    if (late) {late(new Response(stage === 'reading' ? '[]' : '{}')); await new Promise(setImmediate);}
    assert.equal(refreshed(),0);
    assert.equal([...f.timers.values()].filter(t=>t.ms>0&&t.ms<=15000).length,0);
});

test('series name search includes every volume and shared rating refresh comes from server', async () => {
    const f=fixture();let archives=[1,2].map(id=>({id,rel_path:`${id}.zip`,file_type:'ARCHIVE',series_id:1,series_name:'Story',series_position:id,series_representative_id:1,rating:1,tags:[]}));
    f.h.setFiles(archives);f.h.setCurrent(archives[0]);
    f.get('search-input').value='story';f.h.applySearchAndRender();
    assert.equal(f.h.mediaVirtualList.items.length,2);
    f.respond(async(url,options)=>{
        if(options.method==='POST') archives=archives.map(f=>({...f,rating:5}));
        return seriesResponse(url,archives);
    });
    await f.h.updateFileRating(2,5);
    assert.ok(f.h.mediaVirtualList.items.every(f=>f.rating===5));
});


test('series outline follows logical boundaries across virtualization, focus and filters', () => {
    const f=fixture();const files=Array.from({length:5000},(_,i)=>({id:i+1,rel_path:`${i}.zip`,file_type:'ARCHIVE',series_id:7,series_name:'Story',tags:[]}));
    files.push({id:5001,rel_path:'solo.png',file_type:'IMAGE',tags:[]});
    f.h.setFiles(files);const virtual=f.h.mediaVirtualList;
    assert.ok(virtual.nodes.get(0).classList.contains('series-start'));
    assert.ok(!virtual.nodes.get(0).classList.contains('series-end'));
    const card=virtual.nodes.get(0);const input=card.children[1].children[1].children[0].children[0];
    input.focus();
    f.get('media-list').scrollTop=2500*141;virtual.render();
    assert.ok(virtual.nodes.size<30);
    const middle=virtual.nodes.get(2500);
    assert.ok(middle.classList.contains('series-member'));
    assert.ok(!middle.classList.contains('series-start'));
    assert.ok(!middle.classList.contains('series-end'));
    f.get('search-input').value='0.zip';f.h.applySearchAndRender();
    assert.equal(virtual.nodes.get(0),card);
    // Filtering to one member redraws both outer caps even on a focused, retained card.
    f.get('search-input').value='4999.zip';input.blur();f.h.applySearchAndRender();
    assert.equal(virtual.items.length,1);
    assert.ok(virtual.nodes.get(0).classList.contains('series-start'));
    assert.ok(virtual.nodes.get(0).classList.contains('series-end'));
    f.get('search-input').value='';f.h.setFiles(files.slice(-2));
    assert.ok(virtual.nodes.get(0).classList.contains('series-end'));
    assert.ok(!virtual.nodes.get(1).classList.contains('series-member'));
});

test('arena presents series name and representative label while solo files retain their title', () => {
    const f=fixture();const ids=['thumb','fallback','type','title','score','delta','page'];const els=ids.map(id=>f.get(id));
    f.h.renderArenaItem(...els,{id:2,file_type:'ARCHIVE',rel_path:'volume2.zip',series_id:1,series_name:'Story',score:1250});
    assert.equal(f.get('title').textContent,'Story');
    assert.equal(f.get('title').title,'대표 권: volume2.zip');
    assert.equal(f.get('type').textContent,'📚 시리즈 · 대표 권');
    assert.equal(f.get('score').title,'시리즈 공통 점수');
    f.h.renderArenaItem(...els,{id:3,file_type:'IMAGE',rel_path:'solo.png',score:1000});
    assert.equal(f.get('title').textContent,'solo.png');
    assert.equal(f.get('type').textContent,'IMAGE');
});


test('opening series assignment reveals a distant selected file and prefills its filename prefix', async () => {
    const f=fixture();
    const archives=Array.from({length:5000},(_,i)=>({id:i+1,rel_path:`folder_with_underscores/작품_${i+1}_권.tar.zst`,file_type:'ARCHIVE'}));
    f.respond(async url=>seriesResponse(url,archives));
    await f.h.seriesEditor.open(archives[4500]);
    const editor=f.h.seriesEditor; const list=f.get('series-files');
    editor.virtual.render();
    assert.equal(f.get('series-name').value,'작품');
    assert.deepEqual([...editor.orderedIds()],[4501]);
    assert.equal(editor.representative,4501);
    assert.ok(list.scrollTop>0);
    const index=editor.virtual.indexById.get(4501);
    const top=editor.virtual.index.offset(index);
    assert.ok(top>=list.scrollTop && top<list.scrollTop+list.clientHeight);
    assert.ok(editor.virtual.nodes.has(index));
    assert.ok(editor.virtual.nodes.size<35);
});

test('opening an existing series reveals the clicked volume and preserves its saved name', async () => {
    const f=fixture();
    const archives=Array.from({length:1000},(_,i)=>({id:i+1,rel_path:`Story_${i+1}.zip`,file_type:'ARCHIVE',series_id:9}));
    const series=[{id:9,name:'직접 정한 시리즈명',representative_id:1,file_ids:archives.map(f=>f.id)}];
    f.respond(async url=>seriesResponse(url,archives,series));
    await f.h.seriesEditor.open(archives[900]);
    const editor=f.h.seriesEditor;
    assert.equal(f.get('series-name').value,'직접 정한 시리즈명');
    assert.equal(editor.representative,1);
    assert.ok(editor.virtual.nodes.has(editor.virtual.indexById.get(901)));
    assert.ok(f.get('series-files').scrollTop>0);
    await editor.open();
    assert.equal(f.get('series-name').value,'');
    assert.equal(f.get('series-files').scrollTop,0);
});

test('new series defaults use the basename and omit the archive extension without underscores', async () => {
    const f=fixture();
    const files=[
        {id:1,rel_path:'parent_name/여행_01.zip',file_type:'ARCHIVE'},
        {id:2,rel_path:'parent_name/완결.TAR.ZST',file_type:'ARCHIVE'},
        {id:3,rel_path:'parent_name\\작품_2_완결.zip',file_type:'ARCHIVE'}
    ];
    f.respond(async url=>seriesResponse(url,files));
    for (const [index,expected] of [[0,'여행'],[1,'완결'],[2,'작품']]) {
        await f.h.seriesEditor.open(files[index]);
        assert.equal(f.get('series-name').value,expected);
    }
});


const watchRequests = f => f.calls.filter(([url])=>/\/watch\?/.test(url));
async function watchVideoFixture() {
    const f=fixture();
    f.respond(async()=>new Response('[]'));
    await f.h.selectMedia({id:1,rel_path:'movie.mp4',file_type:'VIDEO',tags:[]});
    const video=f.get('main-video');
    Object.assign(video,{currentTime:0,paused:false,seeking:false,playbackRate:1,readyState:4});
    video.listeners.playing();
    return {f,video,step:(seconds,mediaSeconds=seconds)=>{
        f.advance(seconds*1000);video.currentTime+=mediaSeconds;video.listeners.timeupdate();
    }};
}

test('watch count waits for ten actual seconds, excludes pause and seeking, and counts once', async () => {
    const {f,video,step}=await watchVideoFixture();
    assert.equal(watchRequests(f).length,0);
    step(6);assert.equal(watchRequests(f).length,0);
    video.paused=true;video.listeners.pause();f.advance(60000);
    video.paused=false;video.listeners.playing();
    step(3);assert.equal(watchRequests(f).length,0);
    video.seeking=true;video.listeners.seeking();video.currentTime=200;f.advance(5000);
    video.listeners.timeupdate();video.seeking=false;video.listeners.seeked();
    assert.equal(watchRequests(f).length,0);
    step(1);assert.equal(watchRequests(f).length,1);
    step(30);video.paused=true;video.listeners.pause();video.listeners.ended();
    assert.equal(watchRequests(f).length,1);
});

test('buffering, faster playback and media/folder switches cannot inflate playback counts', async () => {
    const {f,video,step}=await watchVideoFixture();
    video.playbackRate=2;video.listeners.ratechange();step(5,10);
    assert.equal(watchRequests(f).length,0);
    video.listeners.waiting();f.advance(30000);video.listeners.timeupdate();
    video.listeners.playing();step(4,8);assert.equal(watchRequests(f).length,0);
    await f.h.selectMedia({id:2,rel_path:'next.mp4',file_type:'VIDEO',tags:[]});
    video.currentTime=0;video.playbackRate=1;video.listeners.playing();step(1);
    assert.equal(watchRequests(f).length,0);
    f.h.resetLibraryView();f.h.setSession(2);step(30);
    assert.equal(watchRequests(f).length,0);
});

test('mixed portrait and landscape pages return to the preceding spread after next and after seeking', async () => {
    const f=fixture(),file={id:1,rel_path:'mixed.zip',file_type:'ARCHIVE'};
    f.h.setCurrent(file);f.h.setArchive(['1.png','2.png','3.png','4.png'],0);
    [true,false,true,true].forEach((portrait,i)=>f.h.cacheImage('mixed.zip',`${i+1}.png`,{isPortrait:portrait}));
    await f.h.showArchiveImage('mixed.zip',1);
    await f.get('btn-img-next').click();assert.match(f.get('img-page-counter').textContent,/3-4/);
    await f.get('btn-img-prev').click();assert.match(f.get('viewer-image').src,/entry=2.png/);
    await f.get('btn-img-next').click();
    await f.get('btn-img-prev').click();assert.match(f.get('viewer-image').src,/entry=2.png/);
    await f.h.showArchiveImage('mixed.zip',2); // seek discards navigation history
    await f.get('btn-img-prev').click();assert.match(f.get('viewer-image').src,/entry=2.png/);
    await f.get('btn-img-prev').click();assert.match(f.get('viewer-image').src,/entry=1.png/);
});

test('a pending previous-page probe cannot repaint a later seek', async () => {
    const f=fixture(),file={id:1,rel_path:'seek.zip',file_type:'ARCHIVE'};
    f.h.setCurrent(file);f.h.setArchive(['1.png','2.png','3.png','4.png'],3);
    const pending=f.get('btn-img-prev').click();
    f.h.cacheImage('seek.zip','1.png',{isPortrait:false});
    await f.h.showArchiveImage('seek.zip',0);
    f.h.clearImageJobs();await pending;
    assert.match(f.get('viewer-image').src,/entry=1.png/);
});

test('late arena archive lists cannot replace a newer pair or repaint after exit', async () => {
    for(const action of ['next','exit']){
        const f=fixture();f.h.setArenaMode();
        const pair=name=>[{id:name==='old'?1:3,rel_path:`${name}.zip`,file_type:'ARCHIVE',thumbnail_data:'/api/thumbnail/1'},
            {id:2,rel_path:'other.png',file_type:'IMAGE'}];
        f.respond(async()=>new Response(JSON.stringify(pair('old'))));await f.h.loadArenaPair();
        let finish;f.respond(()=>new Promise(resolve=>finish=resolve));
        const pending=f.h.handleArenaArchiveWheel({deltaY:1,preventDefault(){},stopPropagation(){}},'A');
        if(action==='next'){
            f.respond(async()=>new Response(JSON.stringify(pair('new'))));await f.h.loadArenaPair();
        }else{
            f.respond(async()=>new Response('[]'));f.h.exitArena();
        }
        const src=f.get('arena-thumb-a').src;
        finish(new Response('["1.png","2.png"]'));await pending;
        assert.equal(f.get('arena-thumb-a').src,src);
        assert.equal(f.get('arena-page-a').textContent,'');
        if(action==='next')assert.equal(f.get('arena-title-a').textContent,'new.zip');
    }
});

test('stalled gallery loading allows hidden entry and clear, and late results are ignored', async () => {
    for(const action of ['entry','clear','timeout']){
        const f=fixture(),p=f.h.privacy;p.setCovered(true);let finish;
        f.respond(()=>new Promise(resolve=>finish=resolve));const pending=p.loadGallery();
        assert.equal(p.clear.disabled,false);assert.equal(p.choose.disabled,false);
        if(action==='entry'){
            f.respond(async()=>new Response('{"folders":[]}'));
            for(let i=0;i<5;i++)await f.get('privacy-library-title').click();
            assert.equal(p.isOpen(),true);assert.equal(p.galleryBusy,false);
        }else if(action==='clear'){
            f.respond(async()=>new Response('{"images":[],"status":"empty"}'));
            await p.loadGallery('clear');assert.equal(f.calls.at(-1)[1].method,'DELETE');
        }else{
            const watchdog=[...f.timers.values()].find(t=>t.ms===15000);assert.ok(watchdog);
            watchdog.fn();await pending;assert.equal(p.galleryBusy,false);
            assert.match(p.galleryMessage.textContent,/읽지 못했습니다/);
        }
        finish(new Response('{"images":[{"id":"0","name":"late.png","src":"/api/gallery/image/1/0"}],"status":"ready"}'));
        await pending;
        assert.equal(p.samples.length,0);assert.equal(p.galleryBusy,false);
        assert.equal(p.choose.disabled,false);assert.equal(p.clear.disabled,false);
    }
});

async function retryWatchTimer(f) {
    const timer = [...f.timers].find(([, timer]) => timer.ms === 2000);
    assert.ok(timer, 'a rejected watch record is queued');
    f.timers.delete(timer[0]); timer[1].fn();
    await new Promise(setImmediate);
}

test('a watch rejected during quick inspection is retried once after pausing and selecting another media', async () => {
    const {f,video,step}=await watchVideoFixture();
    let busy=true, saved=0;
    f.respond(async url=>{
        if (/\/watch\?/.test(url)) {
            if (busy) return new Response('Inspection in progress', {status:409});
            saved++;
        }
        return new Response('[]');
    });
    step(10); await new Promise(setImmediate);
    step(30); assert.equal(watchRequests(f).length,1, 'timeupdate events do not flood rejected writes');
    await retryWatchTimer(f); assert.equal(watchRequests(f).length,2); assert.equal(saved,0);
    video.paused=true; video.listeners.pause();
    await f.h.selectMedia({id:2,rel_path:'next.mp4',file_type:'VIDEO',tags:[]});
    busy=false; await retryWatchTimer(f);
    assert.equal(saved,1); assert.equal(watchRequests(f).length,3);
    assert.ok(watchRequests(f).every(([url])=>url.includes('/files/1/watch?')));
    assert.equal([...f.timers.values()].filter(timer=>timer.ms===2000).length,0);
});

test('changing libraries removes queued watch writes and late conflicts cannot recreate them', async () => {
    const {f,step}=await watchVideoFixture();
    f.respond(async()=>new Response('Inspection in progress', {status:409}));
    step(10); await new Promise(setImmediate);
    assert.equal([...f.timers.values()].filter(timer=>timer.ms===2000).length,1);
    f.h.invalidateRequests(); f.h.setSession(2);
    assert.equal([...f.timers.values()].filter(timer=>timer.ms===2000).length,0);
    const other=await watchVideoFixture(); let resolve;
    other.f.respond(()=>new Promise(r=>resolve=r));
    other.step(10); other.f.h.resetLibraryView(); other.f.h.setSession(2);
    resolve(new Response('Inspection in progress', {status:409})); await new Promise(setImmediate);
    assert.equal([...other.f.timers.values()].filter(timer=>timer.ms===2000).length,0);
    assert.equal(watchRequests(other.f).length,1);
});

test('an ambiguous watch response is not automatically retried and cannot double count', async () => {
    const {f,step}=await watchVideoFixture();
    f.respond(async()=>{throw new TypeError('Response lost after commit');});
    step(10); await new Promise(setImmediate); step(30);
    assert.equal(watchRequests(f).length,1);
    assert.equal([...f.timers.values()].filter(timer=>timer.ms===2000).length,0);
});

for (const wholeFolder of [false,true]) test(`successful ${wholeFolder?'folder':'individual'} watch reset discards earlier queued views`, async () => {
    const {f,step}=await watchVideoFixture();
    f.respond(async url=>new Response(/\/watch\?/.test(url)?'Inspection in progress':'[]',
        {status:/\/watch\?/.test(url)?409:200}));
    step(10); await new Promise(setImmediate);
    if (wholeFolder) await f.get('btn-reset-all-watch').click(); else await f.h.resetWatchCount(1);
    assert.equal(f.calls.filter(([url])=>url.includes('/reset_watch?')).length,1);
    assert.equal([...f.timers.values()].filter(timer=>timer.ms===2000).length,0);
    step(30); assert.equal(watchRequests(f).length,1);
});

test('failed watch reset preserves rejected views for a later retry', async () => {
    const {f,step}=await watchVideoFixture();
    f.respond(async()=>new Response('Inspection in progress', {status:409}));
    step(10); await new Promise(setImmediate); await f.h.resetWatchCount(1);
    f.respond(async()=>new Response('{}'));
    await retryWatchTimer(f);
    assert.equal(watchRequests(f).length,2);
    assert.equal([...f.timers.values()].filter(timer=>timer.ms===2000).length,0);
});

test('watch reset waits for an already submitted retry before clearing the database count', async () => {
    const {f,step}=await watchVideoFixture();
    f.respond(async()=>new Response('Inspection in progress', {status:409}));
    step(10); await new Promise(setImmediate);
    let finishWrite, count=0;
    f.respond(async url=>{
        if (/\/watch\?/.test(url)) return new Promise(resolve=>{finishWrite=()=>{count++;resolve(new Response('{}'));};});
        if (url.includes('/reset_watch?')) count=0;
        return new Response('[]');
    });
    const timer=[...f.timers].find(([,timer])=>timer.ms===2000);
    f.timers.delete(timer[0]);timer[1].fn();
    const reset=f.h.resetWatchCount(1);await new Promise(setImmediate);
    assert.equal(f.calls.filter(([url])=>url.includes('/reset_watch?')).length,0);
    finishWrite();await reset;
    assert.equal(count,0);
    assert.equal(f.calls.filter(([url])=>url.includes('/reset_watch?')).length,1);
    assert.equal([...f.timers.values()].filter(timer=>timer.ms===2000).length,0);
});

async function watchArchiveFixture(portrait=false) {
    const f=fixture();f.respond(async()=>new Response('[]'));
    await f.h.selectMedia({id:1,rel_path:'book.zip',file_type:'ARCHIVE',tags:[]});
    const entries=Array.from({length:10},(_,i)=>`${i}.png`);
    f.h.setArchive(entries,0);
    for (const entry of entries) f.h.cacheImage('book.zip',entry,{width:100,height:portrait?200:100,isPortrait:portrait});
    return f;
}

test('archive watch count requires five distinct displayed pages, not reaching page five', async () => {
    const f=await watchArchiveFixture();
    assert.equal(watchRequests(f).length,0);
    await f.h.showArchiveImage('book.zip',8);
    await f.h.showArchiveImage('book.zip',8);
    for (const i of [0,1,2]) await f.h.showArchiveImage('book.zip',i);
    assert.equal(watchRequests(f).length,0);
    await f.h.showArchiveImage('book.zip',3);
    assert.equal(watchRequests(f).length,1);
    for (const i of [3,4,5,6]) await f.h.showArchiveImage('book.zip',i);
    assert.equal(watchRequests(f).length,1);
});

test('a five-page watch rejected by inspection is saved without requiring another page turn', async () => {
    const f=await watchArchiveFixture(); let busy=true, saved=0;
    f.respond(async url=>{
        if (/\/watch\?/.test(url)) {
            if (busy) return new Response('Inspection in progress', {status:409});
            saved++;
        }
        return new Response('[]');
    });
    for (const i of [0,1,2,3,4]) await f.h.showArchiveImage('book.zip',i);
    await new Promise(setImmediate); assert.equal(saved,0); assert.equal(watchRequests(f).length,1);
    busy=false; await retryWatchTimer(f);
    assert.equal(saved,1); assert.equal(watchRequests(f).length,2);
    await f.h.showArchiveImage('book.zip',5);
    assert.equal(watchRequests(f).length,2);
});

test('two-page spreads count each displayed page while failed and stale decodes do not count', async () => {
    const f=await watchArchiveFixture(true);
    await f.h.showArchiveImage('book.zip',0);
    await f.h.showArchiveImage('book.zip',2);
    assert.equal(watchRequests(f).length,0);
    f.get('viewer-image').decode=()=>Promise.reject(new Error('bad image'));
    f.get('viewer-image-2').decode=()=>Promise.reject(new Error('bad image'));
    await f.h.showArchiveImage('book.zip',4);
    assert.equal(watchRequests(f).length,0);
    let resolve;
    f.get('viewer-image').decode=()=>new Promise(r=>resolve=r);
    await f.h.showArchiveImage('book.zip',6);
    f.h.resetLibraryView();f.h.setSession(2);resolve();await Promise.resolve();
    assert.equal(watchRequests(f).length,0);
    const other=await watchArchiveFixture(true);
    for (const i of [0,2,4]) await other.h.showArchiveImage('book.zip',i);
    assert.equal(watchRequests(other).length,1);
});

test('a superseded page decode does not contribute to the five-page threshold', async () => {
    const f=await watchArchiveFixture();
    for (const i of [0,1,2,3]) await f.h.showArchiveImage('book.zip',i);
    let resolve;f.get('viewer-image').decode=()=>new Promise(r=>resolve=r);
    await f.h.showArchiveImage('book.zip',4);
    f.get('viewer-image').decode=()=>Promise.resolve();
    await f.h.showArchiveImage('book.zip',0);resolve();await Promise.resolve();
    assert.equal(watchRequests(f).length,0);
    await f.h.showArchiveImage('book.zip',5);
    assert.equal(watchRequests(f).length,1);
});


function tagEditor(f,index=0) {
    const card=f.h.mediaVirtualList.nodes.get(index);
    const container=card.children[1].children[1];
    const form=container.children.find(el=>el.className==='tag-input-form');
    return {card,container,form,input:form.children[0]};
}
const tagKey = (key,extra={}) => ({key,code:key===' '?'Space':key,preventDefault(){this.prevented=true;},stopPropagation(){},...extra});
const tagNames = container => container.children.filter(el=>el.className==='tag-badge').map(el=>el.textContent);

test('Space and Enter show saved shared tags immediately without replacing the focused input', async () => {
    const f=fixture();let files=[1,2].map(id=>({id,rel_path:`${id}.zip`,file_type:'ARCHIVE',series_id:1,tags:[]}));
    f.h.setFiles(files);f.h.setCurrent(files[0]);
    const edit=tagEditor(f);edit.input.focus();edit.input.value='여행';
    f.respond(async (url,options)=>{
        if(options.method==='POST') {
            const tag={id:7,name:JSON.parse(options.body).name};
            files=files.map(f=>({...f,tags:[tag]}));return new Response(JSON.stringify(tag));
        }
        return new Response('{}');
    });
    const space=tagKey(' ');const pending=edit.input.listeners.keydown(space);
    await pending;
    assert.equal(f.calls.filter(([url])=>url.startsWith('/api/files?')).length,0);
    assert.equal(space.prevented,true);assert.equal(edit.input.value,'');
    assert.deepEqual(tagNames(edit.container),['#여행']);
    assert.deepEqual(tagNames(tagEditor(f,1).container),['#여행']);
    edit.input.value='다음';
    assert.equal(tagEditor(f).input,edit.input);assert.equal(edit.input.value,'다음');
    f.respond(async (url,options)=>{
        if(options.method==='POST') {
            const tag={id:8,name:JSON.parse(options.body).name};
            files=files.map(f=>({...f,tags:[...f.tags,tag]}));return new Response(JSON.stringify(tag));
        }
        return new Response(JSON.stringify(url.startsWith('/api/files?')?files:{}));
    });
    await edit.input.listeners.keydown(tagKey('Enter'));
    assert.deepEqual(tagNames(edit.container).sort(),['#다음','#여행'].sort());
    assert.equal(tagEditor(f).input,edit.input);assert.equal(edit.input.value,'');
});

test('tag confirmation ignores IME, shortcut modifiers, blank input and duplicate submissions', async () => {
    const f=fixture();const files=[{id:1,rel_path:'1.png',file_type:'IMAGE',tags:[]}];
    f.h.setFiles(files);f.h.setCurrent(files[0]);const edit=tagEditor(f);edit.input.focus();
    edit.input.value='한글';edit.input.listeners.compositionstart();
    await edit.input.listeners.keydown(tagKey('Enter'));await edit.form.onsubmit(tagKey('Enter'));
    edit.input.listeners.compositionend();
    await edit.input.listeners.keydown(tagKey('Enter',{keyCode:229}));
    await edit.input.listeners.keydown(tagKey(' ',{ctrlKey:true}));
    assert.equal(f.calls.length,0);assert.equal(edit.input.value,'한글');
    let finish;f.respond(()=>new Promise(resolve=>finish=resolve));
    const pending=edit.input.listeners.keydown(tagKey('Enter'));
    edit.input.value='한글';await edit.input.listeners.keydown(tagKey(' '));
    await edit.form.onsubmit(tagKey('Enter'));
    assert.equal(f.calls.length,1);
    f.respond(async url=>new Response(JSON.stringify(url.startsWith('/api/files?')?[{...files[0],tags:[{id:1,name:'한글'}]}]:{})));
    finish(new Response(JSON.stringify({id:1,name:'한글'})));await pending;
    edit.input.value='#한글';await edit.input.listeners.keydown(tagKey('Enter'));
    assert.equal(f.calls.filter(([,o])=>o.method==='POST').length,1);
});

test('failed tag saves restore input without phantom tags or overwriting a later draft', async () => {
    const f=fixture();const files=[{id:1,rel_path:'1.png',file_type:'IMAGE',tags:[]}];
    f.h.setFiles(files);f.h.setCurrent(files[0]);const edit=tagEditor(f);edit.input.focus();
    f.respond(async()=>new Response('failed',{status:500}));
    edit.input.value='다시 입력';await edit.input.listeners.keydown(tagKey('Enter'));
    assert.equal(edit.input.value,'다시 입력');assert.deepEqual(tagNames(edit.container),[]);
    let finish;f.respond(()=>new Promise(resolve=>finish=resolve));
    const pending=edit.input.listeners.keydown(tagKey('Enter'));edit.input.value='새 입력';
    finish(new Response('failed',{status:500}));await pending;
    assert.equal(edit.input.value,'새 입력');
});

test('a delayed tag save cannot repaint a different library', async () => {
    const f=fixture();const files=[{id:1,rel_path:'1.png',file_type:'IMAGE',tags:[]}];
    f.h.setFiles(files);f.h.setCurrent(files[0]);const edit=tagEditor(f);edit.input.focus();edit.input.value='old';
    let finish;f.respond(()=>new Promise(resolve=>finish=resolve));
    const pending=edit.input.listeners.keydown(tagKey('Enter'));
    f.h.resetLibraryView();f.h.setSession(2);f.h.setFiles([{...files[0],tags:[]}]);
    finish(new Response(JSON.stringify({id:1,name:'old'})));await pending;
    assert.deepEqual(tagNames(tagEditor(f).container),[]);
    assert.equal(f.calls.length,1);
});


test('archive display sorts numeric filename parts and nested directories naturally', () => {
    const f=fixture();
    const source=['1.png','10.png','11.png','2.png','7.png'];
    const order=f.h.buildArchiveOrder(source);
    assert.deepEqual([...order.entries],['1.png','2.png','7.png','10.png','11.png']);
    assert.deepEqual(source,['1.png','10.png','11.png','2.png','7.png']);
    for(let i=0;i<source.length;i++) assert.equal(order.entries[order.storedToDisplay[i]],source[i]);
    assert.deepEqual([...f.h.buildArchiveOrder(['권10/7.jpg','권2/11.jpg','권2/2.jpg','권2/10.jpg']).entries],
        ['권2/2.jpg','권2/10.jpg','권2/11.jpg','권10/7.jpg']);
    assert.deepEqual([...f.h.buildArchiveOrder(['007.png','7.png','07.png','10.png']).entries],['007.png','7.png','07.png','10.png']);
});

test('natural image order preserves legacy bookmark targets and saves compatible indices', async () => {
    const f=fixture();f.h.setCurrent({id:2,file_type:'ARCHIVE',rel_path:'pages.zip'});
    f.h.setArchive(['1.png','10.png','11.png','2.png','7.png'],2);
    f.respond(async (_url,options)=>new Response(options.method==='POST'?'{}':JSON.stringify([{id:1,position:1,thumbnail_url:'/thumb'}])));
    await f.h.saveBookmark();
    assert.equal(JSON.parse(f.calls[0][1].body).position,4,'display page 3 (7.png) retains stored index 4');
    const jump=f.get('image-bookmarks').children[0].children[0];
    assert.equal(jump.children[1].textContent,'4페이지','legacy index 1 (10.png) moves to display page 4');
    jump.onclick();
    assert.match(f.get('viewer-image').src,/entry=10.png/);
});

test('natural image order preserves thumbnail page offsets for later arena previews', async () => {
    const f=fixture();const file={id:2,file_type:'ARCHIVE',rel_path:'pages.zip',tags:[]};
    f.h.setCurrent(file);f.h.setArchive(['1.png','10.png','11.png','2.png','7.png'],2);
    f.respond(async url=>new Response(JSON.stringify(url.startsWith('/api/files?')?[file]:{})));
    f.h.captureImageThumbnail(2);
    await f.get('thumbnail-canvas').blobCallback(new Blob(['image']));
    const upload=f.calls.find(([url])=>url.startsWith('/api/thumbnail/2'));
    assert.equal(upload[1].body.get('page_offset'),'4');
});


test('database lock clears media and blocks list loads and shortcuts', async () => {
    const f = fixture();
    f.h.setFiles([{id:1,rel_path:'secret.png',file_type:'IMAGE',tags:[]}]);
    f.get('arena-title-a').textContent = 'secret series';
    f.h.adoptLibraryState({generation:2,security:{encrypted:true,locked:true,plaintext_original:false},scan:{job_id:"test-job",running:false}});
    assert.equal(f.h.mediaVirtualList.items.length, 0);
    assert.equal(f.get('arena-title-a').textContent, '');
    assert.equal(f.get('db-security-dialog').open, true);
    assert.equal(f.get('db-confirm-row').hidden, true);
    assert.equal(f.get('db-password').focused, true);
    const count = f.calls.length;
    await f.h.loadMediaFiles();
    f.h.handleShortcut(keyEvent('KeyV'));
    assert.equal(f.calls.length, count);
    assert.equal(f.get('btn-backup-db').disabled, true);
    f.get('db-password').value = 'sensitive password';
    f.get('db-security-close').listeners.click();
    assert.equal(f.get('db-password').value, '');
    assert.equal(f.h.dbSecurity.locked, true);
});

test('encryption form requires confirmation and never sends passwords in URLs', async () => {
    const f = fixture(); f.h.dbSecurity.open();
    f.get('db-encryption-risk-ack').checked=true;
    f.get('db-password').value = 'long password 123';
    f.get('db-password-confirm').value = 'does not match';
    f.get('db-security-form').listeners.submit({preventDefault(){}});
    assert.equal(f.calls.length, 0);
    let resolve;
    f.respond(() => new Promise(r=>resolve=r));
    f.get('db-password-confirm').value = 'long password 123';
    f.get('db-security-form').listeners.submit({preventDefault(){}});
    assert.equal(f.calls.length, 1);
    assert.match(f.calls[0][0], /^\/api\/security\/encrypt\?/);
    assert.ok(!f.calls[0][0].includes('password'));
    assert.equal(JSON.parse(f.calls[0][1].body).password, 'long password 123');
    assert.equal(f.get('db-password').value, '');
    assert.equal(f.get('db-password-confirm').value, '');
    f.get('db-security-form').listeners.submit({preventDefault(){}});
    assert.equal(f.calls.length, 1, 'pending operations cannot be submitted twice');
    f.h.invalidateRequests();
    resolve(new Response('{}', {status:200}));
    await new Promise(r=>setImmediate(r));
    assert.equal(f.h.dbSecurity.pending, false);
    assert.equal(f.get('db-password').value, '');
});

test('encrypted status exposes plaintext cleanup only after unlocking', () => {
    const f = fixture();
    f.h.dbSecurity.update({encrypted:true,locked:false,plaintext_original:true}); f.h.dbSecurity.open();
    assert.equal(f.get('db-delete-plaintext').hidden, false);
    assert.equal(f.get('db-security-form').hidden, true);
    assert.match(f.get('db-security-note').textContent, /평문 원본/);
    f.confirm(false); f.get('db-delete-plaintext').listeners.click();
    assert.equal(f.calls.length, 0);
    f.h.dbSecurity.update({encrypted:true,locked:true,plaintext_original:true});
    assert.equal(f.get('db-delete-plaintext').hidden, true);
    assert.equal(f.get('btn-db-lock').hidden, true);
});

test('password change validates current and new passwords, clears inputs and shows backup reminder', async () => {
    const f=fixture(), security=f.h.dbSecurity;
    const state={encrypted:true,locked:false,plaintext_original:false};
    security.update(state); security.open();
    assert.equal(f.get('db-change-password').hidden,false);
    f.get('db-change-password').listeners.click();
    assert.equal(f.get('db-current-row').hidden,false);
    assert.equal(f.get('db-password-label').textContent,'새 비밀번호');
    assert.equal(f.get('db-current-password').focused,true);
    f.get('db-password').value='new password 123';
    f.get('db-password-confirm').value='mismatch';
    const submit=()=>f.get('db-security-form').listeners.submit({preventDefault(){}});
    submit(); assert.equal(f.calls.length,0);
    f.get('db-password-confirm').value='new password 123';
    submit(); assert.equal(f.calls.length,0);
    f.get('db-current-password').value='old password 123';
    let finish, body;
    f.respond((_url,options)=>{body=JSON.parse(options.body);return new Promise(r=>finish=r);});
    security.onChanged=async payload=>security.update(payload.security);
    submit(); submit();
    assert.equal(f.calls.length,1);
    assert.match(f.calls[0][0],/^\/api\/security\/password\?/);
    assert.deepEqual(body,{current_password:'old password 123',password:'new password 123',confirmation:'new password 123'});
    for(const id of ['db-current-password','db-password','db-password-confirm']) assert.equal(f.get(id).value,'');
    finish(new Response(JSON.stringify({security:state,generation:2,password_changed:true,message:'새 백업을 만들어 주세요.'})));
    await new Promise(r=>setImmediate(r));
    assert.equal(security.pending,false);
    assert.equal(f.get('db-security-dialog').open,true);
    assert.match(f.get('db-security-message').textContent,/백업/);
    security.update({...state,locked:true});
    assert.equal(f.get('db-current-row').hidden,true);
    assert.equal(f.get('db-change-password').hidden,true);
});


test('archive encryption selects only eligible files and batches with server verification', async () => {
    const f=fixture(); let encrypted=false;
    const files=[{id:1,name:'book.zip',state:'plain',original:true,trash:false},{id:2,name:'done.zip',state:'ready',original:false,trash:false},{id:3,name:'trash.tar.zst',state:'plain',original:true,trash:true}];
    f.respond(async url => {
        if(url.includes('/1/encrypt')){encrypted=true;files[0].state='ready';return new Response('{"status":"started","job_id":"test-job"}');}
        if(url.includes('/api/archive-security'))return new Response(JSON.stringify({files,job:{job_id:"test-job",running:false,completed:encrypted}}));
        return new Response('[]');
    });
    await f.h.archiveSecurity.open();f.get('archive-security-all').click();
    assert.deepEqual([...f.h.archiveSecurity.selected],[1,2]);
    f.get('media-encryption-risk-ack').checked=true;
    await f.h.archiveSecurity.batch();
    assert.equal(f.calls.filter(([u])=>u.includes('/encrypt')).length,1);
    assert.equal(f.h.archiveSecurity.pending,false);
    assert.match(f.get('archive-security-status').textContent,/vault.key/);
    assert.deepEqual([...f.h.archiveSecurity.selected],[2]);
});

test('archive cleanup cancellation sends no deletion and lock clears its filenames', async () => {
    const f=fixture();const file={id:1,name:'private.zip',state:'ready',original:true,trash:false};
    f.respond(async()=>new Response(JSON.stringify({files:[file],job:{job_id:"test-job",running:false}})));
    await f.h.archiveSecurity.open();f.confirm(false);await f.h.archiveSecurity.single(file,'plaintext');
    assert.equal(f.calls.some(([,o])=>o?.method==='DELETE'),false);
    f.h.resetLibraryView();assert.equal(f.h.archiveSecurity.files.length,0);assert.equal(f.get('archive-security-list').children.length,0);assert.equal(f.h.archiveSecurity.isOpen(),false);
});

test('failed server conversion stops the remaining batch and preserves selections',async()=>{
    const f=fixture();let attempts=0;
    f.respond(async url=>{if(url.includes('/encrypt')){attempts++;return new Response('{"status":"started","job_id":"test-job"}');}return new Response(JSON.stringify({files:[],job:{job_id:"test-job",running:false,error:'disk full'}}));});
    f.get('media-encryption-risk-ack').checked=true;
    f.h.archiveSecurity.files=[{id:1,state:'plain',name:'1.zip'},{id:2,state:'plain',name:'2.zip'}];f.h.archiveSecurity.selected=new Set([1,2]);await f.h.archiveSecurity.batch();
    assert.equal(attempts,1);assert.equal(f.h.archiveSecurity.selected.size,2);assert.equal(f.h.archiveSecurity.pending,false);assert.match(f.get('archive-security-status').textContent,/disk full/);
});

test('batch cleanup filters selected files, confirms once and reports per-file results',async()=>{
    const f=fixture(),s=f.h.archiveSecurity;
    const files=[{id:1,name:'plain.zip',state:'plain',original:true},{id:2,name:'ready.zip',state:'ready',original:true},{id:3,name:'only-vta.zip',state:'ready',original:false},{id:4,name:'trash.zip',state:'ready',original:true,trash:true}];
    const job={job_id:"test-job",running:false,batch:{total:1,completed:1,skipped:0,failed:0,failures:[]}};
    f.respond(async url=>new Response(JSON.stringify(url.includes('/batch')?{status:'started',job_id:'test-job'}:{files,job})));
    await s.open();f.get('archive-security-all').click();
    assert.deepEqual([...s.selected],[1,2,3]);
    assert.match(f.get('archive-security-delete').textContent,/1개/);
    assert.match(f.get('archive-security-restore').textContent,/2개/);
    f.confirm(false);await s.batchAction('plaintext');
    assert.equal(f.calls.filter(([,o])=>o?.method==='POST').length,0);
    f.confirm(true);s.onChanged=async()=>{};await s.batchAction('plaintext');
    const posts=f.calls.filter(([,o])=>o?.method==='POST');
    assert.equal(posts.length,1);assert.deepEqual(JSON.parse(posts[0][1].body),{action:'plaintext',ids:[2]});
    assert.match(f.get('archive-security-status').textContent,/완료 1개/);
    assert.equal(s.pending,false);
    f.get('archive-security-none').click();assert.equal(s.selected.size,0);
    assert.equal(f.get('archive-security-delete').disabled,true);
});

test('batch restore sends one request, includes originals and reports skipped and failed files',async()=>{
    const f=fixture(),s=f.h.archiveSecurity;
    s.files=[{id:1,name:'first.zip',state:'ready',original:true},{id:2,name:'second.zip',state:'ready',original:false}];
    s.selected=new Set([1,2]);s.onChanged=async()=>{};
    let finish;
    const job={job_id:"test-job",running:false,batch:{total:2,completed:0,skipped:1,failed:1,failures:['second.zip: 손상된 파일']}};
    f.respond(async url=>url.includes('/batch')?new Promise(r=>finish=r):new Response(JSON.stringify({files:s.files,job})));
    const pending=s.batchAction('export');await s.batchAction('export');
    assert.equal(f.calls.length,1);
    assert.deepEqual(JSON.parse(f.calls[0][1].body),{action:'export',ids:[1,2]});
    assert.equal(f.get('archive-security-restore').disabled,true);
    assert.equal(f.get('archive-security-cancel').hidden,false);
    finish(new Response('{"status":"started","job_id":"test-job"}'));await pending;
    assert.match(f.get('archive-security-status').textContent,/건너뜀 1개 · 실패 1개/);
    assert.match(f.get('archive-security-status').textContent,/second.zip: 손상된 파일/);
    assert.equal(f.confirmations.length,0);
});

test('batch cancellation preserves completion counts and stale results cannot repaint a new library',async()=>{
    const f=fixture(),s=f.h.archiveSecurity;
    s.files=[{id:1,name:'first.zip',state:'ready',original:true}];s.selected=new Set([1]);
    f.respond(async url=>new Response(JSON.stringify(url.includes('/batch')?{status:'started',job_id:'test-job'}:{files:s.files,job:{job_id:"test-job",running:false,error:'취소되었습니다.',batch:{total:1,completed:0,skipped:0,failed:0}}})));
    await s.batchAction('export');assert.match(s.status.textContent,/완료 0개.*\n취소되었습니다/);
    let finish;
    f.respond(()=>new Promise(r=>finish=r));
    const pending=s.batchAction('export');
    f.h.resetLibraryView();f.h.setSession(2);
    finish(new Response('{"status":"started","job_id":"test-job"}'));await pending;
    assert.equal(s.status.textContent,'');assert.equal(s.isOpen(),false);
});

test('last archive page opens the next supported archive in filtered list order and reveals its card', async () => {
    const f=fixture();
    const files=[
        {id:1,rel_path:'series_02.zip',file_type:'ARCHIVE',rating:5},
        {id:2,rel_path:'series_video.mp4',file_type:'VIDEO'},
        {id:3,rel_path:'series_image.png',file_type:'IMAGE'},
        {id:4,rel_path:'series_03.rar',file_type:'ARCHIVE'},
        {id:5,rel_path:'excluded.zip',file_type:'ARCHIVE'},
        {id:6,rel_path:'series_04.tar.zst',file_type:'ARCHIVE',rating:5},
        {id:7,rel_path:'series_01.zip',file_type:'ARCHIVE',rating:5}
    ];
    f.h.setFiles(files);f.get('search-input').value='series';f.h.setCurrent(files[0]);
    f.h.setArchive(['last.png'],0);
    f.respond(async url=>new Response(JSON.stringify(url.includes('/api/archive/list/')?['10.png','2.png','1.png']:[])));
    await f.get('btn-img-next').click();
    const lists=f.calls.filter(([u])=>u.includes('/api/archive/list/'));
    assert.equal(lists.length,1);assert.match(lists[0][0],/series_04.tar.zst/);
    assert.match(f.get('viewer-image').src,/series_04.tar.zst.*entry=1.png/);
    const cards=f.get('media-list').children[0].children;
    assert.ok(cards.some(card=>card.dataset.id===6&&card.classList.contains('active')));
});

test('last two-page spread advances through keyboard and wheel, and rapid clicks do not skip an archive while loading', async () => {
    for(const input of ['keyboard','wheel','button']){
        const f=fixture();const files=[{id:1,rel_path:'first.zip',file_type:'ARCHIVE'},{id:2,rel_path:'second.zip',file_type:'ARCHIVE'},{id:3,rel_path:'third.zip',file_type:'ARCHIVE'}];
        f.h.setFiles(files);f.h.setCurrent(files[0]);f.h.setArchive(['1.png','2.png'],0);
        for(const entry of ['1.png','2.png'])f.h.cacheImage('first.zip',entry,{isPortrait:true});
        await f.h.showArchiveImage('first.zip',0);
        assert.match(f.get('img-page-counter').textContent,/1-2/);
        let finish;f.respond(url=>url.includes('/api/archive/list/')?new Promise(r=>finish=r):Promise.resolve(new Response('[]')));
        if(input==='keyboard')f.h.handleShortcut(keyEvent('ArrowRight'));
        else if(input==='wheel')f.get('image-viewer-stage').listeners.wheel({deltaY:1,preventDefault(){},stopPropagation(){}});
        else f.get('btn-img-next').click();
        f.get('btn-img-next').click();
        assert.equal(f.calls.filter(([u])=>u.includes('/api/archive/list/')).length,1);
        assert.match(f.calls[0][0],/second.zip/);
        finish(new Response('["first.png"]'));await new Promise(r=>setImmediate(r));
        assert.match(f.get('viewer-image').src,/second.zip.*entry=first.png/);
    }
});

test('next page stays at the end when no following archive is listed and does not jump from a filtered-out file', async () => {
    const f=fixture();const files=[{id:1,rel_path:'first.zip',file_type:'ARCHIVE'},{id:2,rel_path:'last.zip',file_type:'ARCHIVE'},{id:3,rel_path:'video.mp4',file_type:'VIDEO'}];
    f.h.setFiles(files);f.h.setCurrent(files[1]);f.h.setArchive(['1.png','2.png'],1);
    f.h.cacheImage('last.zip','2.png',{isPortrait:false});await f.h.showArchiveImage('last.zip',1);
    const before=f.get('viewer-image').src;
    await f.get('btn-img-next').click();assert.equal(f.calls.length,0);assert.equal(f.get('viewer-image').src,before);
    f.get('search-input').value='first';await f.get('btn-img-next').click();
    assert.equal(f.calls.length,0);assert.equal(f.get('viewer-image').src,before);
});

test('first archive page opens the previous supported filtered archive at its natural last page', async () => {
    const f=fixture();const files=[
        {id:1,rel_path:'series_10.tar.zst',file_type:'ARCHIVE'},
        {id:2,rel_path:'excluded.zip',file_type:'ARCHIVE'},
        {id:3,rel_path:'series_image.png',file_type:'IMAGE'},
        {id:4,rel_path:'series_bad.7z',file_type:'ARCHIVE'},
        {id:5,rel_path:'series_20.zip',file_type:'ARCHIVE'}
    ];
    f.h.setFiles(files);f.get('search-input').value='series';f.h.setCurrent(files[4]);f.h.setArchive(['1.png'],0);
    f.respond(async url=>new Response(JSON.stringify(url.includes('/api/archive/list/')?['1.png','10.png','2.png']:[])));
    await f.get('btn-img-prev').click();
    assert.match(f.calls[0][0],/series_10.tar.zst/);
    assert.match(f.get('viewer-image').src,/series_10.tar.zst.*entry=10.png/);
    assert.match(f.get('img-page-counter').textContent,/3 \/ 3/);
    assert.ok(f.get('media-list').children[0].children.some(card=>card.dataset.id===1&&card.classList.contains('active')));
});

test('previous archive works from a first spread with keyboard and wheel without skipping while loading', async () => {
    for(const input of ['keyboard','wheel']){
        const f=fixture();const files=[{id:1,rel_path:'first.zip',file_type:'ARCHIVE'},{id:2,rel_path:'second.zip',file_type:'ARCHIVE'}];
        f.h.setFiles(files);f.h.setCurrent(files[1]);f.h.setArchive(['1.png','2.png'],0);
        for(const entry of ['1.png','2.png'])f.h.cacheImage('second.zip',entry,{isPortrait:true});
        await f.h.showArchiveImage('second.zip',0);
        let finish;f.respond(url=>url.includes('/api/archive/list/')?new Promise(r=>finish=r):Promise.resolve(new Response('[]')));
        if(input==='keyboard')f.h.handleShortcut(keyEvent('ArrowLeft'));
        else f.get('image-viewer-stage').listeners.wheel({deltaY:-1,preventDefault(){},stopPropagation(){}});
        f.get('btn-img-prev').click();
        assert.equal(f.calls.filter(([u])=>u.includes('/api/archive/list/')).length,1);
        finish(new Response('["1.png","2.png"]'));await new Promise(r=>setImmediate(r));
        assert.match(f.get('viewer-image').src,/first.zip.*entry=2.png/);
    }
});

test('previous page stops at the first listed archive and a spread beginning at page two moves to page one', async () => {
    const f=fixture(),file={id:1,rel_path:'first.zip',file_type:'ARCHIVE'};
    f.h.setFiles([file]);f.h.setCurrent(file);f.h.setArchive(['1.png','2.png','3.png'],1);
    for(const entry of ['1.png','2.png','3.png'])f.h.cacheImage('first.zip',entry,{isPortrait:true});
    await f.h.showArchiveImage('first.zip',1);
    await f.get('btn-img-prev').click();assert.match(f.get('viewer-image').src,/entry=1.png/);
    await f.get('btn-img-prev').click();assert.equal(f.calls.length,0);assert.match(f.get('viewer-image').src,/entry=1.png/);
    f.get('search-input').value='no matching file';
    await f.get('btn-img-prev').click();assert.equal(f.calls.length,0);
});

test('DB restore selection shows backup password requirements, submits once and clears secrets',async()=>{
    const f=fixture(),r=f.h.dbRestore;
    f.get('btn-restore-db').click();assert.equal(r.isOpen(),true);
    f.respond(async()=>new Response(JSON.stringify({status:'selected',token:'chosen',name:'test.vtbackup',encrypted:true,key_included:true})));
    await r.choose();assert.equal(r.passwordRow.hidden,false);assert.equal(r.password.required,true);
    assert.match(r.file.textContent,/vault.key 포함/);
    const count=f.calls.length;await r.restore();assert.equal(f.calls.length,count);
    r.password.value='backup password 123';let finish,body;
    f.respond((_url,o)=>{body=JSON.parse(o.body);return new Promise(resolve=>finish=resolve);});
    let adopted;r.onChanged=async data=>{adopted=data;r.reset();};
    const pending=r.restore();await r.restore();
    assert.equal(f.calls.length,count+1);assert.equal(r.password.value,'');assert.equal(r.close.disabled,false);
    assert.deepEqual(body,{token:'chosen',password:'backup password 123'});
    assert.ok(!f.calls.at(-1)[0].includes('backup%20password'));
    finish(new Response(JSON.stringify({status:'success',generation:2,restore_path:'D:/restore/new',security:{encrypted:true,locked:false}})));
    await pending;assert.equal(adopted.generation,2);assert.equal(r.isOpen(),true);
    assert.match(r.status.textContent,/DB 백업 복원 완료/);assert.match(r.status.textContent,/D:\/restore\/new/);
    assert.equal(r.submit.disabled,true);assert.equal(r.password.value,'');
});

test('DB restore cancellation and errors retain selection for retry; close clears passwords',async()=>{
    const f=fixture(),r=f.h.dbRestore;r.onResync=async()=>{};
    f.get('btn-restore-db').click();
    f.respond(async()=>new Response(JSON.stringify({status:'selected',token:'plain',name:'plain.vtbackup',encrypted:false,key_included:false})));
    await r.choose();assert.equal(r.passwordRow.hidden,true);assert.equal(r.password.required,false);
    f.respond(async()=>new Response('{"status":"cancelled"}'));await r.restore();
    assert.equal(r.selection.token,'plain');assert.match(r.status.textContent,/취소/);
    f.respond(async()=>new Response('손상된 DB',{status:400}));await r.restore();
    assert.match(r.status.textContent,/손상된 DB/);assert.equal(r.selection.token,'plain');assert.equal(r.pending,false);
    f.respond(async()=>new Response('{"status":"cleared"}'));
    r.password.value='secret';f.get('db-restore-close').click();
    assert.equal(r.password.value,'');assert.equal(r.selection,null);assert.equal(r.isOpen(),false);
    assert.match(f.calls.at(-1)[0],/restore\/clear/);
});

test('DB restore rejects stale file selection and blocks media shortcuts while open',async()=>{
    const f=fixture(),r=f.h.dbRestore;
    f.get('btn-restore-db').click();const count=f.calls.length;
    f.h.handleShortcut(keyEvent('KeyV'));assert.equal(f.calls.length,count);
    let finish;f.respond(()=>new Promise(resolve=>finish=resolve));
    const pending=r.choose();f.h.resetLibraryView();f.h.setSession(2);
    finish(new Response('{"status":"selected","token":"old","encrypted":true}'));await pending;
    assert.equal(r.selection,null);assert.equal(r.isOpen(),false);assert.equal(r.password.value,'');
});

test('concealment clears visible media and folders and blocks list loads and shortcuts',async()=>{
    const f=fixture();f.h.setFiles([{id:1,rel_path:'private.png',file_type:'IMAGE',tags:[]}]);
    f.get('folder-select').appendChild(f.get('private-folder'));f.h.dbSecurity.open();
    f.h.adoptLibraryState({generation:2,covered:true});
    assert.equal(f.h.privacy.covered,true);assert.equal(f.h.mediaVirtualList.items.length,0);
    assert.equal(f.get('folder-select').children.length,0);assert.equal(f.h.dbSecurity.isOpen(),false);
    const count=f.calls.length;await f.h.loadMediaFiles();f.h.handleShortcut(keyEvent('ArrowDown'));
    assert.equal(f.calls.length,count);assert.equal(f.get('privacy-dialog').open,undefined);
});

test('five timely photo library label clicks fetch folder choices only then and cancellation clears the dialog',async()=>{
    const f=fixture(),p=f.h.privacy;await f.h.adoptLibraryState({generation:1,covered:true});
    const click=()=>f.get('privacy-library-title').click();
    for(let i=0;i<4;i++)click();assert.ok(!p.isOpen());
    f.advance(2600);for(let i=0;i<4;i++)click();assert.ok(!p.isOpen());
    assert.equal(f.calls.length,1);assert.match(f.calls[0][0],/api\/gallery/);await click();assert.equal(p.isOpen(),true);assert.equal(p.covered,true);assert.equal(f.calls.length,2);
    assert.match(f.calls[1][0],/security\/folders/);
    p.password.value='sensitive';f.get('privacy-close').click();
    assert.equal(p.password.value,'');assert.equal(p.isOpen(),false);assert.equal(p.covered,true);
});

function sampleGallery(f) {
    const names=['산과 호수.png','해안 산책.png','숲길.png','오후의 건물.png','창가의 고양이.png','들꽃.png'];
    const ids=['lake','coast','forest','architecture','cat','flowers'];
    f.h.privacy.applyGallery({status:'ready',folder_name:'Example',images:names.map((name,index)=>({id:ids[index],name,src:`/api/gallery/image/1/${index}`}))});
}

test('folder gallery selects, filters, sorts and navigates without reading library APIs',()=>{
    const f=fixture(),p=f.h.privacy;p.setCovered(true);sampleGallery(f);
    assert.equal(f.get('privacy-count').textContent,'6개');
    const list=f.get('privacy-list');
    list.children.find(card=>card.dataset.id==='cat').click();
    assert.match(f.get('privacy-image').src,/gallery\/image\/1\/4\?/);
    p.handleKey(keyEvent('ArrowRight'));
    assert.match(f.get('privacy-image').src,/gallery\/image\/1\/1\?/);
    assert.equal(f.get('privacy-next').disabled,true);p.move(1);
    f.get('privacy-search').value='고양이';f.get('privacy-search').listeners.input();
    assert.equal(list.children.length,1);assert.match(f.get('privacy-image').src,/gallery\/image\/1\/4\?/);
    f.get('privacy-search').value='no match';f.get('privacy-search').listeners.input();
    assert.equal(list.children.length,0);assert.equal(f.get('privacy-image').hidden,true);
    assert.equal(f.get('privacy-image').src,'');assert.equal(f.get('privacy-position').textContent,'0 / 0');
    assert.equal(f.get('privacy-prev').disabled,true);assert.equal(f.get('privacy-next').disabled,true);
    f.get('privacy-search').value='';f.get('privacy-sort').value='name';f.get('privacy-search').listeners.input();
    const ascending=Array.from(list.children,card=>card.dataset.id);
    f.get('privacy-sort').value='name-desc';f.get('privacy-sort').listeners.change();
    assert.deepEqual(Array.from(list.children,card=>card.dataset.id),ascending.reverse());
    assert.equal(f.calls.length,0);
});

test('folder gallery navigation respects editing and unlock modal; relocking starts without stale images',async()=>{
    const f=fixture(),p=f.h.privacy;p.setCovered(true);sampleGallery(f);p.move(1);
    const selected=f.get('privacy-image').src;
    p.handleKey(keyEvent('ArrowRight',{target:{closest:()=>({})}}));
    assert.equal(f.get('privacy-image').src,selected);
    for(let i=0;i<5;i++)await f.get('privacy-library-title').click();
    p.handleKey(keyEvent('ArrowRight'));p.move(1);
    assert.equal(f.get('privacy-image').src,selected);
    p.dismiss();p.setCovered(true);assert.equal(f.get('privacy-image').src,selected);
    p.setCovered(false);p.handleKey(keyEvent('ArrowRight'));assert.equal(f.get('privacy-image').src,'');
    f.get('privacy-search').value='고양이';p.setCovered(true);
    assert.equal(f.get('privacy-search').value,'');assert.equal(f.get('privacy-image').src,'');assert.equal(f.calls.length,1);
});

test('private entry validates on server, erases passwords, suppresses detailed errors and reveals only after success',async()=>{
    const f=fixture(),p=f.h.privacy;await f.h.adoptLibraryState({generation:1,covered:true});
    f.respond(async()=>new Response(JSON.stringify({folders:[{name:'A',path:'D:/A'},{name:'B',path:'D:/B'}],active_folder:'D:/A'})));
    for(let i=0;i<5;i++)await f.get('privacy-library-title').click();
    assert.equal(p.folder.value,'D:/A');p.password.value='old draft';
    p.folder.value='D:/B';p.folder.listeners.change();assert.equal(p.password.value,'');
    p.onResync=async()=>{};p.password.value='wrong';
    f.respond(async()=>new Response('private-folder/vault.key: error',{status:400}));await p.reveal();
    assert.equal(p.covered,true);assert.equal(p.password.value,'');assert.equal(p.message.textContent,'선택한 폴더와 비밀번호를 확인해 주세요.');
    let finish,body;f.respond((_url,options)=>{body=JSON.parse(options.body);return new Promise(r=>finish=r);});
    p.password.value='correct fixture password';p.onRevealed=async payload=>f.h.adoptLibraryState(payload);
    const pending=p.reveal();await p.reveal();assert.equal(f.calls.length,4);assert.equal(p.password.value,'');assert.equal(p.close.disabled,false);
    assert.deepEqual(body,{path:'D:/B',password:'correct fixture password'});
    assert.match(f.calls.at(-1)[0],/security\/open-folder/);assert.equal(p.folder.disabled,true);
    finish(new Response(JSON.stringify({generation:2,covered:false,security:{encrypted:true,locked:false,plaintext_original:false}})));await pending;
    assert.equal(p.covered,false);assert.equal(p.isOpen(),false);assert.equal(p.password.value,'');
});


test('closing hidden entry discards delayed folder lists and reopening makes a fresh selection',async()=>{
    const f=fixture(),p=f.h.privacy;p.setCovered(true);
    let finish;f.respond(()=>new Promise(resolve=>finish=resolve));
    const loading=p.openEntry();assert.equal(p.submit.disabled,true);p.dismiss();
    finish(new Response(JSON.stringify({folders:[{name:'private',path:'D:/private'}],active_folder:'D:/private'})));
    await loading;assert.equal(p.isOpen(),false);assert.equal(p.folder.children.length,0);
    assert.equal(p.folder.value,'');assert.equal(p.folders.length,0);
    f.respond(async()=>new Response(JSON.stringify({folders:[{name:'B',path:'D:/B'}],active_folder:'D:/removed'})));
    await p.openEntry();assert.equal(p.folder.value,'D:/B');assert.equal(p.submit.disabled,false);
    p.password.value='secret';p.dismiss();assert.equal(p.password.value,'');assert.equal(p.folder.children.length,0);
});

test('folder entry retries the session invalidated by startup without leaving its picker disabled',async()=>{
    const f=fixture(),p=f.h.privacy;p.setCovered(true);let folderReply,requests=0;
    f.respond(async url=>{
        if(url.startsWith('/api/security/folders')){
            if(++requests===1)return new Promise(resolve=>folderReply=resolve);
            return new Response('{"folders":[]}');
        }
        return new Response(JSON.stringify(url.startsWith('/api/session')?{generation:2,covered:true}:{images:[]}));
    });
    const entry=p.openEntry();await new Promise(setImmediate);
    await f.h.initializeVaultSession();
    folderReply(new Response('{"folders":[{"name":"stale","path":"D:/stale"}]}'));await entry;
    assert.equal(requests,2);assert.equal(p.isOpen(),true);assert.equal(p.loading,false);
    assert.equal(p.entryReady,true);assert.equal(p.pickFolder.disabled,false);
    assert.equal(p.folders.length,0);assert.match(p.message.textContent,/폴더 지정을 눌러/);
});

test('repeated session changes stop automatic folder retries and leave manual recovery available',async()=>{
    const f=fixture(),p=f.h.privacy;p.setCovered(true);let requests=0;
    f.respond(async()=>{requests++;f.h.invalidateRequests();return new Response('{"folders":[]}');});
    await p.openEntry();
    assert.equal(requests,2);assert.equal(p.loading,false);assert.equal(p.pickFolder.disabled,true);
    assert.equal(p.retry.hidden,false);assert.equal(p.retry.disabled,false);
    assert.match(p.message.textContent,/다시 시도/);
    f.respond(async()=>new Response('{"folders":[]}'));await p.retry.click();
    assert.equal(p.pickFolder.disabled,false);assert.equal(p.retry.hidden,true);
});

test('empty and failed folder lists never allow submitting a password or expose server paths',async()=>{
    const f=fixture(),p=f.h.privacy;p.setCovered(true);
    f.respond(async()=>new Response(JSON.stringify({folders:[],active_folder:null})));
    await p.openEntry();assert.equal(p.submit.disabled,true);assert.match(p.message.textContent,/폴더 지정을 눌러/);
    p.password.value='secret';await p.reveal();assert.equal(f.calls.length,1);
    p.dismiss();f.respond(async()=>new Response('D:/private/config.json',{status:500}));
    await p.openEntry();assert.equal(p.submit.disabled,true);assert.ok(!p.message.textContent.includes('private'));
    p.dismiss();assert.equal(p.isOpen(),false);
});


test('gallery folder picking preserves cancellation, scopes image URLs and clears selection without deleting photos',async()=>{
    const f=fixture(),p=f.h.privacy;p.setCovered(true);
    f.respond(async()=>new Response(JSON.stringify({images:[{id:'0',name:'chosen.png',src:'/api/gallery/image/5/0'}],folder_name:'Chosen',status:'ready'})));
    await f.get('privacy-choose').click();
    assert.match(f.calls[0][0],/gallery\/select/);assert.equal(f.calls[0][1].method,'POST');
    assert.match(p.image.src,/session=test-token/);assert.match(p.image.src,/library=1/);
    const selected=p.image.src;
    f.respond(async()=>new Response('{"cancelled":true}'));await p.loadGallery('select');assert.equal(p.image.src,selected);
    f.respond(async()=>new Response(JSON.stringify({images:[],status:'empty',folder_name:null})));await f.get('privacy-clear').click();
    assert.equal(f.calls.at(-1)[1].method,'DELETE');assert.equal(p.image.src,'');assert.equal(p.list.children.length,0);assert.match(p.galleryMessage.textContent,/사진 폴더를 선택/);assert.equal(p.frame.startButton.disabled,true);
    assert.ok(f.calls.every(([url])=>url.startsWith('/api/gallery')));
});

test('late gallery responses cannot repaint an unlocked library and missing folders remain recoverable',async()=>{
    const f=fixture(),p=f.h.privacy;p.setCovered(true);let finish;
    f.respond(()=>new Promise(resolve=>finish=resolve));const pending=p.loadGallery('select');
    await p.loadGallery('select');assert.equal(f.calls.length,1);assert.equal(p.choose.disabled,true);
    p.setCovered(false);finish(new Response('{"images":[{"id":"0","name":"late.png","src":"/api/gallery/image/1/0"}],"status":"ready"}'));
    await pending;assert.equal(p.list.children.length,0);assert.equal(p.image.src,'');assert.equal(p.choose.disabled,false);
    p.setCovered(true);f.respond(async()=>new Response('{"images":[],"status":"unavailable"}'));await p.loadGallery();
    assert.match(p.galleryMessage.textContent,/찾을 수 없습니다/);assert.equal(p.choose.disabled,false);
    assert.equal(p.image.hidden,true);
});


test('first-run hidden entry can pick a plain folder and open without a password; encrypted selections require it',async()=>{
    const f=fixture(),p=f.h.privacy;p.setCovered(true);
    f.respond(async()=>new Response('{"folders":[],"active_folder":null}'));await p.openEntry();
    assert.equal(p.pickFolder.disabled,false);assert.equal(p.submit.disabled,true);
    f.respond(async()=>new Response('{"status":"selected","folder":{"name":"New","path":"D:/new","encrypted":false}}'));
    await p.pickFolder.click();assert.equal(p.folder.value,'D:/new');assert.equal(p.password.required,false);
    assert.equal(p.passwordRow.hidden,true);assert.equal(p.submit.disabled,false);
    let body;p.onRevealed=async data=>p.setCovered(data.covered);
    f.respond(async(_url,options)=>{body=JSON.parse(options.body);return new Response('{"covered":false}');});
    await p.reveal();assert.deepEqual(body,{path:'D:/new',password:''});assert.equal(p.covered,false);
    p.setCovered(true);f.respond(async()=>new Response('{"folders":[]}'));await p.openEntry();
    f.respond(async()=>new Response('{"status":"selected","folder":{"name":"Protected","path":"D:/protected","encrypted":true}}'));
    await p.chooseLibrary();assert.equal(p.password.required,true);assert.equal(p.passwordRow.hidden,false);
    const calls=f.calls.length;await p.reveal();assert.equal(f.calls.length,calls);
});

test('native library picker cancellation preserves selection and pending picker blocks duplicate requests',async()=>{
    const f=fixture(),p=f.h.privacy;p.setCovered(true);
    f.respond(async()=>new Response('{"folders":[{"name":"A","path":"D:/A","encrypted":true}],"active_folder":"D:/A"}'));
    await p.openEntry();let finish;f.respond(()=>new Promise(resolve=>finish=resolve));
    const pending=p.chooseLibrary();await p.chooseLibrary();p.dismiss();
    assert.equal(p.isOpen(),true);assert.equal(p.pickFolder.disabled,true);assert.equal(f.calls.length,2);
    finish(new Response('{"status":"cancelled"}'));await pending;
    assert.equal(p.folder.value,'D:/A');assert.equal(p.pickFolder.disabled,false);assert.equal(p.password.value,'');
    p.dismiss();assert.equal(p.isOpen(),false);
});

test('frame uses the whole gallery despite search and stops before opening the hidden entry or revealing media',async()=>{
    const f=fixture(),p=f.h.privacy;p.setCovered(true);sampleGallery(f);
    f.get('privacy-search').value='고양이';p.renderGallery();assert.equal(p.visible.length,1);
    p.frame.root.requestFullscreen=async()=>{};
    await p.frame.start();assert.equal(p.frame.images.length,6);assert.equal(p.frame.active,true);
    await p.openEntry();assert.equal(p.frame.active,false);p.dismiss();
    await p.frame.start();assert.equal(p.frame.active,true);
    p.setCovered(false);assert.equal(p.frame.active,false);assert.equal(p.frame.canvas.children.length,0);
});

test('close library needs no folder selection or confirmation and returns to decoy',async()=>{
    const f=fixture();
    let finish;
    f.respond(async url=>url.includes('/api/folders/close')?new Promise(resolve=>finish=resolve):new Response(JSON.stringify({images:[],status:'empty'})));
    const closing=f.get('btn-close-library').click();
    await f.get('btn-close-library').click();
    assert.equal(f.calls.length,1);
    assert.equal(f.calls[0][1].method,'POST');
    assert.equal(f.calls[0][1].body,undefined);
    assert.equal(f.confirmations.length,0);
    finish(new Response(JSON.stringify({generation:2,covered:true})));
    await closing;
    assert.equal(f.h.privacy.covered,true);
    assert.ok(f.calls.every(([url])=>!url.includes('/api/files')&&!url.includes('/api/folders/remove')));
    await f.get('btn-close-library').click();
    assert.equal(f.calls.filter(([url])=>url.includes('/api/folders/close')).length,1);
});

test('session bootstrap uses native injected credential and ignores server supplied tokens', async () => {
    const f = fixture();
    f.respond(async()=>new Response(JSON.stringify({generation:1,covered:false,token:'untrusted-response-token'})));
    await f.h.initializeVaultSession();
    assert.equal(f.calls[0][0],'/api/session');
    assert.equal(f.calls[0][1].headers['X-Vault-Token'],'native-bootstrap-token');
    f.calls.length=0;
    await f.h.vaultFetch('/api/files');
    assert.equal(f.calls[0][1].headers.get('X-Vault-Token'),'native-bootstrap-token');
});

test('missing media stay visible with reconnect control and playback skips them', async () => {
    const f=fixture();
    const files=[{id:1,rel_path:'first.jpg',file_type:'IMAGE',tags:[]},
        {id:2,rel_path:'missing.jpg',file_type:'IMAGE',missing:true,tags:[],rating:5},
        {id:3,rel_path:'last.jpg',file_type:'IMAGE',tags:[]}];
    f.h.setFiles(files);
    const walk=element=>[element,...element.children.flatMap(walk)];
    const card=f.h.mediaVirtualList.nodes.get(1);
    const reconnect=walk(card).find(element=>element.className==='missing-relink-btn');
    assert.ok(reconnect);
    f.calls.length=0;
    await f.h.selectMedia(files[1]);
    assert.equal(f.calls.length,0);
    f.h.setCurrent(files[0]);
    f.h.navigateMedia('next');
    assert.ok(f.h.mediaVirtualList.nodes.get(2).classList.contains('active'));
    let paths=[];
    f.respond(async url=>{
        paths.push(url);
        if(url.includes('/relink')) return new Response(JSON.stringify({status:'cancelled'}));
        return new Response('[]');
    });
    await reconnect.onclick({stopPropagation(){}});
    assert.ok(paths.some(url=>url.includes('/api/files/2/relink')));
    assert.ok(!paths.some(url=>url.includes('/api/files?')));
});


test('encrypted archive rename saves an alias and search uses it without changing media URLs',async()=>{
    const f=fixture();const file={id:1,rel_path:'folder/original.zip',file_type:'ARCHIVE',encrypted:true,tags:[],file_size:1};
    f.h.setFiles([file]);
    const walk=el=>[el,...el.children.flatMap(walk)];
    const button=walk(f.h.mediaVirtualList.nodes.get(0)).find(el=>el.className==='card-rename-btn');
    assert.ok(button);button.onclick({stopPropagation(){}});
    assert.equal(f.h.nameEditor.isOpen(),true);
    assert.equal(f.get('media-name-input').value,'original.zip');
    f.get('media-name-input').value='changed.zip';f.get('media-name-include-restore').checked=false;
    f.respond(async url=>new Response(JSON.stringify(url.includes('/api/files?')?[{...file,display_name:'folder/changed.zip'}]:url.includes('/archive/list/')?['page.jpg']:{id:1,display_name:'folder/changed.zip'})));
    await f.h.nameEditor.submit();
    const call=f.calls.find(([url])=>url.includes('/api/files/1/name'));
    assert.deepEqual(JSON.parse(call[1].body),{name:'changed.zip',restore_name:false});
    assert.equal(f.h.nameEditor.isOpen(),false);
    f.get('search-input').value='changed';f.h.applySearchAndRender();
    assert.equal(f.h.mediaVirtualList.items.length,1);
    assert.ok(walk(f.h.mediaVirtualList.nodes.get(0)).some(el=>el.textContent==='folder/changed.zip'));
    await f.h.selectMedia({...file,display_name:'folder/changed.zip'});
    assert.ok(f.calls.some(([url])=>url.includes('/api/archive/list/folder/original.zip')));
    assert.ok(f.calls.every(([url])=>!url.includes('/archive/list/folder/changed.zip')));
});

test('rename errors retain input and library reset discards a pending reply',async()=>{
    const f=fixture();const editor=f.h.nameEditor,file={id:1,rel_path:'old.tar.zst',file_type:'ARCHIVE',encrypted:true};
    editor.open(file);f.get('media-name-input').value='taken.tar.zst';
    f.respond(async()=>new Response('Name already exists',{status:400}));
    await editor.submit();
    assert.equal(editor.isOpen(),true);
    assert.equal(f.get('media-name-input').value,'taken.tar.zst');
    assert.equal(f.get('media-name-message').textContent,'Name already exists');
    f.get('media-name-restore').click();assert.equal(f.get('media-name-input').value,'old.tar.zst');
    let finish;f.respond(async()=>new Promise(resolve=>finish=resolve));
    const pending=editor.submit();await editor.submit();
    const count=f.calls.length;
    f.h.resetLibraryView();f.h.setSession(2);
    finish(new Response('{}'));await pending;
    assert.equal(editor.isOpen(),false);assert.equal(f.calls.length,count);
    assert.equal(f.get('media-name-input').value,'');
});

test('renaming a focused virtual card updates its title without detaching it',()=>{
    const f=fixture();const file={id:1,rel_path:'old.zip',file_type:'ARCHIVE',encrypted:true,tags:[]};
    f.h.setFiles([file]);const card=f.h.mediaVirtualList.nodes.get(0);
    const walk=el=>[el,...el.children.flatMap(walk)];
    walk(card).find(el=>el.className==='card-rename-btn').focus();
    f.h.setFiles([{...file,display_name:'new.zip'}]);
    assert.equal(f.h.mediaVirtualList.nodes.get(0),card);
    assert.equal(walk(card).find(el=>el.className==='media-title').textContent,'new.zip');
});


test('restore filename option defaults on and waits for verified background completion',async()=>{
    const f=fixture(),editor=f.h.nameEditor;
    const file={id:1,rel_path:'original.zip',restore_name:'current.zip',file_type:'ARCHIVE',encrypted:true};
    editor.open(file);assert.equal(f.get('media-name-include-restore').checked,true);
    assert.match(f.get('media-name-original').textContent,/current.zip/);
    f.get('media-name-input').value='new.zip';
    f.respond(async url=>new Response(JSON.stringify(url.includes('/api/files/1/name')?{status:'started',job_id:'test-job'}:url.includes('/api/archive-security')?{job:{job_id:"test-job",running:false,completed:true,phase:'verified',done:100,total:100}}:url.includes('/api/files?')?[]:{})));
    await editor.submit();
    assert.deepEqual(JSON.parse(f.calls[0][1].body),{name:'new.zip',restore_name:true});
    assert.ok(f.calls.some(([url])=>url.includes('/api/archive-security')));
    assert.equal(editor.isOpen(),false);
});

test('restore name job can be cancelled and retains editable input after server failure',async()=>{
    const f=fixture(),editor=f.h.nameEditor;
    editor.open({id:1,rel_path:'old.zip',file_type:'ARCHIVE',encrypted:true});
    f.get('media-name-input').value='new.zip';let finish;
    f.respond(async url=>url.includes('/api/archive-security/progress?')?new Promise(resolve=>finish=resolve):new Response(JSON.stringify({status:'started',job_id:'test-job'})));
    const pending=editor.submit();await new Promise(resolve=>setImmediate(resolve));
    assert.equal(editor.pending,true);assert.equal(f.get('media-name-cancel').hidden,false);
    await f.get('media-name-close').click();assert.equal(editor.isOpen(),false);
    editor.open(editor.file);assert.equal(editor.isOpen(),true);
    await f.get('media-name-cancel').click();
    assert.ok(f.calls.some(([url])=>url.includes('/api/archive-security/cancel')));
    finish(new Response(JSON.stringify({job:{job_id:"test-job",running:false,completed:false,error:'Cancelled; original preserved'}})));
    await pending;
    assert.equal(editor.isOpen(),true);assert.equal(editor.pending,false);
    assert.match(f.get('media-name-message').textContent,/original preserved/);
    assert.equal(f.get('media-name-input').value,'new.zip');
});


test('page deletion chooses an exact image from a spread, blocks shortcuts and reloads on completion',async()=>{
    const f=fixture(),file={id:1,rel_path:'book.zip',file_type:'ARCHIVE',encrypted:true,tags:[]};
    f.h.setFiles([file]);f.h.setCurrent(file);f.h.setArchive(['1.png','2.png','3.png'],0);
    assert.equal(f.get('btn-delete-archive-page').hidden,false);
    const editor=f.h.pageEditor;editor.open(file,['1.png','2.png','3.png'],0,2);
    assert.equal(f.get('archive-page-entry').children.length,3);
    editor.selected=new Set(['2.png']);editor.render();
    const key=keyEvent('Delete');f.h.handleShortcut(key);assert.equal(f.confirmations.length,0);
    f.respond(async url=>new Response(JSON.stringify(url.includes('/1/page')?{status:'started',job_id:'test-job'}:url.includes('/api/archive-security')?{job:{job_id:"test-job",running:false,completed:true}}:url.includes('/api/files?')?[file]:url.includes('/archive/list/')?['1.png','3.png']:[])));
    await editor.submit();
    const call=f.calls.find(([url])=>url.includes('/1/page'));
    assert.equal(call[1].method,'DELETE');assert.deepEqual(JSON.parse(call[1].body),{entries:['2.png']});
    assert.equal(editor.isOpen(),false);assert.ok(f.calls.some(([url])=>url.includes('/archive/list/')));
    assert.ok(f.calls.some(([url])=>url.includes('/bookmarks')));
});

test('page deletion preserves selection on failure, can cancel and ignores stale replies',async()=>{
    const f=fixture(),editor=f.h.pageEditor,file={id:1,rel_path:'book.zip',file_type:'ARCHIVE',encrypted:true};
    editor.open(file,['a.png','b.png'],0);let finish;
    f.respond(async url=>url.includes('/api/archive-security/progress?')?new Promise(resolve=>finish=resolve):new Response(JSON.stringify({status:'started',job_id:'test-job'})));
    const pending=editor.submit();await new Promise(resolve=>setImmediate(resolve));
    await f.get('archive-page-close').click();assert.equal(editor.isOpen(),false);
    editor.open(file,['a.png','b.png'],0);assert.equal(editor.isOpen(),true);
    await f.get('archive-page-cancel').click();assert.ok(f.calls.some(([url])=>url.includes('/cancel')));
    finish(new Response(JSON.stringify({job:{job_id:"test-job",running:false,error:'Cancelled; original preserved'}})));await pending;
    assert.equal(editor.pending,false);assert.equal(editor.isOpen(),true);assert.deepEqual([...editor.selected],['a.png']);
    assert.match(f.get('archive-page-message').textContent,/original preserved/);
    let reply;f.respond(async()=>new Promise(resolve=>reply=resolve));
    const stale=editor.submit();f.h.resetLibraryView();f.h.setSession(2);reply(new Response(JSON.stringify({status:'started',job_id:'test-job'})));await stale;
    assert.equal(editor.isOpen(),false);assert.equal(f.get('archive-page-entry').children.length,0);
});

test('page deletion is unavailable for plain archives and the final image',()=>{
    const f=fixture(),file={id:1,rel_path:'book.zip',file_type:'ARCHIVE'};
    f.h.setCurrent(file);f.h.setArchive(['a.png','b.png'],0);
    assert.equal(f.get('btn-delete-archive-page').hidden,true);
    f.h.pageEditor.open(file,['a.png','b.png'],0);assert.ok(!f.h.pageEditor.isOpen());
    file.encrypted=true;f.h.setArchive(['a.png'],0);assert.equal(f.get('btn-delete-archive-page').disabled,true);
    f.h.pageEditor.open(file,['a.png'],0);assert.ok(!f.h.pageEditor.isOpen());
});


test('encrypted video cards and media encryption actions work without archive page editing',async()=>{
    const f=fixture(),file={id:71,rel_path:'movie.mp4',file_type:'VIDEO',encrypted:true,tags:[],file_size:170000};
    f.h.setFiles([file]);
    const walk=el=>[el,...el.children.flatMap(walk)];
    const card=f.h.mediaVirtualList.nodes.get(0);
    assert.ok(walk(card).some(el=>el.textContent?.includes('🔒 암호화')));
    assert.ok(!walk(card).some(el=>el.className==='card-rename-btn'));
    const s=f.h.archiveSecurity;
    s.files=[{id:71,name:'movie.mp4',file_type:'VIDEO',state:'ready',original:true,size:170000},{id:72,name:'next.webm',file_type:'VIDEO',state:'plain',original:true,size:100}];
    s.selected=new Set([71,72]);s.render();
    assert.deepEqual([...s.eligible('encrypt').map(f=>f.id)],[72]);
    assert.deepEqual([...s.eligible('plaintext').map(f=>f.id)],[71]);
    assert.deepEqual([...s.eligible('export').map(f=>f.id)],[71]);
});

for (const language of ['en','ja']) test(`localized dynamic UI preserves user data (${language})`,async()=>{
    const f=fixture(language),name='완료_シリーズ_01.zip',tag='영상';
    const file={id:19,rel_path:name,file_type:'ARCHIVE',encrypted:true,tags:[{id:1,name:tag}],file_size:100};
    f.h.setFiles([file]);
    const walk=el=>[el,...el.children.flatMap(walk)],nodes=walk(f.h.mediaVirtualList.nodes.get(0));
    assert.ok(nodes.some(el=>el.textContent===name));
    assert.ok(nodes.some(el=>el.textContent?.includes('#'+tag)));
    assert.ok(nodes.some(el=>el.textContent?.includes(language==='en'?'Encrypted':'暗号化')));
    f.h.dbSecurity.update({encrypted:false,locked:false});
    assert.equal(f.get('btn-db-security').textContent,language==='en'?'🔐 Encrypt DB':'🔐 DB暗号化');
});


test('storage reconnect preserves the list, reloads the video at its position and never rescans or replays writes', async()=>{
    const f=fixture('en'); f.h.privacy.covered=false;
    const file={id:20,rel_path:'movie.mp4',file_type:'VIDEO',tags:[],file_size:10};
    f.h.setFiles([file]); f.h.setCurrent(file);
    const video=f.get('main-video'); video.src=f.h.scopedUrl('/api/media/movie.mp4');
    video.currentTime=37;video.duration=100;video.paused=true;
    let loads=0;video.load=()=>{loads++;};
    f.respond(async()=>new Response('{}'));
    await f.h.storageRecovery.reconnect();
    assert.equal(loads,1);assert.equal(f.h.mediaVirtualList.items.length,1);
    video.currentTime=0;video.listeners.loadedmetadata();assert.equal(video.currentTime,37);
    assert.equal(f.calls.length,1);assert.match(f.calls[0][0],/storage\/reconnect/);
    assert.equal(f.calls[0][1].method,'POST');
});

test('storage reconnect failure keeps existing rows and a late reply cannot reload another library',async()=>{
    const f=fixture();f.h.privacy.covered=false;
    f.h.setFiles([{id:1,rel_path:'keep.jpg',file_type:'IMAGE',tags:[],file_size:1}]);
    f.respond(async()=>new Response('NAS unavailable',{status:503}));
    await f.h.storageRecovery.reconnect();
    assert.equal(f.h.mediaVirtualList.items.length,1);
    assert.equal(f.get('storage-status').textContent,'NAS unavailable');
    assert.equal(f.get('btn-reconnect-storage').disabled,false);
    let finish;f.respond(url=>url.includes("/storage/cancel")?Promise.resolve(new Response("{}")):new Promise(resolve=>finish=resolve));
    const pending=f.h.storageRecovery.reconnect();
    await f.h.storageRecovery.reconnect(); // duplicate click is coalesced
    assert.equal(f.calls.length,2);
    f.h.invalidateRequests();f.h.setSession(2);finish(new Response('{}'));await pending;
    assert.equal(f.get('storage-status').hidden,true);
    f.h.privacy.covered=true;await f.h.storageRecovery.reconnect();assert.equal(f.calls.length,3);
});

test('failed read reports storage trouble without retrying a mutation or clearing the list',async()=>{
    const f=fixture('en');f.h.privacy.covered=false;
    f.respond(async()=>new Response('unavailable',{status:503}));
    await assert.rejects(f.h.vaultFetch('/api/files'));
    assert.match(f.get('storage-status').textContent,/Could not read media/);
    assert.equal(f.calls.length,1);
});


test('batch page selection survives pagination, excludes the final image and submits one job',async()=>{
    const f=fixture(),e=f.h.pageEditor,file={id:5,rel_path:'batch.zip',file_type:'ARCHIVE',encrypted:true};
    const names=Array.from({length:100},(_,i)=>`${i+1}.png`);
    e.open(file,names,50);assert.equal(e.page,1);assert.equal(e.boxes.length,48);
    e.boxes[0].checked=true;e.boxes[0].listeners.change();
    f.get('archive-page-next').click();e.boxes[0].checked=true;e.boxes[0].listeners.change();
    assert.deepEqual([...e.selected],['51.png','49.png','97.png']);
    f.get('archive-page-prev').click();assert.equal(e.boxes[0].checked,true);assert.equal(e.boxes[2].checked,true);
    e.selected=new Set(names);e.controls();assert.equal(e.save.disabled,true);await e.submit();assert.equal(f.calls.length,0);
    e.selected=new Set(['1.png','51.png','97.png']);e.controls();
    let changed;e.onChanged=async(...args)=>changed=args;
    f.respond(async url=>new Response(JSON.stringify(url.includes('/5/page')?{status:'started',job_id:'test-job'}:{job:{job_id:"test-job",running:false,completed:true}})));
    await e.submit();assert.deepEqual(JSON.parse(f.calls[0][1].body),{entries:['1.png','51.png','97.png']});
    assert.equal(f.calls.filter(([url])=>url.includes('/5/page')).length,1);
    assert.equal(changed[2],49);assert.equal(e.isOpen(),false);
});


test('DB encryption requires risk confirmation and cancellation preserves the entered password',async()=>{
    const f=fixture('en'),s=f.h.dbSecurity;
    s.riskAck.checked=true;
    s.password.value=s.confirmation.value='valid password 123';
    f.confirm(false);
    f.get('db-security-form').listeners.submit({preventDefault(){}});
    assert.equal(f.calls.length,0);assert.equal(s.pending,false);
    assert.equal(s.password.value,'valid password 123');assert.equal(s.confirmation.value,'valid password 123');
    assert.match(f.confirmations[0],/forget the password/);assert.match(f.confirmations[0],/NAS/);
    s.onChanged=async()=>{};f.respond(async()=>new Response(JSON.stringify({security:{encrypted:true,locked:false,plaintext_original:false}})));
    f.confirm(true);await s.perform('/api/security/encrypt',{password:s.password.value,confirmation:s.confirmation.value});
    assert.equal(f.calls.length,1);assert.match(f.calls[0][0],/security\/encrypt/);
    assert.equal(s.password.value,'');assert.equal(s.confirmation.value,'');
});

for(const language of ['ko','en','ja'])test(`media encryption asks once per batch and cancellation starts no files (${language})`,async()=>{
    const f=fixture(language),s=f.h.archiveSecurity;
    s.files=[{id:1,state:'plain',name:'one.zip'},{id:2,state:'plain',name:'two.mp4'},{id:3,state:'ready',name:'ready.zip'}];
    s.selected=new Set([1,2,3]);f.confirm(false);
    f.get('media-encryption-risk-ack').checked=true;
    await s.batch();assert.equal(f.calls.length,0);assert.equal(s.pending,false);assert.equal(s.selected.size,3);
    assert.match(f.confirmations[0],/vault.key/);assert.match(f.confirmations[0],/vtbackup/);assert.match(f.confirmations[0],/2/);
    if(language!=='ko')assert.ok(!/[가-힣]/.test(f.confirmations[0]));
    s.waitJob=async()=>({completed:true});s.refresh=async()=>{};s.onChanged=async()=>{};
    f.respond(async()=>new Response('{"status":"started","job_id":"test-job"}'));
    f.get('media-encryption-risk-ack').checked=true;
    f.confirm(true);await s.batch();assert.equal(f.confirmations.length,2);
    assert.equal(f.calls.length,2);assert.ok(f.calls.every(([url])=>url.includes('/encrypt')));
    f.confirm(false);await s.single(s.files[0],'encrypt');assert.equal(f.calls.length,2);
});


test('reconnect cancel aborts HTTP, uses matching attempt ID and ignores late success',async()=>{
    const f=fixture('en');f.h.privacy.covered=false;
    let finish, recovered=0;f.h.storageRecovery.onRecovered=async()=>{recovered++;};
    f.respond(url=>url.includes('/storage/cancel')?Promise.resolve(new Response('{}')):new Promise(resolve=>finish=resolve));
    const first=f.h.storageRecovery.reconnect();
    assert.equal(f.get('btn-cancel-reconnect').hidden,false);
    f.get('btn-cancel-reconnect').click();
    assert.equal(f.calls[0][1].signal.aborted,true);
    assert.equal(f.calls[0][1].headers.get('X-Vault-Reconnect'),f.calls[1][1].headers.get('X-Vault-Reconnect'));
    assert.equal(f.h.storageRecovery.pending,false);
    assert.match(f.get('storage-status').textContent,/Stopped waiting/);
    const oldFinish=finish;
    const second=f.h.storageRecovery.reconnect();
    oldFinish(new Response('{}'));await first;
    assert.equal(f.h.storageRecovery.pending,true,'old finally must not unlock new attempt');
    assert.equal(recovered,0);
    finish(new Response('{}'));await second;assert.equal(recovered,1);
});

test('reconnect HTTP watchdog releases controls without clearing the list',async()=>{
    const f=fixture('en');f.h.privacy.covered=false;
    f.h.setFiles([{id:1,rel_path:'keep.jpg',file_type:'IMAGE',tags:[]}]);
    let finish;
    f.respond(url=>url.includes('/storage/cancel')?Promise.resolve(new Response('{}')):new Promise(resolve=>finish=resolve));
    const pending=f.h.storageRecovery.reconnect();
    [...f.timers.values()].find(t=>t.ms===20000).fn();
    assert.equal(f.h.storageRecovery.pending,false);
    assert.equal(f.get('btn-cancel-reconnect').hidden,true);
    assert.equal(f.h.mediaVirtualList.items.length,1);
    assert.match(f.get('storage-status').textContent,/No reconnection response/);
    finish(new Response('{}'));await pending;
    assert.match(f.get('storage-status').textContent,/No reconnection response/);
});


test('library ownership error explains other-PC lock without exposing server paths',async()=>{
    const f=fixture('en'),p=f.h.privacy;await f.h.adoptLibraryState({generation:1,covered:true});
    f.respond(async()=>new Response(JSON.stringify({folders:[{name:'A',path:'D:/A'}],active_folder:'D:/A'})));
    for(let i=0;i<5;i++)await f.get('privacy-library-title').click();
    p.onResync=async()=>{};p.password.value='test password';
    f.respond(async()=>new Response('private NAS path',{status:423}));await p.reveal();
    assert.match(p.message.textContent,/another app or PC/);
    assert.ok(!p.message.textContent.includes('private NAS path'));
    assert.equal(p.password.value,'');assert.equal(p.covered,true);
});


async function nextEncryptionPoll(f){
    await new Promise(setImmediate);
    const entry=[...f.timers.entries()].find(([,t])=>t.ms===400);
    assert.ok(entry,'waiting for next progress poll');
    f.timers.delete(entry[0]);entry[1].fn();await new Promise(setImmediate);
}
for(const language of ['ko','en','ja'])test('encryption batch reports overall completions and current phase in '+language,async()=>{
    const f=fixture(language),s=f.h.archiveSecurity;s.onChanged=async()=>{};
    const mb=1048576;
    const files=[{id:1,name:'book.zip',display_name:'<private_日本語>.zip',state:'plain'},
        {id:2,name:'영상.mp4',state:'plain',file_type:'VIDEO'}];
    s.files=files;s.selected=new Set([1,2]);
    const jobs=[
        {job_id:"test-job",running:true,phase:'원본 검사',done:50*mb,total:100*mb},
        {job_id:"test-job",running:true,phase:'원본 검사',done:100*mb,total:100*mb},
        {job_id:"test-job",running:true,phase:'암호화',done:25*mb,total:100*mb},
        {job_id:"test-job",running:true,phase:'사본 검증',done:50*mb,total:100*mb},
        {job_id:"test-job",running:false,completed:true},
        {job_id:"test-job",running:true,phase:'준비',done:0,total:0},
        {job_id:"test-job",running:true,phase:'해시·암호화',done:75*mb,total:100*mb},
        {job_id:"test-job",running:false,completed:true}
    ];
    f.respond(async url=>new Response(JSON.stringify(url.includes('/encrypt')?{status:'started',job_id:'test-job'}:
        url.includes('/progress')?{job:jobs.shift()}:{files,job:{job_id:"test-job",running:false,completed:true}})));
    f.get('media-encryption-risk-ack').checked=true;
    const work=s.batch();await new Promise(setImmediate);
    assert.equal(s.progress.max,2);assert.equal(s.progress.value,0);assert.equal(s.fileProgress.value,.5);
    assert.ok(s.currentFileLabel.textContent.includes('<private_日本語>.zip'));
    assert.ok(s.fileProgressLabel.textContent.includes('50%'));assert.ok(s.fileProgressLabel.textContent.includes('50.0 MB'));
    if(language!=='ko')assert.ok(!/[가-힣]/.test(s.progressLabel.textContent+s.fileProgressLabel.textContent));
    await nextEncryptionPoll(f);assert.equal(s.fileProgress.value,1);assert.equal(s.progress.value,0,'hash completion is not file completion');
    await nextEncryptionPoll(f);assert.equal(s.fileProgress.value,.25);assert.equal(s.progress.value,0);
    await nextEncryptionPoll(f);assert.equal(s.fileProgress.value,.5);
    await nextEncryptionPoll(f);assert.equal(s.progress.value,1);assert.equal(s.progress.max,2);
    assert.ok(s.progressLabel.textContent.includes('50%'));assert.ok(s.currentFileLabel.textContent.includes('영상.mp4'));
    assert.equal(s.fileProgress.value,'','unknown totals use an indeterminate bar');
    await nextEncryptionPoll(f);assert.equal(s.fileProgress.value,.75);assert.equal(s.progress.value,1);
    await nextEncryptionPoll(f);await work;
    assert.equal(s.progress.value,2);assert.ok(s.progressLabel.textContent.includes('100%'));assert.equal(s.pending,false);
    assert.equal(f.calls.filter(([url])=>new URL(url,'http://localhost').pathname==='/api/archive-security').length,1,'full listing only refreshed on completion');
    s.reset();assert.equal(s.progressLabel.textContent,'');assert.equal(s.currentFileLabel.textContent,'');assert.equal(s.fileProgressSection.hidden,true);
});

for(const cancelled of [false,true])test('partial encryption '+(cancelled?'cancellation':'failure')+' retains completed count without claiming 100 percent',async()=>{
    const f=fixture(),s=f.h.archiveSecurity;s.onChanged=async()=>{};
    s.files=[1,2,3].map(id=>({id,name:id+'.zip',state:'plain'}));s.selected=new Set([1,2,3]);
    const jobs=[{job_id:"test-job",running:false,completed:true},{job_id:"test-job",running:true,phase:'사본 검증',done:30,total:100},
        {job_id:"test-job",running:false,error:cancelled?'작업이 취소되었습니다.':'disk full',phase:'사본 검증',done:30,total:100}];
    f.respond(async url=>new Response(JSON.stringify(url.includes('/encrypt')||url.includes('/cancel')?{status:'started',job_id:'test-job'}:
        url.includes('/progress')?{job:jobs.shift()}:{files:s.files})));
    f.get('media-encryption-risk-ack').checked=true;
    const work=s.batch();await new Promise(setImmediate);
    assert.equal(s.progress.value,1);assert.equal(s.progress.max,3);
    if(cancelled)await s.cancel.click();
    await nextEncryptionPoll(f);await work;
    assert.equal(s.progress.value,1);assert.equal(s.progress.max,3);assert.ok(s.progressLabel.textContent.includes('33%'));
    assert.ok(s.status.textContent.includes('완료 1/3개'));assert.deepEqual([...s.selected],[2,3]);
    assert.equal(f.calls.filter(([url])=>url.includes('/encrypt')).length,2);
});

test('encryption progress is erased on library change and delayed responses cannot restore it',async()=>{
    const f=fixture(),s=f.h.archiveSecurity;let finish;
    s.files=[{id:1,name:'private.zip',state:'plain'}];s.selected=new Set([1]);
    f.respond(async url=>url.includes('/encrypt')?new Response('{"status":"started","job_id":"test-job"}'):new Promise(resolve=>finish=resolve));
    f.get('media-encryption-risk-ack').checked=true;
    const work=s.batch();await new Promise(setImmediate);
    assert.ok(s.currentFileLabel.textContent.includes('private.zip'));
    f.h.resetLibraryView();f.h.setSession(2);
    finish(new Response('{"job":{"running":false,"completed":true}}'));await work;
    assert.equal(s.currentFileLabel.textContent,'');assert.equal(s.progressLabel.textContent,'');assert.equal(s.fileProgressSection.hidden,true);
});

test('images use scoped thumbnails, filtered image navigation and series assignment', async () => {
    const f=fixture(); const files=[
        {id:1,rel_path:'one.png',file_type:'IMAGE',tags:[]},
        {id:2,rel_path:'clip.mp4',file_type:'VIDEO',tags:[]},
        {id:3,rel_path:'two.avif',file_type:'IMAGE',tags:[]},
        {id:4,rel_path:'book.pdf',file_type:'PDF',tags:[]}];
    f.h.setFiles(files); await f.h.selectMedia(files[0]);
    const card=f.h.mediaVirtualList.nodes.get(0);
    assert.match(card.children[0].children[0].children[0].src,/api\/media\/one.png.*library=1/);
    assert.ok(card.children[0].children[1].children[0].children.some(el=>el.className==='card-series-btn'));
    f.h.handleShortcut(keyEvent('ArrowRight'));
    assert.match(f.get('viewer-image').src,/two.avif/);
    f.h.handleShortcut(keyEvent('ArrowLeft'));
    assert.match(f.get('viewer-image').src,/one.png/);
    f.get('media-type-filter').value='PDF'; f.get('media-type-filter').listeners.change();
    assert.deepEqual([...f.h.getFilteredFiles()].map(x=>x.id),[4]);
});

async function bookmarkRecoveryFixture(kind) {
    const f=fixture('en');
    if(kind==='pdf') { fakePdf(f); await f.h.pdfViewer.open({id:1,rel_path:'sample.pdf',thumbnail_data:'existing'},'/sample.pdf'); }
    else {
        f.h.setCurrent({id:1,file_type:kind==='video'?'VIDEO':'ARCHIVE',rel_path:kind==='video'?'sample.mp4':'sample.zip'});
        if(kind==='video')Object.assign(f.get('main-video'),{currentTime:42,readyState:2,videoWidth:640,videoHeight:360});
        else f.h.setArchive(['a.png','b.png'],0);
    }
    return {f,load:()=>kind==='pdf'?f.h.pdfViewer.loadBookmarks():f.h.loadBookmarks(),
        save:()=>kind==='pdf'?f.h.pdfViewer.saveBookmark():f.h.saveBookmark(),
        strip:f.get(kind+'-bookmarks'),status:f.get(kind+'-bookmark-status'),retry:f.get(kind+'-bookmark-retry')};
}
for (const spread of [false, true]) test('archive bookmark decode timeout releases controls and ignores late completion, spread=' + spread, async () => {
    const {f, save, load, status, strip} = await bookmarkRecoveryFixture('image');
    f.respond(async (url, options) => new Response(options.method === 'POST' ? '{}' : '[{"id":3,"position":0,"thumbnail_url":"/old"}]'));
    if (spread) {
        for (const entry of ['a.png', 'b.png']) f.h.cacheImage('sample.zip', entry, {isPortrait:true});
        await f.h.showArchiveImage('sample.zip', 0);
    }
    await load();
    const image = f.get(spread ? 'viewer-image-2' : 'viewer-image');
    let finishOld;
    image.decode = () => new Promise(resolve => { finishOld = resolve; });
    const work = save();
    await new Promise(setImmediate);
    assert.equal(f.get('btn-save-image-bookmark').disabled, true);
    assert.equal(strip.children[0].children[1].disabled, true);
    const [id, timer] = [...f.timers].find(([, t]) => t.ms === 10000);
    f.timers.delete(id); timer.fn();
    await work;
    assert.match(status.textContent, /Preparing the image is taking too long/);
    assert.equal(f.get('btn-save-image-bookmark').disabled, false);
    assert.equal(strip.children[0].children[1].disabled, false);
    const writes = () => f.calls.filter(([, options]) => options.method === 'POST').length;
    assert.equal(writes(), 0);
    let finishNew;
    image.decode = () => new Promise(resolve => { finishNew = resolve; });
    const retry = save();
    finishOld();
    await new Promise(setImmediate);
    assert.equal(writes(), 0);
    assert.equal(f.get('btn-save-image-bookmark').disabled, true);
    finishNew();
    await retry;
    assert.equal(writes(), 1);
    assert.equal(f.get('btn-save-image-bookmark').disabled, false);
    assert.equal([...f.timers.values()].filter(t => t.ms === 10000).length, 0);
});

for (const action of ['page', 'media', 'library', 'invalidate', 'reset']) test('archive bookmark preparation cancels on ' + action, async () => {
    const {f, save, status} = await bookmarkRecoveryFixture('image');
    f.respond(async () => new Response('[]'));
    let finishOld;
    f.get('viewer-image').decode = () => new Promise(resolve => { finishOld = resolve; });
    const old = save();
    if (action === 'page') {
        f.h.cacheImage('sample.zip', 'b.png', {isPortrait:false});
        await f.h.showArchiveImage('sample.zip', 1);
    } else if (action === 'media') await f.h.selectMedia({id:2, file_type:'VIDEO', rel_path:'other.mp4'});
    else if (action === 'library') f.h.resetLibraryView();
    else if (action === 'invalidate') f.h.invalidateRequests();
    else f.h.resetBookmarks();
    let next, finishNext;
    if (action === 'reset') {
        // Start a new save before the cancelled save's continuation can run.
        f.get('viewer-image').decode = () => new Promise(resolve => { finishNext = resolve; });
        next = save();
    }
    await old;
    assert.equal(f.calls.filter(([, options]) => options.method === 'POST').length, 0);
    assert.equal(f.get('btn-save-image-bookmark').disabled, action === 'reset');
    if (action === 'page') assert.match(status.textContent, /page changed/i);
    const text = status.textContent;
    finishOld();
    await new Promise(setImmediate);
    assert.equal(status.textContent, text);
    assert.equal(f.calls.filter(([, options]) => options.method === 'POST').length, 0);
    if (next) { finishNext(); await next; }
    assert.equal([...f.timers.values()].filter(t => t.ms === 10000).length, 0);
});

test('archive bookmark decode failure clears its deadline and permits another save', async () => {
    const {f, save, status} = await bookmarkRecoveryFixture('image');
    f.get('viewer-image').decode = () => Promise.reject(new Error('Invalid image'));
    await save();
    assert.match(status.textContent, /Invalid image/);
    assert.equal(f.get('btn-save-image-bookmark').disabled, false);
    assert.equal([...f.timers.values()].filter(t => t.ms === 10000).length, 0);
    f.get('viewer-image').decode = () => Promise.resolve();
    f.respond(async (url, options) => new Response(options.method === 'POST' ? '{}' : '[]'));
    await save();
    assert.equal(f.calls.filter(([, options]) => options.method === 'POST').length, 1);
});

for (const outcome of ['remapped', 'other-request', 'legacy', 'removed']) test('archive bookmark receipt after a lost response: ' + outcome, async () => {
    const {f, save, load, status, strip, retry} = await bookmarkRecoveryFixture('image');
    f.h.setCurrent({id:1, file_type:'ARCHIVE', encrypted:true, rel_path:'sample.zip'});
    f.h.setArchive(['a.png', 'b.png'], 1);
    let stored;
    f.respond(async (url, options) => {
        if (options.method === 'POST') {
            stored = {id:3, ...JSON.parse(options.body), thumbnail_url:'/saved'};
            throw new TypeError('Lost save acknowledgement');
        }
        return new Response('offline', {status:503});
    });
    const work = save();
    await new Promise(setImmediate);
    f.advance(10001);
    const [id, timer] = [...f.timers].find(([, t]) => t.ms > 9000 && t.ms <= 10000);
    f.timers.delete(id); timer.fn();
    await work;
    assert.equal(stored.position, 1);
    assert.equal(retry.hidden, false);
    assert.equal(f.get('btn-save-image-bookmark').disabled, true);
    if (outcome === 'remapped') {
        // Deleting the preceding image remaps the surviving bookmark, preserving its receipt.
        stored.position = 0;
        f.h.setArchive(['b.png'], 0);
    } else if (outcome === 'other-request') stored.request_id = '0'.repeat(32);
    else if (outcome === 'legacy') delete stored.request_id;
    f.h.resetBookmarks();
    const records = outcome === 'removed' ? [] : [stored];
    f.respond(async () => new Response(JSON.stringify(records)));
    await load();
    assert.match(status.textContent, outcome === 'remapped' ? /confirmed/ : /not applied/);
    assert.equal(strip.children.length, records.length);
    if (outcome === 'remapped') assert.equal(strip.children[0].children[0].children[1].textContent, 'Page 1');
    assert.equal(retry.hidden, true);
    assert.equal(f.get('btn-save-image-bookmark').disabled, false);
    assert.equal(f.calls.filter(([, options]) => options.method === 'POST').length, 1);
});

for(const kind of ['video','image','pdf']) {
    for(const operation of ['save','delete'])for(const failure of ['lost','stalled'])for(const committed of [false,true])
    test(`${kind} bookmark ${operation}: ${failure} acknowledgement, committed=${committed}`,async()=>{
        const {f,load,save,strip,status}=await bookmarkRecoveryFixture(kind);
        let records=[{id:3,position:0,thumbnail_url:'/old',request_id:'old'}],late;
        f.respond(async()=>new Response(JSON.stringify(records)));await load();
        const remove=strip.children[0].children[1],mutate=()=>operation==='save'?save():kind==='pdf'?remove.click():remove.onclick();
        f.respond(async(url,options)=>{
            if(['POST','DELETE'].includes(options.method)){
                assert.ok(Number(new Headers(options.headers).get('X-Vault-Write-Deadline'))>Date.now());
                if(committed)records=operation==='delete'?[]:[{id:3,...JSON.parse(options.body),thumbnail_url:'/saved'}];
                if(failure==='lost')throw new TypeError('Lost acknowledgement');
                return new Promise(resolve=>late=resolve);
            }
            return new Response(JSON.stringify(records));
        });
        const work=mutate();await new Promise(setImmediate);
        const writes=()=>f.calls.filter(([,o])=>['POST','DELETE'].includes(o.method)).length;
        const count=writes();await mutate();assert.equal(writes(),count,'uncertain writes cannot be repeated');
        f.advance(10001);
        const timer=[...f.timers].find(([,t])=>t.ms>9000&&t.ms<=10000);assert.ok(timer);
        f.timers.delete(timer[0]);timer[1].fn();await work;
        assert.equal(writes(),count);assert.equal(strip.children.length,records.length);
        assert.match(status.textContent,committed?(operation==='delete'?/deleted/:/confirmed/):/not applied/);
        assert.equal(f.get(kind==='pdf'?'pdf-save':'btn-save-'+kind+'-bookmark').disabled,false);
        if(late){const result=status.textContent;late(new Response('{}'));await new Promise(setImmediate);assert.equal(status.textContent,result);}
    });
    test(kind+' uncertain bookmark write survives reselection and blocks writes until a successful read',async()=>{
        const {f,load,save,strip,status,retry}=await bookmarkRecoveryFixture(kind);let receipt;
        f.respond(async(url,options)=>{
            if(options.method==='POST'){receipt=JSON.parse(options.body);throw new TypeError('Lost');}
            return new Response('offline',{status:503});
        });
        const work=save();await new Promise(setImmediate);f.advance(10001);
        const [id,timer]=[...f.timers].find(([,t])=>t.ms>9000&&t.ms<=10000);f.timers.delete(id);timer.fn();await work;
        assert.equal(retry.hidden,false);assert.equal(f.get(kind==='pdf'?'pdf-save':'btn-save-'+kind+'-bookmark').disabled,true);
        const writes=()=>f.calls.filter(([,o])=>o.method==='POST').length,before=writes();await save();assert.equal(writes(),before);
        f.respond(async url=>new Response(url.includes('/bookmarks')?JSON.stringify([{id:3,...receipt,thumbnail_url:'/saved'}]):'{"page":1,"revision":0}'));
        if(kind==='pdf')await f.h.pdfViewer.open({id:1,rel_path:'sample.pdf',thumbnail_data:'existing'},'/sample.pdf');
        else {f.h.resetBookmarks();await load();}
        assert.equal(writes(),before);assert.equal(strip.children.length,1);assert.equal(retry.hidden,true);assert.match(status.textContent,/confirmed/);
        assert.equal(f.get(kind==='pdf'?'pdf-save':'btn-save-'+kind+'-bookmark').disabled,false);
    });
    test(kind+' delete 404 reconciles an absent bookmark without resending DELETE',async()=>{
        const {f,load,strip,status}=await bookmarkRecoveryFixture(kind);
        f.respond(async()=>new Response('[{"id":3,"position":0,"thumbnail_url":"/old"}]'));await load();
        f.respond(async(url,options)=>options.method==='DELETE'?new Response('',{status:404}):new Response('[]'));
        const remove=strip.children[0].children[1];await (kind==='pdf'?remove.click():remove.onclick());
        assert.equal(strip.children.length,0);assert.match(status.textContent,/deleted/);
        assert.equal(f.calls.filter(([,o])=>o.method==='DELETE').length,1);
    });
    test(kind+' reselection waits for an in-flight bookmark acknowledgement before showing its list',async()=>{
        const {f,load,save,strip,status}=await bookmarkRecoveryFixture(kind);let finish,records=[];
        f.respond(async(url,options)=>{
            if(options.method==='POST')return new Promise(resolve=>{finish=()=>{records=[{id:3,...JSON.parse(options.body),thumbnail_url:'/saved'}];resolve(new Response('{}'));};});
            return new Response(url.includes('/bookmarks')?JSON.stringify(records):'{"page":1,"revision":0}');
        });
        const writing=save();await new Promise(setImmediate);
        let reading;
        if(kind==='pdf')reading=f.h.pdfViewer.open({id:1,rel_path:'sample.pdf',thumbnail_data:'existing'},'/sample.pdf');
        else {f.h.bumpSelection();f.h.resetBookmarks();reading=load();}
        await new Promise(setImmediate);
        assert.equal(f.get(kind==='pdf'?'pdf-save':'btn-save-'+kind+'-bookmark').disabled,true);
        finish();await Promise.all([writing,reading]);
        assert.equal(strip.children.length,1);assert.ok(!status.textContent.includes('Checking'));
        assert.equal(f.get(kind==='pdf'?'pdf-save':'btn-save-'+kind+'-bookmark').disabled,false);
        assert.equal(f.calls.filter(([,o])=>o.method==='POST').length,1);
    });
    test(kind+' bookmark refresh can retry without repeating a successful save or deletion',async()=>{
        const {f,load,save,strip,status,retry}=await bookmarkRecoveryFixture(kind);
        const records=[{id:3,position:0,thumbnail_url:'/thumb'}];
        f.respond(async(_url,options)=>new Response(options.method?'{}':'offline',{status:options.method?200:503}));
        await save();assert.match(status.textContent,/saved/i);assert.match(status.textContent,/Could not load bookmarks/);
        assert.equal(retry.hidden,false);assert.equal(retry.disabled,false);
        const writes=()=>f.calls.filter(([,o])=>o.method==='POST').length, before=writes();
        f.respond(async()=>new Response(JSON.stringify(records)));await retry.click();
        assert.equal(writes(),before);assert.equal(strip.children.length,1);assert.equal(retry.hidden,true);
        const remove=strip.children[0].children[1];
        f.respond(async(_url,options)=>new Response(options.method==='DELETE'?'{}':'offline',{status:options.method==='DELETE'?200:503}));
        await (kind==='pdf'?remove.click():remove.onclick());
        assert.equal(strip.children.length,0,'a confirmed deletion must disappear despite refresh failure');
        assert.match(status.textContent,/deleted/);assert.match(status.textContent,/Could not load bookmarks/);assert.equal(retry.hidden,false);
        f.respond(async()=>new Response('[]'));await retry.click();assert.equal(retry.hidden,true);
        assert.equal(f.calls.filter(([,o])=>o.method==='DELETE').length,1);
    });
    for(const bodyStall of [false,true])test(kind+' bookmark list bounds a stalled '+(bodyStall?'body':'response')+' and ignores it after retry',async()=>{
        const {f,load,strip,retry}=await bookmarkRecoveryFixture(kind);let finish;
        f.respond(async()=>bodyStall?{ok:true,json:()=>new Promise(resolve=>finish=resolve)}:new Promise(resolve=>finish=resolve));
        const pending=load();await new Promise(setImmediate);
        const [id,timer]=[...f.timers].find(([,t])=>t.ms===10000);f.timers.delete(id);timer.fn();await pending;
        assert.equal(retry.hidden,false);assert.equal(retry.disabled,false);
        f.respond(async()=>new Response('[{"id":2,"position":1,"thumbnail_url":"/new"}]'));await retry.click();
        finish(bodyStall?[]:new Response('[]'));await new Promise(setImmediate);
        assert.equal(strip.children.length,1);assert.equal(retry.hidden,true);
        assert.equal(f.h.requestCount(),0);
    });
}

for(const stalled of ['encoding','upload'])test('PDF automatic thumbnail '+stalled+' cannot block opening/bookmarks and has a bounded lifetime',async()=>{
    const f=fixture('en');fakePdf(f);const v=f.h.pdfViewer;let finish,updates=0;
    v.onThumbnail=()=>updates++;
    const original=v.shot.bind(v);
    v.shot=()=>{const canvas=original();if(canvas)canvas.toBlob=callback=>{if(stalled==='encoding')finish=callback;else callback(new Blob(['synthetic']));};return canvas;};
    f.respond(async(url,options)=>{
        if(url.includes('/api/thumbnail/')&&options.method==='POST')return new Promise(resolve=>finish=resolve);
        return new Response(url.includes('/bookmarks')?(options.method?'{}':'[]'):'{"page":1,"revision":0}');
    });
    await v.open({id:1,rel_path:'sample.pdf'},'/sample.pdf');
    assert.equal(v.ready,true);assert.equal(f.get('pdf-save').disabled,false);
    assert.ok(f.calls.some(([u])=>u.includes('/bookmarks')));await v.saveBookmark();
    assert.equal(f.get('pdf-save').disabled,false);
    const [id,timer]=[...f.timers].find(([,t])=>t.ms===15000);f.timers.delete(id);timer.fn();await new Promise(setImmediate);
    assert.equal(v.thumbnailJob,null);assert.equal(f.get('pdf-thumbnail').disabled,false);
    finish(stalled==='encoding'?new Blob(['late']):new Response('{}'));await new Promise(setImmediate);assert.equal(updates,0);
    const uploads=f.calls.filter(([u,o])=>u.includes('/api/thumbnail/')&&o.method==='POST');
    assert.equal(uploads.length,stalled==='encoding'?0:1);
    if(uploads.length)assert.ok(Number(new Headers(uploads[0][1].headers).get('X-Vault-Write-Deadline'))>0);
    v.shot=()=>{const canvas=original();if(canvas)canvas.toBlob=callback=>callback(new Blob(['retry']));return canvas;};
    f.respond(async()=>new Response('{}'));await v.captureThumbnail();assert.equal(updates,1);
});

test('PDF thumbnail and bookmark reads are cancelled on selection change without touching the new controls',async()=>{
    const f=fixture('en');fakePdf(f);const v=f.h.pdfViewer;let finish;const original=v.shot.bind(v);
    v.shot=()=>{const canvas=original();if(canvas)canvas.toBlob=callback=>callback(new Blob(['synthetic']));return canvas;};
    f.respond(async(url)=>url.includes('/api/thumbnail/')?new Promise(resolve=>finish=resolve):new Response(url.includes('/bookmarks')?'[]':'{"page":1,"revision":0}'));
    await v.open({id:1,rel_path:'a.pdf'},'/a.pdf');
    const upload=f.calls.find(([url])=>url.includes('/api/thumbnail/'));
    f.respond(async()=>new Promise(()=>{}));const bookmarks=v.loadBookmarks();await new Promise(setImmediate);
    v.reset(true);await bookmarks;assert.equal(upload[1].signal.aborted,true);
    assert.equal(v.thumbnailJob,null);assert.equal(f.get('pdf-bookmark-retry').hidden,true);
    finish(new Response('{}'));await new Promise(setImmediate);
    assert.equal(f.get('pdf-thumbnail').disabled,true);assert.equal(f.get('pdf-status').textContent,'');
    assert.equal([...f.timers.values()].filter(t=>[10000,15000].includes(t.ms)).length,0);
});

function fakePdf(f,{numPages=6, getPage, promise}={}) {
    const state={destroyed:0,parameters:null,renders:[]};
    const document={numPages, getPage:getPage|| (async n => ({
        getViewport:({scale})=>({width:600*scale,height:800*scale}),
        render:options=>{state.renders.push({page:n,...options});return {promise:Promise.resolve(),cancel(){}};}
    }))};
    f.h.pdfViewer.load=async()=>({PasswordResponses:{INCORRECT_PASSWORD:2},getDocument:parameters=>{
        state.parameters=parameters;
        return state.loading={promise:promise||Promise.resolve(document),destroy(){state.destroyed++;return Promise.resolve();}};
    }});
    f.respond(async (url,options)=>new Response(url.includes('/bookmarks') && !options?.method?'[]':'{}'));
    return {...state, get parameters(){return state.parameters;},get destroyed(){return state.destroyed;}, get loading(){return state.loading;},document};
}

test('PDF renders pages, counts five distinct pages, saves zero-based bookmarks and supports shortcuts',async()=>{
    const f=fixture(),pdf=fakePdf(f);
    const file={id:1,rel_path:'book.pdf',file_type:'PDF',thumbnail_data:'/api/thumbnail/1',tags:[]};
    await f.h.selectMedia(file);
    assert.match(pdf.parameters.url,/api\/media\/book.pdf.*library=1/);
    assert.equal(pdf.parameters.enableXfa,false);assert.equal(pdf.parameters.isEvalSupported,false);
    assert.equal(pdf.parameters.disableAutoFetch,true);
    assert.equal(f.get('pdf-viewer-stage').style.display,'flex');
    assert.equal(f.get('image-viewer-stage').style.display,'none');
    await f.h.pdfViewer.show(2); await f.h.pdfViewer.show(2);
    await f.h.pdfViewer.show(3); await f.h.pdfViewer.show(4);
    assert.equal(f.calls.filter(([u])=>/\/watch(?:\?|$)/.test(u)).length,0);
    await f.h.pdfViewer.show(5);
    assert.equal(f.calls.filter(([u])=>/\/watch(?:\?|$)/.test(u)).length,1);
    await f.h.pdfViewer.saveBookmark();
    const saved=f.calls.find(([u,o])=>u.includes('/bookmarks')&&o?.method==='POST');
    assert.equal(JSON.parse(saved[1].body).position,4);
    f.h.handleShortcut(keyEvent('End'));await new Promise(r=>setImmediate(r));
    assert.equal(f.h.pdfViewer.page,6);
    f.h.handleShortcut(keyEvent('Home'));await new Promise(r=>setImmediate(r));
    assert.equal(f.h.pdfViewer.page,1);
    f.h.resetLibraryView();assert.equal(pdf.destroyed,1);
    assert.equal(f.get('pdf-pages').children.length,0);
    assert.equal(f.get('pdf-title').textContent,'');
});

test('late PDF document and page responses cannot replace a newer selection',async()=>{
    const f=fixture();let finishOld;
    fakePdf(f,{promise:new Promise(resolve=>finishOld=resolve)});
    const old=f.h.selectMedia({id:1,rel_path:'old.pdf',file_type:'PDF',tags:[],thumbnail_data:'saved'});
    await new Promise(r=>setImmediate(r));
    const newer=fakePdf(f,{numPages:3});
    await f.h.selectMedia({id:2,rel_path:'new.pdf',file_type:'PDF',tags:[],thumbnail_data:'saved'});
    finishOld({numPages:10});await old;
    assert.equal(f.h.pdfViewer.doc,newer.document);assert.equal(f.get('pdf-title').textContent,'new.pdf');
    let finishPage;const doc=f.h.pdfViewer.doc,original=doc.getPage;
    doc.getPage=n=>n===2?new Promise(resolve=>finishPage=resolve):original(n);
    const pending=f.h.pdfViewer.show(2);await f.h.pdfViewer.show(3);
    finishPage(await original(2));await pending;
    assert.equal(f.h.pdfViewer.page,3);
    assert.equal(f.get('pdf-page').value,'3');
});

test('PDF password prompts clear secrets on switching files and errors allow another document',async()=>{
    const f=fixture('en');let fail;
    const pdf=fakePdf(f,{promise:new Promise((_,reject)=>fail=reject)});
    const opening=f.h.selectMedia({id:1,rel_path:'locked.pdf',file_type:'PDF',tags:[],thumbnail_data:'saved'});
    await new Promise(r=>setImmediate(r));
    let password;pdf.loading.onPassword(value=>password=value,2);
    f.get('pdf-password').value='example';
    f.get('pdf-password-form').listeners.submit({preventDefault(){}});
    assert.equal(password,'example');assert.equal(f.get('pdf-password').value,'');
    fail(new Error('private path'));await opening;
    assert.match(f.get('pdf-status').textContent,/Cannot open PDF/);
    assert.ok(!f.get('pdf-status').textContent.includes('private path'));
    await f.h.selectMedia({id:2,rel_path:'image.png',file_type:'IMAGE',tags:[]});
    assert.equal(f.get('pdf-password-form').hidden,true);assert.equal(f.h.pdfViewer.passwordCallback,null);
    assert.equal(f.get('pdf-viewer-stage').style.display,'none');
});



function noteFixture() {
    const f=fixture();f.respond(async()=>new Response(JSON.stringify({revision:'rev-1',title:'Title',body:'  sample\r\n日本語 🔐  '})));
    return {...f,notes:f.h.noteEditor,file:{id:73,rel_path:'notes/test.json',display_name:'Title',file_type:'NOTE',encrypted:true,tags:[]}};
}
test('standalone note opens directly as canvas, and resets all content when locked',async()=>{
    const f=noteFixture();const body='  0O 1Il\t日本語 🔐\r\n'.repeat(4000);
    f.respond(async()=>new Response(JSON.stringify({revision:'rev-1',title:'Title',body})));
    await f.notes.open(f.file);assert.ok(f.calls[0][0].includes('/api/notes/73'));
    assert.equal(f.notes.text,body);assert.equal(f.get('notes-body').value,'');assert.ok(f.get('notes-canvas').height<1000);
    f.h.resetLibraryView();assert.equal(f.notes.text,undefined);assert.equal(f.get('notes-canvas').height,1);assert.equal(f.notes.isOpen(),false);
});
test('new note needs no photo, confirms encryption and preserves exact draft on failed save',async()=>{
    const f=noteFixture();f.notes.openNew();assert.equal(f.calls.length,0);assert.equal(f.get('notes-editor').hidden,false);
    const body='  \tline one\r\n日本語 🔐\n  ';f.get('notes-title').value='Note title';f.get('notes-body').value=body;
    f.confirm(false);await f.notes.submit();assert.equal(f.calls.length,0);
    f.confirm(true);let payload;f.respond(async(url,opts)=>{payload=JSON.parse(opts.body);return new Response('save failure',{status:500});});
    await f.notes.submit();assert.equal(payload.body,body);assert.equal(payload.revision,null);assert.equal(f.get('notes-body').value,body);
    assert.equal(f.notes.pending,false);assert.equal(f.get('notes-setup').hidden,true);assert.ok(f.notes.saveAttempt);
});
test('late note read cannot repopulate after a library change',async()=>{
    const f=noteFixture();let resolve;f.respond(()=>new Promise(r=>resolve=r));const open=f.notes.open(f.file);
    assert.equal(f.get('notes-cancel').hidden,true);f.h.invalidateRequests();f.h.resetLibraryView();resolve(new Response(JSON.stringify({revision:'r',title:'secret',body:'sensitive'})));await open;
    assert.equal(f.get('notes-body').value,'');assert.equal(f.notes.isOpen(),false);
});
test('independent note cards filter by type, use the note viewer and have no photo attachment button',async()=>{
    const f=noteFixture();f.h.setFiles([f.file,{id:74,rel_path:'photo.png',file_type:'IMAGE',tags:[]}]);
    f.get('media-type-filter').value='NOTE';f.h.applySearchAndRender();assert.equal(f.h.mediaVirtualList.items.length,1);
    await f.h.selectMedia(f.file);assert.equal(f.get('note-stage').hidden,false);assert.equal(f.notes.isOpen(),true);
    assert.equal(f.get('note-stage-title').textContent,'Title');
    assert.equal(f.notes.text,'  sample\r\n日本語 🔐  ');assert.equal(f.get('notes-editor').hidden,true);assert.equal(f.get('notes-viewport').hidden,false);
    const html=fs.readFileSync(path.join(__dirname,'../static/index.html'),'utf8');assert.ok(!html.includes('btn-image-notes'));assert.ok(html.includes('btn-new-note'));assert.ok(!html.includes('notes-preview'));
});
test('saved standalone note uses the returned media ID to refresh and select its item',async()=>{
    const f=noteFixture();let saved=null;f.notes.onChanged=async(scope,id)=>saved=id;
    f.notes.openNew();f.get('notes-body').value='new body';
    f.respond(async url=>new Response(JSON.stringify(url.includes('/note-saves/')?{job:{job_id:"test-job",completed:true,running:false,result_id:81}}:{status:'started',job_id:'test-job'})));
    await f.notes.submit();assert.equal(saved,81);assert.equal(f.get('notes-body').value,'');assert.equal(f.get('notes-progress').hidden,true);
});
test('note editor rejects invalid Unicode and preserves real newlines in the viewer',async()=>{
    const f=noteFixture();f.notes.openNew();f.get('notes-body').value=String.fromCharCode(0xd800);await f.notes.submit();assert.equal(f.calls.length,0);
    f.notes.text='a'.repeat(100)+'\nlast';f.notes.layout();assert.equal(f.notes.lines.filter(x=>x.end).length,1);assert.equal(f.notes.lines.at(-1).end,false);
});


function pdfWheel(f, deltaY, extra = {}) {
    const event = {deltaY, deltaX:0, deltaMode:0, preventDefault(){this.prevented=true;}, ...extra};
    return {event, work:f.get('pdf-pages').listeners.wheel(event)};
}
test('PDF wheel changes pages once per gesture and respects document ends and zoom shortcuts', async()=>{
    const f=fixture(), pdf=fakePdf(f), viewer=f.h.pdfViewer, area=f.get('pdf-pages');
    await viewer.open({id:1,rel_path:'test.pdf',thumbnail_data:'test'},'/test.pdf');
    area.scrollHeight=area.clientHeight;
    await pdfWheel(f,-100).work;assert.equal(viewer.page,1);
    await pdfWheel(f,100,{ctrlKey:true}).work;assert.equal(viewer.page,1);
    await pdfWheel(f,10,{deltaX:100}).work;assert.equal(viewer.page,1);
    await pdfWheel(f,100).work;assert.equal(viewer.page,2);
    for(let i=0;i<10;i++){f.advance(100);await pdfWheel(f,100).work;}
    assert.equal(viewer.page,2,'continuous inertia must not skip pages');
    f.advance(400);await pdfWheel(f,15).work;f.advance(40);await pdfWheel(f,25).work;
    assert.equal(viewer.page,3,'small trackpad deltas accumulate');
    f.advance(400);await pdfWheel(f,-3,{deltaMode:1}).work;assert.equal(viewer.page,2);
    await viewer.show(6);f.advance(400);await pdfWheel(f,100).work;assert.equal(viewer.page,6);
});
test('PDF wheel scrolls enlarged pages normally, turns at edges and waits for rendering',async()=>{
    const f=fixture(), pdf=fakePdf(f), viewer=f.h.pdfViewer, area=f.get('pdf-pages');
    await viewer.open({id:1,rel_path:'test.pdf',thumbnail_data:'test'},'/test.pdf');
    area.scrollHeight=1800;area.clientHeight=600;area.scrollTop=300;
    const middle=pdfWheel(f,100);await middle.work;assert.equal(middle.event.prevented,undefined);assert.equal(viewer.page,1);
    area.scrollTop=1200;await pdfWheel(f,100).work;assert.equal(viewer.page,2);assert.equal(area.scrollTop,0);
    f.advance(400);await pdfWheel(f,-100).work;assert.equal(viewer.page,1);assert.equal(area.scrollTop,1800);
    await viewer.show(2);area.scrollTop=1200;f.advance(400);let finish;
    const original=pdf.document.getPage;pdf.document.getPage=()=>new Promise(resolve=>finish=()=>original(3).then(resolve));
    const next=pdfWheel(f,100);assert.equal(viewer.targetPage,3);assert.equal(viewer.ready,false);
    f.advance(500);await pdfWheel(f,100).work;assert.equal(viewer.targetPage,3);
    finish();await next.work;assert.equal(viewer.page,3);
});


test('PDF two-page layout navigates spreads, counts both pages and saves the first page', async()=>{
    const f=fixture(), pdf=fakePdf(f,{numPages:5}), viewer=f.h.pdfViewer;
    f.get('pdf-layout').value='2';f.get('pdf-zoom').value='fit';
    await f.h.selectMedia({id:1,rel_path:'spread.pdf',file_type:'PDF',thumbnail_data:'saved',tags:[]});
    assert.deepEqual(pdf.renders.map(x=>x.page),[1,2]);
    assert.equal(f.get('pdf-pages').children[0].children.length,2);
    assert.equal(f.get('pdf-count').textContent,'– 2 / 5');
    const sizes=pdf.renders.map(x=>x.viewport);assert.ok(sizes[0].width+sizes[1].width+12<=f.get('pdf-pages').clientWidth-24+1);
    await viewer.step(1);assert.equal(viewer.page,3);assert.equal(f.calls.filter(([u])=>/\/watch(?:\?|$)/.test(u)).length,0);
    f.get('pdf-pages').scrollHeight=f.get('pdf-pages').clientHeight;
    await pdfWheel(f,100).work;assert.equal(viewer.page,5);assert.equal(f.get('pdf-next').disabled,true);
    assert.equal(f.get('pdf-pages').children[0],viewer.canvas);assert.equal(f.calls.filter(([u])=>/\/watch(?:\?|$)/.test(u)).length,1);
    f.advance(400);await pdfWheel(f,-100).work;assert.equal(viewer.page,3);
    await viewer.show(4);assert.equal(viewer.page,3);
    await viewer.saveBookmark();const saved=f.calls.find(([u,o])=>u.includes('/bookmarks')&&o?.method==='POST');assert.equal(JSON.parse(saved[1].body).position,2);
    f.get('pdf-layout').value='1';await f.get('pdf-layout').listeners.change();assert.equal(f.get('pdf-pages').children[0],viewer.canvas);
    await viewer.step(1);assert.equal(viewer.page,4);
});
test('PDF two-page layout shares pixel budget and handles an even last spread',async()=>{
    const f=fixture(),pdf=fakePdf(f),viewer=f.h.pdfViewer;f.get('pdf-layout').value='2';f.get('pdf-zoom').value='4';
    await viewer.open({id:1,rel_path:'large.pdf',thumbnail_data:'saved'},'/large.pdf',6);
    assert.deepEqual(pdf.renders.map(x=>x.page),[5,6]);assert.equal(viewer.page,5);assert.equal(f.get('pdf-next').disabled,true);
    const total=pdf.renders.reduce((sum,r)=>sum+r.viewport.width*r.viewport.height,0);assert.ok(total<=8388609);
    await viewer.step(-1);assert.equal(viewer.page,3);
});
test('late second PDF page cannot replace a newer layout or repopulate after reset',async()=>{
    const f=fixture(),pdf=fakePdf(f),viewer=f.h.pdfViewer;await viewer.open({id:1,rel_path:'spread.pdf',thumbnail_data:'saved'},'/spread.pdf');
    let finish;const original=pdf.document.getPage;pdf.document.getPage=n=>n===2?new Promise(resolve=>finish=resolve):original(n);
    f.get('pdf-layout').value='2';const pending=viewer.show(1);await new Promise(setImmediate);
    f.get('pdf-layout').value='1';await viewer.show(3);finish(await original(2));await pending;
    assert.equal(viewer.page,3);assert.equal(f.get('pdf-pages').children[0],viewer.canvas);
    f.get('pdf-layout').value='2';const locked=viewer.show(1);await new Promise(setImmediate);viewer.reset();finish(await original(2));await locked;
    assert.equal(f.get('pdf-pages').children.length,0);assert.equal(viewer.ready,false);
});


test('PDF reuses identical rendering work and only rerenders for relevant geometry changes',async()=>{
    const f=fixture(),pdf=fakePdf(f),viewer=f.h.pdfViewer;f.get('pdf-zoom').value='fit';
    await viewer.open({id:1,rel_path:'test.pdf',thumbnail_data:'saved'},'/test.pdf');
    for(let i=0;i<20;i++)await viewer.show(1);assert.equal(pdf.renders.length,1);
    let finish;const original=pdf.document.getPage;pdf.document.getPage=n=>n===2?new Promise(resolve=>finish=()=>original(n).then(resolve)):original(n);
    const first=viewer.show(2),again=viewer.show(2);assert.equal(first,again);finish();await first;assert.equal(pdf.renders.length,2);
    pdf.document.getPage=original;
    f.get('pdf-pages').clientHeight+=100;await viewer.show(2);assert.equal(pdf.renders.length,3);
    f.get('pdf-zoom').value='width';await viewer.show(2);const before=pdf.renders.length;
    f.get('pdf-pages').clientHeight+=100;await viewer.show(2);assert.equal(pdf.renders.length,before);
    f.get('pdf-pages').clientWidth+=100;await viewer.show(2);assert.equal(pdf.renders.length,before+1);
});
test('PDF failed requests can retry and reset cannot reuse a previous library render',async()=>{
    const f=fixture(),pdf=fakePdf(f),viewer=f.h.pdfViewer;await viewer.open({id:1,rel_path:'test.pdf',thumbnail_data:'saved'},'/test.pdf');
    const original=pdf.document.getPage;pdf.document.getPage=async()=>{throw Error('offline');};await viewer.show(2);assert.equal(viewer.ready,false);
    pdf.document.getPage=original;await viewer.show(2);assert.equal(viewer.ready,true);assert.equal(viewer.page,2);
    viewer.reset();await viewer.open({id:2,rel_path:'other.pdf',thumbnail_data:'saved'},'/other.pdf',2);assert.equal(pdf.renders.length,3);
});
test('selecting media keeps mounted thumbnails and the 10000-item height index',async()=>{
    const f=fixture(),files=Array.from({length:10000},(_,i)=>({id:i+1,rel_path:i+'.png',file_type:'IMAGE',tags:[]}));f.h.setFiles(files);
    const list=f.h.mediaVirtualList,index=list.index,nodes=new Map(list.nodes);let created=0;const create=list.createCard;list.createCard=file=>{created++;return create(file);};
    for(let i=0;i<100;i++)await f.h.selectMedia(files[i%2]);
    assert.equal(list.index,index);assert.equal(created,0);
    for(const [i,node] of nodes)assert.equal(list.nodes.get(i),node);
    assert.equal(list.nodes.get(1).classList.contains('active'),true);assert.equal(list.nodes.get(0).classList.contains('active'),false);
});


test('shared rating patches all members, keeps card DOM, and does not fetch the entire list',async()=>{
    const f=fixture(),files=[1,2,3].map(id=>({id,rel_path:id+'.png',file_type:'IMAGE',series_id:id<3?7:null,rating:0,tags:[]}));
    f.h.setFiles(files);f.h.setCurrent({...files[1]});const cards=new Map(f.h.mediaVirtualList.nodes);
    await f.h.updateFileRating(1,4);assert.deepEqual(files.map(x=>x.rating),[4,4,0]);assert.equal(f.calls.length,1);
    for(const [i,card]of cards)assert.equal(f.h.mediaVirtualList.nodes.get(i),card);
    assert.equal(cards.get(1).children[1].children[0].children[3].classList.contains('active'),true);
    assert.equal(cards.get(2).children[1].children[0].children[0].classList.contains('active'),false);
});
test('rating sort uses authoritative order while tag filters refresh membership without a full fetch',async()=>{
    const f=fixture(),files=[1,2,3].map(id=>({id,rel_path:id+'.png',file_type:'IMAGE',series_id:id<3?7:null,rating:0,tags:[]}));f.h.setFiles(files);f.h.setCurrent(files[0]);
    f.h.setSort('rating');f.respond(async(url,opts)=>new Response(JSON.stringify(url.startsWith('/api/files?')?[files[2],{...files[0],rating:1},{...files[1],rating:1}]:{})));
    await f.h.updateFileRating(1,1);assert.deepEqual([...f.h.mediaVirtualList.items].map(x=>x.id),[3,1,2]);assert.equal(f.calls.filter(([u])=>u.startsWith('/api/files?')).length,1);
    f.get('tag-search-input').value='#shared';f.h.applySearchAndRender();assert.equal(f.h.mediaVirtualList.items.length,0);
    f.respond(async()=>new Response(JSON.stringify({id:10,name:'shared'})));const before=f.calls.length;await f.h.addTag(1,'shared');
    assert.deepEqual([...f.h.mediaVirtualList.items].map(x=>x.id),[1,2]);await f.h.removeTag(2,10);assert.equal(f.h.mediaVirtualList.items.length,0);assert.equal(f.calls.length-before,2);
});
test('metadata writes are ordered and stale list reads cannot overwrite committed changes',async()=>{
    const f=fixture(),files=[{id:1,rel_path:'1.png',file_type:'IMAGE',rating:0,tags:[]}];f.h.setFiles(files);f.h.setCurrent(files[0]);let finishList,finishWrite;
    f.respond((url,opts)=>url.startsWith('/api/files?')?new Promise(r=>finishList=r):new Promise(r=>finishWrite=r));const reading=f.h.loadMediaFiles();
    const first=f.h.updateFileRating(1,1),second=f.h.updateFileRating(1,5);assert.equal(f.calls.filter(([u])=>u.includes('/rating')).length,1);
    finishWrite(new Response('{}'));await first;await new Promise(setImmediate);assert.equal(f.calls.filter(([u])=>u.includes('/rating')).length,2);
    finishWrite(new Response('{}'));await second;finishList(new Response(JSON.stringify([{...files[0],rating:0}])));await reading;assert.equal(files[0].rating,5);assert.equal(f.h.mediaVirtualList.items[0].rating,5);
});
test('partial metadata errors and library changes do not apply phantom updates',async()=>{
    const f=fixture(),files=[{id:1,rel_path:'old.png',file_type:'IMAGE',rating:0,tags:[]}];f.h.setFiles(files);f.respond(async()=>new Response('offline',{status:500}));await f.h.updateFileRating(1,4);assert.equal(files[0].rating,0);
    let finish;f.respond(()=>new Promise(r=>finish=r));const pending=f.h.updateFileRating(1,5);f.h.invalidateRequests();f.h.setSession(2);
    const newer={id:1,rel_path:'new.png',file_type:'IMAGE',rating:2,tags:[]};f.h.setFiles([newer]);finish(new Response('{}'));await pending;assert.equal(newer.rating,2);
});
test('adaptive archive prefetch reserves a visible-page slot and promotes queued next pages',async()=>{
    const f=fixture();f.h.setCurrent({id:1,rel_path:'book.zip',file_type:'ARCHIVE'});f.h.setArchive(Array.from({length:20},(_,i)=>i+'.png'),0);
    f.h.preloadArchiveImages('book.zip',2);assert.equal(f.images.length,1);assert.equal(f.h.stats().pending,4);
    const next=f.h.getImageMeta('book.zip','3.png');assert.equal(f.images.length,2);assert.ok(f.images[1].src.includes('3.png'));
    f.images[1].onload();await next;assert.equal(f.images.length,2,'extra speculation must leave the foreground slot free');
    const visible=f.h.getImageMeta('book.zip','0.png');assert.equal(f.images.length,3);assert.ok(f.images[2].src.includes('0.png'));f.images[2].onload();await visible;
    f.h.clearImageJobs();assert.equal(f.h.stats().active,0);assert.equal(f.h.stats().pending,0);
});
test('slow or large archive images keep the next two images and obsolete queued work is pruned without cancelling active reads',async()=>{
    const f=fixture();f.h.setCurrent({id:1,rel_path:'book.zip',file_type:'ARCHIVE'});f.h.setArchive(Array.from({length:30},(_,i)=>i+'.png'),0);
    const sample=f.h.getImageMeta('book.zip','0.png');f.advance(700);f.images[0].onload();await sample;
    f.h.preloadArchiveImages('book.zip',2);assert.equal(f.h.stats().pending,2);const active=f.images[1],url=active.src;
    f.h.preloadArchiveImages('book.zip',20);assert.equal(active.src,url);assert.equal(f.h.stats().pending,3,'active read plus two new targets');
    active.onload();assert.ok(f.images.at(-1).src.includes('20.png'));f.h.clearImageJobs();
    const large=f.h.getImageMeta('book.zip','1.png');const image=f.images.at(-1);image.naturalWidth=5000;image.naturalHeight=5000;image.onload();await large;
    f.h.preloadArchiveImages('book.zip',2);assert.equal(f.h.stats().pending,2);f.h.clearImageJobs();
});
test('fast archive storage may look farther ahead while prefetch stays bounded',async()=>{
    const f=fixture();f.h.setCurrent({id:1,rel_path:'book.zip',file_type:'ARCHIVE'});f.h.setArchive(Array.from({length:30},(_,i)=>i+'.png'),0);
    const sample=f.h.getImageMeta('book.zip','0.png');f.advance(40);f.images[0].onload();await sample;
    f.h.preloadArchiveImages('book.zip',2);assert.equal(f.h.stats().pending,6);assert.equal(f.h.stats().active,1);f.h.clearImageJobs();
});


async function runPdfPrefetch(f) {
    const timer=[...f.timers].find(([,t])=>t.ms===180);assert.ok(timer);f.timers.delete(timer[0]);timer[1].fn();await new Promise(setImmediate);
}
test('PDF prepares only the next spread and reuses it without counting it as viewed',async()=>{
    const f=fixture(),pdf=fakePdf(f),viewer=f.h.pdfViewer,seen=[];viewer.onPage=n=>seen.push(n);f.get('pdf-layout').value='2';
    await viewer.open({id:1,rel_path:'test.pdf',thumbnail_data:'saved'},'/test.pdf');assert.deepEqual(seen,[0,1]);
    await runPdfPrefetch(f);assert.deepEqual(pdf.renders.map(x=>x.page),[1,2,3,4]);assert.deepEqual(seen,[0,1]);assert.equal(viewer.page,1);
    assert.equal([...f.timers.values()].filter(t=>t.ms===180).length,0,'prefetch completion must not walk the document');
    await viewer.step(1);assert.equal(pdf.renders.length,4);assert.equal(viewer.page,3);assert.deepEqual(seen,[0,1,2,3]);
    await viewer.step(-1);assert.equal(pdf.renders.length,4);assert.equal(viewer.page,1);
});
test('selecting a PDF spread during prefetch shares the pending render',async()=>{
    const f=fixture(),pdf=fakePdf(f),viewer=f.h.pdfViewer;await viewer.open({id:1,rel_path:'test.pdf',thumbnail_data:'saved'},'/test.pdf');
    const original=pdf.document.getPage;let finish,requests=0;pdf.document.getPage=n=>{requests++;return new Promise(resolve=>finish=()=>original(n).then(resolve));};
    await runPdfPrefetch(f);const selection=viewer.show(2);assert.equal(requests,1);finish();await selection;
    assert.equal(viewer.page,2);assert.equal(pdf.renders.length,2);
});
test('PDF cache has byte and entry limits, and reset erases all retained pixel buffers',async()=>{
    const f=fixture(),pdf=fakePdf(f,{numPages:10}),viewer=f.h.pdfViewer;f.get('pdf-zoom').value='4';
    await viewer.open({id:1,rel_path:'test.pdf',thumbnail_data:'saved'},'/test.pdf');const first=viewer.canvas;
    await viewer.show(2);await viewer.show(3);assert.equal(first.width,1);assert.ok(viewer.cacheBytes<=72*1024*1024);assert.ok(viewer.renderCache.size<=3);
    const retained=[...viewer.renderCache.values()].flatMap(e=>e.canvases);viewer.reset();assert.equal(viewer.cacheBytes,0);assert.equal(viewer.renderCache.size,0);
    assert.ok(retained.every(canvas=>canvas.width===1&&canvas.height===1));assert.equal([...f.timers.values()].filter(t=>t.ms===180).length,0);
    f.get('pdf-zoom').value='fit';await viewer.open({id:2,rel_path:'small.pdf',thumbnail_data:'saved'},'/small.pdf');for(let n=2;n<=6;n++)await viewer.show(n);assert.equal(viewer.renderCache.size,3);
});
test('PDF two-page rendering starts both tasks and a new selection cancels both',async()=>{
    const f=fixture(),pdf=fakePdf(f),viewer=f.h.pdfViewer;await viewer.open({id:1,rel_path:'test.pdf',thumbnail_data:'saved'},'/test.pdf');
    let started=0,cancelled=0;const original=pdf.document.getPage;
    pdf.document.getPage=async n=>({...await original(n),render(){started++;let reject;return {promise:new Promise((_,r)=>reject=r),cancel(){cancelled++;reject(Error('cancelled'));}};}});
    f.get('pdf-layout').value='2';const pending=viewer.show(3);await new Promise(setImmediate);assert.equal(started,2);
    viewer.reset();await pending;assert.equal(cancelled,2);assert.equal(viewer.renderCache.size,0);assert.equal(f.get('pdf-pages').children.length,0);
});
test('failed PDF prefetch leaves the visible page intact and the next manual request can retry',async()=>{
    const f=fixture(),pdf=fakePdf(f),viewer=f.h.pdfViewer;await viewer.open({id:1,rel_path:'test.pdf',thumbnail_data:'saved'},'/test.pdf');const canvas=viewer.canvas,original=pdf.document.getPage;
    pdf.document.getPage=async()=>{throw Error('NAS disconnected');};await runPdfPrefetch(f);assert.equal(viewer.canvas,canvas);assert.equal(viewer.ready,true);assert.equal(f.get('pdf-status').textContent,'');
    pdf.document.getPage=original;await viewer.show(2);assert.equal(viewer.page,2);
});


test('video wheel seeks five seconds, updates the bar, clamps limits and preserves paused state',async()=>{
    const {f,video}=await watchVideoFixture();Object.assign(video,{duration:100,currentTime:20,paused:true,volume:.5,muted:false});
    const wheel=(y,extra={})=>{const e={deltaY:y,preventDefault(){this.prevented=true;},stopPropagation(){this.stopped=true;},...extra};f.get('video-player-stage').listeners.wheel(e);return e;};
    const event=wheel(100);assert.equal(video.currentTime,25);assert.equal(f.get('seek-bar').value,25);assert.equal(f.get('current-time').textContent,'00:25');assert.ok(event.prevented&&event.stopped);assert.equal(video.paused,true);assert.equal(video.volume,.5);
    wheel(100);assert.equal(video.currentTime,25,'burst must not cause a huge jump');f.advance(150);wheel(-100);assert.equal(video.currentTime,20);
    f.advance(150);video.currentTime=98;wheel(100);assert.equal(video.currentTime,100);
    f.advance(150);video.currentTime=2;wheel(-100);assert.equal(video.currentTime,0);
    f.advance(250);wheel(15);f.advance(30);wheel(25);assert.equal(video.currentTime,5);
    f.advance(150);wheel(3,{deltaMode:1});assert.equal(video.currentTime,10);
});
test('video wheel keeps volume controls and ignores unrelated controls or invalid duration',async()=>{
    const {f,video}=await watchVideoFixture();Object.assign(video,{duration:100,currentTime:20,volume:.5,muted:false});
    const wheel=(extra={})=>{const e={deltaY:100,preventDefault(){this.prevented=true;},stopPropagation(){},...extra};f.get('video-player-stage').listeners.wheel(e);return e;};
    wheel({deltaY:-100,target:{closest:s=>s==='.volume-container'}});assert.equal(video.volume,.55);assert.equal(video.currentTime,20);
    for(const extra of [{ctrlKey:true},{shiftKey:true},{deltaX:200},{target:{closest:s=>s.includes('button')}}]){assert.equal(wheel(extra).prevented,undefined);assert.equal(video.currentTime,20);}
    video.duration=NaN;assert.equal(wheel().prevented,undefined);video.duration=Infinity;assert.equal(wheel().prevented,undefined);
});

test('saved-position overlays keep wheel input from seeking or turning the underlying media',async()=>{
    const {f,video}=await watchVideoFixture();Object.assign(video,{duration:100,currentTime:20});
    const event=()=>({deltaY:100,target:{closest:selector=>selector.includes('.bookmark-overlay')},
        preventDefault(){this.prevented=true;},stopPropagation(){this.stopped=true;}});
    const videoEvent=event();f.get('video-player-stage').listeners.wheel(videoEvent);
    assert.equal(videoEvent.prevented,undefined);assert.equal(video.currentTime,20);
    f.h.setCurrent({id:2,rel_path:'sample.zip',file_type:'ARCHIVE'});f.get('image-viewer-stage').style.display='flex';
    const imageEvent=event(),timers=f.timers.size;
    f.get('image-viewer-stage').listeners.wheel(imageEvent);
    assert.equal(imageEvent.prevented,undefined);assert.equal(imageEvent.stopped,undefined);assert.equal(f.timers.size,timers);
});

test('filename and tag searches are independent and combine with series and type filters', () => {
    const f=fixture();
    f.h.setFiles([
        {id:1,rel_path:'trip.png',file_type:'IMAGE',tags:[{id:1,name:'Nature'}]},
        {id:2,rel_path:'nature.png',file_type:'IMAGE',tags:[{id:2,name:'Trip'}]},
        {id:3,rel_path:'03.mp4',series_name:'Trip collection',file_type:'VIDEO',tags:[{id:1,name:'Nature'}]},
        {id:4,rel_path:'trip.pdf',file_type:'PDF',tags:[]},
    ]);
    const ids=()=>Array.from(f.h.getFilteredFiles(),x=>x.id);
    f.get('search-input').value='nature';assert.deepEqual(ids(),[2]);
    f.get('search-input').value='';f.get('tag-search-input').value=' #nAtU ';assert.deepEqual(ids(),[1,3]);
    f.get('search-input').value='trip';assert.deepEqual(ids(),[1,3]);
    f.get('media-type-filter').value='VIDEO';assert.deepEqual(ids(),[3]);
    f.get('tag-search-input').value='missing';assert.deepEqual(ids(),[]);
});

test('tag clicks preserve filename search and each clear button clears only its own condition', async () => {
    const f=fixture();f.h.setFiles([
        {id:1,rel_path:'trip.png',file_type:'IMAGE',tags:[{id:1,name:'Nature'}]},
        {id:2,rel_path:'trip.pdf',file_type:'PDF',tags:[]},
        {id:3,rel_path:'other.png',file_type:'IMAGE',tags:[{id:1,name:'Nature'}]},
    ]);
    f.get('search-input').value='trip';f.h.applySearchAndRender();
    const find=(node,cls)=>node.className===cls?node:node.children.map(child=>find(child,cls)).find(Boolean);
    const badge=find(f.h.mediaVirtualList.nodes.get(0),'tag-badge');assert.ok(badge);
    badge.onclick({stopPropagation(){}});
    assert.equal(f.get('search-input').value,'trip');assert.equal(f.get('tag-search-input').value,'#Nature');
    assert.deepEqual(Array.from(f.h.mediaVirtualList.items,x=>x.id),[1]);
    await f.get('btn-clear-tag-search').click();
    assert.equal(f.get('search-input').value,'trip');assert.equal(f.get('btn-clear-tag-search').style.display,'none');
    assert.deepEqual(Array.from(f.h.mediaVirtualList.items,x=>x.id),[1,2]);
    f.get('tag-search-input').value='Nature';await f.get('btn-clear-search').click();
    assert.equal(f.get('tag-search-input').value,'Nature');assert.deepEqual(Array.from(f.h.mediaVirtualList.items,x=>x.id),[1,3]);
});

test('tag search debounces input and library reset removes both searches and pending work', () => {
    const f=fixture();f.h.setFiles([{id:1,rel_path:'image.png',file_type:'IMAGE',tags:[]}]);
    const input=f.get('tag-search-input');input.value='absent';input.listeners.input();
    assert.equal(f.get('btn-clear-tag-search').style.display,'flex');
    const timer=[...f.timers.values()].find(x=>x.ms===120);assert.ok(timer);timer.fn();
    assert.equal(f.h.mediaVirtualList.items.length,0);
    f.get('search-input').value='image';f.get('search-input').listeners.input();
    input.focus();input.closest=()=>input;
    f.h.handleShortcut(keyEvent('Escape',{key:'Escape',target:input}));assert.equal(input.focused,false);
    f.h.resetLibraryView();
    for(const id of ['search-input','tag-search-input'])assert.equal(f.get(id).value,'');
    for(const id of ['btn-clear-search','btn-clear-tag-search'])assert.equal(f.get(id).style.display,'none');
    assert.equal([...f.timers.values()].some(x=>x.ms===120),false);
});

test('stalled note reads can close immediately and late responses never reopen the note',async()=>{
    const f=noteFixture();let resolve;f.respond(()=>new Promise(r=>resolve=r));
    const opening=f.notes.open(f.file);assert.equal(f.get('notes-close').disabled,false);
    f.get('notes-dialog').listeners.cancel({preventDefault(){}});await opening;
    assert.equal(f.notes.isOpen(),false);assert.equal(f.notes.pending,false);
    resolve(new Response(JSON.stringify({revision:'old',title:'old',body:'old body'})));await new Promise(setImmediate);
    assert.equal(f.notes.text,undefined);assert.equal(f.get('notes-body').value,'');
});

test('note read deadline covers stalled response bodies and permits another read',async()=>{
    const f=noteFixture();let body;
    f.respond(async()=>({ok:true,json:()=>new Promise(r=>body=r),text:async()=>'',blob:async()=>new Blob()}));
    const opening=f.notes.open(f.file);await new Promise(setImmediate);
    [...f.timers.values()].find(t=>t.ms===15000).fn();await opening;
    assert.equal(f.notes.pending,false);assert.equal(f.get('notes-close').disabled,false);assert.match(f.get('notes-status').textContent,/15/);
    f.respond(async()=>new Response(JSON.stringify({revision:'new',title:'new',body:'new body'})));
    await f.notes.read(false);body({revision:'old',title:'old',body:'late body'});await new Promise(setImmediate);
    assert.equal(f.notes.text,'new body');
});

test('lost note save acknowledgement reconciles its own receipt without creating another note',async()=>{
    const f=noteFixture();f.notes.openNew();f.get('notes-body').value='exact body';let created=0,requestId,saved;
    f.notes.onChanged=async(scope,id)=>saved=id;
    f.respond(async(url,options)=>{
        if(options.method==='POST'){created++;requestId=JSON.parse(options.body).request_id;throw new TypeError('lost acknowledgement');}
        assert.ok(url.includes('/api/note-saves/'+requestId));
        return new Response(JSON.stringify({job:{job_id:"test-job",running:false,completed:true,result_id:99}}));
    });
    await f.notes.submit();assert.equal(f.get('notes-body').disabled,true);assert.equal(f.get('notes-save').disabled,false);
    assert.ok(f.notes.saveAttempt);await f.notes.submit();
    assert.equal(created,1);assert.equal(saved,99);assert.equal(f.notes.saveAttempt,null);
});

test('unknown note save retry retains the exact ID and payload; failed jobs allow editing',async()=>{
    const f=noteFixture();f.notes.openNew();f.get('notes-body').value='first';const payloads=[];let polling=0;
    f.notes.onChanged=async()=>{};
    f.respond(async(url,options)=>{
        if(options.method==='POST'){
            payloads.push(options.body);
            if(payloads.length===1)throw new TypeError('request may not have arrived');
            return new Response('{"status":"started","job_id":"test-job"}');
        }
        polling++;
        return new Response(JSON.stringify(polling===1?{status:'unknown'}:{job:{job_id:"test-job",running:false,completed:true,result_id:5}}));
    });
    await f.notes.submit();await f.notes.submit();assert.equal(payloads.length,2);assert.equal(payloads[0],payloads[1]);
    f.notes.openNew();f.get('notes-body').value='retry after failure';
    f.respond(async(url,options)=>new Response(JSON.stringify(options.method==='POST'?{status:'started',job_id:'test-job'}:{job:{job_id:"test-job",running:false,error:'cancelled'}})));
    await f.notes.submit();assert.equal(f.notes.saveAttempt,null);assert.equal(f.get('notes-body').disabled,false);assert.equal(f.get('notes-body').value,'retry after failure');
});

test('lost note progress response retains receipt and reset cancels in-flight reconciliation',async()=>{
    const f=noteFixture();f.notes.openNew();f.get('notes-body').value='draft';let created=0,requestId;
    f.respond(async(url,options)=>{if(options.method==='POST'){created++;requestId=JSON.parse(options.body).request_id;return new Response('{"status":"started","job_id":"test-job"}');}throw new TypeError('poll failed');});
    await f.notes.submit();assert.equal(f.notes.saveAttempt.payload.request_id,requestId);
    let release;f.respond(()=>new Promise(r=>release=r));const retry=f.notes.submit();
    f.h.resetLibraryView();await retry;release(new Response(JSON.stringify({job:{job_id:"test-job",running:false,completed:true,result_id:1}})));await new Promise(setImmediate);
    assert.equal(created,1);assert.equal(f.notes.isOpen(),false);assert.equal(f.notes.saveAttempt,null);
});

test('a rejected status lookup or replay does not erase an uncertain save request',async()=>{
    const f=noteFixture();f.notes.openNew();f.get('notes-body').value='receipt kept';let posts=0;
    f.respond(async(url,options)=>{if(options.method==='POST'){posts++;return new Response('{"status":"started","job_id":"test-job"}');}return new Response('temporary DB read failure',{status:400});});
    await f.notes.submit();const requestId=f.notes.saveAttempt.payload.request_id;
    f.respond(async(url,options)=>options.method==='POST'?new Response('busy after middleware',{status:400}):new Response('{"status":"unknown"}'));
    await f.notes.submit();assert.equal(f.notes.saveAttempt.payload.request_id,requestId);assert.equal(f.get('notes-body').disabled,true);
    f.notes.onChanged=async()=>{};
    f.respond(async()=>new Response(JSON.stringify({job:{job_id:"test-job",running:false,completed:true,result_id:8}})));
    await f.notes.submit();assert.equal(f.notes.saveAttempt,null);assert.equal(posts,1);
});

for(const source of ['arena','trash'])test(source+' delayed completion preserves a new memo draft',async()=>{
    const f=fixture('en'),file={id:1,rel_path:'book.zip',file_type:'ARCHIVE',encrypted:true,tags:[]};let release;
    f.h.setFiles([file]);f.h.setCurrent(file);
    f.respond(async url=>{
        if(source==='arena'?url.includes('/api/arena/pair'):url.includes('/1/trash'))return new Promise(resolve=>release=resolve);
        return new Response(JSON.stringify(url.includes('/api/files?')?(source==='trash'?[]:[file]):
            url.includes('/trash/count')?{count:source==='trash'?1:0}:url.includes('/archive/list/')?['a.png']:[]));
    });
    const pending=source==='trash'?f.h.moveToTrash(1):(f.get('btn-start-arena').click(),null);
    await new Promise(setImmediate);assert.equal(typeof release,'function');
    f.get('btn-new-note').click();f.get('notes-title').value='Private draft';f.get('notes-body').value='Unsaved text';
    const version=f.h.noteEditor.version,confirmations=f.confirmations.length;
    release(new Response(source==='arena'?'[]':'{}'));if(pending)await pending;await new Promise(setImmediate);
    assert.equal(f.get('notes-body').value,'Unsaved text');assert.equal(f.get('notes-title').value,'Private draft');
    assert.equal(f.h.noteEditor.isOpen(),true);assert.equal(f.h.noteEditor.version,version);
    assert.equal(f.confirmations.length,confirmations,'background operations must not ask to discard a draft');
    if(source==='trash'){
        assert.equal(f.h.getFilteredFiles().length,0);
        assert.equal(Number(f.get('sidebar-trash-count').textContent),1);
        assert.equal(f.get('image-viewer-stage').style.display,'none');
    }else assert.ok(!f.calls.some(([url])=>url.includes('/archive/list/')),'arena exit must not reselect media over the draft');
});

test('ordinary note drafts resist selection and reopen until explicitly discarded',async()=>{
    const f=fixture(),n=f.h.noteEditor;
    n.openNew();f.get('notes-title').value='Draft';f.get('notes-body').value='Keep this';
    const version=n.version;
    const other={id:2,rel_path:'other.vtn',file_type:'NOTE',tags:[]};
    await f.h.selectMedia({id:1,rel_path:'other.png',file_type:'IMAGE',tags:[]});
    await f.h.selectMedia(other);await n.open(other);n.openNew();
    assert.equal(n.version,version);assert.equal(n.file,null);assert.equal(n.isOpen(),true);
    assert.equal(f.get('notes-body').value,'Keep this');assert.equal(f.get('notes-title').value,'Draft');
    f.confirm(false);n.close();assert.equal(n.isOpen(),true);assert.equal(f.get('notes-body').value,'Keep this');
    f.confirm(true);n.close();assert.equal(n.isOpen(),false);assert.equal(f.get('notes-body').value,'');
    await f.h.selectMedia({id:1,rel_path:'other.png',file_type:'IMAGE',tags:[]});
    assert.equal(f.get('image-viewer-stage').style.display,'flex');
});

test('an emptied existing note edit survives navigation but library reset clears it',async()=>{
    const f=fixture(),n=f.h.noteEditor,file={id:1,rel_path:'note.vtn',file_type:'NOTE',tags:[]};
    f.respond(async()=>new Response(JSON.stringify({revision:'r1',title:'Title',body:'Original'})));
    await f.h.selectMedia(file);await n.read(true);
    f.get('notes-title').value='';f.get('notes-body').value='';const version=n.version;
    await f.h.selectMedia(file);n.openNew();await n.open(file);
    assert.equal(n.version,version);assert.equal(n.file.id,1);assert.equal(n.isOpen(),true);
    assert.equal(f.get('notes-body').value,'');assert.equal(f.get('notes-editor').hidden,false);
    f.get('notes-body').value='Sensitive draft';f.h.resetLibraryView();
    assert.equal(n.isOpen(),false);assert.equal(n.file,null);assert.equal(f.get('notes-body').value,'');
});

test('late note cancellation cannot overwrite a new draft or a newer operation',async()=>{
    const f=fixture('en'),n=f.h.noteEditor;let progress,cancel;
    n.openNew();f.get('notes-body').value='first';
    f.respond(async url=>url.includes('/cancel?')?new Promise(r=>cancel=r):url.includes('/note-saves/')?new Promise(r=>progress=r):new Response('{"status":"started","job_id":"old-job"}'));
    const saving=n.submit();await new Promise(setImmediate);
    const cancelling=f.get('notes-cancel').click();await new Promise(setImmediate);
    await f.get('notes-cancel').click();assert.equal(f.calls.filter(([u])=>u.includes('/cancel?')).length,1);
    progress(new Response(JSON.stringify({job:{job_id:'old-job',running:false,error:'cancelled'}})));await saving;
    n.close();n.openNew();f.get('notes-body').value='second';f.get('notes-status').textContent='New draft status';
    cancel(new Response('Old cancellation failed',{status:500}));await cancelling;
    assert.equal(f.get('notes-status').textContent,'New draft status');assert.equal(f.get('notes-body').value,'second');
    assert.equal(f.get('notes-cancel').disabled,false);
});

for(const mode of ['list failure','list timeout','read failure'])test('saved note stays accessible after '+mode,async()=>{
    const f=fixture('en'),n=f.h.noteEditor;let releaseList,posts=0;
    n.openNew();f.get('notes-title').value='Saved note';f.get('notes-body').value='Saved body';
    f.respond(async(url,options)=>{
        if(url.includes('/api/files?'))return mode==='list timeout'?new Promise(r=>releaseList=r):new Response('NAS unavailable',{status:503});
        if(url.includes('/note-saves/'))return new Response(JSON.stringify({job:{job_id:'save-note',running:false,completed:true,result_id:17}}));
        if(url.includes('/notes/17'))return mode==='read failure'?new Response('Read unavailable',{status:503}):new Response(JSON.stringify({revision:'r1',title:'Saved note',body:'Saved body'}));
        posts++;return new Response('{"status":"started","job_id":"save-note"}');
    });
    const saving=n.submit();await new Promise(setImmediate);
    if(mode==='list timeout'){const timer=[...f.timers.values()].find(t=>t.ms===15000);assert.ok(timer);timer.fn();}
    await saving;assert.equal(n.file.id,17);assert.equal(n.saveAttempt,null);assert.equal(f.get('notes-view').disabled,false);
    if(mode==='read failure'){
        assert.match(f.get('notes-status').textContent,/Read unavailable/);
        f.respond(async()=>new Response(JSON.stringify({revision:'r1',title:'Saved note',body:'Saved body'})));
        await f.get('notes-view').click();
    }
    assert.equal(n.text,'Saved body');assert.equal(posts,1);
    if(releaseList){releaseList(new Response('[]'));await new Promise(setImmediate);assert.equal(n.text,'Saved body');}
});

for(const mode of ['headers','body'])test('metadata timeout releases the queue and ignores late '+mode,async()=>{
    const f=fixture('en'),file={id:1,rel_path:'one.png',file_type:'IMAGE',tags:[],rating:0};let release;
    f.h.setFiles([file]);
    f.respond(async(url,options)=>{
        if(url.includes('/1/tags')){
            if(mode==='headers')return new Promise(r=>release=r);
            return {ok:true,json:()=>new Promise(r=>release=r),text:async()=>'',blob:async()=>new Blob()};
        }
        return new Response('{}');
    });
    const first=f.h.addTag(1,'slow'),second=f.h.updateFileRating(1,4);await new Promise(setImmediate);
    assert.equal(f.calls.length,1);assert.ok(Number(f.calls[0][1].headers.get('X-Vault-Write-Deadline'))>Date.now());
    const deadline=[...f.timers.values()].find(t=>t.ms===15000);assert.ok(deadline);deadline.fn();
    assert.equal(await first,false);await second;
    assert.equal(file.rating,4);assert.equal(file.tags.length,0);assert.equal(f.get('metadata-state').hidden,false);
    assert.match(f.get('metadata-status').textContent,/may have been saved/);
    release(mode==='headers'?new Response('{"id":2,"name":"slow"}'):{id:2,name:'slow'});await new Promise(setImmediate);
    assert.equal(file.tags.length,0,'late response cannot apply a phantom tag');
    f.respond(async url=>new Response(JSON.stringify(url.includes('/api/files?')?[{...file,tags:[{id:2,name:'slow'}]}]:{count:0})));
    await f.get('metadata-check').click();
    assert.equal(f.h.getFilteredFiles()[0].tags[0].name,'slow');assert.equal(f.get('metadata-check').disabled,false);
    assert.match(f.get('metadata-status').textContent,/latest list/);
});

test('metadata result checks time out and old failures cannot appear in another library',async()=>{
    const f=fixture('en'),file={id:1,rel_path:'one.png',file_type:'IMAGE',tags:[]};let late;
    f.h.setFiles([file]);f.respond(async()=>new Response('offline',{status:503}));await f.h.updateFileRating(1,4);
    f.respond(async()=>new Promise(r=>late=r));const checking=f.get('metadata-check').click();await new Promise(setImmediate);
    [...f.timers.values()].find(t=>t.ms===15000).fn();await checking;
    assert.equal(f.get('metadata-check').disabled,false);assert.match(f.get('metadata-status').textContent,/Could not check/);
    const changing=f.h.addTag(1,'old');await new Promise(setImmediate);f.h.resetLibraryView();f.h.setSession(2);
    [...f.timers.values()].find(t=>t.ms===15000).fn();await changing;
    assert.equal(f.get('metadata-state').hidden,true);assert.equal(f.get('metadata-status').textContent,'');
    late(new Response('{}'));await new Promise(setImmediate);assert.equal(f.get('metadata-state').hidden,true);
});

test('pending note saves can hide, resume and finish without reopening the dialog',async()=>{
    const f=noteFixture();f.notes.openNew();f.get('notes-body').value='kept draft';let finish,saved;
    f.notes.onChanged=async(scope,id,options)=>saved={id,...options};
    f.respond(async(url,options)=>options.method==='POST'?new Response('{"status":"started","job_id":"test-job"}'):new Promise(r=>finish=r));
    const saving=f.notes.submit();await new Promise(setImmediate);
    const receipt=f.notes.saveAttempt.payload.request_id;
    assert.equal(f.get('notes-close').disabled,false);
    f.notes.close();assert.equal(f.notes.isOpen(),false);
    await f.h.selectMedia({id:9,rel_path:'other.png',file_type:'IMAGE',tags:[]});
    assert.equal(f.notes.saveAttempt.payload.request_id,receipt);
    f.notes.openNew();assert.equal(f.notes.isOpen(),true);assert.equal(f.get('notes-body').value,'kept draft');
    f.notes.close();finish(new Response(JSON.stringify({job:{job_id:"test-job",running:false,completed:true,result_id:4}})));await saving;
    assert.equal(f.notes.pending,false);assert.equal(f.notes.saveAttempt,null);
    assert.equal(f.notes.isOpen(),false);assert.deepEqual(saved,{id:4,reveal:false});
});

test('hidden failed note saves retain their draft for reopening',async()=>{
    const f=noteFixture();f.notes.openNew();f.get('notes-body').value='retry body';let finish;
    f.respond(async(url,options)=>options.method==='POST'?new Response('{"status":"started","job_id":"test-job"}'):new Promise(r=>finish=r));
    const saving=f.notes.submit();await new Promise(setImmediate);f.notes.close();
    finish(new Response(JSON.stringify({job:{job_id:"test-job",running:false,error:'cancelled'}})));await saving;
    assert.equal(f.notes.pending,false);assert.equal(f.notes.saveAttempt,null);
    f.notes.openNew();assert.equal(f.get('notes-body').value,'retry body');assert.equal(f.notes.isOpen(),true);
});

test('encryption dialog can close and reopen while cancellation waits for I/O',async()=>{
    const f=fixture(),s=f.h.archiveSecurity;let finish;
    s.files=[{id:1,name:'book.zip',size:100,state:'plain'}];s.selected.add(1);s.dialog.showModal();
    f.respond(async url=>url.includes('/progress')?new Promise(r=>finish=r):new Response('{"status":"started","job_id":"test-job"}'));
    f.get('media-encryption-risk-ack').checked=true;
    const job=s.batch();await new Promise(setImmediate);
    await s.cancel.click();assert.equal(s.close.disabled,false);assert.equal(s.headerClose.disabled,false);
    s.headerClose.click();assert.equal(s.isOpen(),false);assert.equal(s.pending,true);
    await s.open();assert.equal(s.isOpen(),true);assert.equal(s.pending,true);
    s.dialog.listeners.cancel({preventDefault(){}});assert.equal(s.isOpen(),false);
    finish(new Response(JSON.stringify({job:{job_id:"test-job",running:false,error:'cancelled'}})));await job;
    assert.equal(s.pending,false);assert.equal(s.isOpen(),false);
});

test('old encryption polling cannot change a new library operation',async()=>{
    const f=fixture(),s=f.h.archiveSecurity;let oldReply,newReply;
    s.files=[{id:1,name:'one.zip',size:100,state:'plain'}];s.selected.add(1);
    f.respond(async url=>url.includes('/progress')?new Promise(r=>oldReply=r):new Response('{"status":"started","job_id":"test-job"}'));
    f.get('media-encryption-risk-ack').checked=true;
    const old=s.batch();await new Promise(setImmediate);
    f.h.resetLibraryView();f.h.setSession(2);
    s.files=[{id:2,name:'two.zip',size:100,state:'plain'}];s.selected.add(2);
    f.respond(async url=>url.includes('/progress')?new Promise(r=>newReply=r):new Response('{"status":"started","job_id":"test-job"}'));
    f.get('media-encryption-risk-ack').checked=true;
    const current=s.batch();await new Promise(setImmediate);
    oldReply(new Response(JSON.stringify({job:{job_id:"test-job",running:false,error:'old failure'}})));await old;
    assert.equal(s.pending,true);assert.doesNotMatch(s.status.textContent,/old failure/);
    newReply(new Response(JSON.stringify({job:{job_id:"test-job",running:false,error:'new failure'}})));await current;
    assert.equal(s.pending,false);assert.match(s.status.textContent,/new failure/);
});

test('partial empty-trash failure refreshes committed deletions, counts and logs and blocks duplicates',async()=>{
    const f=fixture();let items=[1,2,3].map(id=>({id,rel_path:id+'.png',file_type:'IMAGE',file_size:10}));
    f.respond(async(url,options)=>{
        if(options.method==='DELETE'){
            if(url.includes('/1/permanent')){items=items.filter(x=>x.id!==1);return new Response('{}');}
            return new Response('disk unavailable',{status:400});
        }
        return new Response(JSON.stringify(url.includes('/trash/count')?{count:items.length}:url.includes('/trash/logs')?[{id:1,original_rel_path:'1.png'}]:items));
    });
    const first=f.h.emptyTrash();await f.h.emptyTrash();await first;
    assert.equal(f.calls.filter(([u,o])=>o.method==='DELETE').length,2);
    assert.equal(f.get('trash-items-grid').children.length,2);
    assert.equal(Number(f.get('trash-count').textContent),2);
    assert.equal(Number(f.get('sidebar-trash-count').textContent),2);
    assert.equal(f.get('trash-logs-tbody').children.length,1);
    assert.match(f.get('trash-items-status').textContent,/1\/3/);
    assert.match(f.get('trash-items-status').textContent,/disk unavailable/);
    assert.equal(f.get('btn-empty-trash').disabled,false);
});

test('trashing the last PDF clears document, canvases and rendering cache',async()=>{
    const f=fixture(),pdf=fakePdf(f);
    const file={id:1,rel_path:'last.pdf',file_type:'PDF',thumbnail_data:'/api/thumbnail/1',tags:[]};
    await f.h.selectMedia(file);
    assert.ok(f.get('pdf-pages').children.length>0);
    f.respond(async(url,options)=>new Response(JSON.stringify(url.includes('/api/files?')?[]:url.includes('/trash/count')?{count:1}:{})));
    await f.h.moveToTrash(1);
    assert.equal(f.h.pdfViewer.file,null);assert.equal(f.h.pdfViewer.renderCache.size,0);
    assert.equal(f.get('pdf-viewer-stage').style.display,'none');assert.equal(f.get('pdf-pages').children.length,0);
    assert.equal(pdf.destroyed,1);assert.equal(f.h.getFilteredFiles().length,0);
});

test('encryption refuses an unrelated successful job and queries its acknowledged job ID',async()=>{
    const f=fixture(),s=f.h.archiveSecurity;
    s.files=[{id:1,name:'one.zip',size:100,state:'plain'}];s.selected.add(1);
    f.respond(async(url,options)=>new Response(JSON.stringify(options.method==='POST'?{status:'started',job_id:'encrypt-1'}:
        url.includes('/progress')?{job:{job_id:'note-2',running:false,completed:true}}:{files:s.files})));
    f.get('media-encryption-risk-ack').checked=true;
    const working=s.batch();await new Promise(setImmediate);
    assert.ok(f.calls.some(([u])=>u.includes('/progress?job_id=encrypt-1')));
    assert.equal(s.encryptionBatch.completed,0);assert.equal(s.selected.has(1),true);
    assert.equal(s.pending,true);assert.equal(s.cancel.hidden,false);
    assert.match(s.status.textContent,/다시 확인/);
    s.reset();await working;
});

test('cancellation while waiting for start acknowledgement targets only the new job',async()=>{
    const f=fixture(),s=f.h.archiveSecurity;let start;
    s.files=[{id:1,name:'one.zip',size:100,state:'plain'}];s.selected.add(1);s.jobId='old-job';
    f.respond(async url=>url.includes('/encrypt')?new Promise(r=>start=r):new Response(JSON.stringify(url.includes('/progress')?
        {job:{job_id:'new-job',running:false,error:'cancelled'}}:{files:s.files})));
    f.get('media-encryption-risk-ack').checked=true;
    const working=s.batch();await new Promise(setImmediate);await s.cancel.click();
    assert.equal(f.calls.filter(([u])=>u.includes('/cancel')).length,0);
    start(new Response(JSON.stringify({status:'started',job_id:'new-job'})));await working;
    assert.ok(f.calls.some(([u])=>u.includes('/cancel?job_id=new-job')));
    assert.ok(!f.calls.some(([u])=>u.includes('job_id=old-job')));
});

test('missing job acknowledgement never claims successful encryption',async()=>{
    const f=fixture(),s=f.h.archiveSecurity;
    s.files=[{id:1,name:'one.zip',size:100,state:'plain'}];s.selected.add(1);
    f.get('media-encryption-risk-ack').checked=true;
    f.respond(async()=>new Response('{"status":"started"}'));
    const working=s.batch();await new Promise(setImmediate);
    assert.equal(s.encryptionBatch.completed,0);assert.match(s.status.textContent,/다시 확인/);
    assert.ok(!f.calls.some(([u])=>u.includes('/progress')));
    assert.equal(s.pending,true);s.reset();await working;
});

test('a pending or failed hidden note cannot change the selected note to another edit target',async()=>{
    const f=fixture(),n=f.h.noteEditor;
    const a={id:1,rel_path:'notes/a.json',file_type:'NOTE',display_name:'Note A',tags:[]};
    const b={id:2,rel_path:'notes/b.json',file_type:'NOTE',display_name:'Note B',tags:[]};
    f.respond(async()=>new Response(JSON.stringify({revision:'r1',title:'Note A',body:'A body'})));
    await f.h.selectMedia(a);await n.read(true);f.get('notes-body').value='A draft';let finish;
    f.respond(async(url,options)=>options.method==='POST'?new Response('{"status":"started","job_id":"save-A"}'):new Promise(r=>finish=r));
    const saving=n.submit();await new Promise(setImmediate);n.close();
    await f.h.selectMedia(b);assert.equal(n.file.id,a.id);assert.equal(n.isOpen(),false);
    assert.equal(f.get('note-stage-title').textContent,'Note A');
    n.openNew();assert.equal(n.isOpen(),false);
    finish(new Response(JSON.stringify({job:{job_id:'save-A',running:false,error:'disk full'}})));await saving;
    await f.h.selectMedia(b);assert.equal(f.get('note-stage-title').textContent,'Note A');
    await f.h.selectMedia(a);assert.equal(n.isOpen(),true);assert.equal(f.get('notes-body').value,'A draft');
    n.close(); // Explicit discard now allows selection of B.
    f.respond(async()=>new Response(JSON.stringify({revision:'r2',title:'Note B',body:'B body'})));
    await f.h.selectMedia(b);assert.equal(n.file.id,b.id);assert.equal(n.text,'B body');
    assert.equal(f.get('note-stage-title').textContent,'Note B');
});


for(const initial of ['existing','new'])for(const next of ['new draft','closed','same note edit','unchanged']){
    if(initial==='new'&&next==='same note edit')continue;
    test('late '+initial+' note save refresh preserves '+next,async()=>{
        const f=fixture(),n=f.h.noteEditor,a={id:1,rel_path:'notes/a.json',file_type:'NOTE',display_name:'Note A',tags:[]};
        f.respond(async()=>new Response(JSON.stringify({revision:'r1',title:'Note A',body:'old A'})));
        if(initial==='existing'){f.h.setFiles([a]);await f.h.selectMedia(a);await n.read(true);}else n.openNew();
        f.get('notes-title').value='Note A';f.get('notes-body').value='updated A';let release;
        f.respond(async(url,options)=>{
            if(url.includes('/api/files?'))return new Promise(r=>release=r);
            if(url.includes('/note-saves/'))return new Response(JSON.stringify({job:{job_id:'save-A',running:false,completed:true,result_id:1}}));
            if(options.method==='POST')return new Response('{"status":"started","job_id":"save-A"}');
            if(url.includes('/notes/1'))return new Response(JSON.stringify({revision:'r2',title:'Note A',body:'updated A'}));
            return new Response('{"count":0}');
        });
        const saving=n.submit();await new Promise(setImmediate);assert.equal(typeof release,'function');
        if(next==='new draft'){n.close();n.openNew();f.get('notes-body').value='UNSAVED B';}
        else if(next==='closed')n.close();
        else if(next==='same note edit'){await n.read(true);f.get('notes-body').value='UNSAVED A2';}
        const confirmations=f.confirmations.length;
        release(new Response(JSON.stringify([a])));await saving;
        assert.equal(f.confirmations.length,confirmations);
        if(next==='new draft'){assert.equal(n.file,null);assert.equal(f.get('notes-body').value,'UNSAVED B');assert.equal(n.isOpen(),true);}
        else if(next==='closed')assert.equal(n.isOpen(),false);
        else if(next==='same note edit'){assert.equal(n.file.id,1);assert.equal(f.get('notes-body').value,'UNSAVED A2');}
        else {assert.equal(n.file.id,1);assert.equal(n.text,'updated A');assert.equal(n.isOpen(),true);}
    });
}


for(const transition of ['vote','pass'])test('arena blocks old pair votes throughout '+transition+' transition',async()=>{
 const f=fixture(),pair=[1,2].map(id=>({id,rel_path:id+'.png',file_type:'IMAGE',tags:[],score:1000}));
 f.h.setFiles(pair);f.h.setArenaMode();f.respond(async()=>new Response(JSON.stringify(pair)));
 await f.h.loadArenaPair();let release;
 f.respond(async(url)=>{
  if(url.includes('/arena/pair'))return new Promise(r=>release=r);
  if(url.includes('/arena/vote'))return new Response(JSON.stringify({winner_score:1016,loser_score:984,winner_delta:16,loser_delta:-16}));
  return new Response(JSON.stringify(url.includes('/api/files?')?pair:{count:0}));
 });
 if(transition==='vote')await f.h.handleVote(1,2,'A');else await f.h.handleArenaPass();
 const before=f.calls.filter(([url])=>url.includes('/arena/vote')).length;
 const timer=[...f.timers.values()].find(t=>t.ms===(transition==='vote'?550:220));timer.fn();await new Promise(setImmediate);
 assert.equal(typeof release,'function');assert.equal(f.get('btn-arena-pass').disabled,true);
 f.get('arena-card-a').click();f.h.handleShortcut(keyEvent('KeyB'));await new Promise(setImmediate);
 assert.equal(f.calls.filter(([url])=>url.includes('/arena/vote')).length,before);
 const next=pair.map(x=>({...x,id:x.id+2}));release(new Response(JSON.stringify(next)));await new Promise(setImmediate);
 assert.equal(f.get('btn-arena-pass').disabled,false);
 await f.h.handleVote(3,4,'A');assert.equal(f.calls.filter(([url])=>url.includes('/arena/vote')).length,before+1);
});

test('failed arena loading keeps old cards unvoteable and permits a retry',async()=>{
 const f=fixture(),pair=[1,2].map(id=>({id,rel_path:id+'.png',file_type:'IMAGE',tags:[]}));
 f.h.setArenaMode();f.respond(async()=>new Response(JSON.stringify(pair)));await f.h.loadArenaPair();
 f.respond(async()=>new Response('offline',{status:503}));await f.h.loadArenaPair();
 f.get('arena-card-a').click();f.h.handleShortcut(keyEvent('KeyV'));await new Promise(setImmediate);
 assert.equal(f.calls.filter(([u])=>u.includes('/arena/vote')).length,0);
 assert.equal(f.get('btn-arena-pass').disabled,false);
 f.respond(async()=>new Response(JSON.stringify(pair)));await f.h.loadArenaPair();
 assert.equal(f.get('arena-card-a')['aria-disabled'],'false');
});

for(const fail of [false,true])test('late arena vote '+(fail?'failure':'success')+' cannot change a later match',async()=>{
 const f=fixture(),pair=base=>[1,2].map(id=>({id:id+base,rel_path:(id+base)+'.png',file_type:'IMAGE',tags:[],score:1000}));
 f.h.setArenaMode();f.respond(async()=>new Response(JSON.stringify(pair(0))));await f.h.loadArenaPair();let reply;
 f.respond(async()=>new Promise(r=>reply=r));const voting=f.h.handleVote(1,2,'A');await new Promise(setImmediate);
 f.respond(async url=>new Response(JSON.stringify(url.includes('/arena/pair')?pair(2):[])));f.h.exitArena();f.h.setArenaMode();await f.h.loadArenaPair();
 let next;f.respond(async()=>new Promise(r=>next=r));const loading=f.h.loadArenaPair();await new Promise(setImmediate);
 reply(fail?new Response('offline',{status:503}):new Response(JSON.stringify({winner_score:1016,loser_score:984,winner_delta:16,loser_delta:-16})));
 await voting;
 assert.equal(f.get('btn-arena-pass').disabled,true);assert.equal(f.get('arena-score-a').textContent,1000);
 assert.equal([...f.timers.values()].filter(t=>t.ms===550).length,0);
 next(new Response(JSON.stringify(pair(4))));await loading;
});
test('old trash response cannot resurrect a restored card',async()=>{
 const f=fixture(),a={id:1,rel_path:'restored.png',file_type:'IMAGE',file_size:7,score:1000,tags:[]};let reply;
 f.respond(async()=>new Promise(r=>reply=r));const old=f.h.loadTrashItems();await new Promise(setImmediate);
 f.respond(async(url,options)=>new Response(JSON.stringify(url.includes('/trash/count')?{count:0}:url.includes('/api/files?')?[a]:[])));
 await f.h.restoreFromTrash(1);
 assert.equal(Number(f.get('trash-count').textContent),0);
 reply(new Response(JSON.stringify([a])));await old;
 assert.equal(Number(f.get('trash-count').textContent),0);assert.equal(f.get('trash-items-grid').children.length,0);
});
test('bulk empty-trash blocks concurrent restore and individual delete actions',async()=>{
 const f=fixture(),files=[1,2].map(id=>({id,rel_path:id+'.png',file_type:'IMAGE',file_size:7,tags:[]}));let release;let restored=false;
 f.respond(async(url,options)=>{
  if(url.includes('/1/permanent'))return new Promise(r=>release=r);
  if(url.includes('/2/restore')){restored=true;return new Response('{}');}
  return new Response(JSON.stringify(url.includes('/api/trash?')?files:url.includes('/api/files?')?[]:url.includes('/trash/count')?{count:0}:[]));
 });
 const clearing=f.h.emptyTrash();await new Promise(setImmediate);assert.equal(typeof release,'function');
 await f.h.restoreFromTrash(2);assert.equal(restored,false);
 await f.h.permanentlyDelete(2);assert.ok(!f.calls.some(([u])=>u.includes('/2/permanent')));
 release(new Response('{}'));await clearing;
 assert.ok(f.calls.some(([u,o])=>u.includes('/2/permanent')&&o.method==='DELETE'));
});


test('old count responses cannot overwrite a newer trash list or completed mutation',async()=>{
 const f=fixture();let reply;
 f.respond(async()=>new Promise(r=>reply=r));const old=f.h.updateTrashCount();await new Promise(setImmediate);
 f.respond(async()=>new Response('[]'));await f.h.loadTrashItems();
 reply(new Response('{"count":9}'));await old;
 assert.equal(Number(f.get('trash-count').textContent),0);
 assert.equal(Number(f.get('sidebar-trash-count').textContent),0);
});

test('older trash list cannot overwrite the latest list even in the same library',async()=>{
 const f=fixture(),item={id:1,rel_path:'old.png',file_type:'IMAGE',file_size:1};let reply;
 f.respond(async()=>new Promise(r=>reply=r));const old=f.h.loadTrashItems();await new Promise(setImmediate);
 f.respond(async()=>new Response('[]'));await f.h.loadTrashItems();
 reply(new Response(JSON.stringify([item])));await old;
 assert.equal(f.get('trash-items-grid').children.length,0);assert.equal(Number(f.get('trash-count').textContent),0);
});

for(const action of ['restore','permanent'])test(action+' prevents a competing empty-trash operation and reconciles failure',async()=>{
 const f=fixture(),item={id:1,rel_path:'keep.png',file_type:'IMAGE',file_size:1};let reply;
 f.respond(async(u,o)=>o.method==='POST'||o.method==='DELETE'?new Promise(r=>reply=r):new Response(JSON.stringify(u.includes('/api/trash?')?[item]:u.includes('/trash/count')?{count:1}:[])));
 const work=action==='restore'?f.h.restoreFromTrash(1):f.h.permanentlyDelete(1);await new Promise(setImmediate);
 await f.h.emptyTrash();assert.equal(f.calls.filter(([u])=>u.includes('/api/trash?')).length,0);
 reply(new Response('operation failed',{status:409}));await work;
 assert.equal(f.get('btn-empty-trash').disabled,false);assert.equal(f.get('trash-items-grid').children.length,1);
 assert.match(f.get('trash-items-status').textContent,/operation failed/);
});

const reviewTick=()=>new Promise(r=>setImmediate(r));
test('delayed list cannot restore reset watch count',async()=>{
 const f=fixture();f.h.privacy.covered=false;const file={id:1,rel_path:'movie.mp4',file_type:'VIDEO',tags:[],watch_count:5,last_watched_at:123};
 f.h.setFiles([file]);f.h.setCurrent(file);let finish;
 f.respond(async url=>url.startsWith('/api/files?')?new Promise(r=>finish=r):new Response(JSON.stringify({count:0})));
 const pending=f.h.loadMediaFiles();await reviewTick();await f.h.resetWatchCount(1);
 assert.equal(f.h.mediaVirtualList.items[0].watch_count,0);
 finish(new Response(JSON.stringify([{...file,watch_count:5,last_watched_at:123}])));await pending;
 assert.equal(f.h.mediaVirtualList.items[0].watch_count,0);
 });
test('delayed list cannot remove a newly recorded watch',async()=>{
 const f=fixture();f.h.privacy.covered=false;const file={id:1,rel_path:'movie.mp4',file_type:'VIDEO',tags:[],watch_count:0};
 f.h.setFiles([file]);f.respond(async()=>new Response('[]'));await f.h.selectMedia(file);
 let finish;f.respond(async url=>url.startsWith('/api/files?')?new Promise(r=>finish=r):new Response(JSON.stringify({count:0})));
 const pending=f.h.loadMediaFiles();await reviewTick();
 const v=f.get('main-video');Object.assign(v,{currentTime:0,paused:false,seeking:false,playbackRate:1,readyState:4});v.listeners.playing();
 for(let i=0;i<10;i++){f.advance(1000);v.currentTime++;v.listeners.timeupdate();}await reviewTick();
 assert.equal(f.h.mediaVirtualList.items[0].watch_count,1);
 finish(new Response(JSON.stringify([{...file,watch_count:0}])));await pending;
 assert.equal(f.h.mediaVirtualList.items[0].watch_count,1);
 });

test('a list observing a committed watch before its response cannot double count',async()=>{
 const {f,step}=await watchVideoFixture();f.h.privacy.covered=false;
 const file={id:1,rel_path:'movie.mp4',file_type:'VIDEO',tags:[],watch_count:0};f.h.setFiles([file]);f.h.setCurrent(file);
 let finish;f.respond(async url=>url.includes('/watch?')?new Promise(r=>finish=r):new Response(JSON.stringify(url.startsWith('/api/files?')?[{...file,watch_count:1}]:{count:0})));
 step(10);await reviewTick();await f.h.loadMediaFiles();
 assert.equal(f.h.mediaVirtualList.items[0].watch_count,0);
 finish(new Response('{}'));await reviewTick();assert.equal(f.h.mediaVirtualList.items[0].watch_count,1);
});
for(const failure of [false,true])test('lock conceals immediately while response is pending'+(failure?' and stays hidden on failure':''),async()=>{
 const f=fixture();f.h.privacy.covered=false;f.respond(async()=>new Response('[]'));
 const file={id:1,rel_path:'movie.mp4',file_type:'VIDEO',tags:[]};f.h.setFiles([file]);await f.h.selectMedia(file);
 let finish;f.respond(async url=>url.startsWith('/api/security/lock')?new Promise(r=>finish=r):new Response('{"images":[]}'));
 const pending=f.h.dbSecurity.perform('/api/security/lock');
 assert.equal(f.h.privacy.covered,true);assert.equal(f.h.mediaVirtualList.items.length,0);assert.equal(f.get('main-video').src,'');
 finish(new Response(failure?'synthetic lock failure':JSON.stringify({generation:2,covered:true}),{status:failure?400:200}));await pending;
 assert.equal(f.h.privacy.covered,true);assert.equal(f.h.mediaVirtualList.items.length,0);
 assert.equal(f.calls.some(([url])=>url.startsWith('/api/session')),false);
});


test('rejected lock can be retried from hidden entry without revealing media or error paths',async()=>{
 const f=fixture(),p=f.h.privacy;let busy=true,locks=0;
 f.respond(async url=>{
  if(url.startsWith('/api/session'))return new Response(JSON.stringify({generation:1,covered:false,security:{encrypted:true,locked:false}}));
  if(url.startsWith('/api/security/lock')){locks++;return busy?new Response('D:/private/busy',{status:409}):new Response('{"generation":2,"covered":true}');}
  if(url.startsWith('/api/security/folders')){assert.match(url,/library=2/);return new Response('{"folders":[]}');}
  return new Response('{}');
 });
 await f.h.dbSecurity.perform('/api/security/lock');await p.openEntry();
 assert.equal(locks,2);assert.equal(p.covered,true);assert.equal(p.pickFolder.disabled,true);assert.equal(p.retry.hidden,false);
 assert.equal(p.message.textContent.includes('private'),false);await p.chooseLibrary();
 assert.equal(f.calls.some(([url])=>url.startsWith('/api/security/select-folder')),false);
 busy=false;await p.retry.click();
 assert.equal(locks,3);assert.equal(p.retry.hidden,true);assert.equal(p.pickFolder.disabled,false);assert.equal(p.lockUnconfirmed,false);
 assert.equal(p.covered,true);assert.equal(f.h.mediaVirtualList.items.length,0);
 assert.equal(f.calls.some(([url])=>url.startsWith('/api/files')),false);
});

test('lost successful lock response reconciles generation without relocking or exposing the library',async()=>{
 const f=fixture(),p=f.h.privacy;let locks=0;
 f.respond(async url=>{
  if(url.startsWith('/api/security/lock')){locks++;throw new TypeError('lost response');}
  if(url.startsWith('/api/session'))return new Response('{"generation":2,"covered":true}');
  if(url.startsWith('/api/security/folders')){assert.match(url,/library=2/);return new Response('{"folders":[]}');}
  return new Response('{}');
 });
 await f.h.dbSecurity.perform('/api/security/lock');await p.openEntry();
 assert.equal(locks,1);assert.equal(p.lockUnconfirmed,false);assert.equal(p.pickFolder.disabled,false);assert.equal(p.covered,true);
});

test('hidden entry prevents duplicate pending lock retries and permits retry after dismissal',async()=>{
 const f=fixture(),p=f.h.privacy;let finish,locks=0;
 f.respond(async url=>{
  if(url.startsWith('/api/security/lock')){locks++;return new Response('busy',{status:409});}
  if(url.startsWith('/api/session'))return new Promise(r=>finish=r);
  return new Response('{"folders":[]}');
 });
 await f.h.dbSecurity.perform('/api/security/lock');const pending=p.openEntry();
 await p.openEntry();assert.equal(f.calls.filter(([url])=>url.startsWith('/api/session')).length,1);
 p.dismiss();finish(new Response('{"generation":1,"covered":false}'));await pending;
 assert.equal(locks,1);assert.equal(p.isOpen(),false);assert.equal(p.lockUnconfirmed,true);assert.equal(p.covered,true);
});

test('archive page editor links to explicit original cleanup without submitting deletion',async()=>{
 const f=fixture(),p=f.h.pageEditor;f.respond(async()=>new Response('{"files":[]}'));
 p.open({id:1,rel_path:'series.zip',file_type:'ARCHIVE',encrypted:true},['1.png','2.png'],0);
 p.setPending(true);await p.manageOriginal.click();assert.equal(p.isOpen(),true);assert.equal(f.calls.length,0);
 p.setPending(false);await p.manageOriginal.click();assert.equal(p.isOpen(),false);assert.equal(f.h.archiveSecurity.isOpen(),true);
 assert.ok(f.calls.every(([,options])=>!options.method||options.method==='GET'));
});


test('late scan list does not erase a new note or select media until the editor closes',async()=>{
 const f=fixture(),n=f.h.noteEditor;let release;
 const file={id:1,rel_path:'example.png',file_type:'IMAGE',tags:[]};
 f.respond(async url=>url.startsWith('/api/files?')?new Promise(r=>release=r):new Response(url.includes('bookmarks')?'[]':'{"count":0}'));
 const loading=f.h.loadMediaFiles();n.openNew();f.get('notes-body').value='Unsaved recovery codes';
 release(new Response(JSON.stringify([file])));await loading;
 assert.equal(n.isOpen(),true);assert.equal(f.get('notes-body').value,'Unsaved recovery codes');assert.equal(f.get('viewer-image').src||'','');
 n.close();f.respond(async url=>new Response(JSON.stringify(url.startsWith('/api/files?')?[file]:url.includes('bookmarks')?[]:{count:0})));
 await f.h.loadMediaFiles();assert.equal(n.isOpen(),false);assert.match(f.get('viewer-image').src,/\/api\/media\/example\.png/);
});

for(const lost of [false,true])test('library close stays hidden and hidden entry retries the close action: '+(lost?'lost success':'rejected'),async()=>{
 const f=fixture(),p=f.h.privacy;let closeCalls=0,finish;let confirmed=lost,plays=0;
 const file={id:1,rel_path:'private-example.mp4',file_type:'VIDEO',tags:[]};
 f.get('main-video').play=()=>{plays++;return Promise.resolve();};f.h.setFiles([file]);f.h.setCurrent(file);
 f.respond(async url=>{
  if(url.startsWith('/api/folders/close')){closeCalls++;if(closeCalls===1)return new Promise((resolve,reject)=>finish=()=>lost?reject(new TypeError('lost')):resolve(new Response('private/path',{status:400})));return new Response('{"generation":2,"covered":true}');}
  if(url.startsWith('/api/session')){assert.ok(url.includes('concealment=true'));return new Response(JSON.stringify({generation:confirmed?2:1,covered:confirmed}));}
  if(url.startsWith('/api/security/folders'))return new Response('{"folders":[]}');
  return new Response('{}');
 });
 const closing=f.get('btn-close-library').click();assert.equal(p.covered,true);assert.equal(f.h.mediaVirtualList.items.length,0);
 await p.openEntry();assert.equal(p.retry.hidden,false);assert.equal(p.pickFolder.disabled,true);assert.equal(closeCalls,1);
 finish();await closing;assert.equal(p.covered,true);assert.equal(plays,0);
 await p.retry.click();assert.equal(p.covered,true);assert.equal(p.closeUnconfirmed,false);assert.equal(p.pickFolder.disabled,false);
 assert.equal(closeCalls,lost?1:2);assert.equal(f.calls.some(([url])=>url.startsWith('/api/security/lock')||url.startsWith('/api/files?')),false);
});

for(const kind of ['restore','encrypt'])for(const destination of ['same','closed','another'])
test(kind+' completion stays in its own library: '+destination,async()=>{
 const f=fixture();let release;
 if(kind==='restore'){
  f.get('btn-restore-db').click();
  f.respond(async()=>new Response('{"status":"selected","token":"synthetic","name":"test.vtbackup","encrypted":false}'));
  await f.h.dbRestore.choose();
 }
 f.respond(async url=>{
  if(url.startsWith('/api/backup/restore')||url.startsWith('/api/security/encrypt'))return new Response(JSON.stringify({status:'success',generation:2,covered:false,restore_path:'D:/synthetic/restore',security:{encrypted:true,locked:false,plaintext_original:true}}));
  if(url.startsWith('/api/folders/close'))return new Response('{"generation":3,"covered":true}');
  if(url.startsWith('/api/folders/rescan'))return new Response('{"running":false}');
  if(url.startsWith('/api/folders'))return new Promise(resolve=>release=resolve);
  return new Response('[]');
 });
 f.get('db-encryption-risk-ack').checked=true;
 const pending=kind==='restore'?f.h.dbRestore.restore():f.h.dbSecurity.perform('/api/security/encrypt',{password:'synthetic-password',confirmation:'synthetic-password'});
 await new Promise(setImmediate);assert.equal(typeof release,'function');
 if(destination!=='same')await f.get('btn-close-library').click();
 if(destination==='another')await f.h.adoptLibraryState({generation:4,covered:false,security:{encrypted:false,locked:false}});
 release(new Response('{"folders":[]}'));await pending;
 const dialog=kind==='restore'?f.h.dbRestore:f.h.dbSecurity;
 assert.equal(dialog.isOpen(),destination==='same');
 const rescans=f.calls.filter(([url])=>url.startsWith('/api/folders/rescan'));
 assert.equal(rescans.length,kind==='encrypt'&&destination==='same'?1:0);
 if(rescans.length)assert.equal(new URL(rescans[0][0],'http://localhost').searchParams.get('library'),'2');
 if(kind==='restore'&&destination==='same')assert.match(f.h.dbRestore.status.textContent,/synthetic/);
 assert.equal(f.h.privacy.covered,destination==='closed');
});

for(const kind of ['restore','encrypt'])test(kind+' error resync cannot undo a later library close',async()=>{
 const f=fixture();let release;
 if(kind==='encrypt')f.h.dbSecurity.open();
 if(kind==='restore'){
  f.get('btn-restore-db').click();
  f.respond(async()=>new Response('{"status":"selected","token":"synthetic","name":"test.vtbackup","encrypted":false}'));
  await f.h.dbRestore.choose();
 }
 f.respond(async url=>{
  if(url.startsWith('/api/backup/restore')||url.startsWith('/api/security/encrypt'))return new Response('synthetic failure',{status:500});
  if(url.startsWith('/api/session'))return new Promise(resolve=>release=resolve);
  if(url.startsWith('/api/folders/close'))return new Response('{"generation":2,"covered":true}');
  return new Response('[]');
 });
 f.get('db-encryption-risk-ack').checked=true;
 const pending=kind==='restore'?f.h.dbRestore.restore():f.h.dbSecurity.perform('/api/security/encrypt',{password:'synthetic-password',confirmation:'synthetic-password'});
 await new Promise(setImmediate);assert.equal(typeof release,'function');
 await f.get('btn-close-library').click();
 release(new Response('{"generation":1,"covered":false,"security":{"encrypted":false,"locked":false}}'));await pending;
 assert.equal(f.h.privacy.covered,true);
 assert.equal(f.h.dbRestore.isOpen(),false);assert.equal(f.h.dbSecurity.isOpen(),false);
 assert.equal(f.calls.filter(([url])=>url.startsWith('/api/folders/rescan')).length,0);
});

for(const kind of ['restore','encrypt'])test(kind+' allows hiding pending work without cancelling it or reopening its result',async()=>{
 const f=fixture();let release;
 const dialog=kind==='restore'?f.h.dbRestore:f.h.dbSecurity;
 if(kind==='restore'){
  f.get('btn-restore-db').click();
  f.respond(async()=>new Response('{"status":"selected","token":"synthetic","name":"test.vtbackup","encrypted":false}'));
  await dialog.choose();
 }else dialog.open();
 f.respond(async url=>{
  if(url.startsWith('/api/backup/restore')||url.startsWith('/api/security/encrypt'))return new Promise(resolve=>release=resolve);
  if(url.startsWith('/api/folders/rescan'))return new Response('{"running":false}');
  return new Response('[]');
 });
 f.get('db-encryption-risk-ack').checked=true;
 const pending=kind==='restore'?dialog.restore():dialog.perform('/api/security/encrypt',{password:'synthetic-password',confirmation:'synthetic-password'});
 await new Promise(setImmediate);
 assert.notEqual(dialog.close.disabled,true);dialog.close.click();
 assert.equal(dialog.isOpen(),false);assert.equal(dialog.pending,true);
 release(new Response(JSON.stringify({status:'success',generation:2,covered:false,restore_path:'D:/synthetic/restore',security:{encrypted:true,locked:false,plaintext_original:true}})));
 await pending;assert.equal(dialog.pending,false);assert.equal(dialog.isOpen(),false);
 assert.equal(f.h.dbSecurity.state.encrypted,true);
});

for(const destination of ['same','closed','another'])test('hidden entry refresh stays in its library: '+destination,async()=>{
 const f=fixture(),p=f.h.privacy;let release;
 await f.h.adoptLibraryState({generation:1,covered:true});
 f.respond(async()=>new Response('{"folders":[{"name":"A","path":"D:/synthetic/A","encrypted":true}],"active_folder":"D:/synthetic/A"}'));
 await p.openEntry();p.password.value='synthetic password';
 f.respond(async url=>{
  if(url.startsWith('/api/security/open-folder'))return new Response('{"generation":2,"covered":false,"security":{"encrypted":true,"locked":false}}');
  if(url.startsWith('/api/folders/close'))return new Response('{"generation":3,"covered":true}');
  if(url.startsWith('/api/folders/rescan'))return new Response('{"running":false}');
  if(url.startsWith('/api/folders'))return new Promise(resolve=>release=resolve);
  return new Response('[]');
 });
 const revealing=p.reveal();await new Promise(setImmediate);
 assert.equal(typeof release,'function');assert.equal(p.covered,false);
 if(destination!=='same')await f.get('btn-close-library').click();
 if(destination==='another')await f.h.adoptLibraryState({generation:4,covered:false,security:{encrypted:false,locked:false}});
 release(new Response('{"folders":[]}'));await revealing;
 const rescans=f.calls.filter(([url])=>url.startsWith('/api/folders/rescan'));
 assert.equal(rescans.length,destination==='same'?1:0);
 if(rescans.length)assert.equal(new URL(rescans[0][0],'http://localhost').searchParams.get('library'),'2');
 assert.equal(p.covered,destination==='closed');assert.equal(p.pending,false);
});

for(const error of [false,true])test('dismissing a pending hidden entry revokes it and ignores late '+(error?'failure':'success'),async()=>{
 const f=fixture(),p=f.h.privacy;let release;
 await f.h.adoptLibraryState({generation:1,covered:true});
 f.respond(async()=>new Response('{"folders":[{"name":"A","path":"D:/synthetic/A","encrypted":true}],"active_folder":"D:/synthetic/A"}'));
 await p.openEntry();p.password.value='synthetic password';
 f.respond(async url=>{
  if(url.startsWith('/api/security/open-folder'))return new Promise(resolve=>release=resolve);
  if(url.startsWith('/api/folders/close'))return new Response('{"generation":2,"covered":true}');
  return new Response('[]');
 });
 const revealing=p.reveal();await new Promise(setImmediate);
 assert.equal(p.close.disabled,false);await p.close.click();
 assert.equal(p.isOpen(),false);assert.equal(p.covered,true);
 release(error?new Response('synthetic failure',{status:500}):new Response('{"generation":2,"covered":false,"security":{"encrypted":true,"locked":false}}'));
 await revealing;
 assert.equal(p.covered,true);assert.equal(p.isOpen(),false);assert.equal(p.pending,false);
 assert.equal(f.calls.filter(([url])=>url.startsWith('/api/folders/close')).length,1);
 assert.equal(f.calls.filter(([url])=>url.startsWith('/api/folders/rescan')||url.startsWith('/api/session')).length,0);
 assert.equal(p.password.value,'');
});

for(const language of ['ko','en','ja'])test(`DB encryption cannot start without explicit risk acknowledgement (${language})`,async()=>{
    const f=fixture(language),s=f.h.dbSecurity;
    s.open();s.password.value=s.confirmation.value='synthetic password';
    assert.equal(s.risk.hidden,false);assert.equal(s.riskAck.checked,false);assert.equal(s.submit.disabled,true);
    await s.perform('/api/security/encrypt',{password:s.password.value,confirmation:s.confirmation.value});
    assert.equal(f.calls.length,0);assert.equal(f.confirmations.length,0);
    assert.equal(s.password.value,'synthetic password');assert.equal(s.riskAck.focused,true);
    if(language!=='ko')assert.ok(!/[가-힣]/.test(s.message.textContent));
    s.riskAck.checked=true;s.riskAck.listeners.change();assert.equal(s.submit.disabled,false);
    f.confirm(false);await s.perform('/api/security/encrypt',{password:s.password.value,confirmation:s.confirmation.value});
    assert.equal(f.calls.length,0);assert.equal(f.confirmations.length,1);
    s.dismiss();s.open();assert.equal(s.riskAck.checked,false);assert.equal(s.submit.disabled,true);
    s.state={encrypted:true,locked:true};s.open();assert.equal(s.risk.hidden,true);assert.equal(s.submit.disabled,false);
    s.state={encrypted:true,locked:false};s.open(true);assert.equal(s.risk.hidden,true);assert.equal(s.submit.disabled,false);
});

for(const language of ['ko','en','ja'])test(`media encryption acknowledgement gates batch and single actions and resets on reopen (${language})`,async()=>{
    const f=fixture(language),s=f.h.archiveSecurity;
    s.files=[{id:1,state:'plain',name:'synthetic.zip'}];s.selected.add(1);s.updateActions();
    assert.equal(s.run.disabled,true);
    await s.batch();await s.single(s.files[0],'encrypt');
    assert.equal(f.calls.length,0);assert.equal(f.confirmations.length,0);assert.equal(s.riskAck.focused,true);
    if(language!=='ko')assert.ok(!/[가-힣]/.test(s.status.textContent));
    s.riskAck.checked=true;s.riskAck.listeners.change();assert.equal(s.run.disabled,false);
    f.confirm(false);await s.batch();assert.equal(f.calls.length,0);assert.equal(f.confirmations.length,1);
    s.reset();assert.equal(s.riskAck.checked,false);
    s.riskAck.checked=true;await s.open();assert.equal(s.riskAck.checked,false);
});

test('prominent DB and media warning text is translated and never acknowledges risks by default',()=>{
    const html=fs.readFileSync(path.join(__dirname,'../static/index.html'),'utf8');
    const catalog=JSON.parse(fs.readFileSync(path.join(__dirname,'../static/i18n-catalog.json'),'utf8'));
    for(const kind of ['db','media']){
        const panel=html.match(new RegExp(`<section id="${kind}-encryption-risk"[\\s\\S]*?</section>`))[0];
        assert.ok(!/<input[^>]*\bchecked\b/.test(panel));
        const strings=[...panel.matchAll(/<(?:h3|li|span)[^>]*>([^<]+)<\//g)].map(m=>m[1]);
        assert.ok(strings.length>=6);
        for(const text of strings)for(const language of ['en','ja'])assert.ok(catalog[language][text],`${language}: ${text}`);
    }
});

test('PDF progress slider previews without rendering and resumes persisted pages including spreads',async()=>{
    const f=fixture(),pdf=fakePdf(f,{numPages:9}),v=f.h.pdfViewer;
    let saved=6;const writes=[];
    f.respond(async(url,options)=>{
        if(url.includes('/pdf-position')){
            if(options.method==='POST'){saved=JSON.parse(options.body).page;writes.push(saved);return new Response(null,{status:204});}
            return new Response(JSON.stringify({page:saved}));
        }
        return new Response(url.includes('/bookmarks')?'[]':'{}');
    });
    const file={id:1,rel_path:'resume.pdf',file_type:'PDF',thumbnail_data:'saved',tags:[]};
    await v.open(file,'/resume.pdf');assert.equal(v.page,6);
    const seek=f.get('pdf-seek-bar');assert.equal(seek.max,'9');assert.equal(seek.value,'6');
    const renders=pdf.renders.length;seek.value='8';seek.listeners.input();
    assert.equal(pdf.renders.length,renders);assert.equal(v.page,6);assert.match(f.get('pdf-seek-label').textContent,/8 \/ 9/);
    await seek.listeners.change();await v.positionRecord.tail;assert.equal(v.page,8);assert.equal(saved,8);
    await runPdfPrefetch(f);assert.equal(saved,8);assert.deepEqual(writes,[8],'prefetch is not a displayed page');
    v.reset(true);f.get('pdf-layout').value='2';await v.open(file,'/resume.pdf');
    await v.positionRecord.tail;assert.equal(v.page,7);assert.equal(saved,7);assert.equal(seek.value,'7');
    v.reset(true);saved=500;await v.open(file,'/resume.pdf');await v.positionRecord.tail;
    assert.equal(v.page,9);assert.equal(saved,9,'shortened documents clamp their saved position');
});

test('PDF saves are ordered when a user returns to the saved page while another save is pending',async()=>{
    const f=fixture();fakePdf(f);const v=f.h.pdfViewer,writes=[];let finish;
    f.respond(async(url,options)=>{
        if(url.includes('/pdf-position') && options.method==='POST'){
            const page=JSON.parse(options.body).page;writes.push(page);
            if(page===2)return new Promise(resolve=>finish=()=>resolve(new Response('{}')));
        }
        return new Response(url.includes('/bookmarks')?'[]':JSON.stringify({page:1}));
    });
    await v.open({id:1,rel_path:'ordered.pdf',thumbnail_data:'saved'},'/ordered.pdf');
    await v.show(2);await v.show(1);assert.deepEqual(writes,[2]);
    finish();await v.positionRecord.tail;assert.deepEqual(writes,[2,1]);
    await v.show(1);await v.positionRecord.tail;assert.deepEqual(writes,[2,1]);
});

test('failed PDF resume lookup never overwrites history on initial render or resize',async()=>{
    const f=fixture('en');fakePdf(f);const v=f.h.pdfViewer,writes=[];
    f.respond(async(url,options)=>{
        if(url.includes('/pdf-position')){
            if(options.method==='POST'){writes.push(JSON.parse(options.body).page);return new Response('{}');}
            return new Response('unavailable',{status:503});
        }
        return new Response(url.includes('/bookmarks')?'[]':'{}');
    });
    await v.open({id:1,rel_path:'offline.pdf',thumbnail_data:'saved'},'/offline.pdf');
    assert.equal(v.page,1);assert.match(f.get('pdf-status').textContent,/Could not load/);
    f.get('pdf-pages').clientHeight+=10;await v.show(1);await v.positionRecord.tail;assert.deepEqual(writes,[]);
    await v.show(2);await v.positionRecord.tail;assert.deepEqual(writes,[2]);
});

test('PDF resume lookup and queued writes cannot cross a library change',async()=>{
    const f=fixture();fakePdf(f);const v=f.h.pdfViewer;let finish;
    f.respond(async(url,options)=>{
        if(url.includes('/pdf-position'))return new Promise(resolve=>finish=resolve);
        return new Response('[]');
    });
    const opening=v.open({id:1,rel_path:'old.pdf',thumbnail_data:'saved'},'/old.pdf');
    await new Promise(setImmediate);f.h.resetLibraryView();f.h.setSession(2);
    finish(new Response('{"page":5}'));await opening;
    assert.equal(v.file,null);assert.equal(v.positions.size,0);assert.equal(f.get('pdf-seek-bar').disabled,true);
    fakePdf(f);await v.open({id:1,rel_path:'new.pdf',thumbnail_data:'saved'},'/new.pdf');
    const writes=[];let saveReply;
    f.respond(async(url,options)=>{
        if(options.method==='POST'){writes.push(JSON.parse(options.body).page);return new Promise(resolve=>saveReply=resolve);}
        return new Response('[]');
    });
    await v.show(2);await v.show(3);const queued=v.positionRecord.tail;
    f.h.invalidateRequests();f.h.resetLibraryView();f.h.setSession(3);
    saveReply(new Response('{}'));await queued;
    assert.deepEqual(writes,[2]);assert.equal(v.positions.size,0);
});

for(const stalledBody of [false,true])test(`PDF position deadline permits rendering and ignores late ${stalledBody?'body':'response'} results`,async()=>{
 const f=fixture('en'),pdf=fakePdf(f),v=f.h.pdfViewer;let finish;const writes=[];
 f.respond(async(url,options)=>{
  if(url.includes('/pdf-position')){
   if(options.method==='POST'){writes.push(JSON.parse(options.body).page);return new Response('{}');}
   if(stalledBody){const response=new Response('{}');response.json=()=>new Promise(resolve=>finish=resolve);return response;}
   return new Promise(resolve=>finish=resolve);
  }
  return new Response('[]');
 });
 const opening=v.open({id:1,rel_path:'timeout.pdf',thumbnail_data:'saved'},'/timeout.pdf');
 await new Promise(setImmediate);
 assert.ok(pdf.loading,'PDF document loading proceeds while its position is pending');
 const lookup=f.calls.find(([url])=>url.includes('/pdf-position'));
 const timer=[...f.timers.values()].find(t=>t.ms===1500);assert.ok(timer);timer.fn();
 await opening;assert.equal(v.page,1);assert.equal(v.ready,true);
 // vaultFetch releases its forwarded abort listener after headers; the deadline must also work when the body ignores abort.
 if(!stalledBody)assert.equal(lookup[1].signal.aborted,true);
 assert.equal(v.positionLookup,null);
 assert.match(f.get('pdf-status').textContent,/Could not load/);assert.deepEqual(writes,[]);
 await v.show(3);await v.positionRecord.tail;
 finish(stalledBody?{page:5}:new Response('{"page":5}'));await new Promise(setImmediate);
 assert.equal(v.page,3);assert.equal(v.positionRecord.page,3);assert.deepEqual(writes,[3]);
});

test('PDF retries a failed resume lookup on reopen without overwriting the stored page',async()=>{
 const f=fixture(),v=f.h.pdfViewer;fakePdf(f);let reads=0;const writes=[];
 f.respond(async(url,options)=>{
  if(url.includes('/pdf-position')){
   if(options.method==='POST'){writes.push(JSON.parse(options.body).page);return new Response('{}');}
   return ++reads===1?new Response('temporary failure',{status:503}):new Response('{"page":5}');
  }
  return new Response('[]');
 });
 const file={id:1,rel_path:'retry.pdf',thumbnail_data:'saved'};
 await v.open(file,'/retry.pdf');assert.equal(v.page,1);
 await v.open(file,'/retry.pdf');assert.equal(v.page,5);assert.equal(reads,2);assert.deepEqual(writes,[]);
});

test('PDF begins position lookup before engine startup and cancels it immediately on reset',async()=>{
 const f=fixture(),v=f.h.pdfViewer;const pdf=fakePdf(f);let engineReady,reply;
 const load=v.load;v.load=()=>new Promise(resolve=>engineReady=async()=>resolve(await load()));
 f.respond(async()=>new Promise(resolve=>reply=resolve));
 const opening=v.open({id:1,rel_path:'parallel.pdf',thumbnail_data:'saved'},'/parallel.pdf');
 await new Promise(setImmediate);
 assert.equal(f.calls.length,1);assert.ok(f.calls[0][0].includes('/pdf-position'));assert.equal(pdf.parameters,null);
 v.reset(true);assert.equal(f.calls[0][1].signal.aborted,true);
 await engineReady();await opening;
 reply(new Response('{"page":6}'));await new Promise(setImmediate);
 assert.equal(v.doc,null);assert.equal(v.positions.size,0);
 assert.equal([...f.timers.values()].filter(t=>t.ms===1500).length,0);
});

for(const committed of [false,true])test(`PDF timed-out saves reconcile revisions and cannot overwrite newer progress (committed=${committed})`,async()=>{
 const f=fixture('en');fakePdf(f);const v=f.h.pdfViewer;let late;let db={page:1,revision:0};const writes=[];
 f.respond(async(url,options)=>{
  if(!url.includes('/pdf-position'))return new Response('[]');
  if(options.method!=='POST')return new Response(JSON.stringify(db));
  const request=JSON.parse(options.body);writes.push(request);
  const commit=()=>{
   if(request.revision!==db.revision)return new Response('stale revision',{status:409});
   db={page:request.page,revision:db.revision+1};return new Response(null,{status:204});
  };
  if(request.page===2){const response=committed?commit():null;return new Promise(resolve=>late=()=>resolve(response||commit()));}
  return commit();
 });
 await v.open({id:1,rel_path:'race.pdf',thumbnail_data:'saved'},'/race.pdf');
 await v.show(2);await v.show(4);const record=v.positionRecord;
 const deadline=[...f.timers.values()].find(t=>t.ms===5000);assert.ok(deadline);deadline.fn();await record.tail;
 assert.equal(f.get('pdf-position-state').hidden,false);assert.match(f.get('pdf-position-status').textContent,/not been saved/);
 const retry=[...f.timers.values()].find(t=>t.ms===2000);assert.ok(retry);retry.fn();await record.tail;
 assert.equal(db.page,4);assert.equal(record.saved,4);assert.equal(record.revision,db.revision);
 assert.equal(f.get('pdf-position-state').hidden,true);
 late();await new Promise(setImmediate);assert.equal(db.page,4);assert.equal(record.saved,4);
 assert.equal(writes.length,2);assert.equal(writes[1].revision,committed?1:0);
});

test('PDF save retry budget is bounded, stays visible across rendering, and reset cancels pending work',async()=>{
 const f=fixture('en');fakePdf(f);const v=f.h.pdfViewer;
 f.respond(async(url,options)=>{
  if(url.includes('/pdf-position')&&options.method==='POST')return new Response('unavailable',{status:503});
  return new Response(url.includes('/pdf-position')?'{"page":1,"revision":0}':'[]');
 });
 await v.open({id:1,rel_path:'offline.pdf',thumbnail_data:'saved'},'/offline.pdf');await v.show(2);const record=v.positionRecord;await record.tail;
 for(let i=0;i<2;i++){const timer=[...f.timers.entries()].find(([,t])=>t.ms===2000);assert.ok(timer);f.timers.delete(timer[0]);timer[1].fn();await record.tail;}
 assert.equal(f.calls.filter(([,o])=>o.method==='POST').length,3);
 assert.equal([...f.timers.values()].filter(t=>t.ms===2000).length,0);
 f.get('pdf-pages').clientWidth+=10;await v.show(2);await record.tail;
 assert.equal(f.get('pdf-position-state').hidden,false);assert.equal(f.get('pdf-position-retry').disabled,false);
 let pending;
 f.respond(async(url,options)=>options.method==='POST'?new Promise(r=>pending=r):new Response('{"page":1,"revision":0}'));
 f.get('pdf-position-retry').click();await new Promise(setImmediate);assert.equal(record.syncing,true);
 v.reset(true);await record.tail;
 assert.equal(v.positions.size,0);assert.equal([...f.timers.values()].filter(t=>t.ms===2000||t.ms===5000).length,0);
 pending(new Response('{}'));await new Promise(setImmediate);assert.equal(f.get('pdf-position-state').hidden,true);
});

test('PDF oversized images fail visibly and never update the saved position',async()=>{
 const f=fixture('en');const pdf=fakePdf(f);const v=f.h.pdfViewer;
 const original=pdf.document.getPage;
 pdf.document.getPage=async n=>{if(n===2)throw new Error('Image exceeded maximum allowed size and was removed.');return original(n);};
 await v.open({id:1,rel_path:'oversized.pdf',thumbnail_data:'saved'},'/oversized.pdf');
 assert.equal(pdf.parameters.stopAtErrors,true);assert.equal(pdf.parameters.maxImageSize,67108864);assert.equal(pdf.parameters.canvasMaxAreaInBytes,33554432);
 await v.show(2);assert.equal(v.ready,false);assert.match(f.get('pdf-status').textContent,/resolution is too high/);
 assert.equal(v.positionRecord.saved,1);assert.equal(v.page,1);
});

for(const destination of ['note','arena','trash'])for(const timing of ['before completion','during list refresh']) {
 test('page deletion preserves '+destination+' entered '+timing,async()=>{
  const f=fixture('en'),file={id:1,rel_path:'book.zip',file_type:'ARCHIVE',encrypted:true,tags:[],file_size:2};
  let complete,releaseList;
  f.h.setFiles([file]);f.h.setCurrent(file);f.h.setArchive(['a.png','b.png'],0);
  f.respond(async url=>url.includes('/progress?')?new Promise(resolve=>complete=resolve):
   url.includes('/api/files?')?new Promise(resolve=>releaseList=resolve):new Response(JSON.stringify(
    url.includes('/1/page')?{status:'started',job_id:'delete-job'}:url.includes('/api/arena/pair')?[file,{...file,id:2,rel_path:'other.zip'}]:url.includes('/archive/list/')?['b.png']:[])));
  const editor=f.h.pageEditor;editor.open(file,['a.png','b.png'],0);
  const pending=editor.submit();await new Promise(setImmediate);
  await f.get('archive-page-close').click();
  const finishJob=async()=>{complete(new Response(JSON.stringify({job:{job_id:'delete-job',running:false,completed:true}})));await new Promise(setImmediate);assert.ok(releaseList);};
  if(timing==='during list refresh')await finishJob();
  if(destination==='note'){
   f.get('btn-new-note').click();f.get('notes-title').value='Unsaved title';f.get('notes-body').value='Unsaved private draft';
  }else{f.get(destination==='arena'?'btn-start-arena':'btn-open-trash').click();await new Promise(setImmediate);}
  if(timing==='before completion')await finishJob();
  releaseList(new Response(JSON.stringify([{...file,file_size:1}])));await pending;
  assert.equal(f.h.getFilteredFiles()[0].file_size,1,'List metadata must still refresh');
  assert.equal(editor.isOpen(),false);
  assert.equal(f.calls.filter(([url])=>url.includes('/archive/list/')).length,0,'Background completion must not select the archive');
  if(destination==='note'){
   assert.equal(f.h.noteEditor.isOpen(),true);assert.equal(f.get('notes-title').value,'Unsaved title');
   assert.equal(f.get('notes-body').value,'Unsaved private draft');assert.equal(f.confirmations.length,0);
  }else{
   assert.equal(f.get(destination+'-stage').style.display,'flex');
   assert.equal(f.get('image-viewer-stage').style.display,'none');
  }
 });
}

test('hidden page deletion refreshes the archive when its viewer remains current',async()=>{
 const f=fixture(),file={id:1,rel_path:'book.zip',file_type:'ARCHIVE',encrypted:true,tags:[]};let complete;
 f.h.setFiles([file]);f.h.setCurrent(file);f.h.setArchive(['a.png','b.png'],0);
 f.respond(async url=>url.includes('/progress?')?new Promise(resolve=>complete=resolve):new Response(JSON.stringify(
  url.includes('/1/page')?{status:'started',job_id:'delete-job'}:url.includes('/api/files?')?[file]:url.includes('/archive/list/')?['b.png']:[])));
 const editor=f.h.pageEditor;editor.open(file,['a.png','b.png'],0);
 const pending=editor.submit();await new Promise(setImmediate);await f.get('archive-page-close').click();
 complete(new Response(JSON.stringify({job:{job_id:'delete-job',running:false,completed:true}})));await pending;
 assert.equal(editor.isOpen(),false);assert.equal(f.get('image-viewer-stage').style.display,'flex');
 assert.equal(f.calls.filter(([url])=>url.includes('/archive/list/')).length,1);
});

for(const kind of ['name','pages']) {
 function editFixture(){
  const f=fixture('en'),editor=kind==='name'?f.h.nameEditor:f.h.pageEditor;
  const file={id:1,rel_path:'book.zip',file_type:'ARCHIVE',encrypted:true,tags:[]};
  const prefix=kind==='name'?'media-name':'archive-page';
  const open=(target=file)=>kind==='name'?editor.open(target):editor.open(target,['a.png','b.png'],0);
  open();if(kind==='name')f.get('media-name-input').value='renamed.zip';
  const mutations=()=>f.calls.filter(([url,opts])=>opts.method===(kind==='name'?'POST':'DELETE')&&url.includes(kind==='name'?'/1/name':'/1/page'));
  return {f,editor,file,prefix,open,mutations};
 }
 test(kind+' editor can close before the start response and resume without resubmitting',async()=>{
  const {f,editor,file,prefix,open,mutations}=editFixture();let start,progress;
  f.respond(async url=>new Promise(resolve=>{if(url.includes('/progress?'))progress=resolve;else start=resolve;}));
  const pending=editor.submit();await new Promise(setImmediate);
  assert.equal(f.get(prefix+'-close').disabled,false);
  assert.match(f.get(prefix+'-close').title,/Work continues/);
  await f.get(prefix+'-close').click();assert.equal(editor.isOpen(),false);
  open({...file,id:2,rel_path:'different.zip'});assert.equal(editor.isOpen(),true);
  assert.equal(editor.file.id,1);await editor.submit();assert.equal(mutations().length,1);
  if(kind==='name')assert.equal(f.get('media-name-input').value,'renamed.zip');
  else {assert.deepEqual([...editor.selected],['a.png']);assert.equal(f.get('archive-page-file').textContent,'book.zip');}
  start(new Response(JSON.stringify({status:'started',job_id:'nas-job'})));await new Promise(setImmediate);
  assert.equal(editor.jobId,'nas-job');assert.equal(editor.pending,true);
  await f.get(prefix+'-close').click();open();await editor.submit();assert.equal(mutations().length,1);
  progress(new Response(JSON.stringify({job:{job_id:'nas-job',running:false,error:'Cancelled; original preserved'}})));
  await pending;assert.equal(editor.pending,false);assert.equal(editor.file.id,1);
 });

 test(kind+' editor can leave a cancelled NAS job, navigate away and reopen its hidden error',async()=>{
  const {f,editor,file,prefix,open,mutations}=editFixture();let progress;
  f.respond(async url=>url.includes('/progress?')?new Promise(resolve=>progress=resolve):new Response(JSON.stringify(url.includes('/cancel?')?{status:'cancelling'}:{status:'started',job_id:'nas-job'})));
  const pending=editor.submit();await new Promise(setImmediate);
  await f.get(prefix+'-cancel').click();assert.ok(f.calls.some(([url])=>url.includes('/cancel?job_id=nas-job')));
  // A cancelled native read may still be waiting for the NAS to respond.
  editor.dialog.listeners.cancel({preventDefault(){}});assert.equal(editor.isOpen(),false);
  assert.equal(editor.pending,true);assert.equal(editor.jobId,'nas-job');
  await f.h.selectMedia({id:9,rel_path:'other.png',file_type:'IMAGE',thumbnail_data:'saved'});
  progress(new Response(JSON.stringify({job:{job_id:'nas-job',running:false,error:'Cancelled; original preserved'}})));
  await pending;assert.equal(editor.isOpen(),false);assert.equal(editor.pending,false);
  open();assert.equal(editor.isOpen(),true);assert.match(editor.message.textContent,/original preserved/);
  assert.equal(editor.file.id,file.id);assert.equal(mutations().length,1);
  if(kind==='pages')assert.deepEqual([...editor.selected],['a.png']);
  await f.get(prefix+'-close').click();assert.equal(editor.file,null);
 });

 test(kind+' editor finishes a hidden job without reopening or selecting its old media',async()=>{
  const {f,editor,file,prefix,mutations}=editFixture();let progress;
  const other={id:9,rel_path:'other.png',file_type:'IMAGE',thumbnail_data:'saved',tags:[]};
  f.respond(async url=>url.includes('/progress?')?new Promise(resolve=>progress=resolve):new Response(JSON.stringify(url.includes('/api/files?')?[file,other]:url.includes('/1/name')||url.includes('/1/page')?{status:'started',job_id:'nas-job'}:[])));
  const pending=editor.submit();await new Promise(setImmediate);
  await f.get(prefix+'-close').click();await f.h.selectMedia(other);
  const mediaRequests=f.calls.filter(([url])=>url.includes('/archive/list/')).length;
  progress(new Response(JSON.stringify({job:{job_id:'nas-job',running:false,completed:true}})));
  await pending;assert.equal(editor.isOpen(),false);assert.equal(editor.pending,false);assert.equal(editor.file,null);
  assert.equal(mutations().length,1);assert.ok(f.calls.some(([url])=>url.includes('/api/files?')));
  assert.equal(f.calls.filter(([url])=>url.includes('/archive/list/')).length,mediaRequests);
 });

 test(kind+' editor ignores a late cancellation response after library reset',async()=>{
  const {f,editor,prefix,open}=editFixture();let progress,cancel;
  f.respond(async url=>url.includes('/progress?')?new Promise(resolve=>progress=resolve):url.includes('/cancel?')?new Promise(resolve=>cancel=resolve):new Response(JSON.stringify({status:'started',job_id:'old-job'})));
  const pending=editor.submit();await new Promise(setImmediate);
  const cancelling=f.get(prefix+'-cancel').click();await new Promise(setImmediate);
  await f.get(prefix+'-close').click();f.h.resetLibraryView();f.h.setSession(2);
  open({id:2,rel_path:'new-library.zip',file_type:'ARCHIVE',encrypted:true});
  editor.message.textContent='New library';editor.cancel.disabled=true;
  cancel(new Response('Old cancellation failed',{status:500}));await cancelling;
  progress(new Response(JSON.stringify({job:{job_id:'old-job',running:false,completed:true}})));await pending;
  assert.equal(editor.message.textContent,'New library');assert.equal(editor.cancel.disabled,true);
  assert.equal(editor.file.id,2);assert.equal(editor.isOpen(),true);
 });
}

function noteExportFixture(){
 const f=noteFixture(),n=f.notes;
 n.file=f.file;n.scope=n.captureScope();n.el.dialog.showModal();
 const tick=async ms=>{const entry=[...f.timers].find(([,t])=>t.ms===ms);assert.ok(entry,'timer '+ms);f.timers.delete(entry[0]);entry[1].fn();await new Promise(setImmediate);};
 const posts=()=>f.calls.filter(([url,options])=>options.method==='POST'&&url.includes('/export'));
 return {f,n,tick,posts};
}
for(const mode of ['lost','stalled'])test('note export reconciles '+mode+' start response and resumes the same operation',async()=>{
 const {f,n,tick,posts}=noteExportFixture();let receipt,lateBody;
 f.respond(async(url,options)=>{
  if(url.includes('/export')){
   receipt=new Headers(options.headers).get('X-Vault-Request');assert.match(receipt,/^[a-f0-9]{32}$/);
   return {ok:true,json:()=>mode==='lost'?Promise.reject(Error('Lost response')):new Promise(r=>lateBody=r),text:async()=>'',blob:async()=>new Blob()};
  }
  if(url.includes('request_id=')){assert.ok(url.includes(receipt));return new Response(JSON.stringify({job:{request_id:receipt,job_id:'export',running:false,completed:true}}));}
  return new Response('{"job":{"job_id":"export","running":false,"completed":true}}');
 });
 const work=n.export();await new Promise(setImmediate);if(mode==='stalled')await tick(15000);
 assert.equal(n.pending,true);assert.equal(n.el.export.disabled,true);n.close();assert.equal(n.isOpen(),false);
 await n.open(f.file);assert.equal(n.isOpen(),true);await n.export();assert.equal(posts().length,1);
 await tick(1000);await work;assert.equal(n.pending,false);assert.equal(posts().length,1);assert.equal(n.el.cancel.hidden,true);
 assert.match(n.el.status.textContent,/끝났습니다/);
 if(lateBody){lateBody({status:'started',job_id:'late'});await new Promise(setImmediate);assert.equal(n.jobId,'export');}
 assert.equal(f.timers.size,0);
});

test('note export keeps cancellation and retries after a progress read fails',async()=>{
 const {f,n,tick,posts}=noteExportFixture();let polls=0,cancelled=false;
 f.respond(async(url)=>{
  if(url.includes('/cancel?')){assert.ok(url.includes('job_id=export'));cancelled=true;return new Response('{"status":"cancelling"}');}
  if(url.includes('/export'))return new Response('{"status":"started","job_id":"export"}');
  if(++polls===1)throw Error('Lost progress');
  return new Response('{"job":{"job_id":"export","running":false,"error":"Cancelled"}}');
 });
 const work=n.export();await new Promise(setImmediate);
 assert.equal(n.pending,true);assert.equal(n.el.cancel.hidden,false);assert.equal(n.el.export.disabled,true);
 await n.export();assert.equal(posts().length,1);await n.el.cancel.click();assert.equal(cancelled,true);
 await tick(1000);await work;assert.equal(n.pending,false);assert.equal(n.el.status.textContent,'Cancelled');assert.equal(f.timers.size,0);
});

test('note export retransmits an unknown start with the same receipt and survives busy rejection',async()=>{
 const {f,n,tick,posts}=noteExportFixture();let receipt,lookups=0;
 f.respond(async(url,options)=>{
  if(url.includes('/export')){
   const id=new Headers(options.headers).get('X-Vault-Request');
   if(!receipt){receipt=id;throw Error('Lost');}assert.equal(id,receipt);return new Response('Busy',{status:409});
  }
  if(url.includes('request_id='))return new Response(JSON.stringify(++lookups===1?{status:'unknown'}:{job:{job_id:'export',request_id:receipt}}));
  return new Response('{"job":{"job_id":"export","running":false,"completed":true}}');
 });
 const work=n.export();await new Promise(setImmediate);await tick(1000);assert.equal(n.pending,true);
 await tick(2000);await work;assert.equal(posts().length,2);assert.equal(n.pending,false);assert.equal(f.timers.size,0);
});

for(const stage of ['start','progress','retry'])test('note export reset cancels '+stage+' wait and ignores stale results',async()=>{
 const {f,n}=noteExportFixture();let late;
 f.respond(async url=>{
  if(stage==='progress'&&url.includes('/export'))return new Response('{"status":"started","job_id":"old"}');
  if(stage==='retry')throw Error('Lost');
  return {ok:true,json:()=>new Promise(r=>late=r),text:async()=>'',blob:async()=>new Blob()};
 });
 const work=n.export();await new Promise(setImmediate);
 if(stage!=='retry')assert.equal(typeof late,'function');
 f.h.resetLibraryView();f.h.setSession(2);n.openNew();n.el.status.textContent='New draft';
 await work;if(late){late({status:'started',job_id:'old',job:{job_id:'old',running:false,completed:true}});await new Promise(setImmediate);}
 assert.equal(n.el.status.textContent,'New draft');assert.equal(n.pending,false);assert.equal(f.timers.size,0);
});

test('note export shows known start rejection without reporting completion',async()=>{
 const {f,n}=noteExportFixture();f.respond(async()=>new Response('Locked',{status:423}));
 await n.export();assert.equal(n.pending,false);assert.equal(n.el.status.textContent,'Locked');assert.equal(n.el.cancel.hidden,true);assert.equal(f.timers.size,0);
});

for(const action of ['close','view'])test('emptied existing note requires discard approval on '+action,async()=>{
 const f=fixture(),n=f.h.noteEditor,file={id:1,rel_path:'note.vtn',file_type:'NOTE',tags:[]};
 f.respond(async()=>new Response(JSON.stringify({revision:'r1',title:'Title',body:'Original'})));
 await f.h.selectMedia(file);await n.read(true);
 f.get('notes-title').value='';f.get('notes-body').value='';
 f.get('notes-body').listeners.input();assert.equal(f.ipcMessages.at(-1),'notes:dirty');
 f.confirm(false);const before=f.confirmations.length;
 if(action==='close')n.close();else await n.read(false);
 assert.equal(f.confirmations.length,before+1);assert.equal(n.isOpen(),true);
 assert.equal(f.get('notes-editor').hidden,false);assert.equal(n.hasChanges(),true);
 assert.equal(f.get('notes-body').value,'');
 f.confirm(true);n.close();assert.equal(n.isOpen(),false);
 assert.equal(f.ipcMessages.at(-1),'notes:clean');assert.equal(n.originalBody,'');
});

test('native note exit protection tracks changed, reverted, pending and cleared state without text',async()=>{
 const f=fixture(),n=f.h.noteEditor,file={id:1,rel_path:'note.vtn',file_type:'NOTE',tags:[]};
 f.respond(async()=>new Response(JSON.stringify({revision:'r1',title:'Private title',body:'Private body'})));
 await n.open(file);await n.read(true);assert.equal(f.ipcMessages.at(-1),'notes:clean');
 f.get('notes-body').value='Changed secret';f.get('notes-body').listeners.input();
 assert.equal(f.ipcMessages.at(-1),'notes:dirty');
 f.get('notes-body').value='Private body';f.get('notes-body').listeners.input();
 assert.equal(f.ipcMessages.at(-1),'notes:clean');
 const before=f.confirmations.length;n.close();assert.equal(f.confirmations.length,before);
 n.openNew();assert.equal(f.ipcMessages.at(-1),'notes:clean');
 f.get('notes-title').value='New draft';f.get('notes-title').listeners.input();
 assert.equal(f.ipcMessages.at(-1),'notes:dirty');
 n.clearContent();n.pending=true;n.reading=false;n.controls();
 assert.equal(f.ipcMessages.at(-1),'notes:dirty');
 n.leave();assert.equal(n.isOpen(),false);assert.equal(f.ipcMessages.at(-1),'notes:dirty');
 n.pending=false;n.saveAttempt={payload:{title:'Secret',body:'Still secret'}};n.controls();
 assert.equal(f.ipcMessages.at(-1),'notes:dirty');
 n.reset();assert.equal(f.ipcMessages.at(-1),'notes:clean');
 assert.ok(f.ipcMessages.every(message=>['notes:clean','notes:dirty'].includes(message)));
});

for(const stage of ['headers','body'])test('series '+stage+' timeout supports retry and rejects a late old response',async()=>{
 const f=fixture(),editor=f.h.seriesEditor,file={id:1,rel_path:'Story_1.zip',file_type:'ARCHIVE',tags:[]};
 const replies=[];
 f.respond(async url=>{
  if(stage==='headers')return new Promise(resolve=>replies.push(()=>resolve(seriesResponse(url,[]))));
  return {ok:true,json:()=>new Promise(resolve=>replies.push(()=>resolve([]))),text:async()=>'',blob:async()=>new Blob()};
 });
 const pending=editor.open(file);await new Promise(setImmediate);
 const timer=[...f.timers].find(([,value])=>value.ms===15000);assert.ok(timer);
 f.timers.delete(timer[0]);timer[1].fn();await pending;
 assert.equal(editor.busy,false);assert.equal(editor.loaded,false);assert.equal(f.get('series-retry').hidden,false);
 assert.equal(f.get('series-save').disabled,true);assert.equal(f.get('series-fields').disabled,true);
 assert.ok(f.calls.every(([,options])=>options.signal.aborted));
 const calls=f.calls.length;await editor.save();assert.equal(f.calls.length,calls);
 f.respond(async url=>seriesResponse(url,[file]));await f.get('series-retry').click();
 assert.equal(editor.loaded,true);assert.equal(f.get('series-retry').hidden,true);
 assert.equal(f.get('series-name').value,'Story');assert.equal(editor.selected.has(1),true);
 replies.forEach(reply=>reply());await new Promise(setImmediate);
 assert.equal(editor.selected.has(1),true);assert.equal(f.get('series-name').value,'Story');
 assert.equal(f.timers.size,0);
});

test('closing stalled series loading releases the UI and aborts all reads',async()=>{
 const f=fixture(),editor=f.h.seriesEditor;f.respond(()=>new Promise(()=>{}));
 const pending=editor.open();await new Promise(setImmediate);editor.close();await pending;
 assert.equal(editor.isOpen(),false);assert.equal(editor.busy,false);assert.equal(f.timers.size,0);
 assert.ok(f.calls.every(([,options])=>options.signal.aborted));
 f.respond(async url=>seriesResponse(url,[]));await editor.open();assert.equal(editor.loaded,true);
});

for(const method of ['json','text','blob','arrayBuffer'])test('library reset aborts '+method+' body reception after headers',async()=>{
 const f=fixture();let stream;
 f.respond(async(_url,options)=>{
  const body=new ReadableStream({start(controller){stream=controller;}});
  options.signal.addEventListener('abort',()=>stream.error(new DOMException('Aborted','AbortError')),{once:true});
  return new Response(body);
 });
 const response=await f.h.vaultFetch('/api/files');assert.equal(f.h.requestCount(),1);
 const pending=response[method]();const rejected=assert.rejects(pending,e=>e.name==='AbortError');
 f.h.resetLibraryView();assert.equal(f.calls[0][1].signal.aborted,true);
 await rejected;assert.equal(f.h.requestCount(),0);
});

test('completed, rejected, empty and discarded bodies release tracked requests',async()=>{
 const f=fixture();f.respond(async()=>new Response('{}'));
 for(let i=0;i<20;i++){
  const response=await f.h.vaultFetch('/api/files');assert.equal(f.h.requestCount(),1);
  await response.json();assert.equal(f.h.requestCount(),0);
 }
 f.respond(async()=>new Response('invalid json'));
 await assert.rejects((await f.h.vaultFetch('/api/files')).json());assert.equal(f.h.requestCount(),0);
 f.respond(async()=>new Response(null,{status:204}));await f.h.vaultFetch('/api/files/1/rating',{method:'POST'});
 assert.equal(f.h.requestCount(),0);
 let cancelled=0;
 f.respond(async()=>new Response(new ReadableStream({cancel(){cancelled++;}})));
 const response=await f.h.vaultFetch('/api/files/1/bookmarks',{method:'POST',discardBody:true});
 assert.equal(response.ok,true);assert.equal(cancelled,1);assert.equal(f.h.requestCount(),0);
 assert.equal(f.calls.at(-1)[1].discardBody,undefined);
 f.respond(async()=>new Response('Rejected',{status:400}));
 await assert.rejects(f.h.vaultFetch('/api/files/1/bookmarks',{method:'POST',discardBody:true}),/Rejected/);
 assert.equal(f.h.requestCount(),0);
});

test('external abort and late body completion cannot keep or reuse an old request',async()=>{
 const f=fixture(),controller=new AbortController();let finish;
 f.respond(async()=>({ok:true,json:()=>new Promise(resolve=>finish=resolve),text:async()=>'',blob:async()=>new Blob()}));
 const response=await f.h.vaultFetch('/api/files',{signal:controller.signal});
 const pending=response.json();controller.abort();assert.equal(f.h.requestCount(),0);
 finish([]);await assert.rejects(pending,e=>e.name==='AbortError');
 const another=await f.h.vaultFetch('/api/files');f.h.resetLibraryView();
 await assert.rejects(another.json(),e=>e.name==='AbortError');assert.equal(f.h.requestCount(),0);
});

test('reload shortcuts are blocked in a note modal while normal Ctrl+R still syncs',async()=>{
 const f=fixture(),n=f.h.noteEditor;n.openNew();f.get('notes-body').value='Unsaved';
 for(const event of [keyEvent('F5'),keyEvent('F5',{ctrlKey:true}),keyEvent('KeyR',{ctrlKey:true}),keyEvent('KeyR',{ctrlKey:true,shiftKey:true})]){
  f.h.handleShortcut(event);assert.equal(event.prevented,true);
 }
 assert.equal(f.calls.length,0);assert.equal(f.get('notes-body').value,'Unsaved');
 n.reset();f.respond(async()=>new Response('{"running":false}'));
 const sync=keyEvent('KeyR',{ctrlKey:true});f.h.handleShortcut(sync);await new Promise(setImmediate);
 assert.equal(sync.prevented,true);assert.ok(f.calls.some(([url])=>url.startsWith('/api/folders/rescan')));
});

test('language reload preserves drafts created before or during its request',async()=>{
 const f=fixture('en',true),n=f.h.noteEditor,select=f.get('language-control');
 n.openNew();f.get('notes-body').value='Draft';select.value='ja';await select.listeners.change();
 assert.equal(f.calls.length,0);assert.equal(f.reloadCount(),0);assert.equal(select.disabled,false);
 n.reset();let finish;f.respond(()=>new Promise(resolve=>finish=resolve));select.value='ja';
 const pending=select.listeners.change();n.openNew();f.get('notes-body').value='New draft';
 finish(new Response('{}'));await pending;
 assert.equal(f.reloadCount(),0);assert.equal(f.get('notes-body').value,'New draft');assert.equal(select.disabled,false);
 n.reset();f.respond(async()=>new Response('{}'));select.value='ja';await select.listeners.change();
 assert.equal(f.reloadCount(),1);assert.equal(f.h.requestCount(),0);
});

for(const kind of ['restore','password'])for(const outcome of ['success','error',...(kind==='restore'?['cancelled']:[])])
test(kind+' protects note editing while hidden and releases it after '+outcome,async()=>{
 const f=fixture(),n=f.h.noteEditor;let release;
 const file={id:10,rel_path:'notes/synthetic.json',file_type:'NOTE',display_name:'Saved note'};
 f.h.privacy.setCovered(false);f.h.dbSecurity.update({encrypted:true,locked:false});
 const dialog=kind==='restore'?f.h.dbRestore:f.h.dbSecurity;
 if(kind==='restore'){
  f.get('btn-restore-db').click();
  f.respond(async()=>new Response('{"status":"selected","token":"synthetic","name":"test.vtbackup","encrypted":false}'));
  await dialog.choose();
 }else dialog.open(true);
 f.respond(async url=>{
  if(url.startsWith('/api/backup/restore')||url.startsWith('/api/security/password'))return new Promise(resolve=>release=resolve);
  if(url.startsWith('/api/notes/'))return new Response('{"title":"Saved note","body":"Saved text","revision":"r1"}');
  if(url.startsWith('/api/session'))return new Response('{"generation":1,"covered":false,"security":{"encrypted":true,"locked":false}}');
  if(url.startsWith('/api/folders/rescan'))return new Response('{"running":false}');
  if(url.startsWith('/api/folders'))return new Response('{"folders":[]}');
  return new Response('[]');
 });
 const work=kind==='restore'?dialog.restore():dialog.perform('/api/security/password',{current_password:'synthetic-old',password:'synthetic-new',confirmation:'synthetic-new'});
 await new Promise(setImmediate);dialog.close.click();
 assert.equal(dialog.isOpen(),false);assert.equal(dialog.pending,true);
 f.get('btn-new-note').click();
 assert.equal(n.isOpen(),false);assert.equal(n.el.editor.hidden,true);
 assert.match(f.alerts.at(-1),/DB 작업이 진행 중/);
 // Reading remains available, but neither the edit button nor a direct save can create a draft.
 await n.open(file);assert.equal(n.text,'Saved text');
 const calls=f.calls.length;
 await n.el.edit.click();await n.submit();
 assert.equal(f.calls.length,calls);assert.equal(n.el.editor.hidden,true);assert.equal(n.text,'Saved text');
 n.close();
 release(outcome==='error'?new Response('Synthetic failure',{status:500}):new Response(JSON.stringify({
  status:outcome,generation:2,covered:false,restore_path:'D:/synthetic/restore',password_changed:kind==='password',
  security:{encrypted:true,locked:false}
 })));
 await work;
 assert.equal(dialog.pending,false);assert.equal(dialog.isOpen(),false);
 f.get('btn-new-note').click();assert.equal(n.isOpen(),true);assert.equal(n.el.editor.hidden,false);
 n.reset();await n.open(file);await n.el.edit.click();
 assert.equal(n.el.editor.hidden,false);assert.equal(n.el.body.value,'Saved text');
});

for(const language of ['ko','en','ja'])test('note edit guard explains pending DB work in '+language,()=>{
 const f=fixture(language),n=f.h.noteEditor;
 f.h.dbSecurity.pending=true;f.h.dbRestore.pending=true;
 n.openNew();assert.equal(n.isOpen(),false);
 assert.match(f.alerts.at(-1),language==='ko'?/DB 작업/:language==='en'?/database operation/:/データベース/);
 f.h.dbSecurity.pending=false;n.openNew();assert.equal(n.isOpen(),false);
 f.h.dbRestore.pending=false;n.openNew();assert.equal(n.isOpen(),true);
});
