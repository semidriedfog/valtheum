(() => {
    'use strict';
    window.IntegrityCheck = class {
        constructor({fetch, captureScope, isFresh, canRun}) {
            Object.assign(this, {fetch, captureScope, isFresh, canRun});
            const get = id => document.getElementById(id);
            this.dialog = get('integrity-dialog');
            this.button = get('btn-integrity-check');
            this.startButton = get('integrity-start'); this.cancelButton = get('integrity-cancel');
            this.status = get('integrity-status'); this.summary = get('integrity-summary');
            this.progress = get('integrity-progress'); this.results = get('integrity-results');
            this.empty = get('integrity-empty'); this.truncated = get('integrity-truncated');
            this.finished = get('integrity-finished');
            this.version = 0; this.request = 0; this.applied = 0; this.controllers = new Set();
            this.uncertain = false; this.failures = 0; this.failureMessage = '';
            this.button.addEventListener('click', () => this.open());
            this.startButton.addEventListener('click', () => this.start());
            this.cancelButton.addEventListener('click', () => this.cancel());
            get('integrity-close').addEventListener('click', () => this.close());
            get('integrity-header-close').addEventListener('click', () => this.close());
            this.dialog.addEventListener('cancel', event => { event.preventDefault(); this.close(); });
            this.render();
        }
        clearTimer() { clearTimeout(this.timer); this.timer = null; }
        isOpen() { return this.dialog.open; }
        close() {
            ++this.version; this.clearTimer();
            // Aborting an HTTP request does not cancel a check already accepted by the server.
            if (this.pending) this.uncertain = true;
            this.pending = null;
            for (const controller of this.controllers) {
                clearTimeout(controller.timer); controller.abort();
            }
            this.controllers.clear(); this.dialog.close(); this.controls();
        }
        reset() {
            this.close();
            this.job = null; this.scope = null; this.lastCompletedAt = null;
            this.uncertain = false; this.failures = 0; this.failureMessage = '';
            this.status.textContent = ''; this.render(); this.dialog.close();
        }
        async open() {
            if (!this.canRun()) return;
            if (!this.scope || !this.isFresh(this.scope)) { this.reset(); this.scope = this.captureScope(); }
            if (!this.dialog.open) this.dialog.showModal();
            if (!this.pending) await this.update('GET');
        }
        async start() {
            if (this.pending || this.uncertain || this.job?.running || !this.canRun()) return;
            if (!this.scope || !this.isFresh(this.scope)) return;
            await this.update('POST');
        }
        async cancel() {
            if (this.pending || !this.job?.running || !this.scope || !this.isFresh(this.scope)) return;
            await this.update('POST', '/cancel');
        }
        schedule() {
            this.clearTimer();
            if (!this.pending && this.dialog.open && (this.job?.running || this.uncertain) && this.scope && this.isFresh(this.scope)) {
                const delay = Math.min(5000, 1000 * 2 ** Math.min(3, Math.max(0, this.failures - 1)));
                this.timer = setTimeout(() => { this.timer = null; return this.update('GET'); }, delay);
            }
        }
        async update(method, suffix = '') {
            this.clearTimer();
            const scope = this.scope, version = this.version, request = ++this.request;
            const controller = new AbortController(); this.controllers.add(controller);
            const fresh = () => version === this.version && this.scope === scope && this.isFresh(scope);
            const action = method === 'POST';
            // A read sent before a mutation cannot settle that mutation's outcome.
            this.applied = request;
            let reconcile = false;
            if (action) this.pending = suffix ? 'cancel' : 'start';
            this.render();
            if (action) this.status.textContent = window.vt(suffix ? '검사 취소를 요청하고 있습니다…' : '빠른 검사를 시작하고 있습니다…');
            let onAbort;
            const interrupted = new Promise((_, reject) => {
                onAbort = () => reject(Object.assign(new Error('Request cancelled'), {name:'AbortError'}));
                controller.signal.addEventListener('abort', onAbort, {once:true});
            });
            controller.timer = setTimeout(() => controller.abort(), 20000);
            try {
                const options = {method, scope, signal:controller.signal};
                if (suffix === '/cancel') {
                    options.headers = {'Content-Type':'application/json'};
                    options.body = JSON.stringify({id:this.job.id});
                }
                // Bound waiting even if a response body or fetch wrapper does not forward aborts.
                const result = async () => (await this.fetch(`/api/integrity/quick${suffix}`, options)).json();
                const job = await Promise.race([result(), interrupted]);
                if (!fresh() || request < this.applied) return;
                this.applied = request; this.job = job;
                this.uncertain = false; this.failures = 0; this.failureMessage = '';
                if (job.completed && job.finished_at) this.lastCompletedAt = job.finished_at;
                this.render();
            } catch (error) {
                if (fresh() && request >= this.applied) {
                    const message = window.VaultI18n?.message(error.message) || error.message;
                    if (action && !suffix && error.status >= 400 && error.status < 500) {
                        // A rejected start did not create a job. Keep its reason visible and allow retry.
                        this.status.textContent = message;
                    } else {
                        this.uncertain = true;
                        this.failureMessage = error.name === 'AbortError' ? '' : message;
                        if (!action) this.failures += 1;
                        reconcile = action;
                        this.render();
                    }
                }
            } finally {
                clearTimeout(controller.timer); controller.signal.removeEventListener('abort', onAbort);
                this.controllers.delete(controller);
                if (fresh() && request >= this.applied) {
                    if (action) this.pending = null;
                    this.controls();
                    // A lost POST response may hide a running job. Query it before allowing another start.
                    if (reconcile && this.dialog.open) await this.update('GET');
                    else this.schedule();
                }
            }
        }
        controls() {
            this.startButton.disabled = !!this.pending || this.uncertain || !!this.job?.running;
            this.startButton.textContent = window.vt(this.job?.id ? '다시 검사' : '검사 시작');
            this.cancelButton.hidden = !this.job?.running;
            this.cancelButton.disabled = !!this.pending;
        }
        render() {
            this.controls();
            const job = this.job;
            this.progress.hidden = !job?.running;
            this.progress.max = Math.max(1, job?.total || 0); this.progress.value = job?.checked || 0;
            this.summary.textContent = job?.id ? window.vt('확인 {0}/{1} · 빠른 검사 통과 {2} · 문제 {3} · 확인 불가 {4}',
                job.checked, job.total, job.ok, job.issues, job.unavailable) : '';
            this.finished.textContent = this.lastCompletedAt
                ? window.vt('마지막 완료: {0}', new Date(this.lastCompletedAt * 1000).toLocaleString(window.VaultI18n?.locale))
                : window.vt('이 라이브러리를 연 뒤 완료한 검사 없음');
            const phases = {db:'DB와 키를 확인하고 있습니다…', files:'미디어 파일을 확인하고 있습니다…',
                done:'빠른 검사가 완료되었습니다.', cancelled:'검사를 취소했습니다. 확인한 항목의 결과만 표시합니다.', error:'검사를 완료하지 못했습니다.'};
            this.status.textContent = this.uncertain
                ? [this.failureMessage, window.vt('검사 상태를 받지 못해 다시 확인하고 있습니다. 확인될 때까지 새 검사를 시작할 수 없습니다.')].filter(Boolean).join('\n')
                : job?.error ? (window.VaultI18n?.message(job.error) || job.error)
                : window.vt(job?.id ? (phases[job.phase] || '검사 중…') : '검사를 시작하면 현재 라이브러리를 확인합니다.');
            this.results.replaceChildren();
            const labels = {missing:'파일 없음', mismatch:'검증 불일치', invalid_path:'경로 확인 필요', key_error:'키 확인 필요',
                db_error:'DB 확인 필요', pending:'암호화 미완료', unavailable:'확인 불가'};
            for (const result of job?.results || []) {
                const row = document.createElement('li'); row.className = 'integrity-result';
                const head = document.createElement('div'); head.className = 'integrity-result-heading';
                const name = document.createElement('strong'); name.textContent = result.name;
                const badge = document.createElement('span'); badge.className = 'integrity-result-state';
                badge.textContent = window.vt(labels[result.state] || '문제 발견');
                const detail = document.createElement('p'); detail.textContent = window.VaultI18n?.message(result.message) || result.message;
                head.appendChild(name); head.appendChild(badge); row.appendChild(head); row.appendChild(detail);
                this.results.appendChild(row);
            }
            this.empty.hidden = !!(job?.results?.length);
            this.empty.textContent = window.vt(!job?.id ? '문제가 발견되거나 확인하지 못한 항목이 여기에 표시됩니다.'
                : job.running ? '현재까지 문제나 확인 불가 항목이 없습니다.'
                : job.completed ? '빠른 검사 범위에서 문제나 확인 불가 항목이 발견되지 않았습니다.'
                : '확인한 항목에서 문제나 확인 불가 항목이 발견되지 않았습니다.');
            this.truncated.hidden = !job?.results_truncated;
        }
    };
})();
