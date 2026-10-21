(() => {
    window.ArchivePageEditor = class {
        constructor({fetch,captureScope,isFresh,onChanged,mediaUrl,onManageOriginal}) {
            Object.assign(this,{fetch,captureScope,isFresh,onChanged,mediaUrl,onManageOriginal});
            this.monitor=new window.ArchiveJobMonitor(fetch);
            const get=id=>document.getElementById(id);
            this.dialog=get('archive-page-dialog');this.select=get('archive-page-entry');
            this.fileLabel=get('archive-page-file');
            this.message=get('archive-page-message');this.progress=get('archive-page-progress');
            this.save=get('archive-page-delete');this.close=get('archive-page-close');this.cancel=get('archive-page-cancel');
            this.manageOriginal=get('archive-page-manage-original');
            this.manageOriginal.addEventListener('click',()=>{if(!this.pending){this.reset();return this.onManageOriginal?.();}});
            this.version=0;this.pending=false;this.jobStarted=false;
            this.entries=[];this.selected=new Set();this.boxes=[];this.page=0;this.pageSize=48;
            this.count=get('archive-page-count');this.position=get('archive-page-position');
            this.prev=get('archive-page-prev');this.next=get('archive-page-next');
            this.selectPage=get('archive-page-select-visible');this.clear=get('archive-page-clear');
            this.prev.addEventListener('click',()=>{if(!this.pending&&this.page>0){this.page--;this.render();}});
            this.next.addEventListener('click',()=>{if(!this.pending&&(this.page+1)*this.pageSize<this.entries.length){this.page++;this.render();}});
            this.selectPage.addEventListener('click',()=>{if(!this.pending){for(const name of this.entries.slice(this.page*this.pageSize,(this.page+1)*this.pageSize))this.selected.add(name);this.render();}});
            this.clear.addEventListener('click',()=>{if(!this.pending){this.selected.clear();this.render();}});
            this.close.addEventListener('click',()=>this.dismiss());
            this.dialog.addEventListener('cancel',e=>{e.preventDefault();this.dismiss();});
            get('archive-page-form').addEventListener('submit',e=>{e.preventDefault();this.submit();});
            this.cancel.addEventListener('click',async()=>{
                if(!this.pending||!this.jobStarted||this.cancel.disabled)return;
                const version=this.version,scope=this.scope,jobId=this.jobId;
                const fresh=()=>this.version===version&&this.isFresh(scope)&&this.jobId===jobId&&this.pending;
                this.cancel.disabled=true;
                try {await this.fetch('/api/archive-security/cancel?job_id='+encodeURIComponent(jobId),{method:'POST',scope,discardBody:true});if(fresh())this.message.textContent=window.vt("취소 요청 중…");}
                catch(e){if(fresh()){this.message.textContent=e.message;this.cancel.disabled=false;}}
            });
        }
        isOpen(){return this.dialog.open;}
        dismiss(){if(this.pending){this.detached=true;this.dialog.close();}else this.reset();}
        controls(){
            const count=this.selected.size;
            this.save.disabled=this.pending||!count||count>=this.entries.length;
            this.manageOriginal.disabled=this.pending;this.close.disabled=false;
            this.close.title=window.vt(this.pending?'창을 닫아도 작업은 계속됩니다. 취소하려면 취소 버튼을 눌러 주세요.':'닫기');
            this.prev.disabled=this.pending||this.page===0;
            this.next.disabled=this.pending||(this.page+1)*this.pageSize>=this.entries.length;
            this.selectPage.disabled=this.clear.disabled=this.pending;
            for(const box of this.boxes)box.disabled=this.pending;
            this.count.textContent=window.vt("{0}개 선택 / 전체 {1}개",count,this.entries.length);
            this.save.textContent=window.vt("선택 {0}개 삭제",count);
        }
        setPending(value){this.pending=value;this.controls();}
        reset(){this.monitor.stop();this.detached=false;this.jobId=null;++this.version;this.file=null;this.fileLabel.textContent='';this.entries=[];this.selected.clear();this.boxes=[];this.select.replaceChildren();this.message.textContent='';this.progress.hidden=true;this.cancel.hidden=true;this.jobStarted=false;this.setPending(false);this.dialog.close();}
        render(){
            this.select.replaceChildren();this.boxes=[];
            const start=this.page*this.pageSize;
            for(const [offset,name] of this.entries.slice(start,start+this.pageSize).entries()){
                const label=document.createElement('label');label.className='archive-page-choice';
                const box=document.createElement('input');box.type='checkbox';box.checked=this.selected.has(name);
                box.addEventListener('change',()=>{if(this.pending)return;if(box.checked)this.selected.add(name);else this.selected.delete(name);this.controls();});
                const caption=document.createElement('span');caption.textContent=window.vt("{0}페이지 · {1}",start+offset+1,name);
                label.appendChild(box);
                if(this.mediaUrl){const image=document.createElement('img');image.loading='lazy';image.decoding='async';image.alt=caption.textContent;
                    const path=this.file.rel_path.split(/[\\/]/).map(encodeURIComponent).join('/');
                    image.src=this.mediaUrl(`/api/archive/file/${path}`,{entry:name});label.appendChild(image);}
                label.appendChild(caption);this.select.appendChild(label);this.boxes.push(box);
            }
            this.position.textContent=`${this.page+1} / ${Math.max(1,Math.ceil(this.entries.length/this.pageSize))}`;
            this.controls();
        }
        open(file,entries,index,step=1){
            // Keep the captured file and selected images while its job is running.
            if((this.pending||this.detached)&&this.isFresh(this.scope)){this.detached=false;if(!this.isOpen())this.dialog.showModal();return;}
            if(!file?.encrypted||file.file_type!=='ARCHIVE'||entries.length<2)return;
            this.reset();this.file=file;this.scope=this.captureScope();this.index=index;
            this.fileLabel.textContent=file.display_name||file.rel_path;
            this.entries=[...entries];this.selected.add(entries[index]);
            this.page=Math.floor(index/this.pageSize);this.render();this.dialog.showModal();
            const current=this.boxes[index%this.pageSize];current?.focus();
        }
        async submit(){
            if(this.pending||!this.file||!this.isOpen()||!this.selected.size||this.selected.size>=this.entries.length)return;
            const scope=this.scope,version=this.version,file=this.file,entries=this.entries.filter(name=>this.selected.has(name));
            const index=this.entries.slice(0,this.index).filter(name=>!this.selected.has(name)).length;
            const fresh=()=>this.isFresh(scope)&&this.version===version;
            this.setPending(true);this.message.textContent=window.vt("삭제 준비 중…");
            try {
                const result=await this.monitor.start(`/api/archive-security/${file.id}/page`,{method:'DELETE',headers:{'Content-Type':'application/json'},body:JSON.stringify({entries})}, {scope,fresh,onUncertain:()=>{this.message.textContent=window.vt('작업 상태를 받지 못해 다시 확인하고 있습니다. 결과가 확인될 때까지 새 작업을 시작하지 않습니다. 취소하거나 창을 닫을 수 있습니다.');}});
                if(!fresh())return;
                if(result.status!=='started')throw new Error(window.vt("삭제 작업을 시작하지 못했습니다."));
                if(!result.job_id)throw new Error(window.vt("작업 번호를 확인하지 못했습니다. 목록을 새로 확인해 주세요."));
                    this.jobId=result.job_id;this.jobStarted=true;this.cancel.hidden=false;this.cancel.disabled=false;this.progress.hidden=false;
                const job=await this.monitor.wait({jobId:result.job_id,scope,fresh,
                    onProgress:job=>{
                        this.progress.max=job.total||1;this.progress.value=job.done||0;
                        this.message.textContent=`${window.VaultI18n.message(job.phase)||window.vt("처리 중…")}${job.total?' · '+Math.floor(100*(job.done||0)/job.total)+'%':''}`;
                    },
                    onUncertain:()=>{this.message.textContent=window.vt('작업 상태를 받지 못해 다시 확인하고 있습니다. 결과가 확인될 때까지 새 작업을 시작하지 않습니다. 취소하거나 창을 닫을 수 있습니다.');}
                });
                if(!job||!fresh())return;
                if(job.error)throw new Error(window.VaultI18n.message(job.error));
                if(!job.completed)throw new Error(window.vt("삭제 결과를 확인하지 못했습니다."));
                this.reset();await this.onChanged(scope,file,index);
            }catch(e){if(fresh()&&e.name!=='AbortError')this.message.textContent=e.message;}
            finally{if(fresh()){this.setPending(false);this.jobStarted=false;this.cancel.hidden=true;this.progress.hidden=true;}}
        }
    };
})();
