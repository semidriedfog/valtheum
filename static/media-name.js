(() => {
    window.MediaNameEditor = class {
        constructor({fetch,captureScope,isFresh,onChanged}) {
            Object.assign(this,{fetch,captureScope,isFresh,onChanged});
            this.monitor=new window.ArchiveJobMonitor(fetch);
            const get=id=>document.getElementById(id);
            this.dialog=get('media-name-dialog');this.input=get('media-name-input');
            this.original=get('media-name-original');this.message=get('media-name-message');
            this.includeRestore=get('media-name-include-restore');this.cancel=get('media-name-cancel');this.progress=get('media-name-progress');
            this.save=get('media-name-save');this.close=get('media-name-close');this.restore=get('media-name-restore');
            this.pending=false;this.version=0;this.jobStarted=false;
            this.cancel.addEventListener('click',async()=>{
                if(!this.pending||!this.jobStarted||this.cancel.disabled)return;
                const version=this.version,scope=this.scope,jobId=this.jobId;
                const fresh=()=>this.version===version&&this.isFresh(scope)&&this.jobId===jobId&&this.pending;
                this.cancel.disabled=true;
                try{await this.fetch('/api/archive-security/cancel?job_id='+encodeURIComponent(jobId),{method:'POST',scope,discardBody:true});if(fresh())this.message.textContent=window.vt("취소 요청 중…");}
                catch(e){if(fresh()){this.message.textContent=e.message;this.cancel.disabled=false;}}
            });
            this.close.addEventListener('click',()=>this.dismiss());
            this.restore.addEventListener('click',()=>{this.input.value=(this.file.restore_name||this.file.rel_path).split(/[\\/]/).pop();this.input.focus();});
            this.dialog.addEventListener('cancel',e=>{e.preventDefault();this.dismiss();});
            get('media-name-form').addEventListener('submit',e=>{e.preventDefault();this.submit();});
        }
        isOpen(){return this.dialog.open;}
        dismiss(){if(this.pending){this.detached=true;this.dialog.close();}else this.reset();}
        setPending(value){this.pending=value;this.input.disabled=this.save.disabled=this.restore.disabled=this.includeRestore.disabled=value;this.close.disabled=false;this.close.title=window.vt(value?'창을 닫아도 작업은 계속됩니다. 취소하려면 취소 버튼을 눌러 주세요.':'닫기');}
        reset(){this.monitor.stop();this.detached=false;this.jobId=null;++this.version;this.file=null;this.input.value='';this.original.textContent='';this.message.textContent='';this.setPending(false);this.jobStarted=false;this.cancel.hidden=true;this.progress.hidden=true;this.includeRestore.checked=true;this.dialog.close();}
        open(file){
            // Reopening resumes the existing operation, even from another card.
            if((this.pending||this.detached)&&this.isFresh(this.scope)){this.detached=false;if(!this.isOpen())this.dialog.showModal();return;}
            if(!file?.encrypted||file.file_type!=='ARCHIVE')return;
            this.reset();this.file=file;this.scope=this.captureScope();
            this.input.value=(file.display_name||file.rel_path).split(/[\\/]/).pop();
            this.original.textContent=window.vt("현재 복원 파일명: {0}", file.restore_name||file.rel_path);
            this.dialog.showModal();this.input.focus();this.input.select();
        }
        async submit(){
            if(this.pending||!this.file||!this.isOpen())return;
            const scope=this.scope,version=this.version,id=this.file.id;
            const fresh=()=>this.isFresh(scope)&&this.version===version;
            this.setPending(true);this.message.textContent=window.vt("저장 중…");
            try {
                const result=await this.monitor.start(`/api/files/${id}/name`,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({name:this.input.value,restore_name:this.includeRestore.checked}),scope}, {scope,fresh,onUncertain:()=>{this.message.textContent=window.vt('작업 상태를 받지 못해 다시 확인하고 있습니다. 결과가 확인될 때까지 새 작업을 시작하지 않습니다. 취소하거나 창을 닫을 수 있습니다.');}});
                if(!fresh())return;
                if(result.status==='started') {
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
                    if(!job.completed)throw new Error(window.vt("이름 변경 결과를 확인하지 못했습니다."));
                }
                this.reset();await this.onChanged(scope);
            } catch(e) {if(fresh()&&e.name!=='AbortError')this.message.textContent=e.message;}
            finally {if(fresh()){this.setPending(false);this.jobStarted=false;this.cancel.hidden=true;this.progress.hidden=true;}}
        }
    };
})();
