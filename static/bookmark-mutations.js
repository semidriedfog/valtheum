// Keep uncertain writes across media selection changes. Reads reconcile them;
// a lost acknowledgement is never a reason to repeat a mutation automatically.
(() => {
    window.BookmarkMutations = class {
        constructor({fetch,captureScope,isFresh,onChange=()=>{}}) {
            Object.assign(this,{fetch,captureScope,isFresh,onChange});this.pending=new Map();
        }
        reset() { for(const item of this.pending.values())item.cancel?.();this.pending.clear(); }
        get(id) {
            const item=this.pending.get(id);
            if(item&&!this.isFresh(item.scope)){item.cancel?.();this.pending.delete(id);return null;}
            return item;
        }
        busy(id) { return !!this.get(id); }
        uncertainMessage() { return window.vt('저장 위치 변경 결과를 확인하고 있습니다. 목록을 다시 불러와 확인해 주세요.'); }
        async send(id,{bookmarkId,position,thumbnail_data}) {
            if(this.busy(id))return false;
            const scope=this.captureScope(),controller=new AbortController();
            const item={scope,bookmarkId,position,deadline:Date.now()+10000,phase:'writing'};
            item.finished=new Promise(resolve=>{item.finish=resolve;});
            const deleting=bookmarkId!==undefined;
            if(!deleting)item.requestId=Array.from(crypto.getRandomValues(new Uint8Array(16)),n=>n.toString(16).padStart(2,'0')).join('');
            let cancel;
            const stopped=new Promise((_,reject)=>{cancel=()=>{controller.abort();reject(new Error('Bookmark write stopped'));};});
            item.cancel=cancel;this.pending.set(id,item);this.onChange();
            const timer=setTimeout(cancel,10000);
            try {
                const options={method:deleting?'DELETE':'POST',scope,signal:controller.signal,discardBody:true,
                    headers:{'X-Vault-Write-Deadline':String(item.deadline)}};
                if(!deleting){options.headers['Content-Type']='application/json';options.body=JSON.stringify({position,thumbnail_data,request_id:item.requestId});}
                await Promise.race([this.fetch('/api/files/'+id+'/bookmarks'+(deleting?'/'+bookmarkId:''),options),stopped]);
                if(this.pending.get(id)!==item||!this.isFresh(scope))return false;
                this.pending.delete(id);return true;
            } catch(error) {
                if(this.pending.get(id)!==item||!this.isFresh(scope))return false;
                // These responses explicitly reject the write. Other failures,
                // including DELETE 404, are resolved through a fresh list.
                if([400,401,403,408,409,413,422,423].includes(error.status)) {
                    this.pending.delete(id);throw error;
                }
                if(deleting&&error.status===404)item.deadline=Date.now();
                item.phase='uncertain';return false;
            } finally {clearTimeout(timer);item.finish();this.onChange();}
        }
        async beforeRead(id,signal) {
            let item=this.get(id);
            if(item?.phase==='writing') {
                await new Promise((resolve,reject)=>{
                    const abort=()=>{signal.removeEventListener('abort',abort);reject(new Error('Bookmark read stopped'));};
                    if(signal.aborted)return abort();
                    signal.addEventListener('abort',abort,{once:true});
                    item.finished.then(()=>{signal.removeEventListener('abort',abort);resolve();});
                });
                item=this.get(id);
            }
            if(!item||item.phase!=='uncertain')return;
            // After expiry, a delayed request cannot enter the DB write. A list
            // read also waits for any write already holding the DB connection.
            const remaining=Math.max(0,item.deadline-Date.now());
            if(!remaining)return;
            await new Promise((resolve,reject)=>{
                let timer;
                const clean=()=>{clearTimeout(timer);signal.removeEventListener('abort',abort);};
                const abort=()=>{clean();reject(new Error('Bookmark read stopped'));};
                if(signal.aborted)return abort();
                signal.addEventListener('abort',abort,{once:true});
                timer=setTimeout(()=>{clean();resolve();},remaining);
            });
        }
        observe(id,records) {
            const item=this.get(id);
            if(!item||item.phase!=='uncertain'||Date.now()<item.deadline)return null;
            const deleting=item.bookmarkId!==undefined;
            // Archive edits can move a surviving bookmark. Its write receipt stays stable.
            const applied=deleting?!records.some(r=>r.id===item.bookmarkId):records.some(r=>r.request_id===item.requestId);
            this.pending.delete(id);this.onChange();
            return applied?window.vt(deleting?'저장 위치를 삭제했습니다.':'저장 위치가 반영된 것을 확인했습니다.'):
                window.vt('저장 위치 변경이 반영되지 않았습니다. 다시 시도할 수 있습니다.');
        }
    };
})();
