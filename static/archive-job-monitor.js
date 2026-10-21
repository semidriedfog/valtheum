// A failed progress read says nothing about whether the background write finished.
(() => {
    window.ArchiveJobMonitor = class {
        constructor(fetch) { this.fetch = fetch; }
        stop() { this.controller?.abort(); this.controller = null; }
        pause(ms, signal) {
            return new Promise(resolve => {
                const finish = () => { clearTimeout(timer); signal.removeEventListener('abort', finish); resolve(); };
                const timer = setTimeout(finish, ms);
                signal.addEventListener('abort', finish, {once:true});
                if (signal.aborted) finish();
            });
        }
        async request(url, options, signal) {
            const controller = new AbortController();
            const abort = () => controller.abort();
            let timer, interrupt;
            const interrupted = new Promise((_, reject) => {
                interrupt = () => reject(new Error(window.vt('요청을 취소했습니다.')));
                controller.signal.addEventListener('abort', interrupt, {once:true});
                timer = setTimeout(() => controller.abort(), 15000);
            });
            signal.addEventListener('abort', abort, {once:true});
            if (signal.aborted) abort();
            try {
                // Include response-body consumption in the timeout. A late body cannot repaint the UI.
                return await Promise.race([
                    (async () => (await this.fetch(url,
                        {...options, signal:controller.signal, reportError:false})).json())(),
                    interrupted
                ]);
            } finally {
                clearTimeout(timer);
                signal.removeEventListener('abort', abort);
                controller.signal.removeEventListener('abort', interrupt);
            }
        }
        async start(url, options, {scope, fresh, onUncertain}) {
            this.stop();
            const controller=new AbortController(),signal=controller.signal;
            this.controller=controller;
            const current=()=>!signal.aborted&&fresh();
            const bytes=crypto.getRandomValues(new Uint8Array(16));
            const requestId=Array.from(bytes,b=>b.toString(16).padStart(2,'0')).join('');
            const headers=new Headers(options.headers||{});headers.set('X-Vault-Request',requestId);
            const captured={...options,headers,scope};
            let uncertain=false,failures=0;
            try {
                while(current()) {
                    try {
                        if(uncertain) {
                            const data=await this.request('/api/archive-security/progress?request_id='+requestId,{scope},signal);
                            if(!current())return null;
                            if(data.job?.request_id===requestId&&data.job.job_id) {
                                return {status:'started',job_id:data.job.job_id,request_id:requestId};
                            }
                            if(data.status!=='unknown')throw new Error('Invalid job acknowledgement');
                        }
                        // An unknown request can be sent again with the SAME identity and payload.
                        // Registration and worker launch are atomic on the server.
                        const data=await this.request(url,captured,signal);
                        if(!current())return null;
                        const aliasSaved=url.endsWith('/name')&&Number.isSafeInteger(data.id)&&typeof data.display_name==='string';
                        if(!(data.status==='started'&&data.job_id)&&data.status!=='cancelled'&&!aliasSaved)throw new Error('Missing job acknowledgement');
                        return {...data,request_id:requestId};
                    } catch(error) {
                        if(!current())return null;
                        // Once a response was lost, even a later 409 cannot settle the earlier request.
                        if(!uncertain&&error.status>=400&&error.status<500)throw error;
                        uncertain=true;onUncertain(error);
                        await this.pause(Math.min(5000,1000*2**Math.min(3,failures++)),signal);
                    }
                }
                return null;
            } finally {if(this.controller===controller)this.controller=null;}
        }
        async wait({jobId, scope, fresh, onProgress, onUncertain}) {
            this.stop();
            const controller = new AbortController(), signal = controller.signal;
            this.controller = controller;
            const current = () => !signal.aborted && fresh();
            let failures = 0;
            try {
                while (current()) {
                    let job;
                    try {
                        const query='job_id='+encodeURIComponent(jobId);
                        const data = await this.request('/api/archive-security/progress?'+query, {scope}, signal);
                        if (!current()) return null;
                        job = data.job;
                        if (!job || job.job_id !== jobId || typeof job.running !== 'boolean') {
                            throw new Error(window.vt('작업 번호가 일치하지 않습니다.'));
                        }
                    } catch (error) {
                        if (!current()) return null;
                        onUncertain(error);
                        await this.pause(Math.min(5000, 1000 * 2 ** Math.min(3, failures++)), signal);
                        continue;
                    }
                    failures = 0;
                    onProgress(job);
                    if (!job.running) return job;
                    await this.pause(400, signal);
                }
                return null;
            } finally {
                if (this.controller === controller) this.controller = null;
            }
        }
    };
})();
