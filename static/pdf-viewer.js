/* PDF.js is loaded locally on demand; document data never leaves the library server. */
(() => {
    let engine;
    const loadEngine = () => engine ||= import('/vendor/pdfjs/pdf.mjs?v=20261004_retry').then(pdf => {
        pdf.GlobalWorkerOptions.workerSrc = '/vendor/pdfjs/pdf.worker.mjs';
        return pdf;
    }).catch(error => { engine = null; throw error; });
    window.PdfViewer = class {
        constructor({fetch, mediaUrl, onThumbnail, onPage, captureScope, isFresh, load = loadEngine}) {
            Object.assign(this, {fetch, mediaUrl, onThumbnail, onPage, captureScope, isFresh, load});
            this.positions = new Map();
            this.bookmarkMutations = new window.BookmarkMutations({fetch,captureScope,isFresh,onChange:()=>this.controls()});
            this.el = {};
            for (const id of ['viewer-stage','prev','next','page','count','zoom','layout','save','thumbnail','title','status','pages','bookmarks','bookmark-status','bookmark-retry','password-form','password','seek-bar','seek-label','position-state','position-status','position-retry'])
                this.el[id] = document.getElementById('pdf-'+id);
            this.version = 0; this.renderVersion = 0; this.bookmarkVersion = 0;
            this.renderCache = new Map(); this.pendingRenders = new Map(); this.cacheBytes = 0;
            this.el['position-retry'].addEventListener('click', () => {
                const record=this.positionRecord; if(!record||!this.file)return;
                record.retries=0;this.syncPosition(record,this.file.id);
            });
            this.el.prev.addEventListener('click', () => this.step(-1));
            this.el.next.addEventListener('click', () => this.step(1));
            this.el.pages.addEventListener('wheel', event => this.wheel(event), {passive:false});
            this.el.page.addEventListener('change', () => this.show(Number(this.el.page.value)));
            // Preview while dragging; render only the committed page.
            this.el['seek-bar'].addEventListener('input', () => this.seekLabel(Number(this.el['seek-bar'].value)));
            this.el['seek-bar'].addEventListener('change', () => this.show(Number(this.el['seek-bar'].value)));
            this.el.zoom.addEventListener('change', () => this.show(this.targetPage));
            this.el.layout.addEventListener('change', () => this.show(this.targetPage));
            this.el.save.addEventListener('click', () => this.saveBookmark());
            this.el['bookmark-retry'].addEventListener('click', () => this.loadBookmarks());
            this.el.thumbnail.addEventListener('click', () => this.captureThumbnail());
            this.el['password-form'].addEventListener('submit', event => {
                event.preventDefault();
                const submit = this.passwordCallback; this.passwordCallback = null;
                const password = this.el.password.value; this.el.password.value = '';
                this.el['password-form'].hidden = true;
                submit?.(password);
            });
            if (typeof ResizeObserver !== 'undefined') {
                this.observer = new ResizeObserver(() => {
                    clearTimeout(this.resizeTimer);
                    this.resizeTimer = setTimeout(() => {
                        if (this.doc && ['fit','width'].includes(this.el.zoom.value)) this.show(this.targetPage);
                    }, 150);
                });
                this.observer.observe(this.el.pages);
            }
            this.reset();
        }
        reset(clearPositions = false) {
            if(clearPositions)this.bookmarkMutations.reset();
            this.thumbnailJob?.cancel(); this.thumbnailJob = null;
            this.bookmarkCancel?.(); this.bookmarkCancel = null;
            this.bookmarkResult = '';
            this.el['bookmark-status'].textContent = '';
            this.el['bookmark-retry'].hidden = true; this.el['bookmark-retry'].disabled = false;
            this.positionLookup?.cancel(); this.positionLookup = null;
            if (clearPositions) { for(const record of this.positions.values()){record.disposed=true;clearTimeout(record.retryTimer);record.cancelWrite?.();} this.positions.clear(); this.positionScope = null; }
            this.positionRecord = null;
            this.version++; this.renderVersion++; this.bookmarkVersion++;
            clearTimeout(this.resizeTimer);
            clearTimeout(this.prefetchTimer);
            for (const job of this.pendingRenders.values()) this.cancelRender(job);
            this.pendingRenders.clear();
            for (const entry of this.renderCache.values()) this.releaseEntry(entry);
            this.renderCache.clear(); this.cacheBytes = 0; this.displayKey = null;
            const loading = this.loading; this.loading = null;
            if (loading) Promise.resolve(loading.destroy()).catch(() => {});
            this.renderKey = null; this.renderPromise = null;
            this.autoThumbnailAttempted = false;
            this.doc = null; this.file = null; this.canvas = null; this.ready = false;
            this.page = 1; this.targetPage = 1; this.saving = false;
            this.wheelDelta = 0; this.wheelGestureUsed = false; this.lastWheel = -Infinity; this.lastWheelTurn = -Infinity;
            this.passwordCallback = null; this.el.password.value = '';
            this.el['password-form'].hidden = true;
            this.el.pages.replaceChildren(); this.el.bookmarks.replaceChildren();
            this.el.title.textContent = ''; this.el.status.textContent = '';
            this.el.page.value = '1'; this.el.count.textContent = '/ 0';
            this.el['viewer-stage'].style.display = 'none'; this.controls();
        }
        fresh(version) { return version === this.version && !!this.file; }
        controls() {
            const count = this.doc?.numPages || 0;
            this.el.prev.disabled = !count || this.targetPage <= 1;
            this.el.next.disabled = !count || this.targetPage + this.pageStep() > count;
            this.el.page.disabled = !count; this.el.page.max = String(count || 1);
            this.el['seek-bar'].disabled = !count;
            this.el['seek-bar'].max = String(count || 1);
            this.el['seek-bar'].value = String(this.targetPage);
            this.seekLabel(this.targetPage);
            const bookmarksBusy=this.bookmarkMutations.busy(this.file?.id);
            this.el.save.disabled = !this.ready || this.saving || bookmarksBusy;
            for(const card of this.el.bookmarks.children)card.children[1].disabled=this.saving||bookmarksBusy;
            this.el.thumbnail.disabled = !this.ready || this.saving || !!this.thumbnailJob;
            this.positionStatus();
        }
        seekLabel(page) {
            const count = this.doc?.numPages || 0;
            const label = count ? `${page} / ${count} (${Math.round(page/count*100)}%)` : '0 / 0';
            this.el['seek-label'].textContent = label;
            this.el['seek-bar'].setAttribute('aria-valuetext', label);
        }
        async readPosition(file, version, scope) {
            if (this.positionScope && !this.isFresh?.(this.positionScope)) this.positions.clear();
            this.positionScope = scope;
            const cached = this.positions.get(file.id);
            if (cached && !cached.failed) return cached;
            let page = 1, failed = false, revision = 0;
            const controller = new AbortController();
            let cancel;
            const stopped = new Promise((_, reject) => { cancel = () => { controller.abort(); reject(new Error('PDF position lookup stopped')); }; });
            const lookup = {cancel}; this.positionLookup = lookup;
            // Resolve even if a stalled fetch ignores abort; late replies never enter the cache.
            const timer = setTimeout(cancel, 1500);
            try {
                const request = (async () => (await this.fetch('/api/files/'+file.id+'/pdf-position', {scope, signal:controller.signal})).json())();
                const data = await Promise.race([request, stopped]);
                if (Number.isSafeInteger(data.page) && data.page > 0) page = data.page;
                revision = Number.isSafeInteger(data.revision) && data.revision >= 0 ? data.revision : 0;
            } catch (_) { failed = true; }
            finally { clearTimeout(timer); if (this.positionLookup === lookup) this.positionLookup = null; }
            const record = {page, saved:failed?null:page, revision, tail:Promise.resolve(), scope, failed};
            if (this.fresh(version)) this.positions.set(file.id, record);
            return record;
        }
        rememberPage(page) {
            const record = this.positionRecord;
            if (!record) return;
            // A failed resume lookup must not overwrite an existing position with page 1.
            if (record.failed) {
                record.skipPage ??= page;
                if (record.skipPage === page) { record.page = page; return; }
                record.failed = false;
            }
            if(record.page!==page) record.retries=0;
            record.page=page;
            this.syncPosition(record,this.file.id);
        }
        positionStatus() {
            const record=this.positionRecord;
            this.el['position-state'].hidden=!record?.syncError;
            this.el['position-status'].textContent=record?.syncError || '';
            this.el['position-retry'].disabled=!!record?.syncing;
        }
        async positionRequest(record,id,body) {
            const controller=new AbortController();
            let cancel;
            const stopped=new Promise((_,reject)=>{cancel=()=>{controller.abort();reject(new Error('PDF position request stopped'));};});
            record.cancelWrite=cancel;
            const timer=setTimeout(cancel,5000);
            try {
                const request=(async()=>{
                    const response=await this.fetch('/api/files/'+id+'/pdf-position',{
                        scope:record.scope,signal:controller.signal,discardBody:!!body,...(body?{
                            method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)
                        }:{})});
                    return body?null:await response.json();
                })();
                return await Promise.race([request,stopped]);
            } finally {clearTimeout(timer);if(record.cancelWrite===cancel)record.cancelWrite=null;}
        }
        syncPosition(record,id) {
            if(record.syncing || (record.saved===record.page&&!record.uncertain))return record.tail;
            const fresh=()=>!record.disposed && (!this.isFresh || this.isFresh(record.scope));
            if(!fresh())return record.tail;
            clearTimeout(record.retryTimer);record.retryTimer=null;
            record.syncing=true;this.positionStatus();
            record.tail=(async()=>{
                let conflicts=0;
                while(fresh() && (record.saved!==record.page || record.uncertain)){
                    try {
                        if(record.uncertain){
                            const data=await this.positionRequest(record,id);
                            if(!fresh())return;
                            if(!Number.isSafeInteger(data.revision)||data.revision<0)throw new Error('Invalid PDF position revision');
                            record.revision=data.revision;record.saved=data.page;record.uncertain=false;
                            if(record.saved===record.page)break;
                        }
                        const page=record.page,revision=record.revision;
                        await this.positionRequest(record,id,{page,revision});
                        if(!fresh())return;
                        record.saved=page;record.revision=revision+1;
                    } catch(error) {
                        // A timed-out write can still commit. Reconcile its revision before retrying.
                        record.uncertain=true;
                        if(fresh()&&error.status===409&&conflicts++===0)continue;
                        throw error;
                    }
                }
                record.syncError='';record.retries=0;
            })().catch(()=>{
                if(!fresh())return;
                record.syncError=window.vt('PDF 진도가 아직 저장되지 않았습니다. 연결을 확인하고 다시 시도해 주세요.');
                if((record.retries||0)<2){record.retries=(record.retries||0)+1;
                    record.retryTimer=setTimeout(()=>{record.retryTimer=null;this.syncPosition(record,id);},2000);
                }
            }).finally(()=>{record.syncing=false;if(this.positionRecord===record)this.positionStatus();});
            return record.tail;
        }
        async open(file, url, initialPage) {
            this.reset(); this.file = file;
            const version = this.version, scope = this.captureScope?.();
            // Start the small DB lookup alongside engine/document loading, rather than after it.
            const position = this.readPosition(file, version, scope);
            this.el['viewer-stage'].style.display = 'flex';
            this.el.title.textContent = file.display_name || file.rel_path;
            this.el.status.textContent = window.vt('PDF를 불러오는 중…');
            if (!this.el.zoom.value) this.el.zoom.value = 'fit';
            try {
                const pdf = await this.load();
                if (!this.fresh(version)) return;
                const loading = this.loading = pdf.getDocument({url,
                    cMapUrl:'/vendor/pdfjs/cmaps/', cMapPacked:true,
                    standardFontDataUrl:'/vendor/pdfjs/standard_fonts/', wasmUrl:'/vendor/pdfjs/wasm/',
                    iccUrl:'/vendor/pdfjs/iccs/', enableXfa:false, isEvalSupported:false,
                    isImageDecoderSupported:false, maxImageSize:67108864, stopAtErrors:true, canvasMaxAreaInBytes:33554432,
                    disableAutoFetch:true, disableStream:true, rangeChunkSize:131072});
                loading.onPassword = (callback, reason) => {
                    if (!this.fresh(version)) return;
                    this.passwordCallback = callback;
                    this.el.status.textContent = window.vt(reason === pdf.PasswordResponses.INCORRECT_PASSWORD ? 'PDF 비밀번호가 올바르지 않습니다.' : 'PDF 문서 비밀번호를 입력해 주세요.');
                    this.el['password-form'].hidden = false; this.el.password.focus();
                };
                const doc = await loading.promise;
                if (!this.fresh(version)) return;
                const record = await position;
                if (!this.fresh(version)) return;
                this.doc = doc;
                this.positionRecord = record;
                const resumeFailed = record.failed;
                this.el.count.textContent = '/ '+this.doc.numPages;
                await this.show(initialPage ?? record.page);
                if (this.fresh(version)) await this.loadBookmarks();
                if (this.fresh(version) && resumeFailed && initialPage == null) this.el.status.textContent = window.vt('마지막 PDF 페이지를 불러오지 못해 첫 페이지를 표시합니다.');
            } catch (error) {
                if (this.fresh(version)) { this.el.status.textContent = window.vt('PDF를 열 수 없습니다. 파일과 저장소 연결을 확인해 주세요.'); this.controls(); }
            }
        }
        pageStep() { return this.el.layout.value === '2' ? 2 : 1; }
        step(delta) { return this.show(this.targetPage + delta * this.pageStep()); }
        wheel(event) {
            if (!this.doc || event.ctrlKey || event.metaKey || event.shiftKey || !event.deltaY || Math.abs(event.deltaX || 0) > Math.abs(event.deltaY)) return;
            const area = this.el.pages, direction = Math.sign(event.deltaY);
            const now = performance.now(), gap = now - this.lastWheel;
            this.lastWheel = now;
            if (gap > 180) this.wheelGestureUsed = false;
            const canScroll = direction > 0 ? area.scrollTop + area.clientHeight < area.scrollHeight - 2 : area.scrollTop > 2;
            if (canScroll) { this.wheelDelta = 0; return; }
            event.preventDefault();
            if (!this.ready || now - this.lastWheelTurn < 350 || this.wheelGestureUsed) { this.wheelDelta = 0; return; }
            if (gap > 180 || Math.sign(this.wheelDelta) !== direction) this.wheelDelta = 0;
            this.wheelDelta += event.deltaY * (event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? area.clientHeight : 1);
            if (Math.abs(this.wheelDelta) < 40) return;
            this.wheelDelta = 0;
            const next = this.targetPage + direction * this.pageStep();
            if (next < 1 || next > this.doc.numPages) return;
            this.lastWheelTurn = now; this.wheelGestureUsed = true;
            return this.show(next, {scrollToEnd:direction < 0});
        }
        request(number) {
            const step = this.pageStep(), requested = Math.max(1,Math.min(this.doc.numPages,Math.trunc(number)));
            const page = step === 2 ? requested-(requested-1)%2 : requested;
            const mode = this.el.zoom.value, density = window.devicePixelRatio || 1;
            const width = this.el.pages.clientWidth, height = this.el.pages.clientHeight;
            const key = JSON.stringify([page,step,mode,density,['fit','width'].includes(mode)?width:0,mode==='fit'?height:0]);
            return {key,page,last:Math.min(this.doc.numPages,page+step-1),mode,density,width,height};
        }
        cancelRender(job) {
            job.cancelled = true;
            for (const task of job.tasks) task.cancel();
        }
        releaseEntry(entry) {
            for (const canvas of entry.canvases) { canvas.width = 1; canvas.height = 1; }
        }
        trimCache(protectedKey) {
            // At most three recent spreads and 72 MiB of canvas pixel storage.
            for (const [key,entry] of this.renderCache) {
                if (this.renderCache.size<=3 && this.cacheBytes<=72*1024*1024) break;
                if (key===this.displayKey || key===protectedKey) continue;
                this.renderCache.delete(key); this.cacheBytes-=entry.bytes; this.releaseEntry(entry);
            }
        }
        cached(key) {
            const entry=this.renderCache.get(key);
            if (entry) { this.renderCache.delete(key); this.renderCache.set(key,entry); }
            return entry;
        }
        renderEntry(request) {
            const cached=this.cached(request.key);
            if (cached) return Promise.resolve(cached);
            const existing=this.pendingRenders.get(request.key);
            if (existing && !existing.cancelled) return existing.promise;
            const job={version:this.version,cancelled:false,tasks:new Set()};
            this.pendingRenders.set(request.key,job);
            job.promise=this.buildSpread(request,job).then(entry=>{
                if (!entry) return null;
                if (!this.fresh(job.version)||job.cancelled) { this.releaseEntry(entry);return null; }
                this.renderCache.set(request.key,entry);this.cacheBytes+=entry.bytes;this.trimCache(request.key);
                return entry;
            }).finally(()=>{if(this.pendingRenders.get(request.key)===job)this.pendingRenders.delete(request.key);});
            return job.promise;
        }
        schedulePrefetch(request) {
            clearTimeout(this.prefetchTimer);
            if (request.last>=this.doc.numPages) return;
            const version=this.version;
            this.prefetchTimer=setTimeout(()=>{
                if (!this.fresh(version)||!this.ready||this.displayKey!==request.key) return;
                const next=this.request(request.last+1);
                // Only the next spread is prepared; completion does not chain further reads.
                if (!this.renderCache.has(next.key)) this.renderEntry(next).catch(()=>{});
            },180);
        }
        show(number, options = {}) {
            if (!this.doc || !Number.isFinite(number)) return Promise.resolve();
            const request=this.request(number);
            if (request.key===this.renderKey) {
                if (this.ready) {
                    if (options.scrollToEnd) this.el.pages.scrollTop=this.el.pages.scrollHeight;
                    return Promise.resolve();
                }
                if (this.renderPromise) return this.renderPromise;
            }
            clearTimeout(this.prefetchTimer);
            // Promote a matching prefetch; unrelated work cannot compete with the selected page.
            for (const [key,job] of this.pendingRenders) if (key!==request.key) this.cancelRender(job);
            this.renderKey=request.key;
            const pending=this.renderSpread(request,options).finally(()=>{
                if (this.renderPromise!==pending) return;
                this.renderPromise=null;if(!this.ready)this.renderKey=null;
            });
            this.renderPromise=pending;return pending;
        }
        async renderSpread(request,{scrollToEnd=false}={}) {
            const version=this.version,renderVersion=++this.renderVersion;
            const fresh=()=>this.fresh(version)&&renderVersion===this.renderVersion;
            this.targetPage=request.page;this.ready=false;this.controls();
            this.el.page.value=String(request.page);
            this.el.count.textContent=(request.last>request.page?'– '+request.last+' ':'')+'/ '+this.doc.numPages;
            this.el.status.textContent=this.renderCache.has(request.key)?'':window.vt('PDF 페이지를 표시하는 중…');
            try {
                const entry=await this.renderEntry(request);
                if (!fresh()||!entry) return;
                let content=entry.canvases[0];
                if(entry.canvases.length>1){content=document.createElement('div');content.className='pdf-spread';content.append(...entry.canvases);}
                this.canvas=entry.canvases[0];this.page=request.page;this.ready=true;this.displayKey=request.key;
                this.el.pages.replaceChildren(content);this.el.pages.scrollTop=scrollToEnd?this.el.pages.scrollHeight:0;
                this.el.status.textContent='';this.controls();this.trimCache(request.key);
                this.rememberPage(request.page);
                for(let n=request.page;n<=request.last;n++)this.onPage?.(n-1);
                this.schedulePrefetch(request);
                if(!this.file.thumbnail_data&&!this.autoThumbnailAttempted){this.autoThumbnailAttempted=true;void this.captureThumbnail(true);}
            } catch(error) {
                if(fresh()){this.ready=false;this.controls();this.el.status.textContent=window.vt(/Image exceeded maximum allowed size/.test(error.message)?'PDF 이미지 해상도가 너무 높아 표시할 수 없습니다. 해상도를 낮춘 PDF로 다시 시도해 주세요.':'PDF 페이지를 표시하지 못했습니다. 다시 선택해 주세요.');}
            }
        }
        async buildSpread(request,job) {
            const fresh=()=>this.fresh(job.version)&&!job.cancelled;
            const doc=this.doc,canvases=[],work=[];
            try {
                const pages=await Promise.all(Array.from({length:request.last-request.page+1},(_,i)=>doc.getPage(request.page+i)));
                if(!fresh())return null;
                const bases=pages.map(page=>page.getViewport({scale:1}));
                const width=Math.max(200,request.width-24)-(pages.length>1?12:0),height=Math.max(200,request.height-24);
                const baseWidth=bases.reduce((sum,base)=>sum+base.width,0),baseHeight=Math.max(...bases.map(base=>base.height));
                let scale=request.mode==='fit'?Math.min(width/baseWidth,height/baseHeight):request.mode==='width'?width/baseWidth:Number(request.mode);
                scale=Math.max(.05,Math.min(4,scale||1));
                const sizes=pages.map(page=>page.getViewport({scale}));
                const pixels=sizes.reduce((sum,size)=>sum+size.width*size.height,0),maxDimension=Math.max(...sizes.flatMap(size=>[size.width,size.height]));
                const density=Math.min(request.density,2,Math.sqrt(8388608/pixels),8192/maxDimension);
                for(const [index,page]of pages.entries()) {
                    const css=sizes[index],viewport=page.getViewport({scale:scale*density}),canvas=document.createElement('canvas');
                    canvas.width=Math.max(1,Math.ceil(viewport.width));canvas.height=Math.max(1,Math.ceil(viewport.height));
                    canvas.style.width=Math.round(css.width)+'px';canvas.style.height=Math.round(css.height)+'px';canvases.push(canvas);
                    const task=page.render({canvasContext:canvas.getContext('2d'),viewport});job.tasks.add(task);
                    work.push(task.promise.finally(()=>job.tasks.delete(task)));
                }
                await Promise.all(work);
                if(!fresh()){this.releaseEntry({canvases});return null;}
                pages.forEach(page=>page.cleanup?.());
                return {canvases,bytes:canvases.reduce((sum,canvas)=>sum+canvas.width*canvas.height*4,0)};
            } catch(error) {
                this.cancelRender(job);await Promise.allSettled(work);this.releaseEntry({canvases});throw error;
            }
        }
        shot() {
            const source = this.canvas;
            if (!this.ready || !source) return null;
            const shot = document.createElement('canvas');
            const scale = Math.min(320/source.width,320/source.height);
            shot.width = Math.max(1,Math.round(source.width*scale)); shot.height = Math.max(1,Math.round(source.height*scale));
            shot.getContext('2d').drawImage(source,0,0,shot.width,shot.height);
            return shot;
        }
        async captureThumbnail(automatic = false) {
            const shot = this.shot(); if (!shot || this.saving || this.thumbnailJob) return;
            const version = this.version, file = this.file, page = this.page;
            const controller = new AbortController(), deadline = Date.now()+15000;
            const job = {controller};
            const stopped = new Promise((_,reject) => { job.cancel = () => {
                controller.abort(); reject(new Error(window.vt('썸네일 저장 응답을 받지 못했습니다. 저장소 연결을 확인한 뒤 다시 시도해 주세요.')));
            }; });
            const timer = setTimeout(job.cancel,15000);
            this.thumbnailJob = job; this.controls();
            const fresh = () => this.fresh(version) && this.thumbnailJob === job && !controller.signal.aborted;
            try {
                const work = (async () => {
                    const blob = await new Promise(resolve => shot.toBlob(resolve,'image/webp',.85));
                    if (!blob || !fresh()) return false;
                    const body = new FormData(); body.append('image',blob,'thumb.webp');
                    body.append('width',shot.width); body.append('height',shot.height); body.append('page_offset',page-1);
                    await this.fetch('/api/thumbnail/'+file.id,{method:'POST',body,discardBody:true,signal:controller.signal,
                        headers:{'X-Vault-Write-Deadline':String(deadline)}});
                    return true;
                })();
                if (!await Promise.race([work,stopped]) || !fresh()) return;
                this.onThumbnail?.(file.id, '/api/thumbnail/'+file.id+'?v='+Date.now(), {page_offset:page-1});
                if (!automatic) this.el.status.textContent = window.vt('썸네일을 저장했습니다.');
            } catch (error) { if (this.fresh(version) && !automatic) this.el.status.textContent = window.vt('저장 실패: {0}',error.message); }
            finally { clearTimeout(timer); if (this.thumbnailJob === job) { this.thumbnailJob = null; this.controls(); } }
        }
        async saveBookmark() {
            const shot = this.shot(); if (!shot || this.saving || this.bookmarkMutations.busy(this.file.id)) return;
            const version = this.version, id = this.file.id, position = this.page-1;
            this.bookmarkResult = '';
            this.saving = true; this.controls();
            try {
                const thumbnail_data = shot.toDataURL('image/jpeg',.75).split(',')[1];
                const applied=await this.bookmarkMutations.send(id,{position,thumbnail_data});
                if (!this.fresh(version)) return;
                this.bookmarkResult = applied?window.vt('{0}페이지 저장됨',position+1):this.bookmarkMutations.uncertainMessage();
                this.el['bookmark-status'].textContent=this.bookmarkResult;
                await this.loadBookmarks();
            } catch (error) { if (this.fresh(version)) this.el['bookmark-status'].textContent = window.vt('저장 실패: {0}',error.message); }
            finally { if (this.fresh(version)) { this.saving=false; this.controls(); } }
        }
        async loadBookmarks() {
            if (!this.file) return false;
            const version = this.version, request = ++this.bookmarkVersion, id = this.file.id;
            this.bookmarkCancel?.();
            const controller = new AbortController();
            let cancel;
            const stopped = new Promise((_,reject) => { cancel = () => { controller.abort(); reject(new Error(window.vt('저장 목록 응답을 받지 못했습니다.'))); }; });
            this.bookmarkCancel = cancel;
            let timer;
            const fresh = () => this.fresh(version) && request === this.bookmarkVersion;
            if(this.bookmarkMutations.busy(id))this.bookmarkResult=this.bookmarkMutations.uncertainMessage();
            this.controls();
            this.el['bookmark-retry'].disabled = true;
            try {
                const read = (async () => {
                    await this.bookmarkMutations.beforeRead(id,controller.signal);
                    if(controller.signal.aborted)throw new Error('Bookmark read stopped');
                    timer=setTimeout(cancel,10000);
                    return (await this.fetch('/api/files/'+id+'/bookmarks',{signal:controller.signal})).json();
                })();
                const records = await Promise.race([read,stopped]);
                if (!fresh()) return false;
                const result=this.bookmarkMutations.observe(id,records);
                if(result)this.bookmarkResult=result;
                else if(!this.bookmarkMutations.busy(id)&&this.bookmarkResult===this.bookmarkMutations.uncertainMessage())this.bookmarkResult='';
                this.el.bookmarks.replaceChildren(...records.map(record => {
                    const card = document.createElement('div'); card.className = 'bookmark-card';
                    const jump = document.createElement('button'); jump.className = 'bookmark-jump';
                    const image = document.createElement('img'); image.src = this.mediaUrl(record.thumbnail_url); image.loading = 'lazy'; image.alt = '';
                    const label = document.createElement('span'); label.textContent = window.vt('{0}페이지',record.position+1);
                    jump.append(image,label); jump.disabled = record.position >= this.doc.numPages;
                    jump.addEventListener('click', () => { if (this.fresh(version)) this.show(record.position+1); });
                    const remove = document.createElement('button'); remove.className = 'bookmark-delete'; remove.textContent = '×';
                    remove.title = window.vt('{0} 저장 위치 삭제',label.textContent);
                    remove.addEventListener('click', async () => {
                        if (!this.fresh(version) || this.saving || this.bookmarkMutations.busy(id)) return;
                        remove.disabled = true;
                        try {
                            const applied=await this.bookmarkMutations.send(id,{bookmarkId:record.id});
                            if (this.fresh(version)) {
                                if(applied)card.remove();
                                this.bookmarkResult = applied?window.vt('저장 위치를 삭제했습니다.'):this.bookmarkMutations.uncertainMessage();
                                this.el['bookmark-status'].textContent=this.bookmarkResult;
                                await this.loadBookmarks();
                            }
                        } catch (error) { if (this.fresh(version)) { remove.disabled = false; this.el['bookmark-status'].textContent = window.vt('삭제 실패: {0}',error.message); } }
                    });
                    card.append(jump,remove); return card;
                }));
                this.el['bookmark-status'].textContent = this.bookmarkResult;
                this.el['bookmark-retry'].hidden = true;
                return true;
            } catch (error) {
                if (fresh()) { this.el['bookmark-status'].textContent = [this.bookmarkResult,window.vt('저장 위치를 불러오지 못했습니다: {0}',error.message)].filter(Boolean).join(' '); this.el['bookmark-retry'].hidden = false; }
                return false;
            } finally { clearTimeout(timer); if (this.bookmarkCancel === cancel) this.bookmarkCancel = null; if (fresh()) {this.el['bookmark-retry'].disabled = false;this.controls();} }
        }
    };
})();
