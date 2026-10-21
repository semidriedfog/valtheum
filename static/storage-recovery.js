(() => {
    window.StorageRecovery = class {
        constructor({fetch, captureScope, isFresh, canRecover, onRecovered}) {
            Object.assign(this, {fetch, captureScope, isFresh, canRecover, onRecovered});
            this.button = document.getElementById('btn-reconnect-storage');
            this.cancelButton = document.getElementById('btn-cancel-reconnect');
            this.status = document.getElementById('storage-status');
            this.pending = false; this.sequence = 0;
            this.button.addEventListener('click', () => this.reconnect());
            this.cancelButton.addEventListener('click', () => this.cancel());
        }
        note() {
            if (!this.canRecover() || this.pending) return;
            this.status.hidden = false;
            this.status.textContent = window.vt('미디어를 읽지 못했습니다. 저장소 연결 또는 파일 상태를 확인한 뒤 다시 연결해 주세요.');
        }
        finish(job) {
            clearTimeout(job.timer);
            if (this.job !== job) return;
            this.job = null; this.pending = false; this.button.disabled = false; this.cancelButton.hidden = true;
        }
        stop(job) {
            // IDs keep a delayed cancellation from cancelling a newer attempt.
            if (this.isFresh(job.scope)) {
                const controller = new AbortController();
                const timer = setTimeout(() => controller.abort(), 5000);
                this.fetch('/api/storage/cancel', {method:'POST',discardBody:true, scope:job.scope,
                    headers:{'X-Vault-Reconnect':job.id}, signal:controller.signal})
                    .catch(() => {}).finally(() => clearTimeout(timer));
            }
            job.controller.abort();
            this.finish(job);
        }
        cancel() {
            if (!this.job) return;
            this.stop(this.job);
            this.status.hidden = false;
            this.status.textContent = window.vt('재연결 대기를 취소했습니다. 기존 목록은 유지됩니다.');
        }
        reset() {
            if (this.job) this.stop(this.job);
            this.status.hidden = true; this.status.textContent = '';
        }
        async reconnect() {
            if (this.pending || !this.canRecover()) return;
            const scope = this.captureScope();
            const job = {scope, controller:new AbortController(), id:`${Date.now()}-${++this.sequence}`};
            this.job = job; this.pending = true; this.button.disabled = true; this.cancelButton.hidden = false;
            this.status.hidden = false;
            this.status.textContent = window.vt('저장소에 다시 연결하고 있습니다…');
            // The server waits 15 seconds; this also bounds a stalled HTTP response.
            job.timer = setTimeout(() => {
                if (this.job !== job) return;
                this.stop(job);
                if (this.isFresh(scope)) this.status.textContent = window.vt('재연결 응답을 받지 못했습니다. NAS 연결을 확인하거나 라이브러리를 닫아 주세요.');
            }, 20000);
            try {
                await this.fetch('/api/storage/reconnect', {method:'POST',discardBody:true, scope,
                    headers:{'X-Vault-Reconnect':job.id}, signal:job.controller.signal});
                if (this.job !== job || !this.isFresh(scope)) return;
                await this.onRecovered(scope);
                if (this.job === job && this.isFresh(scope)) this.status.textContent = window.vt('연결을 다시 열었습니다. 미디어를 다시 불러옵니다.');
            } catch (error) {
                if (this.job === job && this.isFresh(scope)) this.status.textContent = error.message;
            } finally { this.finish(job); }
        }
    };
})();
