// Library-scoped series metadata; media files themselves are never changed.
(() => {
    const mediaName = file => file.display_name || file.rel_path;
    window.SeriesEditor = class {
        constructor(api) {
            this.api = api;
            this.version = 0;
            this.el = {};
            for (const id of ['dialog','fields','select','name','search','files','count','status','save','dissolve','close','select-visible','cancel','retry']) {
                this.el[id] = document.getElementById(`series-${id}`);
            }
            this.virtual = new window.VirtualMediaList(this.el.files, file => this.createRow(file));
            this.virtual.estimate = 38;
            this.el.close.addEventListener('click', () => this.close());
            if (this.el.cancel) this.el.cancel.addEventListener('click', () => this.close());
            this.el.dialog.addEventListener('cancel', e => { e.preventDefault(); this.close(); });
            this.el.select.addEventListener('change', () => this.choose(Number(this.el.select.value) || null));
            this.el.search.addEventListener('input', () => this.render(true));
            this.el['select-visible'].addEventListener('click', () => {
                for (const file of this.visibleFiles()) if (!this.selected.has(file.id)) this.selected.set(file.id, ++this.nextPosition);
                this.ensureRepresentative(); this.render();
            });
            this.el.save.addEventListener('click', () => this.save());
            this.el.retry.addEventListener('click', () => {
                if (this.mutation) return this.reconcile(this.mutation);
                if (!this.busy) return this.open(this.retryFile);
            });
            this.el.dissolve.addEventListener('click', () => this.dissolve());
        }
        isOpen() { return this.el.dialog.open; }
        close() {
            if (this.mutation && this.api.isFresh(this.mutation.scope)) {
                this.el.dialog.close();
                return;
            }
            this.reset();
        }
        reset() {
            this.mutation?.cancel?.(); this.mutation = null;
            ++this.version;
            this.cancelLoad?.();this.cancelLoad=null;this.retryFile=null;this.loaded=false;this.busy=false;this.el.retry.hidden=true;
            this.el.dialog.close();
            this.virtual.reset();
            this.series = []; this.files = []; this.selected = new Map();
            this.el.select.replaceChildren(); this.el.name.value = ''; this.el.search.value = '';
            this.el.status.textContent = '';
            this.el.cancel.textContent = window.vt('취소');
            this.el.close.title = window.vt('닫기');
        }
        setStatus(msg, type = 'normal') {
            if (!this.el.status) return;
            this.el.status.textContent = msg;
            this.el.status.className = 'series-status-msg' + (type === 'error' ? ' is-error' : type === 'loading' ? ' is-loading' : type === 'success' ? ' is-success' : '');
        }
        fresh(version) { return version === this.version && this.isOpen() && this.api.isFresh(this.scope); }
        async open(file = null) {
            if (this.mutation) {
                if (this.api.isFresh(this.mutation.scope)) {
                    if (!this.isOpen()) this.el.dialog.showModal();
                    return;
                }
                this.reset();
            }
            const version = ++this.version;
            this.cancelLoad?.();
            this.retryFile=file;this.loaded=false;this.el.retry.hidden=true;
            this.el.retry.textContent=window.vt('다시 시도');this.el.retry.disabled=false;
            this.el.save.disabled=this.el.dissolve.disabled=true;
            this.scope = this.api.captureScope();
            const scope=this.scope;
            this.busy = true;
            this.el.fields.disabled = true;
            this.setStatus(window.vt("시리즈를 불러오는 중…"), 'loading');
            this.virtual.reset();
            if (!this.isOpen()) this.el.dialog.showModal();
            const controller=new AbortController();let stop;
            const interrupted=new Promise((_,reject)=>{stop=message=>{
                reject(new Error(message));controller.abort();
            };});
            const cancel=()=>stop(window.vt("요청을 취소했습니다."));this.cancelLoad=cancel;
            const timer=setTimeout(()=>stop(window.vt("응답 대기 시간(15초)을 초과했습니다. 다시 시도하거나 창을 닫아 주세요.")),15000);
            try {
                const [series, active, trash] = await Promise.race([Promise.all(['/api/series', '/api/files?sort_by=name&order=asc', '/api/trash'].map(async url => {
                    const res = await this.api.fetch(url, {scope,signal:controller.signal});
                    if (!res.ok) throw new Error(window.vt("시리즈 목록을 불러오지 못했습니다."));
                    return res.json();
                })),interrupted]);
                if (!this.fresh(version)) return;
                this.series = series;
                this.files = [...active, ...trash].filter(f => ['ARCHIVE','VIDEO','IMAGE','PDF'].includes(f.file_type))
                    .sort((a,b) => mediaName(a).localeCompare(mediaName(b), undefined, {numeric:true}) || a.id-b.id);
                this.el.select.replaceChildren();
                for (const item of [{id:'',name:window.vt("새 시리즈 만들기")}, ...series]) {
                    const option = document.createElement('option');
                    option.value = String(item.id); option.textContent = item.name;
                    this.el.select.appendChild(option);
                }
                const membership = series.find(s => s.file_ids.includes(file?.id));
                this.choose(membership?.id ?? null, file?.id);
                this.setStatus('');
                this.loaded=true;this.el.fields.disabled=false;this.el.save.disabled=this.el.dissolve.disabled=false;
            } catch (err) {
                if (this.fresh(version)) {this.setStatus(err.message, 'error');this.el.retry.hidden=false;}
            } finally {
                clearTimeout(timer);controller.abort();
                if(this.cancelLoad===cancel)this.cancelLoad=null;
                if(this.fresh(version))this.busy=false;
            }
        }
        choose(id, initialFileId = null) {
            this.id = id;
            const series = this.series.find(s => s.id === id);
            this.el.select.value = id == null ? '' : String(id);
            const initialFile = this.files.find(f => f.id === initialFileId);
            const filename = (initialFile ? mediaName(initialFile) : '').split(/[\\/]/).pop() || '';
            const defaultName = filename.replace(/\.(?:tar\.zst|zip|cbz|rar|7z|mp4|mkv|webm|avi|mov|m4v|pdf|jpg|jpeg|png|webp|gif|bmp|avif)$/i, '').split('_')[0].trim();
            this.el.name.value = series?.name ?? defaultName;
            this.el.search.value = '';
            this.selected = new Map((series?.file_ids || []).map((fileId, i) => [fileId, i+1]));
            this.nextPosition = this.selected.size;
            if (!series && initialFileId && this.files.some(f => f.id === initialFileId && !f.series_id)) {
                this.selected.set(initialFileId, ++this.nextPosition);
            }
            this.representative = series?.representative_id;
            this.ensureRepresentative();
            this.el.dissolve.hidden = !series;
            this.setStatus('');
            this.render(true, initialFileId);
        }
        orderedIds() {
            return [...this.selected].sort((a,b) => a[1]-b[1] || a[0]-b[0]).map(([id]) => id);
        }
        ensureRepresentative() {
            if (!this.selected.has(this.representative)) this.representative = this.orderedIds()[0] ?? null;
        }
        visibleFiles() {
            const query = this.el.search.value.trim().toLowerCase();
            return this.files.filter(f => (!f.series_id || f.series_id === this.id) &&
                (!f.is_trash || this.selected.has(f.id)) && (mediaName(f).toLowerCase().includes(query) || f.rel_path.toLowerCase().includes(query)));
        }
        render(resetScroll = false, revealId = null) {
            const count = this.selected.size;
            const repFile = this.files.find(f => f.id === this.representative);
            if (this.el.count) {
                this.el.count.textContent = count > 0 
                    ? window.vt("{0}개 선택 {1}", count, repFile ? window.vt("· 대표: ") + mediaName(repFile) : '')
                    : window.vt("선택된 파일 없음");
                this.el.count.classList.toggle('has-selection', count > 0);
            }
            this.virtual.setItems(this.visibleFiles(), {resetScroll, revealId});
        }
        createRow(file) {
            const row = document.createElement('div');
            const isSelected = this.selected.has(file.id);
            const isRep = file.id === this.representative;
            row.className = `media-card series-file-row${isSelected ? ' selected' : ''}${isRep ? ' is-rep' : ''}`;

            const label = document.createElement('label');
            label.className = 'series-file-label';

            const checkbox = document.createElement('input');
            checkbox.type = 'checkbox';
            checkbox.className = 'series-checkbox';
            checkbox.checked = isSelected;

            label.appendChild(checkbox);

            if (file.is_trash) {
                const trashBadge = document.createElement('span');
                trashBadge.className = 'series-badge-trash';
                trashBadge.textContent = window.vt("휴지통");
                label.appendChild(trashBadge);
            }

            const name = document.createElement('span');
            name.className = 'series-file-name';
            name.textContent = mediaName(file);
            name.title = mediaName(file);
            label.appendChild(name);

            const order = document.createElement('input');
            order.type = 'number';
            order.min = '1';
            order.max = '100000';
            order.step = '1';
            order.className = 'series-order-input';
            order.value = this.selected.get(file.id) ?? '';
            order.disabled = !checkbox.checked;
            order.setAttribute('aria-label', window.vt("{0} 시리즈 순서", mediaName(file)));
            order.title = window.vt("시리즈 순서 (숫자가 작을수록 먼저 표시)");

            const rep = document.createElement('button');
            rep.type = 'button';
            rep.className = isRep ? 'btn series-rep-btn is-rep' : 'btn secondary-btn series-rep-btn';
            rep.disabled = !checkbox.checked;
            rep.textContent = isRep ? window.vt("★ 대표") : window.vt("대표로");
            rep.setAttribute('aria-label', window.vt("{0} 대표 파일 지정", mediaName(file)));
            rep.setAttribute('aria-pressed', String(isRep));
            rep.title = isRep ? window.vt("현재 시리즈의 대표 파일입니다") : window.vt("이 파일을 대표로 설정합니다");

            const updateState = () => {
                const sel = checkbox.checked;
                const repActive = file.id === this.representative;
                row.classList.toggle('selected', sel);
                row.classList.toggle('is-rep', repActive);
                order.disabled = !sel;
                order.value = this.selected.get(file.id) ?? '';
                rep.disabled = !sel;
                rep.className = repActive ? 'btn series-rep-btn is-rep' : 'btn secondary-btn series-rep-btn';
                rep.textContent = repActive ? window.vt("★ 대표") : window.vt("대표로");
                rep.setAttribute('aria-pressed', String(repActive));
            };

            checkbox.addEventListener('change', () => {
                if (checkbox.checked) {
                    this.selected.set(file.id, ++this.nextPosition);
                } else {
                    this.selected.delete(file.id);
                }
                this.ensureRepresentative();
                updateState();
                this.render();
            });

            order.addEventListener('input', () => {
                const value = Number(order.value);
                if (Number.isInteger(value) && value >= 1 && value <= 100000) {
                    this.selected.set(file.id, value);
                    this.nextPosition = Math.max(this.nextPosition, value);
                }
            });

            order.addEventListener('change', () => {
                const value = Number(order.value);
                if (Number.isInteger(value) && value >= 1 && value <= 100000) {
                    this.selected.set(file.id, value);
                    this.nextPosition = Math.max(this.nextPosition, value);
                } else {
                    order.value = this.selected.get(file.id) ?? '';
                }
            });

            rep.addEventListener('click', () => {
                if (!this.selected.has(file.id)) this.selected.set(file.id, ++this.nextPosition);
                this.representative = file.id;
                checkbox.checked = true;
                updateState();
                this.render();
            });

            row.append(label, order, rep);
            return row;
        }
        async save() {
            if (this.busy||!this.loaded) return;
            const name = this.el.name.value.trim();
            if (!name || [...name].length > 200 || !this.selected.size || !this.selected.has(this.representative)) {
                this.setStatus(window.vt("이름(1~200자), 동영상·압축 파일, 대표 파일을 선택해 주세요."), 'error');
                return;
            }
            await this.mutate('/api/series', 'POST', {
                id:this.id, name, file_ids:this.orderedIds(), representative_id:this.representative
            });
        }
        async dissolve() {
            if (this.busy || !this.loaded || !this.id) return;
            if (!confirm(window.vt("시리즈를 해제할까요? 파일은 그대로 유지되며 각 파일의 기존 점수와 별점으로 돌아갑니다."))) return;
            await this.mutate(`/api/series/${this.id}`, 'DELETE');
        }
        async mutate(url, method, body) {
            const version = this.version;
            const scope = this.scope;
            if (!this.fresh(version) || this.mutation) return;
            const operation = {id:this.id, method, body, scope, deadline:Date.now()+15000, phase:'writing'};
            this.mutation = operation;
            this.busy = true; this.el.fields.disabled = this.el.save.disabled = this.el.dissolve.disabled = true;
            this.el.retry.hidden = true;
            this.el.cancel.textContent = window.vt('닫기');
            this.el.close.title = window.vt('창을 닫아도 작업은 계속됩니다.');
            this.setStatus(window.vt("저장 중…"), 'loading');
            try {
                await this.mutationRequest(operation, url, {method, discardBody:true,
                    headers:{'X-Vault-Write-Deadline':String(operation.deadline), ...(body ? {'Content-Type':'application/json'} : {})},
                    ...(body ? {body:JSON.stringify(body)} : {})});
            } catch (err) {
                if (!this.currentMutation(operation)) return;
                if ([400,401,403,404,408,409,413,422,423].includes(err.status)) {
                    return this.finishMutation(operation, false, err.message);
                }
                operation.phase = 'uncertain';
                return this.reconcile(operation);
            }
            return this.finishMutation(operation, true);
        }
        currentMutation(operation) { return this.mutation === operation && this.api.isFresh(operation.scope); }
        async mutationRequest(operation, url, options = {}, json = false, delay = 0) {
            const controller = new AbortController();
            let cancel, timer;
            const stopped = new Promise((_, reject) => { cancel = () => {
                controller.abort(); reject(new Error(window.vt('응답 대기 시간(15초)을 초과했습니다. 다시 시도하거나 창을 닫아 주세요.')));
            }; });
            operation.cancel = cancel;
            try {
                if (delay > 0) {
                    await Promise.race([new Promise(resolve => { timer = setTimeout(resolve, delay); }), stopped]);
                    clearTimeout(timer);
                }
                if (!this.currentMutation(operation)) return;
                timer = setTimeout(cancel, 15000);
                return await Promise.race([(async () => {
                    const response = await this.api.fetch(url, {...options, scope:operation.scope, signal:controller.signal});
                    return json ? await response.json() : response;
                })(), stopped]);
            } finally {
                clearTimeout(timer);
                if (operation.cancel === cancel) operation.cancel = null;
            }
        }
        async reconcile(operation) {
            if (!this.currentMutation(operation) || operation.phase !== 'uncertain' || operation.checking) return;
            operation.checking = true; this.el.retry.disabled = true;
            this.setStatus(window.vt('시리즈 변경 결과를 확인하고 있습니다. 확인이 끝날 때까지 새 변경을 저장할 수 없습니다.'), 'loading');
            try {
                // Wait out the write deadline, then read behind any write already holding the DB lock.
                const series = await this.mutationRequest(operation, '/api/series', {}, true, Math.max(0, operation.deadline-Date.now()));
                if (!this.currentMutation(operation)) return;
                if (!Array.isArray(series)) throw new Error(window.vt('시리즈 목록을 불러오지 못했습니다.'));
                const body = operation.body;
                const applied = operation.method === 'DELETE' ? !series.some(s => s.id === operation.id) : series.some(s =>
                    (body.id == null ? s.name === body.name : s.id === body.id) && s.name === body.name &&
                    s.representative_id === body.representative_id && JSON.stringify(s.file_ids) === JSON.stringify(body.file_ids));
                await this.finishMutation(operation, applied, window.vt('시리즈 변경이 반영되지 않았습니다. 내용을 확인한 뒤 다시 저장해 주세요.'));
            } catch (err) {
                if (this.currentMutation(operation)) {
                    this.setStatus(window.vt('시리즈 변경 결과를 확인하지 못했습니다. 결과 확인을 다시 시도해 주세요.'), 'error');
                    this.el.retry.textContent = window.vt('저장 결과 확인'); this.el.retry.hidden = false;
                }
            } finally {
                operation.checking = false;
                if (this.currentMutation(operation)) this.el.retry.disabled = false;
            }
        }
        async finishMutation(operation, applied, message = '') {
            if (!this.currentMutation(operation)) return;
            this.mutation = null;
            if (applied) {
                this.reset();
                try { await this.api.onSaved(operation.scope); } catch (err) { console.error('Series refresh failed:', err); }
            } else {
                this.busy = false; this.el.fields.disabled = this.el.save.disabled = this.el.dissolve.disabled = false;
                this.el.retry.hidden = true; this.el.cancel.textContent = window.vt('취소'); this.el.close.title = window.vt('닫기');
                this.setStatus(message, 'error');
            }
        }
    };
})();
